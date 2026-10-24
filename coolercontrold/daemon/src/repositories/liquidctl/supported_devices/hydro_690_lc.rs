// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType,
    LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct Hydro690LcSupport;
// asetek.py

impl Hydro690LcSupport {
    pub fn new() -> Self {
        Self {}
    }
}

/// The `Hydro690Lc` driver is based on the `Modern690Lc` driver
impl DeviceSupport for Hydro690LcSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::Hydro690Lc
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(), // pump only supports fixed speed
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 50,
                    max_duty: 100,
                    fixed_enabled: true,
                    extension: None,
                }),
            },
        );
        channels.insert(
            "fan".to_string(), // fan supports speed profiles
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
        let color_channels = vec!["logo".to_string()];
        for channel_name in color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name,
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Lighting(lighting_modes),
                },
            );
        }
        // for this device this is actually time-to-color for 2 modes, like the legacy models
        let lighting_speeds = vec![
            "5".to_string(),
            "4".to_string(),
            "3".to_string(),
            "2".to_string(),
            "1".to_string(),
        ];
        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 6,
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
        // alert temp is also supported for this device
        // "rainbow" color mode is not supported on the Hydro690Lc (Only for EVGA)
        let color_modes = vec![
            ColorMode::new("blackout", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("fading", 1, 2, true, false),
            ColorMode::new("blinking", 1, 1, true, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}
