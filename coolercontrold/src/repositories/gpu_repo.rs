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
use heck::ToTitleCase;
use log::{debug, error, info, warn};
use nu_glob::{glob, GlobResult};
use regex::Regex;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::Instant;
use crate::config::Config;

use crate::device::{ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, SpeedOptions, Status, TempStatus, UID};
use crate::repositories::hwmon::{devices, fans, temps};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

pub const GPU_TEMP_NAME: &str = "GPU Temp";
const GPU_LOAD_NAME: &str = "GPU Load";
// synonymous with amd hwmon fan names:
const NVIDIA_FAN_NAME: &str = "fan1";
const AMD_HWMON_NAME: &str = "amdgpu";
const GLOB_XAUTHORITY_PATH_GDM: &str = "/run/user/*/gdm/Xauthority";
const GLOB_XAUTHORITY_PATH_USER: &str = "/home/*/.Xauthority";
const PATTERN_GPU_INDEX: &str = r"\[gpu:(?P<index>\d+)\]";
const PATTERN_FAN_INDEX: &str = r"\[fan:(?P<index>\d+)\]";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    config: Arc<Config>,
    devices: HashMap<UID, DeviceLock>,
    nvidia_devices: HashMap<u8, DeviceLock>,
    nvidia_device_infos: HashMap<UID, NvidiaDeviceInfo>,
    nvidia_preloaded_statuses: RwLock<HashMap<u8, StatusNvidiaDevice>>,
    amd_device_infos: HashMap<UID, Arc<HwmonDriverInfo>>,
    amd_preloaded_statuses: RwLock<HashMap<u8, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    gpu_type_count: RwLock<HashMap<GpuType, u8>>,
    has_multiple_gpus: RwLock<bool>,
    xauthority_path: String,
}

impl GpuRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let xauthority_path = Self::find_xauthority_path().await;
        Ok(Self {
            config,
            devices: HashMap::new(),
            nvidia_devices: HashMap::new(),
            nvidia_device_infos: HashMap::new(),
            nvidia_preloaded_statuses: RwLock::new(HashMap::new()),
            amd_device_infos: HashMap::new(),
            amd_preloaded_statuses: RwLock::new(HashMap::new()),
            gpu_type_count: RwLock::new(HashMap::new()),
            has_multiple_gpus: RwLock::new(false),
            xauthority_path,
        })
    }

    async fn detect_gpu_types(&self) {
        {
            let mut type_count = self.gpu_type_count.write().await;
            type_count.insert(GpuType::Nvidia, self.get_nvidia_status().await.len() as u8);
            type_count.insert(GpuType::AMD, Self::init_amd_devices().await.len() as u8);
        }
        let number_of_gpus = self.gpu_type_count.read().await.values().sum::<u8>();
        let mut has_multiple_gpus = self.has_multiple_gpus.write().await;
        *has_multiple_gpus = number_of_gpus > 1;
        if number_of_gpus == 0 {
            warn!("No GPU Devices detected")
        }
    }

    async fn try_request_nv_statuses(&self) -> Vec<StatusNvidiaDevice> {
        let mut statuses = vec![];
        if self.gpu_type_count.read().await.get(&GpuType::Nvidia).unwrap() > &0 {
            statuses.extend(
                self.request_nvidia_statuses().await
            )
        }
        statuses
    }

    async fn request_nvidia_statuses(&self) -> Vec<StatusNvidiaDevice> {
        let has_multiple_gpus: bool = self.has_multiple_gpus.read().await.clone();
        let mut statuses = vec![];
        let nvidia_statuses = self.get_nvidia_status().await;
        let starting_gpu_index = if has_multiple_gpus {
            self.gpu_type_count.read().await.get(&GpuType::AMD).unwrap_or(&0) + 1
        } else {
            1
        };
        for nvidia_status in nvidia_statuses.iter() {
            let mut temps = vec![];
            let mut channels = vec![];
            if let Some(temp) = nvidia_status.temp {
                let standard_temp_name = GPU_TEMP_NAME.to_string();
                let gpu_external_temp_name = if has_multiple_gpus {
                    format!("GPU#{} Temp", starting_gpu_index + nvidia_status.index)
                } else {
                    standard_temp_name.to_owned()
                };
                temps.push(
                    TempStatus {
                        name: standard_temp_name.to_owned(),
                        temp,
                        frontend_name: standard_temp_name,
                        external_name: gpu_external_temp_name,
                    }
                );
            }
            if let Some(load) = nvidia_status.load {
                channels.push(
                    ChannelStatus {
                        name: GPU_LOAD_NAME.to_string(),
                        rpm: None,
                        duty: Some(load as f64),
                        pwm_mode: None,
                    }
                );
            }
            if let Some(fan_duty) = nvidia_status.fan_duty {
                channels.push(
                    ChannelStatus {
                        name: NVIDIA_FAN_NAME.to_string(),
                        rpm: None,
                        duty: Some(fan_duty as f64),
                        pwm_mode: None,
                    }
                )
            }
            statuses.push(
                StatusNvidiaDevice {
                    index: nvidia_status.index,
                    name: nvidia_status.name.clone(),
                    temps,
                    channels,
                }
            )
        }
        statuses
    }

    async fn get_nvidia_status(&self) -> Vec<StatusNvidia> {
        let output = Command::new("sh")
            .arg("-c")
            .arg("nvidia-smi --query-gpu=index,gpu_name,temperature.gpu,utilization.gpu,fan.speed --format=csv,noheader,nounits")
            .output().await;
        match output {
            Ok(out) => {
                if out.status.success() {
                    let out_str = String::from_utf8(out.stdout).unwrap();
                    debug!("Nvidia raw status output: {}", out_str.trim());
                    let mut nvidia_statuses = vec![];
                    for line in out_str.trim().lines() {
                        if line.trim().is_empty() {
                            continue;  // skip any empty lines
                        }
                        let values = line.split(", ").collect::<Vec<&str>>();
                        if values.len() >= 5 {
                            let index = values[0].parse::<u8>();
                            if index.is_err() {
                                error!("Something is wrong with nvidia status output");
                                continue;
                            }
                            nvidia_statuses.push(StatusNvidia {
                                index: index.unwrap(),
                                name: values[1].to_string(),
                                temp: values[2].parse::<f64>().ok(),
                                load: values[3].parse::<u8>().ok(),
                                // on laptops for ex., this can be None as their is no fan control
                                fan_duty: values[4].parse::<u8>().ok(),
                            });
                        }
                    }
                    return nvidia_statuses;
                } else {
                    let out_err = String::from_utf8(out.stderr).unwrap();
                    warn!("Error communicating with nvidia-smi: {}", out_err)
                }
            }
            Err(err) => error!("Error running Nvidia command: {}", err)
        }
        vec![]
    }

    async fn get_nvidia_device_infos(&self) -> Result<HashMap<u8, Vec<u8>>> {
        let mut infos = HashMap::new();
        let output = Command::new("sh")
            .arg("-c")
            .arg("nvidia-settings -c :0 -q gpus --verbose")
            .env("XAUTHORITY", self.xauthority_path.as_str())
            .output().await;
        match output {
            Ok(out) => {
                if out.status.success() {
                    let out_str = String::from_utf8(out.stdout).unwrap();
                    debug!("Nvidia gpu info output: {}", out_str.trim());
                    let mut gpu_index_current = 0u8;
                    let regex_gpu_index = Regex::new(PATTERN_GPU_INDEX).expect("This regex should be valid");
                    let regex_fan_index = Regex::new(PATTERN_FAN_INDEX).expect("This regex should be valid");
                    for line_untrimmed in out_str.trim().lines() {
                        let line = line_untrimmed.trim();
                        if line.is_empty() {
                            continue;  // skip any empty lines
                        }
                        if regex_gpu_index.is_match(line) { // happens first in the output
                            let gpu_index_found: u8 = regex_gpu_index
                                .captures(line).expect("GPU index should exist")
                                .name("index").expect("Index Regex Group should exist")
                                .as_str().parse().expect("GPU index should be a valid u8 integer");
                            gpu_index_current = gpu_index_found;
                            infos.insert(gpu_index_current, Vec::new());
                        }
                        if regex_fan_index.is_match(line) {
                            let fan_index: u8 = regex_fan_index
                                .captures(line).expect("Fan index should exist")
                                .name("index").expect("Index Regex Group should exist")
                                .as_str().parse().expect("Fan index should be a valid u8 integer");
                            infos.get_mut(&gpu_index_current).expect("GPU index should already be present")
                                .push(fan_index);
                        }
                    }
                    Ok(infos)
                } else {
                    let out_err = String::from_utf8(out.stderr).unwrap();
                    warn!("Issue communicating with nvidia-settings. If you have a Nvidia card nvidia-settings needs to be installed for fan control. {}", out_err);
                    Err(anyhow!("Nvidia-settings error: {}", out_err))
                }
            }
            Err(err) => {
                error!("Unexpected Error running Nvidia command: {}", err);
                Err(anyhow!("Unexpected Error running Nvidia command: {}", err))
            }
        }
    }

    async fn find_xauthority_path() -> String {
        if let Some(existing_xauthority) = std::env::var("XAUTHORITY").ok() {
            debug!("Found existing Xauthority in the environment: {}", existing_xauthority);
            return existing_xauthority;
        } else {
            let mut xauth_glob_results = glob(GLOB_XAUTHORITY_PATH_GDM).unwrap()
                .collect::<Vec<GlobResult>>();
            if xauth_glob_results.is_empty() {
                xauth_glob_results.extend(
                    glob(GLOB_XAUTHORITY_PATH_USER).unwrap()
                        .collect::<Vec<GlobResult>>()
                )
            }
            let xauthority_paths = xauth_glob_results.into_iter()
                .filter_map(|result| result.ok())
                .filter(|path| path.is_absolute())
                .collect::<Vec<PathBuf>>();
            let xauthority_path_opt = xauthority_paths.first();
            if let Some(xauthority_path) = xauthority_path_opt {
                if let Some(xauthroity_str) = xauthority_path.to_str() {
                    debug!("Xauthority found in file path: {}", xauthroity_str);
                    return xauthroity_str.to_string();
                }
            }
            debug!("Xauthority not found.");
            String::default()
        }
    }

    /// Sets the nvidia fan duty
    async fn set_nvidia_duty(&self, nvidia_info: &NvidiaDeviceInfo, fixed_speed: u8) -> Result<()> {
        let mut command = format!(
            "nvidia-settings -c :0 -a \"[gpu:{0}]/GPUFanControlState=1\"", nvidia_info.gpu_index
        );
        for fan_index in &nvidia_info.fan_indices {
            command.push_str(&format!(" -a \"[fan:{0}]/GPUTargetFanSpeed={1}\"", fan_index, fixed_speed))
        }
        self.send_command_to_nvidia_settings(&command).await
    }

    /// resets the nvidia fan control back to automatic
    async fn reset_nvidia_to_default(&self, gpu_index: u8) -> Result<()> {
        let command = format!(
            "nvidia-settings -c :0 -a \"[gpu:{}]/GPUFanControlState=0\"", gpu_index
        );
        self.send_command_to_nvidia_settings(&command).await
    }

    async fn send_command_to_nvidia_settings(&self, command: &str) -> Result<()> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("XAUTHORITY", self.xauthority_path.as_str())
            .output().await;
        return match output {
            Ok(out) => if out.status.success() {
                let out_std = String::from_utf8(out.stdout).unwrap().trim().to_owned();
                let out_err = String::from_utf8(out.stderr).unwrap().trim().to_owned();
                debug!("Nvidia-settings output: \n{}\n{}", out_std, out_err);
                if out_err.is_empty() {
                    Ok(())
                } else {
                    Err(anyhow!("Error output received when trying to set nvidia fan speed settings. \
                    Some errors don't affect setting the fan speed. YMMV: \n{}", out_err))
                }
            } else {
                let out_err = String::from_utf8(out.stderr).unwrap().trim().to_owned();
                Err(anyhow!("Error communicating with nvidia-settings: {}", out_err))
            },
            Err(err) => Err(anyhow!("Nvidia-settings not found: {}", err))
        };
    }

    async fn init_amd_devices() -> Vec<HwmonDriverInfo> {
        let base_paths = devices::find_all_hwmon_device_paths();
        let mut amd_devices = vec![];
        for path in base_paths {
            let device_name = devices::get_device_name(&path).await;
            if device_name != AMD_HWMON_NAME {
                continue;
            }
            let mut channels = vec![];
            match fans::init_fans(&path, &device_name).await {
                Ok(fans) => channels.extend(fans),
                Err(err) => error!("Error initializing AMD Hwmon Fans: {}", err)
            };
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(temps),
                Err(err) => error!("Error initializing AMD Hwmon Temps: {}", err)
            };
            if let Some(load_channel) = Self::init_amd_load(&path).await {
                channels.push(load_channel)
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
            amd_devices.push(hwmon_driver_info);
        }
        amd_devices
    }

    async fn init_amd_load(base_path: &PathBuf) -> Option<HwmonChannelInfo> {
        match tokio::fs::read_to_string(
            base_path.join("device").join("gpu_busy_percent")
        ).await {
            Ok(load) => match fans::check_parsing_8(load) {
                Ok(_) => Some(HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Load,
                    name: GPU_LOAD_NAME.to_string(),
                    ..Default::default()
                }),
                Err(err) => {
                    warn!("Error reading AMD busy percent value: {}", err);
                    None
                }
            }
            Err(_) => {
                warn!("No AMDGPU load found: {:?}/device/gpu_busy_percent", base_path);
                None
            }
        }
    }

    async fn get_amd_status(&self, amd_driver: &HwmonDriverInfo, id: &u8) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = fans::extract_fan_statuses(amd_driver).await;
        status_channels.extend(Self::extract_load_status(amd_driver).await);
        let has_multiple_gpus = *self.has_multiple_gpus.read().await;
        let temps = temps::extract_temp_statuses(&id, amd_driver).await.iter()
            .map(|temp| {
                let standard_name = format!("{} {}", GPU_TEMP_NAME, temp.name.to_title_case());
                let gpu_external_base_temp_name = if has_multiple_gpus {
                    format!("GPU#{} Temp {}", id, temp.name.to_title_case())
                } else {
                    standard_name.to_owned()
                };
                TempStatus {
                    name: standard_name.to_owned(),
                    temp: temp.temp,
                    frontend_name: standard_name,
                    external_name: gpu_external_base_temp_name,
                }
            }).collect();
        (status_channels, temps)
    }

    async fn extract_load_status(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
        let mut channels = vec![];
        for channel in driver.channels.iter() {
            if channel.hwmon_type != HwmonChannelType::Load {
                continue;
            }
            let load = tokio::fs::read_to_string(
                driver.path.join("device").join("gpu_busy_percent")
            ).await
                .and_then(fans::check_parsing_8)
                .unwrap_or(0);
            channels.push(ChannelStatus {
                name: channel.name.clone(),
                rpm: None,
                duty: Some(load as f64),
                pwm_mode: None,
            })
        }
        channels
    }

    async fn reset_amd_to_default(&self, device_uid: &UID, channel_name: &String) -> Result<()> {
        let amd_hwmon_info = self.amd_device_infos.get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let channel_info = amd_hwmon_info.channels.iter()
            .find(|channel| channel.hwmon_type == HwmonChannelType::Fan && &channel.name == channel_name)
            .with_context(|| format!("Searching for channel name: {}", channel_name))?;
        fans::set_pwm_enable_to_default(&amd_hwmon_info.path, channel_info).await
    }

    async fn set_amd_duty(&self, device_uid: &UID, setting: &Setting, fixed_speed: u8) -> Result<()> {
        let amd_hwmon_info = self.amd_device_infos.get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let channel_info = amd_hwmon_info.channels.iter()
            .find(|channel| channel.hwmon_type == HwmonChannelType::Fan && channel.name == setting.channel_name)
            .with_context(|| "Searching for channel name")?;
        fans::set_pwm_mode(&amd_hwmon_info.path, channel_info, setting.pwm_mode).await?;
        fans::set_pwm_duty(&amd_hwmon_info.path, channel_info, fixed_speed).await
    }
}

#[async_trait]
impl Repository for GpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::GPU
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.detect_gpu_types().await;
        let has_multiple_gpus: bool = self.has_multiple_gpus.read().await.clone();
        for (index, amd_driver) in Self::init_amd_devices().await.into_iter().enumerate() {
            let id = index as u8 + 1;
            let mut channels = HashMap::new();
            for channel in &amd_driver.channels {
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
            let amd_status = self.get_amd_status(&amd_driver, &id).await;
            self.amd_preloaded_statuses.write().await.insert(id, amd_status.clone());
            let status = Status {
                channels: amd_status.0,
                temps: amd_status.1,
                ..Default::default()
            };
            let device = Device::new(
                amd_driver.name.clone(),
                DeviceType::GPU,
                id,
                None,
                Some(DeviceInfo {
                    channels,
                    temp_max: 100,
                    temp_ext_available: true,
                    model: amd_driver.model.clone(),
                    ..Default::default()
                }),
                Some(status),
                Some(amd_driver.u_id.clone()),
            );
            let cc_device_setting = self.config.get_cc_settings_for_device(&device.uid).await?;
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!("Skipping disabled device: {} with UID: {}", device.name, device.uid);
                continue; // skip loading this device into the device list
            }
            self.amd_device_infos.insert(
                device.uid.clone(),
                Arc::new(amd_driver.to_owned()),
            );
            self.devices.insert(
                device.uid.clone(),
                Arc::new(RwLock::new(device)),
            );
        }
        let starting_nvidia_index = if has_multiple_gpus {
            self.gpu_type_count.read().await.get(&GpuType::AMD).unwrap_or(&0) + 1
        } else {
            1
        };
        if let Ok(nvidia_infos) = self.get_nvidia_device_infos().await {
            for nv_status in self.request_nvidia_statuses().await.into_iter() {
                let type_index = nv_status.index + starting_nvidia_index;
                self.nvidia_preloaded_statuses.write().await.insert(type_index, nv_status.clone());
                let status = Status {
                    channels: nv_status.channels,
                    temps: nv_status.temps,
                    ..Default::default()
                };
                let mut channels = HashMap::new();
                if let Some(_) = status.channels.iter().find(
                    |channel| channel.name == NVIDIA_FAN_NAME
                ) {
                    channels.insert(NVIDIA_FAN_NAME.to_string(), ChannelInfo {
                        speed_options: Some(SpeedOptions {
                            profiles_enabled: false,
                            fixed_enabled: true,
                            manual_profiles_enabled: true,
                            ..Default::default()
                        }),
                        ..Default::default()
                    });
                }
                let device = Arc::new(RwLock::new(Device::new(
                    nv_status.name,
                    DeviceType::GPU,
                    type_index,
                    None,
                    Some(DeviceInfo {
                        temp_max: 100,
                        temp_ext_available: true,
                        channels,
                        ..Default::default()
                    }),
                    Some(status),
                    None,
                )));
                let uid = device.read().await.uid.clone();
                let cc_device_setting = self.config.get_cc_settings_for_device(&uid).await?;
                if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                    info!("Skipping disabled device: {} with UID: {}", device.read().await.name, uid);
                    continue; // skip loading this device into the device list
                }
                self.nvidia_devices.insert(
                    type_index,
                    Arc::clone(&device),
                );
                let fan_indexes = nvidia_infos.get(&nv_status.index)
                    .with_context(|| format!("Nvidia GPU index not found! {}, index: {}", uid, nv_status.index))?
                    .to_owned();
                self.nvidia_device_infos.insert(
                    uid.clone(),
                    NvidiaDeviceInfo {
                        gpu_index: nv_status.index,
                        fan_indices: fan_indexes,
                    },
                );
                self.devices.insert(
                    uid,
                    device,
                );
            }
        }
        let mut init_devices = HashMap::new();
        for (uid, device) in self.devices.iter() {
            init_devices.insert(uid.clone(), device.read().await.clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            // pretty output for easy reading
            info!("Initialized Devices: {:#?}", init_devices);
            info!("Initialized AMD HwmonInfos: {:#?}", self.amd_device_infos);
        } else {
            info!("Initialized Devices: {:?}", init_devices);
            info!("Initialized AMD HwmonInfos: {:?}", self.amd_device_infos);
        }
        debug!(
            "Time taken to initialize all GPU devices: {:?}", start_initialization.elapsed()
        );
        info!("GPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();

        let mut tasks = Vec::new();
        for (uid, amd_driver) in self.amd_device_infos.iter() {
            if let Some(device_lock) = self.devices.get(uid) {
                let self = Arc::clone(&self);
                let device_lock = Arc::clone(&device_lock);
                let amd_driver = Arc::clone(&amd_driver);
                let join_handle = tokio::task::spawn(async move {
                    let device_id = device_lock.read().await.type_index;
                    let statuses = self.get_amd_status(&amd_driver, &device_id).await;
                    self.amd_preloaded_statuses.write().await.insert(device_id, statuses);
                }
                );
                tasks.push(join_handle);
            }
        }
        let self = Arc::clone(&self);
        let join_handle = tokio::task::spawn(async move {
            for nv_status in self.try_request_nv_statuses().await.into_iter() {
                let device_index = nv_status.index + 1;
                self.nvidia_preloaded_statuses.write().await.insert(device_index, nv_status);
            }
        });
        tasks.push(join_handle);
        for task in tasks {
            if let Err(err) = task.await {
                error!("{}", err);
            }
        }
        debug!(
            "STATUS PRELOAD Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (uid, amd_driver) in self.amd_device_infos.iter() {
            if let Some(device_lock) = self.devices.get(uid) {
                let preloaded_statuses_map = self.amd_preloaded_statuses.read().await;
                let preloaded_statuses = preloaded_statuses_map.get(&device_lock.read().await.type_index);
                if let None = preloaded_statuses {
                    error!("There is no status preloaded for this AMD device: {}", device_lock.read().await.type_index);
                    continue;
                }
                let (channels, temps) = preloaded_statuses.unwrap().clone();
                let status = Status {
                    channels,
                    temps,
                    ..Default::default()
                };
                debug!("Device: {} status updated: {:?}", amd_driver.name, status);
                device_lock.write().await.set_status(status);
            }
        }
        for (device_id, nv_device_lock) in &self.nvidia_devices {
            let preloaded_statuses_map = self.nvidia_preloaded_statuses.read().await;
            let preloaded_statuses = preloaded_statuses_map.get(&device_id);
            if let None = preloaded_statuses {
                error!("There is no status preloaded for this Nvidia device: {}", device_id);
                continue;
            }
            let nv_status = preloaded_statuses.unwrap().clone();
            let status = Status {
                channels: nv_status.channels.to_owned(),
                temps: nv_status.temps.to_owned(),
                ..Default::default()
            };
            debug!("Device: {} status updated: {:?}", nv_status.name, status);
            nv_device_lock.write().await.set_status(status);
        }
        debug!(
            "STATUS SNAPSHOT Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (uid, device_lock) in self.devices.iter() {
            let gpu_index = device_lock.read().await.type_index - 1;
            let is_amd = self.amd_device_infos.contains_key(uid);
            if is_amd {
                if let Some(info) = &device_lock.read().await.info {
                    for channel_name in info.channels.keys() {
                        self.reset_amd_to_default(uid, channel_name).await.ok();
                    }
                }
            } else {
                self.reset_nvidia_to_default(gpu_index).await.ok();
            };
        }
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
        let is_amd = self.amd_device_infos.contains_key(device_uid);
        info!("Applying device: {} settings: {:?}", device_uid, setting);
        if let Some(true) = setting.reset_to_default {
            return if is_amd {
                self.reset_amd_to_default(device_uid, &setting.channel_name).await
            } else {
                let nvidia_gpu_index = self.nvidia_device_infos.get(device_uid)
                    .with_context(|| format!("Nvidia Device UID not found! {}", device_uid))?
                    .gpu_index;
                self.reset_nvidia_to_default(nvidia_gpu_index).await
            };
        }
        if let Some(fixed_speed) = setting.speed_fixed {
            if fixed_speed > 100 {
                return Err(anyhow!("Invalid fixed_speed: {}", fixed_speed));
            }
            if is_amd {
                self.set_amd_duty(device_uid, setting, fixed_speed).await
            } else {
                let nvidia_gpu_info = self.nvidia_device_infos.get(device_uid)
                    .with_context(|| format!("Device UID not found! {}", device_uid))?;
                self.set_nvidia_duty(nvidia_gpu_info, fixed_speed).await
            }
        } else {
            Err(anyhow!("Only fixed speeds are supported for GPU devices"))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusNvidia {
    index: u8,
    name: String,
    temp: Option<f64>,
    load: Option<u8>,
    fan_duty: Option<u8>,
}

#[derive(Debug, Clone)]
struct StatusNvidiaDevice {
    index: u8,
    name: String,
    channels: Vec<ChannelStatus>,
    temps: Vec<TempStatus>,
}

#[derive(Debug)]
struct NvidiaDeviceInfo {
    gpu_index: u8,
    fan_indices: Vec<u8>,
}
