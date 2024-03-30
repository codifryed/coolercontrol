/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use log::error;
use tokio::sync::RwLock;

use crate::device::{DeviceType, UID};
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use crate::Device;

pub type DeviceLock = Arc<RwLock<Device>>;
pub type DeviceList = Vec<DeviceLock>;

/// A Repository is used to access device hardware data
#[async_trait]
pub trait Repository: Send + Sync {
    fn device_type(&self) -> DeviceType;

    async fn initialize_devices(&mut self) -> Result<()>;

    /// Returns a reference to all the devices in this repository
    async fn devices(&self) -> DeviceList;

    /// This method should be called first before calling update_statuses, as it then doesn't
    /// need a write lock to the device itself and all repos can be done in parallel.
    /// Preloading keeps response times for clients really quick and dependable, despite any
    /// bad timing, like calling during the middle of an update which can cause inconsistent
    /// status responses
    async fn preload_statuses(self: Arc<Self>);

    /// This method should be called after preload_statuses to update the internal
    /// device status history with the last polled status
    async fn update_statuses(&self) -> Result<()>;

    async fn shutdown(&self) -> Result<()>;

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()>;
    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()>;

    /// This is for device-internal profiles only, such as some AIOs.
    /// The temp source must then always belong to the device itself.
    /// Everything else is handled by CoolerControl itself.
    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(f64, u8)],
    ) -> Result<()>;
    async fn apply_setting_lighting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lighting: &LightingSettings,
    ) -> Result<()>;
    async fn apply_setting_lcd(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd: &LcdSettings,
    ) -> Result<()>;
    async fn apply_setting_pwm_mode(
        &self,
        device_uid: &UID,
        channel_name: &str,
        pwm_mode: u8,
    ) -> Result<()>;

    /// This is helpful/necessary after waking from sleep
    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }
}
