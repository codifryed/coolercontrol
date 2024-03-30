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
pub struct AuraLedSupport;

impl AuraLedSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AuraLedSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::AuraLed
    }

    fn extract_info(&self, _device_index: &u8, _device_props: &DeviceProperties) -> DeviceInfo {
        let mut channels = HashMap::new();
        let color_channels = vec![
            "led1".to_string(),
            "led2".to_string(),
            "led3".to_string(),
            "led4".to_string(),
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
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("static", 1, 1, false, false),
            ColorMode::new("breathing", 1, 1, false, false),
            ColorMode::new("flashing", 1, 1, false, false),
            ColorMode::new("spectrum_cycle", 0, 0, false, false),
            ColorMode::new("rainbow", 0, 0, false, false),
            ColorMode::new("spectrum_cycle_breathing", 0, 0, false, false),
            ColorMode::new("chase_fade", 1, 1, false, false),
            ColorMode::new("spectrum_cycle_chase_fade", 0, 0, false, false),
            ColorMode::new("chase", 1, 1, false, false),
            ColorMode::new("spectrum_cycle_chase", 0, 0, false, false),
            ColorMode::new("spectrum_cycle_wave", 0, 0, false, false),
            ColorMode::new("chase_rainbow_pulse", 0, 0, false, false),
            ColorMode::new("rainbow_flicker", 0, 0, false, false),
            ColorMode::new("gentle_transition", 0, 0, false, false),
            ColorMode::new("wave_propagation", 0, 0, false, false),
            ColorMode::new("wave_propagation_pause", 0, 0, false, false),
            ColorMode::new("red_pulse", 0, 0, false, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
