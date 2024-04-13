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

use crate::device::{
    ChannelInfo, DeviceInfo, LcdInfo, LcdMode, LcdModeType, LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct KrakenZ3Support;

impl KrakenZ3Support {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for KrakenZ3Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::KrakenZ3
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
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "fan".to_string(),
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
        let lighting_modes = self.get_color_channel_modes(None);
        channels.insert(
            "external".to_string(),
            ChannelInfo {
                lighting_modes,
                ..Default::default()
            },
        );
        let lighting_speeds = vec![
            "slowest".to_string(),
            "slower".to_string(),
            "normal".to_string(),
            "faster".to_string(),
            "fastest".to_string(),
        ];

        channels.insert(
            "lcd".to_string(),
            ChannelInfo {
                lcd_modes: vec![
                    LcdMode {
                        name: "liquid".to_string(),
                        frontend_name: "Liquid(default)".to_string(),
                        brightness: true,
                        orientation: true,
                        image: false,
                        colors_min: 0,
                        colors_max: 0,
                        type_: LcdModeType::Liquidctl,
                    },
                    LcdMode {
                        name: "image".to_string(),
                        frontend_name: "Image/gif".to_string(),
                        brightness: true,
                        orientation: true,
                        image: true,
                        colors_min: 0,
                        colors_max: 0,
                        type_: LcdModeType::Liquidctl,
                    },
                    LcdMode {
                        name: "temp".to_string(),
                        frontend_name: "Single Temp".to_string(),
                        brightness: true,
                        orientation: true,
                        image: false,
                        colors_min: 0, // for custom types
                        colors_max: 0,
                        type_: LcdModeType::Custom,
                    },
                ],
                lcd_info: Some(LcdInfo {
                    screen_width: 320,
                    screen_height: 320,
                    max_image_size_bytes: 24_320 * 1024, // 24,320 KB/KiB
                }),
                ..Default::default()
            },
        );

        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 9,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        // same as the KrakenX3
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("fading", 1, 8, true, false),
            ColorMode::new("super-fixed", 1, 40, false, false),
            ColorMode::new("spectrum-wave", 0, 0, true, true),
            ColorMode::new("marquee-3", 1, 1, true, true),
            ColorMode::new("marquee-4", 1, 1, true, true),
            ColorMode::new("marquee-5", 1, 1, true, true),
            ColorMode::new("marquee-6", 1, 1, true, true),
            ColorMode::new("covering-marquee", 1, 8, true, true),
            ColorMode::new("alternating-3", 1, 2, true, false),
            ColorMode::new("alternating-4", 1, 2, true, false),
            ColorMode::new("alternating-5", 1, 2, true, false),
            ColorMode::new("alternating-6", 1, 2, true, false),
            ColorMode::new("moving-alternating-3", 1, 2, true, true),
            ColorMode::new("moving-alternating-4", 1, 2, true, true),
            ColorMode::new("moving-alternating-5", 1, 2, true, true),
            ColorMode::new("moving-alternating-6", 1, 2, true, true),
            ColorMode::new("pulse", 1, 8, true, false),
            ColorMode::new("breathing", 1, 8, true, false),
            ColorMode::new("super-breathing", 1, 40, true, false),
            ColorMode::new("candle", 1, 1, false, false),
            ColorMode::new("starry-night", 1, 1, true, false),
            ColorMode::new("rainbow-flow", 0, 0, true, true),
            ColorMode::new("super-rainbow", 0, 0, true, true),
            ColorMode::new("rainbow-pulse", 0, 0, true, true),
            ColorMode::new("loading", 1, 1, true, false),
            ColorMode::new("tai-chi", 1, 2, true, false),
            ColorMode::new("water-cooler", 2, 2, true, false),
            ColorMode::new("wings", 1, 1, true, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
