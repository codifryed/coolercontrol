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

use super::{ensure_plugin_user, find_on_path, ServiceId, ServiceIdExt};
use crate::cc_fs;
use crate::repositories::service_plugin::service_management::manager::{
    ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_plugin_repo::CC_PLUGIN_USER;
use crate::repositories::utils::DirectCommand;
use anyhow::{anyhow, Result};
use std::fmt::Write;
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;

const RC_SERVICE: &str = "rc-service";
const RC_SERVICE_TIMEOUT: Duration = Duration::from_secs(10);
const SERVICE_FILE_PERMISSIONS: u32 = 0o755;

#[derive(Clone, Debug, Default)]
pub struct OpenRcManager {}

impl OpenRcManager {
    pub fn detected() -> bool {
        find_on_path(RC_SERVICE).is_some()
    }

    /// Returns `(exit_code, stdout, stderr)`. `Err` only on spawn failure or timeout.
    async fn rc_service(cmd: &str, service_id: &ServiceId) -> Result<(i32, String, String)> {
        DirectCommand::new(RC_SERVICE, RC_SERVICE_TIMEOUT)
            .arg(service_id.to_service_name())
            .arg(cmd)
            .run_with_code()
            .await
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
            ensure_plugin_user(CC_PLUGIN_USER).await;
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
        let (code, _, stderr) = Self::rc_service("start", service_id).await?;
        if code != 0 {
            Err(anyhow!(
                "rc-service start {} failed: {stderr}",
                service_id.to_service_name()
            ))
        } else {
            Ok(())
        }
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        let (code, _, stderr) = Self::rc_service("stop", service_id).await?;
        if code != 0 {
            Err(anyhow!(
                "rc-service stop {} failed: {stderr}",
                service_id.to_service_name()
            ))
        } else {
            Ok(())
        }
    }

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        let (code, stdout, stderr) = Self::rc_service("status", service_id).await?;
        let status_text = if stderr.trim().is_empty() {
            stdout.trim().to_string()
        } else {
            stderr.trim().to_string()
        };
        #[allow(clippy::match_same_arms)]
        match code {
            0 => Ok(ServiceStatus::Running),
            // Exit code 3 is the POSIX standard for "stopped".
            3 => Ok(ServiceStatus::Stopped(Some(status_text))),
            // Exit code 1: either "does not exist" or a crashed/unclear state.
            1 if status_text.contains("does not exist") => Ok(ServiceStatus::Unmanaged),
            1 => Ok(ServiceStatus::Stopped(Some(status_text))),
            _ => Err(anyhow!(
                "Unexpected rc-service status exit code {} for {}: {}",
                code,
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
