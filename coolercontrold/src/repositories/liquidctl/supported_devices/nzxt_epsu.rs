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

use std::collections::HashMap;

use crate::device::{DeviceInfo, LightingMode};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

#[derive(Debug)]
pub struct NzxtEPsuSupport;

impl NzxtEPsuSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for NzxtEPsuSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::NzxtEPsu
    }

    fn extract_info(&self, _device_index: &u8, _device_props: &DeviceProperties) -> DeviceInfo {
        // fan control currently no supported
        let channels = HashMap::new();
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20, // device has temp
            temp_max: 100,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        Vec::new()
    }
}
