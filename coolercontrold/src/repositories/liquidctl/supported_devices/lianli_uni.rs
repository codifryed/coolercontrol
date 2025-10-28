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
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

#[derive(Debug)]
pub struct LianLiUniSupport;
// lianli_uni.py

impl LianLiUniSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for LianLiUniSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::LianLiUni
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        for i in 1..=4 { // TODO: Is it always 4?
            channels.insert(
                format!("Channel {}", i),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        fixed_enabled: true,
                        extension: None,
                    }),
                    ..Default::default()
                },
            );
        }
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
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
        Vec::new()
    }
}
