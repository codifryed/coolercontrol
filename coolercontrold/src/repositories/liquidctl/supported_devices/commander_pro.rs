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

use std::collections::HashMap;

use crate::device::{ChannelInfo, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct CommanderProSupport;
// commander_pro.py

impl CommanderProSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for CommanderProSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::CommanderPro
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        for channel_name in &device_response.properties.speed_channels {
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        // Internal profiles for the commander pro only work with RPMs! not duty %
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: true,
                    }),
                    ..Default::default()
                },
            );
        }
        for channel_name in &device_response.properties.color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    lighting_modes,
                    ..Default::default()
                },
            );
        }
        let lighting_speeds = vec!["slow".to_string(), "medium".to_string(), "fast".to_string()];
        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 20,
            temp_max: 60,
            driver_info: DriverInfo {
                drv_type: DriverType::Liquidctl,
                name: Some(self.supported_driver().to_string()),
                version: device_response.liquidctl_version.clone(),
                locations: self.collect_driver_locations(device_response),
            },
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("color_shift", 0, 2, true, true),
            ColorMode::new("color_pulse", 0, 2, true, true),
            ColorMode::new("color_wave", 0, 2, true, true),
            ColorMode::new("visor", 0, 2, true, true),
            ColorMode::new("blink", 0, 2, true, true),
            ColorMode::new("marquee", 0, 1, true, true),
            ColorMode::new("sequential", 0, 1, true, true),
            ColorMode::new("rainbow", 0, 0, true, true),
            ColorMode::new("rainbow2", 0, 0, true, true),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
