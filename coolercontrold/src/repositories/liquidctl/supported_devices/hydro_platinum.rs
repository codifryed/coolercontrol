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
use std::sync::RwLock;

use crate::device::{ChannelInfo, DeviceInfo, LightingMode, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct HydroPlatinumSupport {
    led_count: RwLock<u8>,
}

impl HydroPlatinumSupport {
    pub fn new() -> Self {
        Self {
            led_count: RwLock::new(1),
        }
    }
}

impl DeviceSupport for HydroPlatinumSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::HydroPlatinum
    }

    fn extract_info(&self, _device_index: &u8, device_props: &DeviceProperties) -> DeviceInfo {
        if let Some(led_count) = device_props.led_count {
            *self.led_count.write().unwrap() = led_count;
        }
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 20,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        for channel_name in &device_props.speed_channels {
            // fan channels
            channels.insert(
                channel_name.to_owned(),
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
        }
        for channel_name in &device_props.color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    lighting_modes,
                    ..Default::default()
                },
            );
        }
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 7,
            profile_min_length: 2,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes: Vec<ColorMode> = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new(
                "super-fixed",
                1,
                *self.led_count.read().unwrap(),
                false,
                false,
            ),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
