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

use crate::device::{DeviceInfo, LightingMode};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;

/// Support for the Liquidctl KrakenX3 Driver
#[derive(Debug)]
pub struct KrakenX3Support;

impl KrakenX3Support {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for KrakenX3Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::KrakenX3
    }

    fn extract_info(&self) -> DeviceInfo {
        todo!()
    }

    fn get_filtered_color_channel_modes(&self) -> Vec<LightingMode> {
        todo!()
    }
}
