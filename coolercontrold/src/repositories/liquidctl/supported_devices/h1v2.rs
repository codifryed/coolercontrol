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
use std::sync::RwLock;

use crate::device::{ChannelInfo, ChannelStatus, DeviceInfo, LightingMode, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};

#[derive(Debug)]
pub struct H1V2Support {
    init_speed_channel_map: RwLock<HashMap<u8, Vec<String>>>,
}

/// The H1V2 driver is an extension of the `SmartDevice2` driver
impl H1V2Support {
    pub fn new() -> Self {
        Self {
            init_speed_channel_map: RwLock::new(HashMap::new()),
        }
    }
}

impl DeviceSupport for H1V2Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::H1V2
    }

    fn extract_info(&self, device_index: &u8, device_props: &DeviceProperties) -> DeviceInfo {
        // We need to keep track of each device's speed channel names when mapping the status
        //  as for ex. when the fan duty is set to 0, it no longer comes in the status response.
        //  This is a workaround for that so that we always have a status for each fan.
        //  caveat: this doesn't occur anymore if the hwmon driver is present
        let mut init_speed_channel_names = Vec::new();
        let mut channels = HashMap::new();
        for channel_name in &device_props.speed_channels {
            init_speed_channel_names.push(channel_name.clone());
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: false, // no internal temp
                    }),
                    ..Default::default()
                },
            );
        }
        self.init_speed_channel_map
            .write()
            .unwrap()
            .insert(*device_index, init_speed_channel_names);

        // There is a pump channel from which rpms come, but it is not yet controllable.
        // The H1V2 doesn't have any color channels
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        Vec::new()
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        device_index: &u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = vec![];
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
        // Same workaround as the SmartDevice2, since it uses the same base implementation:
        // fan speeds set to 0 will make it disappear from liquidctl status for this driver,
        // (non-0 check) unfortunately that also happens when no fan is attached.
        // caveat: not an issue if hwmon driver is present
        if let Some(speed_channel_names) = self
            .init_speed_channel_map
            .read()
            .unwrap()
            .get(device_index)
        {
            if channel_statuses.len() < speed_channel_names.len() {
                let channel_names_current_status = channel_statuses
                    .iter()
                    .map(|status| status.name.clone())
                    .collect::<Vec<String>>();
                speed_channel_names
                    .iter()
                    .filter(|channel_name| !channel_names_current_status.contains(channel_name))
                    .for_each(|channel_name| {
                        channel_statuses.push(ChannelStatus {
                            name: channel_name.clone(),
                            rpm: Some(0),
                            duty: Some(0.0),
                            ..Default::default()
                        });
                    });
            }
        }
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }
}
