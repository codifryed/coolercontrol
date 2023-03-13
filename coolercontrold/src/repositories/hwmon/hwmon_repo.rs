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

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::RwLock;
use tokio::time::Instant;
use zbus::export::futures_util::future::join_all;
use crate::config::Config;

use crate::device::{ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, SpeedOptions, Status, TempStatus, UID};
use crate::repositories::hwmon::{devices, fans, temps};
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
    pub u_id: UID,
    pub channels: Vec<HwmonChannelInfo>,
}

/// A Repository for Hwmon Devices
pub struct HwmonRepo {
    config: Arc<Config>,
    devices: HashMap<UID, (DeviceLock, Arc<HwmonDriverInfo>)>,
    preloaded_statuses: RwLock<HashMap<u8, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
}

impl HwmonRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        Ok(Self {
            config,
            devices: HashMap::new(),
            preloaded_statuses: RwLock::new(HashMap::new()),
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
            let type_index = (index + 1) as u8;
            let channel_statuses = fans::extract_fan_statuses(&driver).await;
            let temp_statuses = temps::extract_temp_statuses(&type_index, &driver).await;
            self.preloaded_statuses.write().await.insert(
                type_index,
                (channel_statuses.clone(), temp_statuses.clone()),
            );
            let status = Status {
                channels: channel_statuses,
                temps: temp_statuses,
                ..Default::default()
            };
            let device = Device::new(
                driver.name.clone(),
                DeviceType::Hwmon,
                type_index,
                None,
                Some(device_info),
                Some(status),
                Some(driver.u_id.clone()),
            );
            self.devices.insert(
                device.uid.clone(),
                (Arc::new(RwLock::new(device)), Arc::new(driver)),
            );
        }
    }
}

#[async_trait]
impl Repository for HwmonRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Hwmon
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();

        let base_paths = devices::find_all_hwmon_device_paths();
        if base_paths.len() == 0 {
            return Err(anyhow!("No HWMon devices were found, try running sensors-detect"));
        }
        let mut hwmon_drivers: Vec<HwmonDriverInfo> = vec![];
        for path in base_paths {
            let device_name = devices::get_device_name(&path).await;
            if devices::is_already_used_by_other_repo(&device_name) {
                continue;
            }
            let mut channels = vec![];
            match fans::init_fans(&path, &device_name).await {
                Ok(fans) => channels.extend(fans),
                Err(err) => error!("Error initializing Hwmon Fans: {}", err)
            };
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(temps),
                Err(err) => error!("Error initializing Hwmon Temps: {}", err)
            };
            if channels.is_empty() {  // we only add hwmon drivers that have usable data
                continue;
            }
            let model = devices::get_device_model_name(&path).await;
            let u_id = devices::get_device_unique_id(&path).await;
            let hwmon_driver_info = HwmonDriverInfo {
                name: device_name,
                path,
                model,
                u_id,
                channels,
            };
            hwmon_drivers.push(hwmon_driver_info);
        }
        devices::handle_duplicate_device_names(&mut hwmon_drivers).await;
        // re-sorted by name to help keep some semblance of order after reboots & device changes.
        hwmon_drivers.sort_by(|d1, d2| d1.name.cmp(&d2.name));
        self.map_into_our_device_model(hwmon_drivers).await;

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in self.devices.iter() {
            init_devices.insert(
                uid.clone(),
                (device.read().await.clone(), hwmon_info.clone()),
            );
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Devices: {:#?}", init_devices);  // pretty output for easy reading
        } else {
            info!("Initialized Devices: {:?}", init_devices);
        }
        debug!(
            "Time taken to initialize all Hwmon devices: {:?}", start_initialization.elapsed()
        );
        info!("HWMON Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(&self) {
        let start_update = Instant::now();
        let mut futures = Vec::new();
        for (device_lock, driver) in self.devices.values() {
            futures.push(
                async {
                    let device_id = device_lock.read().await.type_index;
                    let hwmon_driver = Arc::clone(driver);
                    self.preloaded_statuses.write().await.insert(
                        device_id,
                        (
                            fans::extract_fan_statuses(&hwmon_driver).await,
                            temps::extract_temp_statuses(&device_id, &hwmon_driver).await
                        ),
                    );
                }
            )
        }
        join_all(futures).await;
        debug!(
            "STATUS PRELOAD Time taken for all HWMON devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (device, _) in self.devices.values() {
            let preloaded_statuses_map = self.preloaded_statuses.read().await;
            let preloaded_statuses = preloaded_statuses_map.get(&device.read().await.type_index);
            if let None = preloaded_statuses {
                error!("There is no status preloaded for this device: {}", device.read().await.type_index);
                continue;
            }
            let (channels, temps) = preloaded_statuses.unwrap().clone();
            let status = Status {
                channels,
                temps,
                ..Default::default()
            };
            debug!("Hwmon device: {} status was updated with: {:?}", device.read().await.name, status);
            device.write().await.set_status(status);
        }
        debug!(
            "STATUS SNAPSHOT Time taken for all HWMON devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (_, hwmon_driver) in self.devices.values() {
            for channel_info in hwmon_driver.channels.iter() {
                if channel_info.hwmon_type != HwmonChannelType::Fan {
                    continue;
                }
                fans::set_pwm_enable_to_default(&hwmon_driver.path, channel_info).await?
            }
        }
        info!("HWMON Repository shutdown");
        Ok(())
    }

    async fn apply_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
        let (_, hwmon_driver) = self.devices.get(device_uid)
            .with_context(|| format!("Device UID not found! {}", device_uid))?;
        let channel_info = hwmon_driver.channels.iter()
            .find(|channel|
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == setting.channel_name
            ).with_context(|| format!("Searching for channel name: {}", setting.channel_name))?;
        info!("Applying device: {} settings: {:?}", device_uid, setting);
        if let Some(true) = setting.reset_to_default {
            return fans::set_pwm_enable_to_default(
                &hwmon_driver.path, channel_info,
            ).await;
        }
        if let Some(fixed_speed) = setting.speed_fixed {
            if fixed_speed > 100 {
                return Err(anyhow!("Invalid fixed_speed: {}", fixed_speed));
            }
            fans::set_pwm_mode(&hwmon_driver.path, channel_info, setting.pwm_mode).await?;
            fans::set_pwm_duty(&hwmon_driver.path, channel_info, fixed_speed).await
        } else {
            Err(anyhow!("Only fixed speeds are currently supported for Hwmon devices"))
        }
    }
}
