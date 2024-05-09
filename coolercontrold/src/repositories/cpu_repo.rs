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
use std::ops::Not;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace, warn};
use psutil::cpu::CpuPercentCollector;
use regex::Regex;
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, Mhz, Status, TempInfo, TempStatus,
    UID,
};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::{devices, temps};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{LcdSettings, LightingSettings, TempSource};

pub const CPU_TEMP_NAME: &str = "CPU Temp";
const SINGLE_CPU_LOAD_NAME: &str = "CPU Load";
const SINGLE_CPU_FREQ_NAME: &str = "CPU Freq";
const INTEL_DEVICE_NAME: &str = "coretemp";
// cpu_device_names have a priority and we want to return the first match
pub const CPU_DEVICE_NAMES_ORDERED: [&str; 3] = ["k10temp", INTEL_DEVICE_NAME, "zenpower"];
const PATTERN_PACKAGE_ID: &str = r"package id (?P<number>\d+)$";

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
            if chg_count == 3 {
                // after each processor's entry
                self.cpu_infos
                    .entry(physical_id)
                    .or_default()
                    .push(processor_id);
                self.cpu_model_names
                    .insert(physical_id, model_name.to_string());
                chg_count = 0;
            }
        }
        if self.cpu_infos.is_empty().not() && self.cpu_model_names.is_empty().not() {
            for processor_list in self.cpu_infos.values_mut() {
                processor_list.sort_unstable();
            }
            trace!("CPUInfo: {:?}", self.cpu_infos);
            Ok(())
        } else {
            Err(anyhow!(
                "cpuinfo either not found or missing data on this system!"
            ))
        }
    }

    async fn init_cpu_temp(path: &PathBuf) -> Result<Vec<HwmonChannelInfo>> {
        let include_all_devices = "";
        temps::init_temps(path, include_all_devices).await
    }

    /// Returns the proper CPU physical ID.
    async fn match_physical_id(
        &self,
        device_name: &str,
        channels: &Vec<HwmonChannelInfo>,
        index: &usize,
    ) -> Result<PhysicalID> {
        if device_name == INTEL_DEVICE_NAME {
            self.parse_intel_physical_id(device_name, channels)
        } else {
            self.parse_amd_physical_id(index).await
        }
    }

    /// For Intel this is given by the package ID in the hwmon temp labels.
    fn parse_intel_physical_id(
        &self,
        device_name: &str,
        channels: &Vec<HwmonChannelInfo>,
    ) -> Result<PhysicalID> {
        let regex_package_id = Regex::new(PATTERN_PACKAGE_ID)?;
        for channel in channels {
            if channel.label.is_none() {
                continue; // package ID is in the label
            }
            let channel_label_lower = channel.label.as_ref().unwrap().to_lowercase();
            if regex_package_id.is_match(&channel_label_lower) {
                let package_id: u8 = regex_package_id
                    .captures(&channel_label_lower)
                    .context("Package ID should exist")?
                    .name("number")
                    .context("Number Group should exist")?
                    .as_str()
                    .parse()?;
                for physical_id in self.cpu_infos.keys() {
                    if physical_id == &package_id {
                        // verify there is a match
                        return Ok(package_id);
                    }
                }
            }
        }
        Err(anyhow!(
            "Could not find and match package ID to physical ID: {}, {:?}",
            device_name,
            channels
        ))
    }

    /// For AMD this is done by comparing hwmon devices to the cpuinfo processor list.
    async fn parse_amd_physical_id(&self, index: &usize) -> Result<PhysicalID> {
        // todo: not currently used due to an apparent bug in the amd hwmon kernel driver:
        // let cpu_list: Vec<ProcessorID> = devices::get_processor_ids_from_node_cpulist(index).await?;
        // for (physical_id, processor_list) in &self.cpu_infos {
        //     if cpu_list.iter().eq(processor_list.iter()) {
        //         return Ok(physical_id.clone());
        //     }
        // }

        // instead we do a simple assumption, that the physical cpu ID == hwmon AMD device index:
        let physical_id = *index as PhysicalID;
        if self.cpu_infos.get(&physical_id).is_some() {
            Ok(physical_id)
        } else {
            Err(anyhow!(
                "Could not match hwmon index to cpuinfo physical id"
            ))
        }
    }

    async fn collect_load(
        &self,
        physical_id: &PhysicalID,
        channel_name: &str,
    ) -> Option<ChannelStatus> {
        // it's not necessarily guaranteed that the processor_id is the index of this list, but it probably is:
        let percent_per_processor = self
            .cpu_percent_collector
            .write()
            .await
            .cpu_percent_percpu()
            .unwrap_or_default();
        let mut percents = Vec::new();
        for (processor_id, percent) in percent_per_processor.into_iter().enumerate() {
            let processor_id = processor_id as ProcessorID;
            if self
                .cpu_infos
                .get(physical_id)
                .expect("physical_id should be present in cpu_infos")
                .contains(&processor_id)
            {
                percents.push(percent);
            }
        }
        let num_percents = percents.len();
        let num_processors = self.cpu_infos.get(physical_id).unwrap().len();
        if num_percents != num_processors {
            error!(
                "No enough mathing processors: {} and percents: {} were found",
                num_processors, num_percents
            );
            None
        } else {
            let load = f64::from(percents.iter().sum::<f32>()) / num_processors as f64;
            Some(ChannelStatus {
                name: channel_name.to_string(),
                duty: Some(load),
                ..Default::default()
            })
        }
    }

    /// Collects the average frequency per Physical CPU.
    async fn collect_freq() -> HashMap<PhysicalID, Mhz> {
        // There a few ways to get this info, but the most reliable is to read the /proc/cpuinfo.
        // cpuinfo not only will return which frequency belongs to which physical CPU,
        // which is important for CoolerControl's full multi-physical-cpu support,
        // but also it's cached and therefor consistently fast across various systems.
        // See: https://github.com/giampaolo/psutil/issues/1851
        // The alternative is to read one of:
        //   /sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq
        //  /sys/devices/system/cpu/cpufreq/policy[0-9]*/scaling_cur_freq
        // But these have been reported to be significantly slower on some systems, and it's not
        // clear how to associate the frequency with the physical CPU on multi-cpu systems.
        let mut cpu_avgs = HashMap::new();
        let mut cpu_info_freqs: HashMap<PhysicalID, Vec<Mhz>> = HashMap::new();
        let Ok(cpu_info) = tokio::fs::read_to_string(PathBuf::from("/proc/cpuinfo")).await else {
            return cpu_avgs;
        };
        let mut cpu_info_physical_id: PhysicalID = 0;
        let mut cpu_info_freq: f64 = 0.;
        let mut chg_count: usize = 0;
        for line in cpu_info.lines() {
            if line.starts_with("physical id").not() && line.starts_with("cpu MHz").not() {
                continue;
            }
            let mut it = line.split(':');
            let (key, value) = match (it.next(), it.next()) {
                (Some(key), Some(value)) => (key.trim(), value.trim()),
                _ => continue,
            };
            if key == "physical id" {
                let Ok(phy_id) = value.parse() else {
                    return cpu_avgs;
                };
                cpu_info_physical_id = phy_id;
                chg_count += 1;
            }
            if key == "cpu MHz" {
                let Ok(freq) = value.parse() else {
                    return cpu_avgs;
                };
                cpu_info_freq = freq;
                chg_count += 1;
            }
            if chg_count == 2 {
                // after each processor's entry
                cpu_info_freqs
                    .entry(cpu_info_physical_id)
                    .or_default()
                    .push(cpu_info_freq.trunc() as Mhz);
                chg_count = 0;
            }
        }
        if cpu_info_freqs.is_empty() {
            warn!("No CPU frequencies found in cpuinfo");
        }
        for (physical_id, freqs) in cpu_info_freqs {
            let avg_freq = freqs.iter().sum::<Mhz>() / freqs.len() as Mhz;
            cpu_avgs.insert(physical_id, avg_freq);
        }
        cpu_avgs
    }

    fn get_status_from_freq_output(
        physical_id: &PhysicalID,
        channel_name: &str,
        cpu_freqs: &mut HashMap<PhysicalID, Mhz>,
    ) -> Option<ChannelStatus> {
        cpu_freqs.remove(physical_id).map(|avg_freq| ChannelStatus {
            name: channel_name.to_string(),
            freq: Some(avg_freq),
            ..Default::default()
        })
    }

    async fn init_cpu_load(&self, physical_id: &PhysicalID) -> Result<HwmonChannelInfo> {
        if self
            .collect_load(physical_id, SINGLE_CPU_LOAD_NAME)
            .await
            .is_none()
        {
            Err(anyhow!("Error: no load percent found!"))
        } else {
            Ok(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Load,
                number: *physical_id,
                name: SINGLE_CPU_LOAD_NAME.to_string(),
                label: Some(SINGLE_CPU_LOAD_NAME.to_string()),
                ..Default::default()
            })
        }
    }

    fn init_cpu_freq(
        physical_id: &PhysicalID,
        cpu_freqs: &mut HashMap<PhysicalID, Mhz>,
    ) -> Result<HwmonChannelInfo> {
        if Self::get_status_from_freq_output(physical_id, SINGLE_CPU_FREQ_NAME, cpu_freqs).is_none()
        {
            Err(anyhow!("Error: no frequency found!"))
        } else {
            Ok(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Freq,
                number: *physical_id,
                name: SINGLE_CPU_FREQ_NAME.to_string(),
                label: Some(SINGLE_CPU_FREQ_NAME.to_string()),
                ..Default::default()
            })
        }
    }

    async fn request_status(
        &self,
        phys_cpu_id: &PhysicalID,
        driver: &HwmonDriverInfo,
        cpu_freqs: &mut HashMap<PhysicalID, Mhz>,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = Vec::new();
        for channel in &driver.channels {
            match channel.hwmon_type {
                HwmonChannelType::Load => {
                    let Some(load_status) = self.collect_load(phys_cpu_id, &channel.name).await
                    else {
                        continue;
                    };
                    status_channels.push(load_status)
                }
                HwmonChannelType::Freq => {
                    let Some(freq_status) =
                        Self::get_status_from_freq_output(phys_cpu_id, &channel.name, cpu_freqs)
                    else {
                        continue;
                    };
                    status_channels.push(freq_status)
                }
                _ => continue,
            }
        }
        let temps = temps::extract_temp_statuses(driver)
            .await
            .iter()
            .map(|temp| TempStatus {
                name: temp.name.clone(),
                temp: temp.temp,
            })
            .collect();
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
        let mut num_cpu_devices_left_to_find = num_of_cpus;
        let mut cpu_freqs = Self::collect_freq().await;
        'outer: for cpu_device_name in CPU_DEVICE_NAMES_ORDERED {
            for (index, (device_name, path)) in potential_cpu_paths.iter().enumerate() {
                // is sorted
                if device_name == cpu_device_name {
                    let mut channels = Vec::new();
                    match Self::init_cpu_temp(path).await {
                        Ok(temps) => channels.extend(temps),
                        Err(err) => error!("Error initializing CPU Temps: {}", err),
                    };
                    let physical_id =
                        match self.match_physical_id(device_name, &channels, &index).await {
                            Ok(id) => id,
                            Err(err) => {
                                error!("Error matching CPU physical ID: {}", err);
                                continue;
                            }
                        };
                    match self.init_cpu_load(&physical_id).await {
                        Ok(load) => channels.push(load),
                        Err(err) => {
                            error!("Error matching cpu load percents to processors: {}", err);
                        }
                    }
                    match Self::init_cpu_freq(&physical_id, &mut cpu_freqs) {
                        Ok(freq) => channels.push(freq),
                        Err(err) => {
                            error!("Error matching cpu frequencies to processors: {}", err);
                        }
                    }
                    let model = devices::get_device_model_name(path).await;
                    let u_id = devices::get_device_unique_id(path).await;
                    let hwmon_driver_info = HwmonDriverInfo {
                        name: device_name.clone(),
                        path: path.clone(),
                        model,
                        u_id,
                        channels,
                    };
                    hwmon_devices.insert(physical_id, hwmon_driver_info);
                    if num_cpu_devices_left_to_find > 1 {
                        num_cpu_devices_left_to_find -= 1;
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

        let mut cpu_freqs = Self::collect_freq().await;
        for (physical_id, driver) in hwmon_devices {
            let (channels, temps) = self
                .request_status(&physical_id, &driver, &mut cpu_freqs)
                .await;
            let type_index = physical_id + 1;
            self.preloaded_statuses
                .write()
                .await
                .insert(type_index, (channels.clone(), temps.clone()));
            let cpu_name = self.cpu_model_names.get(&physical_id).unwrap().clone();
            let temp_infos = driver
                .channels
                .iter()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    let label_base = channel
                        .label
                        .as_ref()
                        .map(|l| l.to_title_case())
                        .unwrap_or_else(|| channel.name.to_title_case());
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: format!("{CPU_TEMP_NAME} {label_base}"),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut channel_infos = HashMap::new();
            for channel in &driver.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Load => {
                        channel_infos.insert(
                            channel.name.clone(),
                            ChannelInfo {
                                label: channel.label.clone(),
                                ..Default::default()
                            },
                        );
                    }
                    HwmonChannelType::Freq => {
                        channel_infos.insert(
                            channel.name.clone(),
                            ChannelInfo {
                                label: channel.label.clone(),
                                ..Default::default()
                            },
                        );
                    }
                    _ => continue,
                }
            }
            let mut device = Device::new(
                cpu_name,
                DeviceType::CPU,
                type_index,
                None,
                DeviceInfo {
                    channels: channel_infos,
                    temps: temp_infos,
                    temp_max: 100,
                    ..Default::default()
                },
                None,
            );
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            device.initialize_status_history_with(status);
            let cc_device_setting = self.config.get_cc_settings_for_device(&device.uid).await?;
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!(
                    "Skipping disabled device: {} with UID: {}",
                    device.name, device.uid
                );
            } else {
                self.devices.insert(
                    device.uid.clone(),
                    (Arc::new(RwLock::new(device)), Arc::new(driver)),
                );
            }
        }

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(
                uid.clone(),
                (device.read().await.clone(), hwmon_info.clone()),
            );
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized CPU Devices: {:?}", init_devices);
        } else {
            info!(
                "Initialized CPU Devices: {:?}",
                init_devices
                    .iter()
                    .map(|d| d.1 .0.name.clone())
                    .collect::<Vec<String>>()
            );
        }
        trace!(
            "Time taken to initialize all CPU devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("CPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices
            .values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();

        let mut tasks = Vec::new();
        let mut cpu_freqs = Self::collect_freq().await;
        for (device_lock, driver) in self.devices.values() {
            let self = Arc::clone(&self);
            let device_id = device_lock.read().await.type_index;
            let physical_id = device_id - 1;
            let mut cpu_freq = HashMap::new();
            if let Some(freq) = cpu_freqs.remove(&physical_id) {
                cpu_freq.insert(physical_id, freq);
            }
            let driver = Arc::clone(driver);
            let join_handle = tokio::task::spawn(async move {
                let (channels, temps) = self
                    .request_status(&physical_id, &driver, &mut cpu_freq)
                    .await;
                self.preloaded_statuses
                    .write()
                    .await
                    .insert(device_id, (channels, temps));
            });
            tasks.push(join_handle);
        }
        for task in tasks {
            if let Err(err) = task.await {
                error!("{}", err);
            }
        }
        trace!(
            "STATUS PRELOAD Time taken for all CPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (device, _) in self.devices.values() {
            let device_id = device.read().await.type_index;
            let preloaded_statuses_map = self.preloaded_statuses.read().await;
            let preloaded_statuses = preloaded_statuses_map.get(&device_id);
            if preloaded_statuses.is_none() {
                error!(
                    "There is no status preloaded for this device: {}",
                    device_id
                );
                continue;
            }
            let (channels, temps) = preloaded_statuses.unwrap().clone();
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            trace!(
                "CPU device #{} status was updated with: {:?}",
                device_id,
                status
            );
            device.write().await.set_status(status);
        }
        trace!(
            "STATUS SNAPSHOT Time taken for all CPU devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("CPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, _device_uid: &UID, _channel_name: &str) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_speed_fixed(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _speed_fixed: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }
}
