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

use crate::repositories::service_plugin::service_management::openrc::OpenRcManager;
use crate::repositories::service_plugin::service_management::systemd::SystemdManager;
use crate::repositories::service_plugin::service_management::ServiceId;
use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub trait ServiceManager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()>;

    async fn remove(&self, service_id: &ServiceId) -> Result<()>;

    async fn start(&self, service_id: &ServiceId) -> Result<()>;

    async fn stop(&self, service_id: &ServiceId) -> Result<()>;

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus>;
}

#[derive(Clone, Debug)]
pub enum Manager {
    OpenRc(OpenRcManager),
    Systemd(SystemdManager),
}

impl ServiceManager for Manager {
    async fn add(&self, service_definition: ServiceDefinition) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.add(service_definition).await,
            Manager::OpenRc(m) => m.add(service_definition).await,
        }
    }

    async fn remove(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.remove(service_id).await,
            Manager::OpenRc(m) => m.remove(service_id).await,
        }
    }

    async fn start(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.start(service_id).await,
            Manager::OpenRc(m) => m.start(service_id).await,
        }
    }

    async fn stop(&self, service_id: &ServiceId) -> Result<()> {
        match self {
            Manager::Systemd(m) => m.stop(service_id).await,
            Manager::OpenRc(m) => m.stop(service_id).await,
        }
    }

    async fn status(&self, service_id: &ServiceId) -> Result<ServiceStatus> {
        match self {
            Manager::Systemd(m) => m.status(service_id).await,
            Manager::OpenRc(m) => m.status(service_id).await,
        }
    }
}

impl Manager {
    pub fn detect() -> Result<Self> {
        if SystemdManager::detected() {
            return Ok(Self::Systemd(SystemdManager::default()));
        } else if OpenRcManager::detected() {
            return Ok(Self::OpenRc(OpenRcManager::default()));
        }
        Err(anyhow!("Failed to detect System Service Manager"))
    }

    // pub fn is_systemd(&self) -> bool {
    //     matches!(self, Self::Systemd(_))
    // }
    //
    // pub fn is_open_rc(&self) -> bool {
    //     matches!(self, Self::OpenRc(_))
    // }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ServiceStatus {
    NotInstalled,
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
    pub envs: Option<Vec<(String, String)>>,
    pub disable_restart_on_failure: bool,
}
