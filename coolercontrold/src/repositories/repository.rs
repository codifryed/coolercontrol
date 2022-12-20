/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/


use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use log::error;
use tokio::sync::RwLock;

use crate::Device;
use crate::device::{DeviceType, UID};
use crate::setting::Setting;

pub type DeviceLock = Arc<RwLock<Device>>;
pub type DeviceList = Vec<DeviceLock>;

/// A Repository is used to access device hardware data
#[async_trait]
pub trait Repository: Send + Sync {
    fn device_type(&self) -> DeviceType;

    async fn initialize_devices(&mut self) -> Result<()>;

    /// Returns a reference to all the devices in this repository
    async fn devices(&self) -> DeviceList;

    async fn update_statuses(&self) -> Result<()>;

    async fn shutdown(&self) -> Result<()>;

    async fn apply_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()>;

    /// This is helpful/necessary after waking from sleep
    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository")
    }
}