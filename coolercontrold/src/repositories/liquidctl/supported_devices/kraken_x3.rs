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
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        let color_channels = vec![
            "external".to_string(),
            "ring".to_string(),
            "logo".to_string(),
            "sync".to_string(),
        ];
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
            profile_max_length: 9,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
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
