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
use anyhow::{anyhow, Result};
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
                    "rc-service command {cmd} for service {service_id} failed with exit code {}: {err} {out}",
                    output.status.code().unwrap_or(-1)
                ))
            }
        })
    }
}

impl ServiceManager for OpenRcManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        let dir_path = service_dir_path();
        cc_fs::create_dir_all(&dir_path)?;
        let service_name = service_definition.service_id.to_service_name();
        let service_description = service_definition.service_id.to_description();
        let service_path = dir_path.join(&service_name);
        let service_file =
            create_service_file(&service_description, &service_name, &service_definition);
        cc_fs::write_string(&service_path, service_file).await?;
        cc_fs::set_permissions(
            &service_path,
            Permissions::from_mode(SERVICE_FILE_PERMISSIONS),
        )
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        let _ = self.stop(service_id).await;
        let service_path = service_dir_path().join(service_id.to_service_name());
        cc_fs::remove_file(service_path)
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        Self::rc_service("start", service_id).await.map(|_| ())
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        Self::rc_service("stop", service_id).await.map(|_| ())
    }

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        let output = Self::rc_service("status", service_id).await?;
        match output.status.code() {
            Some(1) => {
                let mut stdio = String::from_utf8_lossy(&output.stderr);
                if stdio.trim().is_empty() {
                    stdio = String::from_utf8_lossy(&output.stdout);
                }
                if stdio.contains("does not exist") {
                    Ok(ServiceStatus::NotInstalled)
                } else {
                    Err(anyhow!(
                        "Failed to get status of service {}: {}",
                        service_id.to_service_name(),
                        stdio
                    ))
                }
            }
            Some(0) => Ok(ServiceStatus::Running),
            Some(3) => Ok(ServiceStatus::Stopped(None)),
            _ => Err(anyhow!(
                "Failed to get status of service {}: {}",
                service_id.to_service_name(),
                String::from_utf8_lossy(&output.stderr)
            )),
        }
    }
}

#[inline]
fn service_dir_path() -> PathBuf {
    PathBuf::from("/etc/init.d")
}

// User-based services were recently supported by OpenRC. Needs testing.
fn create_service_file(
    description: &str,
    provide: &str,
    service_definition: &ServiceDefinition,
) -> String {
    let args = service_definition.args.join(" ");
    let program_path = service_definition.executable.to_string_lossy();
    format!(
        r#"
#!/sbin/openrc-run

description="{description}"
command="{program_path}"
command_args="{args}"
pidfile="/run/${{RC_SVCNAME}}.pid"
command_background=true

depend() {{
    provide {provide}
}}
    "#
    )
    .trim()
    .to_string()
}
