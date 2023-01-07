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

use crate::device::{DeviceInfo, Status};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liquidctl_repo::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::aquacomputer::AquaComputerSupport;
use crate::repositories::liquidctl::supported_devices::auraled::AuraLedSupport;
use crate::repositories::liquidctl::supported_devices::commandercore::CommanderCoreSupport;
use crate::repositories::liquidctl::supported_devices::commanderpro::CommanderProSupport;
use crate::repositories::liquidctl::supported_devices::device_support::DeviceSupport;
use crate::repositories::liquidctl::supported_devices::kraken2::Kraken2Support;
use crate::repositories::liquidctl::supported_devices::krakenx3::KrakenX3Support;
use crate::repositories::liquidctl::supported_devices::smartdevice2::SmartDevice2Support;

type StatusMap = HashMap<String, String>;

#[derive(Debug)]
pub struct DeviceMapper {
    supported_devices: HashMap<BaseDriver, Box<dyn DeviceSupport>>,
}

impl DeviceMapper {
    pub fn new() -> Self {
        let supported_devices_list: Vec<Box<dyn DeviceSupport>> = vec![
            Box::new(AquaComputerSupport::new()),
            Box::new(AuraLedSupport::new()),
            Box::new(CommanderCoreSupport::new()),
            Box::new(CommanderProSupport::new()),
            Box::new(Kraken2Support::new()),
            Box::new(KrakenX3Support::new()),
            Box::new(SmartDevice2Support::new()),
        ];
        DeviceMapper {
            supported_devices: Self::create_supported_devices_map(supported_devices_list)
        }
    }

    fn create_supported_devices_map(
        supported_devices_list: Vec<Box<dyn DeviceSupport>>
    ) -> HashMap<BaseDriver, Box<dyn DeviceSupport>> {
        let mut supported_devices = HashMap::new();
        for supported_device in supported_devices_list {
            supported_devices.insert(
                supported_device.supported_driver(), supported_device,
            );
        }
        supported_devices
    }

    pub fn is_device_supported(&self, base_driver: &BaseDriver) -> bool {
        self.supported_devices.get(base_driver).is_some()
    }

    pub fn extract_status(&self,
                          driver_type: &BaseDriver,
                          status_map: &StatusMap,
                          device_index: &u8,
    ) -> Status {
        self.supported_devices
            .get(driver_type)
            .expect("Device Support should already have been verified")
            .extract_status(status_map, device_index)
    }

    pub fn extract_info(&self, driver_type: &BaseDriver, device_index: &u8, device_props: &DeviceProperties) -> DeviceInfo {
        self.supported_devices
            .get(driver_type)
            .expect("Device Support should already have been verified")
            .extract_info(device_index, device_props)
    }
}