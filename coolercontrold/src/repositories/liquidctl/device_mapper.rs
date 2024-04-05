/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::HashMap;

use crate::device::{DeviceInfo, Status};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
use crate::repositories::liquidctl::supported_devices::aquacomputer::AquaComputerSupport;
use crate::repositories::liquidctl::supported_devices::aura_led::AuraLedSupport;
use crate::repositories::liquidctl::supported_devices::commander_core::CommanderCoreSupport;
use crate::repositories::liquidctl::supported_devices::commander_pro::CommanderProSupport;
use crate::repositories::liquidctl::supported_devices::corsair_hid_psu::CorsairHidPsuSupport;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};
use crate::repositories::liquidctl::supported_devices::h1v2::H1V2Support;
use crate::repositories::liquidctl::supported_devices::hydro_690_lc::Hydro690LcSupport;
use crate::repositories::liquidctl::supported_devices::hydro_platinum::HydroPlatinumSupport;
use crate::repositories::liquidctl::supported_devices::hydro_pro::HydroProSupport;
use crate::repositories::liquidctl::supported_devices::kraken2::Kraken2Support;
use crate::repositories::liquidctl::supported_devices::kraken_x3::KrakenX3Support;
use crate::repositories::liquidctl::supported_devices::kraken_z3::KrakenZ3Support;
use crate::repositories::liquidctl::supported_devices::kraken_z3_mock::KrakenZ3MockSupport;
use crate::repositories::liquidctl::supported_devices::legacy_690_lc::Legacy690LcSupport;
use crate::repositories::liquidctl::supported_devices::modern_690_lc::Modern690LcSupport;
use crate::repositories::liquidctl::supported_devices::nzxt_epsu::NzxtEPsuSupport;
use crate::repositories::liquidctl::supported_devices::rgb_fusion2::RgbFusion2Support;
use crate::repositories::liquidctl::supported_devices::smart_device::SmartDeviceSupport;
use crate::repositories::liquidctl::supported_devices::smart_device2::SmartDevice2Support;

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
            Box::new(CorsairHidPsuSupport::new()),
            Box::new(H1V2Support::new()),
            Box::new(Hydro690LcSupport::new()),
            Box::new(HydroPlatinumSupport::new()),
            Box::new(HydroProSupport::new()),
            Box::new(Kraken2Support::new()),
            Box::new(KrakenX3Support::new()),
            Box::new(KrakenZ3Support::new()),
            Box::new(KrakenZ3MockSupport::new()),
            Box::new(Legacy690LcSupport::new()),
            Box::new(Modern690LcSupport::new()),
            Box::new(NzxtEPsuSupport::new()),
            Box::new(RgbFusion2Support::new()),
            Box::new(SmartDeviceSupport::new()),
            Box::new(SmartDevice2Support::new()),
        ];
        DeviceMapper {
            supported_devices: Self::create_supported_devices_map(supported_devices_list),
        }
    }

    fn create_supported_devices_map(
        supported_devices_list: Vec<Box<dyn DeviceSupport>>,
    ) -> HashMap<BaseDriver, Box<dyn DeviceSupport>> {
        let mut supported_devices = HashMap::new();
        for supported_device in supported_devices_list {
            supported_devices.insert(supported_device.supported_driver(), supported_device);
        }
        supported_devices
    }

    pub fn is_device_supported(&self, base_driver: &BaseDriver) -> bool {
        self.supported_devices.get(base_driver).is_some()
    }

    pub fn extract_status(
        &self,
        driver_type: &BaseDriver,
        status_map: &StatusMap,
        device_index: &u8,
    ) -> Status {
        self.supported_devices
            .get(driver_type)
            .expect("Device Support should already have been verified")
            .extract_status(status_map, device_index)
    }

    pub fn extract_info(
        &self,
        driver_type: &BaseDriver,
        device_index: &u8,
        device_props: &DeviceProperties,
    ) -> DeviceInfo {
        self.supported_devices
            .get(driver_type)
            .expect("Device Support should already have been verified")
            .extract_info(device_index, device_props)
    }
}
