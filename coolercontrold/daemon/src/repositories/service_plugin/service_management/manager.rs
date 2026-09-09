// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::repositories::service_plugin::service_management::openrc::OpenRcManager;
use crate::repositories::service_plugin::service_management::systemd::SystemdManager;
use crate::repositories::service_plugin::service_management::ServiceId;
use crate::repositories::service_plugin::service_manifest::EnvVar;
use crate::ENV_SERVICE_MANAGER;
use anyhow::{anyhow, Result};
use log::info;
use std::env;
use std::ops::Not;
use std::path::PathBuf;

pub trait ServiceManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()>;

    async fn remove(&self, service_id: &ServiceId) -> Result<()>;

    async fn start(&self, service_id: &ServiceId) -> Result<()>;

    async fn stop(&self, service_id: &ServiceId) -> Result<()>;

    /// Replaces a running service with its current definition, and starts it if stopped.
    ///
    /// Separate from `stop` then `start` because each init system has its own correct way
    /// to do this. Doing it by hand leaves a window where the old process is still winding
    /// down while the new one starts, which is how two of them end up alive at once.
    async fn restart(&self, service_id: &ServiceId) -> Result<()>;

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus>;
}

#[derive(Clone, Debug)]
pub enum Manager {
    OpenRc(OpenRcManager),
    Systemd(SystemdManager),
    Disabled,
}

impl ServiceManager for Manager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.add(service_definition).await,
            Manager::OpenRc(m) => m.add(service_definition).await,
            Manager::Disabled => Ok(()),
        }
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.remove(service_id).await,
            Manager::OpenRc(m) => m.remove(service_id).await,
            Manager::Disabled => Ok(()),
        }
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.start(service_id).await,
            Manager::OpenRc(m) => m.start(service_id).await,
            Manager::Disabled => Ok(()),
        }
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.stop(service_id).await,
            Manager::OpenRc(m) => m.stop(service_id).await,
            Manager::Disabled => Ok(()),
        }
    }

    async fn restart(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.restart(service_id).await,
            Manager::OpenRc(m) => m.restart(service_id).await,
            Manager::Disabled => Ok(()),
        }
    }

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        match self {
            Manager::Systemd(m) => m.status(service_id).await,
            Manager::OpenRc(m) => m.status(service_id).await,
            Manager::Disabled => Ok(ServiceStatus::Running),
        }
    }
}

impl Manager {
    pub fn detect() -> Result<Self> {
        let manager_enabled = env::var(ENV_SERVICE_MANAGER)
            .ok()
            .and_then(|env_service_manager| {
                env_service_manager
                    .parse::<u8>()
                    .ok()
                    .map(|num| num != 0)
                    .or_else(|| Some(env_service_manager.trim().to_lowercase() != "off"))
            })
            .unwrap_or(true);
        if manager_enabled.not() {
            info!("Plugin Service Manager disabled. All plugins will need to be started manually.");
            Ok(Self::Disabled)
        } else if SystemdManager::detected() {
            Ok(Self::Systemd(SystemdManager::default()))
        } else if OpenRcManager::detected() {
            Ok(Self::OpenRc(OpenRcManager::default()))
        } else {
            Err(anyhow!(
                "Failed to detect System Service Manager. The daemon will not be able to manage the plugin processes."
            ))
        }
    }

    pub fn is_systemd(&self) -> bool {
        matches!(self, Self::Systemd(_))
    }

    pub fn is_open_rc(&self) -> bool {
        matches!(self, Self::OpenRc(_))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ServiceStatus {
    Unmanaged,
    Running,
    Stopped(Option<String>), // Provide a reason if possible
}

#[derive(Debug, Clone)]
pub struct ServiceDefinition {
    pub service_id: ServiceId,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub username: Option<String>,
    pub wrk_dir: Option<PathBuf>,
    pub envs: Option<Vec<EnvVar>>,
    pub disable_restart_on_failure: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: `Disabled` stands in wherever no init system manages the plugins, so every
    /// lifecycle call on it has to succeed and do nothing. A `restart` that fell through
    /// to `unreachable!` would abort plugin registration on those systems.
    /// Method: call it and require success.
    #[test]
    fn disabled_manager_restart_succeeds_and_does_nothing() {
        crate::sidecar::ensure_test_handle();
        crate::rt::test_runtime(async {
            let service_id = "test-plugin".to_string();
            assert!(Manager::Disabled.restart(&service_id).await.is_ok());
        });
    }
}
