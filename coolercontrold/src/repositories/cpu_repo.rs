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
use std::ops::Not;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info};
use psutil::cpu::CpuPercentCollector;
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{ChannelStatus, Device, DeviceInfo, DeviceType, Status, TempStatus, UID};
use crate::repositories::hwmon::{devices, temps};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

pub const CPU_TEMP_NAME: &str = "CPU Temp";
const SINGLE_CPU_LOAD_NAME: &str = "CPU Load";
// cpu_device_names have a priority and we want to return the first match
pub const CPU_DEVICE_NAMES_ORDERED: [&'static str; 3] =
    ["k10temp", "coretemp", "zenpower"];
pub const CPU_TEMP_BASE_LABEL_NAMES_ORDERED: [&'static str; 5] =
    ["tctl", "physical", "package", "tdie", "temp1"];

// The ID of the actual physical CPU. On most systems there is only one:
type PhysicalID = u8;
type ProcessorID = u16; // the logical processor ID

/// A CPU Repository for CPU status
pub struct CpuRepo {
    config: Arc<Config>,
    devices: HashMap<UID, (DeviceLock, Arc<HwmonDriverInfo>)>,
    cpu_infos: HashMap<PhysicalID, Vec<ProcessorID>>,
    cpu_model_names: HashMap<PhysicalID, String>,
    cpu_percent_collector: RwLock<CpuPercentCollector>,
    preloaded_statuses: RwLock<HashMap<u8, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
}

impl CpuRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        Ok(Self {
            config,
            devices: HashMap::new(),
            cpu_infos: HashMap::new(),
            cpu_model_names: HashMap::new(),
            cpu_percent_collector: RwLock::new(CpuPercentCollector::new()?),
            preloaded_statuses: RwLock::new(HashMap::new()),
        })
    }

    async fn set_cpu_infos(&mut self) -> Result<()> {
        let cpu_info_data = tokio::fs::read_to_string(PathBuf::from("/proc/cpuinfo")).await?;
        let mut physical_id: PhysicalID = 0;
        let mut model_name = "";
        let mut processor_id: ProcessorID = 0;
        let mut chg_count: usize = 0;
        for line in cpu_info_data.lines() {
            let mut it = line.split(':');
            let (key, value) = match (it.next(), it.next()) {
                (Some(key), Some(value)) => (key.trim(), value.trim()),
                _ => continue, // will skip empty lines and non-key-value lines
            };

            if key == "processor" {
                processor_id = value.parse()?;
                chg_count += 1;
            }
            if key == "model name" {
                model_name = value;
                chg_count += 1;
            }
            if key == "physical id" {
                physical_id = value.parse()?;
                chg_count += 1;
            }
            if chg_count == 3 { // after each processor's entry
                self.cpu_infos.entry(physical_id).or_default().push(processor_id);
                self.cpu_model_names.insert(physical_id, model_name.to_string());
                chg_count = 0;
            }
        }
        if self.cpu_infos.is_empty().not() && self.cpu_model_names.is_empty().not() {
            for processor_list in self.cpu_infos.values_mut() {
                processor_list.sort_unstable();
            }
            Ok(())
        } else {
            Err(anyhow!("cpuinfo either not found or missing data on this system!"))
        }
    }

    async fn init_cpu_temp(path: &PathBuf) -> Result<Vec<HwmonChannelInfo>> {
        let include_all_devices = "";
        temps::init_temps(path, include_all_devices).await
    }

    async fn match_hwmon_to_cpuinfos(&self, index: &usize) -> Result<PhysicalID> {
        if self.cpu_infos.len() > 1 {
            let cpu_list: Vec<ProcessorID> = devices::get_processor_ids_from_cpulist(index).await?;
            for (physical_id, processor_list) in &self.cpu_infos {
                if cpu_list.iter().eq(processor_list.iter()) {
                    return Ok(physical_id.clone());
                }
            }
            Err(anyhow!("Could not match HWMON cpulist to cpuinfos processors"))
        } else { // for single cpus let's skip the above
            Ok(self.cpu_infos.keys().last()
                .map(|id| id.to_owned())
                .unwrap_or_default())
        }
    }

    async fn collect_load(&self, physical_id: &PhysicalID, channel_name: &str) -> Option<ChannelStatus> {
        // it's not necessarily guaranteed that the processor_id is the index of this list, but it probably is:
        let percent_per_processor = self.cpu_percent_collector.write().await
            .cpu_percent_percpu()
            .unwrap_or_default();
        let mut percents = Vec::new();
        for (processor_id, percent) in percent_per_processor.into_iter().enumerate() {
            let processor_id = processor_id as ProcessorID;
            if self.cpu_infos
                .get(physical_id).expect("physical_id should be present in cpu_infos")
                .contains(&processor_id) {
                percents.push(percent);
            }
        }
        let num_percents = percents.len();
        let num_processors = self.cpu_infos.get(physical_id).unwrap().len();
        if num_percents != num_processors {
            error!("No enough mathing processors: {} and percents: {} were found", num_processors, num_percents);
            None
        } else {
            let load = percents.iter().sum::<f32>() as f64 / num_processors as f64;
            Some(ChannelStatus {
                name: channel_name.to_string(),
                rpm: None,
                duty: Some(load),
                pwm_mode: None,
            })
        }
    }

    async fn init_cpu_load(&self, physical_id: &PhysicalID) -> Result<HwmonChannelInfo> {
        if self.collect_load(physical_id, SINGLE_CPU_LOAD_NAME).await.is_none() {
            Err(anyhow!("Error: no load percent found!"))
        } else {
            Ok(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Load,
                number: physical_id.clone(),
                name: SINGLE_CPU_LOAD_NAME.to_string(),
                ..Default::default()
            })
        }
    }

    async fn request_status(
        &self, phys_cpu_id: &PhysicalID, driver: &HwmonDriverInfo,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = Vec::new();
        for channel in driver.channels.iter() {
            if channel.hwmon_type != HwmonChannelType::Load {
                continue;
            }
            if let Some(load) = self.collect_load(phys_cpu_id, &channel.name).await {
                status_channels.push(load);
            }
        }
        let temps = temps::extract_temp_statuses(&phys_cpu_id, driver).await.iter()
            .map(|temp| {
                let standard_name = format!("{} {}", CPU_TEMP_NAME, temp.name.to_title_case());
                let cpu_external_temp_name = if self.cpu_infos.len() > 1 {
                    format!("CPU#{} Temp {}", phys_cpu_id + 1, temp.name.to_title_case())
                } else {
                    standard_name.to_owned()
                };
                TempStatus {
                    name: standard_name.to_owned(),
                    temp: temp.temp,
                    frontend_name: standard_name,
                    external_name: cpu_external_temp_name,
                }
            }).collect();
        (status_channels, temps)
    }
}

#[async_trait]
impl Repository for CpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::CPU
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();

        self.set_cpu_infos().await?;

        let mut potential_cpu_paths = Vec::new();
        for path in devices::find_all_hwmon_device_paths() {
            let device_name = devices::get_device_name(&path).await;
            if CPU_DEVICE_NAMES_ORDERED.contains(&device_name.as_str()) {
                potential_cpu_paths.push((device_name, path));
            }
        }

        let mut hwmon_devices = HashMap::new();
        let num_of_cpus = self.cpu_infos.len();
        let mut num_cpu_devices_to_find = num_of_cpus.clone();
        'outer: for cpu_device_name in CPU_DEVICE_NAMES_ORDERED {
            for (index, (device_name, path))
            in potential_cpu_paths.iter().enumerate() { // is sorted
                if device_name == cpu_device_name {
                    let mut channels = Vec::new();
                    match Self::init_cpu_temp(&path).await {
                        Ok(temps) => channels.extend(temps),
                        Err(err) => error!("Error initializing CPU Temps: {}", err)
                    };
                    let physical_id = match self.match_hwmon_to_cpuinfos(&index).await {
                        Ok(id) => id,
                        Err(err) => {
                            error!("Error matching hwmon cpus to cpuinfos: {}", err);
                            continue;
                        }
                    };
                    match self.init_cpu_load(&physical_id).await {
                        Ok(load) => channels.push(load),
                        Err(err) => {
                            error!("Error matching cpu load percents to processors: {}", err);
                        }
                    }
                    let model = devices::get_device_model_name(&path).await;
                    let u_id = devices::get_device_unique_id(&path).await;
                    let hwmon_driver_info = HwmonDriverInfo {
                        name: device_name.clone(),
                        path: path.to_path_buf(),
                        model,
                        u_id,
                        channels,
                    };
                    hwmon_devices.insert(physical_id, hwmon_driver_info);
                    if num_cpu_devices_to_find > 1 {
                        num_cpu_devices_to_find -= 1;
                        continue;
                    } else {
                        break 'outer;
                    }
                }
            }
        }
        if hwmon_devices.len() != num_of_cpus {
            return Err(anyhow!("Something has gone wrong - missing Hwmon devices. cpuinfo count: {} hwmon devices found: {}",
                num_of_cpus, hwmon_devices.len()
            ));
        }

        for (physical_id, driver) in hwmon_devices.into_iter() {
            let (channels, temps) = self.request_status(&physical_id, &driver).await;
            let type_index = physical_id + 1;
            self.preloaded_statuses.write().await
                .insert(type_index, (channels.clone(), temps.clone()));
            let status = Status {
                channels,
                temps,
                ..Default::default()
            };
            let cpu_name = self.cpu_model_names.get(&physical_id).unwrap().clone();
            let device = Device::new(
                cpu_name,
                DeviceType::CPU,
                type_index,
                None,
                Some(DeviceInfo {
                    temp_max: 100,
                    temp_ext_available: true,
                    ..Default::default()
                }),
                Some(status),
                None,
            );
            let cc_device_setting = self.config.get_cc_settings_for_device(&device.uid).await?;
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!("Skipping disabled device: {} with UID: {}", device.name, device.uid);
            } else {
                self.devices.insert(
                    device.uid.clone(),
                    (Arc::new(RwLock::new(device)), Arc::new(driver)),
                );
            }
        }

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
            "Time taken to initialize all CPU devices: {:?}", start_initialization.elapsed()
        );
        info!("CPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();

        let mut tasks = Vec::new();
        for (device_lock, driver) in self.devices.values() {
            let self = Arc::clone(&self);
            let device_lock = Arc::clone(&device_lock);
            let driver = Arc::clone(&driver);
            let join_handle = tokio::task::spawn(async move {
                let device_id = device_lock.read().await.type_index;
                let physical_id = device_id - 1;
                let (channels, temps) = self.request_status(&physical_id, &driver).await;
                self.preloaded_statuses.write().await.insert(
                    device_id,
                    (channels, temps),
                );
            });
            tasks.push(join_handle);
        }
        for task in tasks {
            if let Err(err) = task.await {
                error!("{}", err);
            }
        }
        debug!(
            "STATUS PRELOAD Time taken for all CPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (device, _) in self.devices.values() {
            let device_id = device.read().await.type_index.clone();
            let preloaded_statuses_map = self.preloaded_statuses.read().await;
            let preloaded_statuses = preloaded_statuses_map.get(&device_id);
            if let None = preloaded_statuses {
                error!("There is no status preloaded for this device: {}", device_id);
                continue;
            }
            let (channels, temps) = preloaded_statuses.unwrap().clone();
            let status = Status {
                channels,
                temps,
                ..Default::default()
            };
            debug!("CPU device #{} status was updated with: {:?}", device_id, status);
            device.write().await.set_status(status);
        }
        debug!(
            "STATUS SNAPSHOT Time taken for all CPU devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("CPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting(&self, _device_uid: &UID, _setting: &Setting) -> Result<()> {
        Err(anyhow!("Applying settings is not supported for CPU devices"))
    }
}