// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{DeviceInfo, DriverInfo, DriverType, LightingMode};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

#[derive(Debug)]
pub struct NzxtEPsuSupport;
// nzxt_epsu.py

impl NzxtEPsuSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for NzxtEPsuSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::NzxtEPsu
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        // fan control currently no supported
        let channels = HashMap::new();
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 20, // device has temp
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
