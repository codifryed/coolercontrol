// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use std::cell::RefCell;
use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
    StatusRefill,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};

#[derive(Debug)]
pub struct H1V2Support {
    init_speed_channel_map: RefCell<HashMap<u8, Vec<String>>>,
}
// smart_device.py

/// The H1V2 driver is an extension of the `SmartDevice2` driver
impl H1V2Support {
    pub fn new() -> Self {
        Self {
            init_speed_channel_map: RefCell::new(HashMap::new()),
        }
    }
}

impl DeviceSupport for H1V2Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::H1V2
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        // We need to keep track of each device's speed channel names when mapping the status
        //  as for ex. when the fan duty is set to 0, it no longer comes in the status response.
        //  This is a workaround for that so that we always have a status for each fan.
        //  caveat: this doesn't occur anymore if the hwmon driver is present
        let mut init_speed_channel_names = Vec::new();
        let mut channels = HashMap::new();
        for channel_name in &device_response.properties.speed_channels {
            init_speed_channel_names.push(channel_name.clone());
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        fixed_enabled: true,
                        extension: None,
                    }),
                },
            );
        }
        self.init_speed_channel_map
            .borrow_mut()
            .insert(device_response.id, init_speed_channel_names);

        // There is a pump channel from which rpms come, but it is not yet controllable.
        // The H1V2 doesn't have any color channels
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

    fn refill_channel_statuses(
        &self,
        status_map: &StatusMap,
        device_index: u8,
        channel_statuses: &mut StatusRefill<'_>,
    ) {
        self.add_multiple_fans_status(status_map, channel_statuses);
        // Same workaround as the SmartDevice2, since it uses the same base implementation:
        // fan speeds set to 0 will make it disappear from liquidctl status for this driver,
        // (non-0 check) unfortunately that also happens when no fan is attached.
        // caveat: not an issue if hwmon driver is present
        if let Some(speed_channel_names) = self.init_speed_channel_map.borrow().get(&device_index) {
            self.add_missing_speed_channels(speed_channel_names, channel_statuses);
        }
        channel_statuses
            .channels_mut()
            .sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
    }
}
