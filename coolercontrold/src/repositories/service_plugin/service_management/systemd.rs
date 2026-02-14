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

use crate::cc_fs;
use crate::repositories::service_plugin::service_management::manager::{
    ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_management::{
    find_on_path, ServiceId, ServiceIdExt,
};
use crate::repositories::service_plugin::service_plugin_repo::CC_PLUGIN_USER;
use anyhow::{anyhow, Result};
use derive_more::Display;
use log::debug;
use std::fs::Permissions;
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Command;

const SYSTEMCTL: &str = "systemctl";
const SERVICE_FILE_PERMISSIONS: u32 = 0o644;

#[derive(Clone, Debug)]
pub struct SystemdConfig {
    /// interval in seconds to limit number of `burst` starts
    pub start_limit_interval_sec: Option<u32>,
    /// number of starts allowed in `interval`
    pub start_limit_burst: Option<u32>,
    /// restart type (on-failure, always, etc.)
    pub restart: SystemdServiceRestartType,
    /// number of seconds to wait between stopping and starting service
    pub restart_sec: Option<u32>,
    /// number of seconds to wait for service to exit on it own, before sending SIGTERM
    pub timeout_stop_sec: Option<u32>,
}

impl Default for SystemdConfig {
    fn default() -> Self {
        Self {
            start_limit_interval_sec: Some(60),
            start_limit_burst: Some(10),
            restart: SystemdServiceRestartType::OnFailure,
            restart_sec: Some(1),
            timeout_stop_sec: Some(3),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SystemdManager {
    pub config: SystemdConfig,
}

impl SystemdManager {
    pub fn detected() -> bool {
        find_on_path(SYSTEMCTL).is_some()
    }

    async fn systemctl(cmd: &str, service_id: &ServiceId) -> Result<i32> {
        Command::new(SYSTEMCTL)
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg(cmd)
            .arg(service_id.to_service_name())
            .output()
            .await
            .map_err(Into::into)
            .and_then(|output| {
                if output.status.success() {
                    Ok(output.status.code().unwrap_or(-1))
                } else {
                    let err = String::from_utf8_lossy(&output.stderr).to_string();
                    let out = String::from_utf8_lossy(&output.stdout).to_string();
                    Err(anyhow!(
                        "systemctl command {cmd} for service {service_id} failed with exit code {}: {err} {out}",
                        output.status.code().unwrap_or(-1)
                    ))
                }
            })
    }

    /// This will return an error if the user already exists.
    async fn create_plugin_user(username: &str) -> Result<()> {
        Command::new("useradd")
            .arg("--system") // no home dir and id < 1000
            .arg("--comment")
            .arg("CoolerControl unprivileged plugin user")
            .arg("--shell")
            .arg("/usr/sbin/nologin") // no login shell
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

    /// This deletes the user if it exists.
    pub async fn delete_plugin_user(username: &str) -> Result<()> {
        Command::new("userdel")
            .arg(username)
            .status()
            .await
            .map_err(Into::into)
            .and_then(|status| {
                if status.success() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Failed to delete user {username} with exit code: {}",
                        status.code().unwrap_or(-1)
                    ))
                }
            })
    }
}

impl ServiceManager for SystemdManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        let dir_path = systemd_global_dir_path();
        cc_fs::create_dir_all(&dir_path).await?;
        let service_name = service_definition.service_id.to_service_name();
        let service_path = dir_path.join(format!("{service_name}.service"));
        let service_description = service_definition.service_id.to_description();
        if service_definition.username.is_some() {
            if let Err(err) = Self::create_plugin_user(CC_PLUGIN_USER).await {
                debug!("Failed to create plugin user (expected when exists): {err}");
            }
        }
        let unit_file = create_unit_file(&self.config, &service_description, service_definition)?;
        cc_fs::write_string(&service_path, unit_file).await?;
        cc_fs::set_permissions(
            &service_path,
            Permissions::from_mode(SERVICE_FILE_PERMISSIONS),
        )
        .await
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        let dir_path = systemd_global_dir_path();
        let service_name = service_id.to_service_name();
        let service_path = dir_path.join(format!("{service_name}.service"));
        let _ = self.stop(service_id).await;
        cc_fs::remove_file(service_path).await
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        Self::systemctl("start", service_id).await.map(|_| ())
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        Self::systemctl("stop", service_id).await.map(|_| ())
    }

    /// See: `https://www.freedesktop.org/software/systemd/man/latest/systemctl.html#Exit%20status`
    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        let status_code = Self::systemctl("status", service_id).await?;
        match status_code {
            4 => Ok(ServiceStatus::NotInstalled),
            3 => Ok(ServiceStatus::Stopped(None)),
            0 => Ok(ServiceStatus::Running),
            _ => Err(anyhow!(
                "Unexpected systemctl command exit code: {status_code}"
            )),
        }
    }
}

#[inline]
fn systemd_global_dir_path() -> PathBuf {
    PathBuf::from("/etc/systemd/system")
}

fn create_unit_file(
    config: &SystemdConfig,
    description: &String,
    service_definition: ServiceDefinition,
) -> Result<String> {
    use std::fmt::Write;
    let mut service = String::new();
    writeln!(service, "[Unit]")?;
    writeln!(service, "Description={description}")?;
    if let Some(start_limit_interval) = config.start_limit_interval_sec {
        writeln!(service, "StartLimitIntervalSec={start_limit_interval}")?;
    }
    if let Some(start_limit_burst) = config.start_limit_burst {
        writeln!(service, "StartLimitBurst={start_limit_burst}")?;
    }
    writeln!(service, "[Service]")?;
    writeln!(service, "Type=simple")?;
    if let Some(username) = service_definition.username {
        writeln!(service, "User={username}")?;
        writeln!(service, "Group={username}")?;
    }
    if let Some(working_directory) = service_definition.wrk_dir {
        writeln!(
            service,
            "WorkingDirectory={}",
            working_directory.to_string_lossy()
        )?;
    }
    if let Some(env_vars) = service_definition.envs {
        for (var, val) in env_vars {
            let _ = writeln!(service, "Environment=\"{var}={val}\"");
        }
    }
    let program = service_definition.executable.to_string_lossy();
    let args = service_definition.args.join(" ");
    writeln!(service, "ExecStart={program} {args}")?;
    if service_definition.disable_restart_on_failure.not() {
        if config.restart != SystemdServiceRestartType::No {
            writeln!(service, "Restart={}", config.restart)?;
        }
        if let Some(restart_secs) = config.restart_sec {
            writeln!(service, "RestartSec={restart_secs}")?;
        }
    }
    if let Some(timeout_stop_sec) = config.timeout_stop_sec {
        writeln!(service, "TimeoutStopSec={timeout_stop_sec}")?;
    }
    Ok(service.trim().to_string())
}

#[derive(Copy, Clone, Display, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SystemdServiceRestartType {
    #[default]
    #[display("no")]
    No,
    #[display("always")]
    Always,
    #[display("on-success")]
    OnSuccess,
    #[display("on-failure")]
    OnFailure,
    #[display("on-abnormal")]
    OnAbnormal,
    #[display("on-abort")]
    OnAbort,
    #[display("on-watch")]
    OnWatch,
}
