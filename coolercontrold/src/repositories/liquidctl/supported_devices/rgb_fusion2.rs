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

use crate::device::{ChannelInfo, DeviceInfo, LightingMode};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct RgbFusion2Support;

impl RgbFusion2Support {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for RgbFusion2Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::RgbFusion2
    }

    fn extract_info(&self, _device_index: &u8, _device_props: &DeviceProperties) -> DeviceInfo {
        let mut channels = HashMap::new();
        let color_channels = vec![
            "led1".to_string(),
            "led2".to_string(),
            "led3".to_string(),
            "led4".to_string(),
            "led5".to_string(),
            "led6".to_string(),
            "led7".to_string(),
            "led8".to_string(),
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
            "ludicrous".to_string(),
        ];
        DeviceInfo {
            channels,
            lighting_speeds,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("pulse", 1, 1, true, false),
            ColorMode::new("flash", 1, 1, true, false),
            ColorMode::new("double-flash", 1, 1, true, false),
            ColorMode::new("color-cycle", 0, 0, true, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
