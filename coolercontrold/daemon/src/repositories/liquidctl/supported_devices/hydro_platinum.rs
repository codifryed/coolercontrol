// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::Cell;
use std::collections::HashMap;

use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType,
    LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct HydroPlatinumSupport {
    led_count: Cell<u8>,
}
// hydro_platinum.py

impl HydroPlatinumSupport {
    pub fn new() -> Self {
        Self {
            led_count: Cell::new(1),
        }
    }
}

impl DeviceSupport for HydroPlatinumSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::HydroPlatinum
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        if let Some(led_count) = device_response.properties.led_count {
            self.led_count.set(led_count);
        }
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
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
        for channel_name in &device_response.properties.speed_channels {
            // fan channels
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        fixed_enabled: true,
                        extension: Some(ChannelExtensionNames::AutoHWCurve),
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
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 7,
            profile_min_length: 2,
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
        let color_modes: Vec<ColorMode> = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("super-fixed", 1, self.led_count.get(), false, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
