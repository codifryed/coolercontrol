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

use crate::device::{ChannelInfo, DeviceInfo, LightingMode, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct Hydro690LcSupport;

impl Hydro690LcSupport {
    pub fn new() -> Self {
        Self {}
    }
}

/// The `Hydro690Lc` driver is based on the `Modern690Lc` driver
impl DeviceSupport for Hydro690LcSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::Hydro690Lc
    }

    fn extract_info(&self, _device_index: &u8, _device_props: &DeviceProperties) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(), // pump only supports fixed speed
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 50,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "fan".to_string(), // fan supports speed profiles
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    profiles_enabled: true,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        let color_channels = vec!["logo".to_string()];
        for channel_name in color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name,
                ChannelInfo {
                    lighting_modes,
                    ..Default::default()
                },
            );
        }
        // for this device this is actually time-to-color for 2 modes, like the legacy models
        let lighting_speeds = vec![
            "5".to_string(),
            "4".to_string(),
            "3".to_string(),
            "2".to_string(),
            "1".to_string(),
        ];
        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 6,
            profile_min_length: 2,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        // alert temp is also supported for this device
        // "rainbow" color mode is not supported on the Hydro690Lc (Only for EVGA)
        let color_modes = vec![
            ColorMode::new("blackout", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("fading", 1, 2, true, false),
            ColorMode::new("blinking", 1, 1, true, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
