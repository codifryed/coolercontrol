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

use std::collections::HashMap;

use crate::device::{ChannelInfo, DeviceInfo, LightingMode, LightingModeType, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liquidctl_repo::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

/// Support for the Liquidctl KrakenX3 Driver
#[derive(Debug)]
pub struct KrakenX3Support;

impl KrakenX3Support {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for KrakenX3Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::KrakenX3
    }

    fn extract_info(&self, _device_index: &u8, _device_props: &DeviceProperties) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 20,
                    max_duty: 100,
                    profiles_enabled: true,
                    fixed_enabled: true,
                    manual_profiles_enabled: false,
                }),
                ..Default::default()
            },
        );
        let color_channels_krakenx = vec![
            "external".to_string(),
            "ring".to_string(),
            "logo".to_string(),
            "sync".to_string(),
        ];
        for channel_name in color_channels_krakenx {
            let lighting_modes = self.get_color_channel_modes(Some(&channel_name));
            channels.insert(
                channel_name,
                ChannelInfo {
                    lighting_modes,
                    ..Default::default()
                },
            );
        }

        let lighting_speeds = vec![
            "slowest".to_string(),
            "slower".to_string(),
            "normal".to_string(),
            "faster".to_string(),
            "fastest".to_string(),
        ];
        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 20,
            temp_max: 60,
            temp_ext_available: true,
            profile_max_length: 9,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, channel_name: Option<&String>) -> Vec<LightingMode> {
        let color_modes: Vec<(String, u8, u8, bool, bool)> = vec![
            //name, min_colors, max_colors, speed_enabled, backward_enabled
            ("off".to_string(), 0, 0, false, false),
            ("fixed".to_string(), 1, 1, false, false),
            ("fading".to_string(), 1, 8, true, false),
            ("super-fixed".to_string(), 1, 40, false, false),
            ("spectrum-wave".to_string(), 0, 0, true, true),
            ("marquee-3".to_string(), 1, 1, true, true),
            ("marquee-4".to_string(), 1, 1, true, true),
            ("marquee-5".to_string(), 1, 1, true, true),
            ("marquee-6".to_string(), 1, 1, true, true),
            ("covering-marquee".to_string(), 1, 8, true, true),
            ("alternating-3".to_string(), 1, 2, true, false),
            ("alternating-4".to_string(), 1, 2, true, false),
            ("alternating-5".to_string(), 1, 2, true, false),
            ("alternating-6".to_string(), 1, 2, true, false),
            ("moving-alternating-3".to_string(), 1, 2, true, true),
            ("moving-alternating-4".to_string(), 1, 2, true, true),
            ("moving-alternating-5".to_string(), 1, 2, true, true),
            ("moving-alternating-6".to_string(), 1, 2, true, true),
            ("pulse".to_string(), 1, 8, true, false),
            ("breathing".to_string(), 1, 8, true, false),
            ("super-breathing".to_string(), 1, 40, true, false),
            ("candle".to_string(), 1, 1, false, false),
            ("starry-night".to_string(), 1, 1, true, false),
            ("rainbow-flow".to_string(), 0, 0, true, true),
            ("super-rainbow".to_string(), 0, 0, true, true),
            ("rainbow-pulse".to_string(), 0, 0, true, true),
            ("loading".to_string(), 1, 1, true, false),
            ("tai-chi".to_string(), 1, 2, true, false),
            ("water-cooler".to_string(), 2, 2, true, false),
            ("wings".to_string(), 1, 1, true, false),
        ];
        let mut channel_modes = vec![];
        for (name, min_colors, max_colors, speed_enabled, backward_enabled) in color_modes {
            channel_modes.push(
                LightingMode {
                    frontend_name: self.channel_to_frontend_name(&name),
                    name,
                    min_colors,
                    max_colors,
                    speed_enabled,
                    backward_enabled,
                    type_: LightingModeType::Liquidctl,
                }
            );
        }
        channel_modes
    }
}
