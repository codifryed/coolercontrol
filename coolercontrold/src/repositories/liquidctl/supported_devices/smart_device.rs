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
use crate::repositories::liquidctl::supported_devices::device_support::{
    ColorMode, DeviceSupport, StatusMap,
};

#[derive(Debug)]
pub struct SmartDeviceSupport {
    init_speed_channel_map: RwLock<HashMap<u8, Vec<String>>>,
}

impl SmartDeviceSupport {
    pub fn new() -> Self {
        Self {
            init_speed_channel_map: RwLock::new(HashMap::new()),
        }
    }
}

impl DeviceSupport for SmartDeviceSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::SmartDevice
    }

    fn extract_info(&self, device_index: &u8, device_props: &DeviceProperties) -> DeviceInfo {
        // We need to keep track of each device's speed channel names when mapping the status
        //  as for ex. when the fan duty is set to 0, it no longer comes in the status response.
        //  This is a workaround for that so that we always have a status for each fan.
        //  caveat: this doesn't occur anymore if the hwmon driver is present
        let mut init_speed_channel_names = vec![];
        let mut channels = HashMap::new();
        for name in &device_props.speed_channels {
            init_speed_channel_names.push(name.clone());
            channels.insert(
                name.clone(),
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

        for name in &device_props.color_channels {
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
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        // slightly different than the SmartDevice2
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("super-fixed", 1, 40, false, false),
            ColorMode::new("fading", 1, 8, true, false),
            ColorMode::new("spectrum-wave", 0, 0, true, true),
            ColorMode::new("marquee-3", 1, 1, true, true),
            ColorMode::new("marquee-4", 1, 1, true, true),
            ColorMode::new("marquee-5", 1, 1, true, true),
            ColorMode::new("marquee-6", 1, 1, true, true),
            ColorMode::new("covering-marquee", 1, 8, true, true),
            ColorMode::new("alternating", 2, 2, true, false),
            ColorMode::new("moving-alternating", 2, 2, true, true),
            ColorMode::new("pulse", 1, 8, true, false),
            ColorMode::new("breathing", 1, 8, true, false),
            ColorMode::new("super-breathing", 1, 40, true, false),
            ColorMode::new("candle", 1, 1, false, false),
            ColorMode::new("wings", 1, 1, true, false),
            ColorMode::new("super-wave", 1, 40, true, true),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        device_index: &u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = vec![];
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
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
