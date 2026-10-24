// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
    StatusRefill,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{
    ColorMode, DeviceSupport, StatusMap,
};

#[derive(Debug)]
pub struct SmartDeviceSupport {
    init_speed_channel_map: RefCell<HashMap<u8, Vec<String>>>,
}
// smart_device.py

impl SmartDeviceSupport {
    pub fn new() -> Self {
        Self {
            init_speed_channel_map: RefCell::new(HashMap::new()),
        }
    }
}

impl DeviceSupport for SmartDeviceSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::SmartDevice
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        // We need to keep track of each device's speed channel names when mapping the status
        //  as for ex. when the fan duty is set to 0, it no longer comes in the status response.
        //  This is a workaround for that so that we always have a status for each fan.
        //  caveat: this doesn't occur anymore if the hwmon driver is present
        let mut init_speed_channel_names = vec![];
        let mut channels = HashMap::new();
        for name in &device_response.properties.speed_channels {
            init_speed_channel_names.push(name.clone());
            channels.insert(
                name.clone(),
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

        for name in &device_response.properties.color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Lighting(lighting_modes),
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

    fn refill_channel_statuses(
        &self,
        status_map: &StatusMap,
        device_index: u8,
        refill: &mut StatusRefill<'_>,
    ) {
        self.add_multiple_fans_status(status_map, refill);
        // fan speeds set to 0 will make it disappear from liquidctl status for this driver,
        // (non-0 check) unfortunately that also happens when no fan is attached.
        // caveat: not an issue if hwmon driver is present
        if let Some(speed_channel_names) = self.init_speed_channel_map.borrow().get(&device_index) {
            self.add_missing_speed_channels(speed_channel_names, refill);
        }
        refill
            .channels_mut()
            .sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
    }
}
