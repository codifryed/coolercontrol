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

use log::error;
use crate::device::{DeviceInfo, Status};

use crate::liquidctl::base_driver::BaseDriver;
use crate::liquidctl::supported_devices::device_support::{DeviceSupport, KrakenX3Support};

type StatusMap = HashMap<String, String>;

#[derive(Debug)]
pub struct DeviceMapper {
    supported_devices: HashMap<BaseDriver, Box<dyn DeviceSupport>>,
}

impl DeviceMapper {
    pub fn new() -> Self {
        // todo: create all supported device structs

        let mut supported_devices: HashMap<BaseDriver, Box<dyn DeviceSupport>> = HashMap::new();
        supported_devices.insert(BaseDriver::KrakenX3, Box::new(KrakenX3Support::new()));
        DeviceMapper {
            supported_devices
        }
    }

    pub fn is_device_supported(&self, base_driver: &BaseDriver) -> bool {
        match self.supported_devices.get(base_driver) {
            Some(_) => true,
            None => {
                error!("Device has does not have an implementation: {:?}", base_driver);
                false
            }
        }
    }

    pub fn extract_status(&self,
                          device_type: &BaseDriver,
                          status_map: &StatusMap,
                          device_id: &u8,
    ) -> Status {
        self.supported_devices
            .get(device_type)
            .expect("Device Support should already have been verified")
            .extract_status(status_map, device_id)
    }

    pub fn extract_info(&self, device_type: &BaseDriver) -> Option<DeviceInfo> {
        todo!()
    }
}