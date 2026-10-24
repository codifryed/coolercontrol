// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct CommanderProSupport;
// commander_pro.py

impl CommanderProSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for CommanderProSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::CommanderPro
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        for channel_name in &device_response.properties.speed_channels {
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        fixed_enabled: true,
                        // Internal profiles for the commander pro only work with RPMs! not duty %
                        extension: None,
                    }),
                },
            );
        }
        for channel_name in &device_response.properties.color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Lighting(lighting_modes),
                },
            );
        }
        let lighting_speeds = vec!["slow".to_string(), "medium".to_string(), "fast".to_string()];
        DeviceInfo {
            channels,
            lighting_speeds,
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
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("color_shift", 0, 2, true, true),
            ColorMode::new("color_pulse", 0, 2, true, true),
            ColorMode::new("color_wave", 0, 2, true, true),
            ColorMode::new("visor", 0, 2, true, true),
            ColorMode::new("blink", 0, 2, true, true),
            ColorMode::new("marquee", 0, 1, true, true),
            ColorMode::new("sequential", 0, 1, true, true),
            ColorMode::new("rainbow", 0, 0, true, true),
            ColorMode::new("rainbow2", 0, 0, true, true),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
