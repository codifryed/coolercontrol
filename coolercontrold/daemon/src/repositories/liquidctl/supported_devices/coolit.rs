// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType,
    LightingMode, SpeedOptions, StatusRefill,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};
use std::collections::HashMap;

#[derive(Debug)]
pub struct CoolitSupport;
// coolit.py

impl CoolitSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for CoolitSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::Coolit
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    fixed_enabled: false,
                    extension: None,
                }),
            },
        );
        channels.insert(
            "fan1".to_string(),
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
        channels.insert(
            "fan2".to_string(),
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
        DeviceInfo {
            channels,
            // liquid temp:
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 7,
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

    fn refill_temperatures(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        self.add_liquid_temp(status_map, refill);
    }

    fn refill_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
        refill: &mut StatusRefill<'_>,
    ) {
        self.add_single_pump_status(status_map, refill);
        self.add_multiple_fans_status(status_map, refill);
        refill
            .channels_mut()
            .sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
    }
}
