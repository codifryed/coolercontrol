/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use super::{find_on_path, ServiceId, ServiceIdExt};
use crate::cc_fs;
use crate::repositories::service_plugin::service_management::manager::{
    ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_plugin_repo::CC_PLUGIN_USER;
use anyhow::{anyhow, Result};
use log::debug;
use std::fmt::Write;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::{Output, Stdio};
use tokio::process::Command;

const RC_SERVICE: &str = "rc-service";
const SERVICE_FILE_PERMISSIONS: u32 = 0o755;

#[derive(Clone, Debug, Default)]
pub struct OpenRcManager {}

impl OpenRcManager {
    pub fn detected() -> bool {
        find_on_path(RC_SERVICE).is_some()
    }

    /// Creates the plugin user. Returns an error if user creation fails.
    /// An error is also returned if the user already exists.
    async fn create_plugin_user(username: &str) -> Result<()> {
        // Try `useradd` first - works on Gentoo, Artix, and Void Linux.
        let useradd_ok = Command::new("useradd")
            .arg("--system")
            .arg("--comment")
            .arg("CoolerControl unprivileged plugin user")
            .arg("--shell")
            .arg("/usr/sbin/nologin")
            .arg(username)
            .status()
            .await
            .is_ok_and(|s| s.success());
        if useradd_ok {
            return Ok(());
        }
        // Fall back to `adduser` for Alpine Linux (BusyBox).
        Command::new("adduser")
            .arg("-S") // system user
            .arg("-D") // no password
            .arg("-H") // no home directory
            .arg("-h")
            .arg("/dev/null")
            .arg("-s")
            .arg("/sbin/nologin")
            .arg(username)
            .status()
            .await
            .map_err(Into::into)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Failed to create user {username} with exit code: {}",
                        status.code().unwrap_or(-1)
                    ))
                }
            })
    }

    async fn rc_service(cmd: &str, service_id: &ServiceId) -> Result<Output> {
        Command::new(RC_SERVICE)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg(service_id.to_service_name())
            .arg(cmd)
            .output()
            .await
            .map_err(Into::into)
            .and_then(|output| {
                if output.status.success() {
                    Ok(output)
                } else {
                    let err = String::from_utf8_lossy(&output.stderr).to_string();
                    let out = String::from_utf8_lossy(&output.stdout).to_string();
                    Err(anyhow!(
                        "rc-service {cmd} for {service_id} failed (code {}): {err} {out}",
                        output.status.code().unwrap_or(-1)
                    ))
                }
            })
    }

    /// Like `rc_service`, but returns the raw `Output` regardless of exit code.
    /// Required for `status`, where the exit code itself carries the service state.
    async fn rc_service_output(cmd: &str, service_id: &ServiceId) -> Result<Output> {
        Command::new(RC_SERVICE)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg(service_id.to_service_name())
            .arg(cmd)
            .output()
            .await
            .map_err(Into::into)
    }
}

impl ServiceManager for OpenRcManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        let dir_path = service_dir_path();
        cc_fs::create_dir_all(&dir_path).await?;
        let service_name = service_definition.service_id.to_service_name();
        let service_description = service_definition.service_id.to_description();
        let service_path = dir_path.join(&service_name);
        if service_definition.username.is_some() {
            if let Err(err) = Self::create_plugin_user(CC_PLUGIN_USER).await {
                debug!("Failed to create plugin user (expected when exists): {err}");
            }
        }
        let service_file =
            create_service_file(&service_description, &service_name, &service_definition);
        cc_fs::write_string(&service_path, service_file).await?;
        cc_fs::set_permissions(
            &service_path,
            Permissions::from_mode(SERVICE_FILE_PERMISSIONS),
        )
        .await
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        let _ = self.stop(service_id).await;
        let service_path = service_dir_path().join(service_id.to_service_name());
        cc_fs::remove_file(service_path).await
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        Self::rc_service("start", service_id).await.map(|_| ())
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        Self::rc_service("stop", service_id).await.map(|_| ())
    }

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        // Use rc_service_output (not rc_service) so that non-zero exit codes
        // reach the match below -- they encode the service state, not errors.
        let output = Self::rc_service_output("status", service_id).await?;
        let status_text = {
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.trim().is_empty() {
                String::from_utf8_lossy(&output.stdout).trim().to_string()
            } else {
                stderr.trim().to_string()
            }
        };
        match output.status.code() {
            Some(0) => Ok(ServiceStatus::Running),
            // Exit code 3 is the POSIX standard for "stopped".
            Some(3) => Ok(ServiceStatus::Stopped(Some(status_text))),
            // Exit code 1: either "does not exist" or a crashed/unclear state.
            Some(1) if status_text.contains("does not exist") => Ok(ServiceStatus::NotInstalled),
            Some(1) => Ok(ServiceStatus::Stopped(Some(status_text))),
            _ => Err(anyhow!(
                "Unexpected rc-service status exit code {} for {}: {}",
                output.status.code().unwrap_or(-1),
                service_id.to_service_name(),
                status_text,
            )),
        }
    }
}

#[inline]
fn service_dir_path() -> PathBuf {
    PathBuf::from("/etc/init.d")
}

fn create_service_file(
    description: &str,
    provide: &str,
    service_definition: &ServiceDefinition,
) -> String {
    let mut script = String::new();
    let args = service_definition.args.join(" ");
    let program_path = service_definition.executable.to_string_lossy();
    let _ = writeln!(script, "#!/sbin/openrc-run");
    let _ = writeln!(script);
    let _ = writeln!(script, "description=\"{description}\"");
    let _ = writeln!(script, "command=\"{program_path}\"");
    let _ = writeln!(script, "command_args=\"{args}\"");
    if let Some(username) = &service_definition.username {
        let _ = writeln!(script, "command_user=\"{username}:{username}\"");
    }
    let _ = writeln!(script, "pidfile=\"/run/${{RC_SVCNAME}}.pid\"");
    let _ = writeln!(script, "command_background=true");
    if let Some(envs) = &service_definition.envs {
        if !envs.is_empty() {
            let _ = writeln!(script);
            for (var, val) in envs {
                let _ = writeln!(script, "export {var}=\"{val}\"");
            }
        }
    }
    let _ = writeln!(script);
    let _ = writeln!(script, "depend() {{");
    let _ = writeln!(script, "    provide {provide}");
    let _ = write!(script, "}}");
    script
}
