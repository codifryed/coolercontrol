// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

#[derive(Debug)]
pub struct CommanderCoreSupport;
// commander_core.py

impl CommanderCoreSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for CommanderCoreSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::CommanderCore
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        for channel_name in &device_response.properties.speed_channels {
            // currently only "pump"
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: 20,
                        max_duty: 100,
                        fixed_enabled: true,
                        extension: None,
                    }),
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
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20,
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
