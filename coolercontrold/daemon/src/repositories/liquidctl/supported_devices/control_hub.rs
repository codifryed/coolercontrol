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

use crate::device::{
    ChannelInfo, ChannelStatus, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{
    ColorMode, DeviceSupport, StatusMap,
};

const MIN_DUTY: u8 = 0;
const MAX_DUTY: u8 = 100;

#[derive(Debug)]
pub struct ControlHubSupport {}
// control_hub.py

impl ControlHubSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for ControlHubSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::ControlHub
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        for name in &device_response.properties.speed_channels {
            channels.insert(
                name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: MIN_DUTY,
                        max_duty: MAX_DUTY,
                        fixed_enabled: true,
                        extension: None,
                    }),
                    ..Default::default()
                },
            );
        }

        for name in &device_response.properties.color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                name.to_owned(),
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
            ColorMode::new("fading", 1, 8, true, false),
            ColorMode::new("spectrum-wave", 0, 0, true, true),
            ColorMode::new("covering-marquee", 1, 8, true, true),
            ColorMode::new("super-rainbow", 0, 0, true, true),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = vec![];
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }
}
