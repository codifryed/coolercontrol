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
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

#[derive(Debug)]
pub struct CommanderCoreSupport;

impl CommanderCoreSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for CommanderCoreSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::CommanderCore
    }

    fn extract_info(&self, _device_index: &u8, device_props: &DeviceProperties) -> DeviceInfo {
        let mut channels = HashMap::new();
        for channel_name in &device_props.speed_channels {
            // currently only "pump"
            channels.insert(
                channel_name.to_owned(),
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
        }
        let fan_channel_names = vec![
            "fan1".to_string(),
            "fan2".to_string(),
            "fan3".to_string(),
            "fan4".to_string(),
            "fan5".to_string(),
            "fan6".to_string(),
        ];
        for channel_name in fan_channel_names {
            channels.insert(
                channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: true,
                    }),
                    ..Default::default()
                },
            );
        }
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20,
            temp_max: 100,
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        Vec::new()
    }
}
