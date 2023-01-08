/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/

use std::collections::HashMap;

use crate::device::{ChannelInfo, DeviceInfo, LcdMode, LcdModeType, LightingMode, SpeedOptions};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liquidctl_repo::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};
use crate::repositories::liquidctl::supported_devices::kraken_z3::KrakenZ3Support;

#[derive(Debug)]
pub struct KrakenZ3MockSupport {
    kraken_z3_support: KrakenZ3Support,
}

/// This is for testing purposes only (mocking)
impl KrakenZ3MockSupport {
    pub fn new() -> Self {
        Self { kraken_z3_support: KrakenZ3Support::new() }
    }
}

impl DeviceSupport for KrakenZ3MockSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::MockKrakenZ3 // for mock testing
    }

    fn extract_info(&self, _device_index: &u8, _device_props: &DeviceProperties) -> DeviceInfo {
        self.kraken_z3_support.extract_info(_device_index, _device_props)
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        self.kraken_z3_support.get_color_channel_modes(_channel_name)
    }
}
