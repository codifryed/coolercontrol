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
    TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};

#[derive(Debug)]
pub struct AquaComputerSupport;
// aquacomputer.py

impl AquaComputerSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AquaComputerSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::Aquacomputer
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::with_capacity(device_response.properties.speed_channels.len());
        for channel_name in &device_response.properties.speed_channels {
            channels.insert(
                channel_name.to_owned(),
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
            temp_min: 0,
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

    fn get_temperatures(&self, status_map: &StatusMap) -> Vec<TempStatus> {
        let mut temps = Vec::with_capacity(status_map.len());
        self.add_temp_sensors(status_map, &mut temps);
        self.add_software_temp_sensors(status_map, &mut temps);
        temps.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        temps
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = Vec::with_capacity(status_map.len());
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
        self.add_flow_sensor_status(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }
}
