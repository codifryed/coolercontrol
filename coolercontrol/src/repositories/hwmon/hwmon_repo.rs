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
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::device::{ChannelInfo, Device, DeviceInfo, DeviceType, SpeedOptions, Status};
use crate::repositories::hwmon::devices::DeviceFns;
use crate::repositories::hwmon::fans::FanFns;
use crate::repositories::hwmon::temps::TempFns;
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum HwmonChannelType {
    Fan,
    Temp,
    Load,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonChannelInfo {
    pub hwmon_type: HwmonChannelType,
    pub number: u8,
    pub pwm_enable_default: Option<u8>,
    pub name: String,
    pub pwm_mode_supported: bool,
}

impl Default for HwmonChannelInfo {
    fn default() -> Self {
        Self {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            pwm_enable_default: None,
            name: "".to_string(),
            pwm_mode_supported: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonDriverInfo {
    pub name: String,
    pub path: PathBuf,
    pub model: Option<String>,
    pub channels: Vec<HwmonChannelInfo>,
}

/// A Repository for Hwmon Devices
pub struct HwmonRepo {
    devices: HashMap<u8, (DeviceLock, HwmonDriverInfo)>,
}

impl HwmonRepo {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            devices: HashMap::new(),
        })
    }

    /// Maps driver infos to our Devices
    async fn map_into_our_device_model(&mut self, hwmon_drivers: Vec<HwmonDriverInfo>) {
        for (index, driver) in hwmon_drivers.into_iter().enumerate() {
            let mut channels = HashMap::new();
            for channel in driver.channels.iter() {
                if channel.hwmon_type != HwmonChannelType::Fan {
                    continue;  // only Fan channels currently have controls
                }
                let channel_info = ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                channels.insert(channel.name.clone(), channel_info);
            }
            let device_info = DeviceInfo {
                channels,
                temp_min: 0,
                temp_max: 100,
                temp_ext_available: true,
                profile_max_length: 21,
                model: driver.model.clone(),
                ..Default::default()
            };
            let device_id = (index + 1) as u8;
            let status = Status {
                channels: FanFns::extract_fan_statuses(&driver).await,
                temps: TempFns::extract_temp_statuses(&device_id, &driver).await,
                ..Default::default()
            };
            let mut device = Device {
                name: driver.name.clone(),
                d_type: DeviceType::Hwmon,
                type_id: device_id.clone(),
                info: Some(device_info),
                ..Default::default()
            };
            device.set_status(status);
            self.devices.insert(
                device_id,
                (Arc::new(RwLock::new(device)), driver),
            );
        }
    }
}

#[async_trait]
impl Repository for HwmonRepo {
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();

        let base_paths = DeviceFns::find_all_hwmon_device_paths();
        if base_paths.len() == 0 {
            return Err(anyhow!("No HWMon devices were found, try running sensors-detect"));
        }
        let mut hwmon_drivers: Vec<HwmonDriverInfo> = vec![];
        for path in base_paths {
            let device_name = DeviceFns::get_device_name(&path).await;
            if DeviceFns::is_already_used_by_other_repo(&device_name) {
                continue;
            }
            let mut channels = vec![];
            match FanFns::init_fans(&path, &device_name).await {
                Ok(fans) => channels.extend(fans),
                Err(err) => error!("Error initializing Hwmon Fans: {}", err)
            };
            match TempFns::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(temps),
                Err(err) => error!("Error initializing Hwmon Temps: {}", err)
            };
            if channels.is_empty() {  // we only add hwmon drivers that have usable data
                continue;
            }
            let model = DeviceFns::get_device_model_name(&path).await;
            let hwmon_driver_info = HwmonDriverInfo {
                name: device_name,
                path,
                model,
                channels,
            };
            hwmon_drivers.push(hwmon_driver_info);
        }
        DeviceFns::handle_duplicate_device_names(&mut hwmon_drivers).await;
        // resorted by name to help maintain some semblance of order after reboots & device changes.
        //  as for example the hwmon path number can change on reboot.
        hwmon_drivers.sort_by(|d1, d2| d1.name.cmp(&d2.name));
        self.map_into_our_device_model(hwmon_drivers).await;

        let mut init_devices = vec![];
        for (device, _) in self.devices.values() {
            init_devices.push(device.read().await.clone())
        }
        debug!("Initialized Devices: {:?}", init_devices);
        debug!(
            "Time taken to initialize all Hwmon devices: {:?}", start_initialization.elapsed()
        );
        info!("All Hwmon devices initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn update_statuses(&self) -> Result<()> {
        debug!("Updating all HWMON device statuses");
        let start_update = Instant::now();
        for (device, driver) in self.devices.values() {
            let status = Status {
                channels: FanFns::extract_fan_statuses(&driver).await,
                temps: TempFns::extract_temp_statuses(&device.read().await.type_id, &driver).await,
                ..Default::default()
            };
            debug!("Hwmon device: {} status was updated with: {:?}", device.read().await.name, status);
            device.write().await.set_status(status);
        }
        debug!(
            "Time taken to update status for all HWMON devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn apply_setting(&self, device_type_id: u8, setting: Setting) -> Result<()> {
        todo!()
    }
}
