// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::device::{DeviceInfo, LightingMode};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;
use crate::repositories::liquidctl::supported_devices::kraken_z3::KrakenZ3Support;

#[derive(Debug)]
pub struct KrakenZ3MockSupport {
    kraken_z3_support: KrakenZ3Support,
}
// kraken3.py

/// This is for testing purposes only (mocking)
impl KrakenZ3MockSupport {
    pub fn new() -> Self {
        Self {
            kraken_z3_support: KrakenZ3Support::new(),
        }
    }
}

impl DeviceSupport for KrakenZ3MockSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::MockKrakenZ3 // for mock testing
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        self.kraken_z3_support.extract_info(device_response)
    }

    fn get_color_channel_modes(&self, channel_name: Option<&str>) -> Vec<LightingMode> {
        self.kraken_z3_support.get_color_channel_modes(channel_name)
    }
}
