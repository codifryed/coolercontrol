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
pub struct CorsairHidPsuSupport;
// corsair_hid_psu.py

impl CorsairHidPsuSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for CorsairHidPsuSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::CorsairHidPsu
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "fan".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    // We override the driver's min duty of 30%.
                    // 15% is half as fast rpm-wise, noticeably less noisy, and still offers
                    // decent cooling in lower power draw situations.
                    // Tested on real hardware and collaborates with other reports.
                    // The hardware has a different response when attempting to apply <13% and is
                    // pretty much limited to that as the slowest speed achievable for the fan
                    // itself. Manually setting 0 rpm is not possible, except in auto/hardware mode,
                    // and it takes a long time to kick down. Note that in auto mode 0 rpm can
                    // happen a lot, so it's often best to just use auto mode.
                    min_duty: 15,
                    max_duty: 100,
                    fixed_enabled: true,
                    extension: None,
                }),
                ..Default::default()
            },
        );
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20, // device has vrm and case temps
            temp_max: 100,
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
