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
use crate::device::{
    ChannelInfo, ChannelStatus, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
    TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};
use std::collections::HashMap;

#[derive(Debug)]
pub struct AsusRyujinSupport;
// asus_ryujin.py

#[deprecated(
    since = "2.2.1",
    note = "HWMon driver is preferred. Will likely be removed in the future."
)]
impl AsusRyujinSupport {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AsusRyujinSupport {
    fn supported_driver(&self) -> BaseDriver {
        // BaseDriver::AsusRyujin
        // This liquidctl driver currently doesn't support reading from the hwmon driver,
        // and so has problems when used in conjunction with that driver. The hwmon driver also
        // offers feature-parity, and the workarounds needed in CC for control are non-intuitive.
        // For these reasons, the hwmon driver is preferred.
        // see: https://gitlab.com/coolercontrol/coolercontrol/-/issues/457
        BaseDriver::NotSupported
    }

    #[allow(clippy::too_many_lines)]
    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
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
        channels.insert(
            "pump-fan".to_string(),
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
        channels.insert(
            "external-fans".to_string(),
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
        channels.insert(
            "external-fan1".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: false,
                    manual_profiles_enabled: false,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "external-fan2".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: false,
                    manual_profiles_enabled: false,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "external-fan3".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: false,
                    manual_profiles_enabled: false,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "external-fan4".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: false,
                    manual_profiles_enabled: false,
                }),
                ..Default::default()
            },
        );
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            // liquid temp:
            temp_min: 20,
            temp_max: 60,
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
        let mut temps = vec![];
        self.add_liquid_temp(status_map, &mut temps);
        temps
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = vec![];
        self.add_single_pump_status(status_map, &mut channel_statuses);
        self.add_single_pump_fan_status(status_map, &mut channel_statuses);
        self.add_single_external_fans_status(status_map, &mut channel_statuses);
        self.add_multiple_external_fans_status(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }
}
