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


use anyhow::Result;
use async_trait::async_trait;

use crate::Device;
use crate::setting::Setting;

/// A Repository is used to access device hardware data
#[async_trait]
pub trait Repository: Send + Sync {
    async fn initialize_devices(&self) -> Result<()>;
    async fn devices(&self) -> Vec<Device>;
    async fn update_statuses(&self) -> Result<()>;
    async fn shutdown(&self) -> Result<()>;
    async fn apply_setting(&self, device_type_id: u8, setting: Setting) -> Result<()>;
}