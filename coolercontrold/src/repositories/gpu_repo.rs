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
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::device::{ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, SpeedOptions, Status, TempStatus, UID};
use crate::repositories::hwmon::{devices, fans, temps};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

const GPU_TEMP_NAME: &str = "GPU Temp";
const GPU_LOAD_NAME: &str = "GPU Load";
const NVIDIA_FAN_NAME: &str = "fan1";  // synonymous with amd hwmon fan names
// todo: use as default for AMD GPU name just in case.
const DEFAULT_AMD_GPU_NAME: &str = "Radeon Graphics";
const AMD_HWMON_NAME: &str = "amdgpu";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    devices: HashMap<UID, DeviceLock>,
    nvidia_devices: HashMap<u8, DeviceLock>,
    amd_device_infos: HashMap<UID, HwmonDriverInfo>,
    gpu_type_count: RwLock<HashMap<GpuType, u8>>,
    has_multiple_gpus: RwLock<bool>,
}

impl GpuRepo {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            devices: HashMap::new(),
            nvidia_devices: HashMap::new(),
            amd_device_infos: HashMap::new(),
            gpu_type_count: RwLock::new(HashMap::new()),
            has_multiple_gpus: RwLock::new(false),
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

    async fn try_request_nv_statuses(&self) -> Vec<(Status, String)> {
        let mut statuses = vec![];
        if self.gpu_type_count.read().await.get(&GpuType::Nvidia).unwrap() > &0 {
            statuses.extend(
                self.request_nvidia_statuses().await
            )
        }
        statuses
    }

    async fn request_nvidia_statuses(&self) -> Vec<(Status, String)> {
        let has_multiple_gpus: bool = self.has_multiple_gpus.read().await.clone();
        let mut statuses = vec![];
        let nvidia_statuses = self.get_nvidia_status().await;
        let starting_gpu_index = if has_multiple_gpus {
            self.gpu_type_count.read().await.get(&GpuType::AMD).unwrap_or(&0) + 1
        } else {
            1
        };
        for (index, nvidia_status) in nvidia_statuses.iter().enumerate() {
            let index = index as u8;
            let mut temps = vec![];
            let mut channels = vec![];
            if let Some(temp) = nvidia_status.temp {
                let gpu_external_temp_name = if has_multiple_gpus {
                    format!("GPU#{} TEMP", starting_gpu_index + index)
                } else {
                    GPU_TEMP_NAME.to_string()
                };
                temps.push(
                    TempStatus {
                        name: GPU_TEMP_NAME.to_string(),
                        temp,
                        frontend_name: GPU_TEMP_NAME.to_string(),
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
                (
                    Status {
                        temps,
                        channels,
                        ..Default::default()
                    },
                    nvidia_status.name.clone()
                )
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
            Err(err) => warn!("Nvidia driver not found: {}", err)
        }
        vec![]
    }

    /// Sets the nvidia fan duty
    async fn set_nvidia_duty(gpu_index: u8, fixed_speed: u8) -> Result<()> {
        let command = format!(
            "nvidia-settings -a \"[gpu:{0}]/GPUFanControlState=1\" -a \"[fan:{0}]/GPUTargetFanSpeed={1}\"",
            gpu_index, fixed_speed
        );
        Self::send_command_to_nvidia_settings(&command).await
    }

    /// resets the nvidia fan control back to automatic
    async fn reset_nvidia_to_default(gpu_index: u8) -> Result<()> {
        let command = format!(
            "nvidia-settings -a \"[gpu:{}]/GPUFanControlState=0\" ", gpu_index
        );
        Self::send_command_to_nvidia_settings(&command).await
    }

    async fn send_command_to_nvidia_settings(command: &str) -> Result<()> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output().await;
        return match output {
            Ok(out) => if out.status.success() {
                let out_std = String::from_utf8(out.stdout).unwrap().trim().to_owned();
                let out_err = String::from_utf8(out.stderr).unwrap().trim().to_owned();
                debug!("Nvidia-settings output: {}\n{}", out_std, out_err);
                if out_err.is_empty() {
                    Ok(())
                } else {
                    Err(anyhow!("Error trying to set nvidia fan speed settings: {}", out_err))
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
                Ok(temps) => channels.extend(
                    temps.into_iter().map(|temp| HwmonChannelInfo {
                        hwmon_type: temp.hwmon_type,
                        number: temp.number,
                        name: GPU_TEMP_NAME.to_string(),
                        ..Default::default()
                    }).collect::<Vec<HwmonChannelInfo>>()
                ),
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

    async fn get_amd_status(&self, amd_driver: &HwmonDriverInfo, id: &u8) -> Status {
        let mut status_channels = fans::extract_fan_statuses(amd_driver).await;
        status_channels.extend(Self::extract_load_status(amd_driver).await);
        let gpu_external_temp_name = if *self.has_multiple_gpus.read().await {
            format!("GPU#{} TEMP", id)
        } else {
            GPU_TEMP_NAME.to_string()
        };
        let temps = temps::extract_temp_statuses(&id, amd_driver).await
            .iter().map(|temp| {
            TempStatus {
                name: GPU_TEMP_NAME.to_string(),
                temp: temp.temp,
                frontend_name: GPU_TEMP_NAME.to_string(),
                external_name: gpu_external_temp_name.clone(),
            }
        }).collect();
        Status {
            channels: status_channels,
            temps,
            ..Default::default()
        }
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
            let status = self.get_amd_status(&amd_driver, &id).await;
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
            self.amd_device_infos.insert(
                device.uid.clone(),
                amd_driver.to_owned(),
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
        for (index, (status, gpu_name)) in self.request_nvidia_statuses().await.into_iter().enumerate() {
            let id = index as u8 + starting_nvidia_index;
            // todo: also verify fan is writable... this could conflict with other programs, let's leave it for now.
            let mut channels = HashMap::new();
            channels.insert(NVIDIA_FAN_NAME.to_string(), ChannelInfo {
                speed_options: Some(SpeedOptions {
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                    ..Default::default()
                }),
                ..Default::default()
            });
            let device = Arc::new(RwLock::new(Device::new(
                gpu_name,
                DeviceType::GPU,
                id,
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
            self.nvidia_devices.insert(
                id,
                Arc::clone(&device),
            );
            self.devices.insert(
                uid,
                device,
            );
        }
        let mut init_devices = HashMap::new();
        for (uid, device) in self.devices.iter() {
            init_devices.insert(uid.clone(), device.read().await.clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Devices: {:#?}", init_devices);  // pretty output for easy reading
            info!("Initialized AMD HwmonInfos: {:#?}", self.amd_device_infos);
        } else {
            info!("Initialized Devices: {:?}", init_devices);
            info!("Initialized AMD HwmonInfos: {:?}", self.amd_device_infos);
        }
        info!(
            "Time taken to initialize all GPU devices: {:?}", start_initialization.elapsed()
        );
        info!("GPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn update_statuses(&self) -> Result<()> {
        debug!("Updating all GPU device statuses");
        let start_update = Instant::now();
        for (uid, amd_driver) in self.amd_device_infos.iter() {
            if let Some(device_lock) = self.devices.get(uid) {
                let id = device_lock.read().await.type_index;
                let status = self.get_amd_status(amd_driver, &id).await;
                device_lock.write().await.set_status(status.clone());
                debug!("Device: {} status updated: {:?}", amd_driver.name, status);
            }
        }
        for (index, (status, gpu_name)) in self.try_request_nv_statuses().await.iter().enumerate() {
            let index = index as u8 + 1;
            if let Some(device_lock) = self.nvidia_devices.get(&index) {
                device_lock.write().await.set_status(status.clone());
                debug!("Device: {} status updated: {:?}", gpu_name, status);
            }
        }
        debug!(
            "Time taken to update status for all GPU devices: {:?}",
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
                Self::reset_nvidia_to_default(gpu_index).await.ok();
            };
        }
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
        let device_lock = self.devices.get(device_uid)
            .with_context(|| format!("Device UID not found! {}", device_uid))?;
        let gpu_index = device_lock.read().await.type_index - 1;
        let is_amd = self.amd_device_infos.contains_key(device_uid);
        info!("Applying device: {} settings: {:?}", device_uid, setting);
        if let Some(true) = setting.reset_to_default {
            return if is_amd {
                self.reset_amd_to_default(device_uid, &setting.channel_name).await
            } else {
                Self::reset_nvidia_to_default(gpu_index).await
            };
        }
        if let Some(fixed_speed) = setting.speed_fixed {
            if fixed_speed > 100 {
                return Err(anyhow!("Invalid fixed_speed: {}", fixed_speed));
            }
            if is_amd {
                self.set_amd_duty(device_uid, setting, fixed_speed).await
            } else {
                Self::set_nvidia_duty(gpu_index, fixed_speed).await
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