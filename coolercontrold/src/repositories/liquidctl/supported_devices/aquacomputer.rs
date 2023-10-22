/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
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
 */

use std::collections::HashMap;

use crate::device::{ChannelInfo, DeviceInfo, LightingMode, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liquidctl_repo::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

#[derive(Debug)]
pub struct AquaComputerSupport;

impl AquaComputerSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AquaComputerSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::Aquacomputer
    }

    fn extract_info(&self, _device_index: &u8, device_props: &DeviceProperties) -> DeviceInfo {
        let mut channels = HashMap::new();
        for channel_name in &device_props.speed_channels {
            channels.insert(channel_name.to_owned(), ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            });
        }
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 0,
            temp_max: 100,
            temp_ext_available: true,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        Vec::new()
    }
}
