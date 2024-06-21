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
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, SpeedOptions, Status, TempInfo,
    TempStatus, TypeIndex, UID,
};
use crate::repositories::gpu_amd::GpuAMD;
use crate::repositories::gpu_nvidia::{GpuNVidia, StatusNvidiaDeviceSMI};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::{devices, fans, freqs, temps};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{LcdSettings, LightingSettings, TempSource};

pub const GPU_TEMP_NAME: &str = "GPU Temp";
pub const GPU_FREQ_NAME: &str = "GPU Freq";
pub const GPU_LOAD_NAME: &str = "GPU Load";
const AMD_HWMON_NAME: &str = "amdgpu";
pub const COMMAND_TIMEOUT_DEFAULT: Duration = Duration::from_millis(800);
pub const COMMAND_TIMEOUT_FIRST_TRY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    config: Arc<Config>,
    devices: HashMap<UID, DeviceLock>,
    amd_device_infos: HashMap<UID, Arc<HwmonDriverInfo>>,
    amd_preloaded_statuses: RwLock<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    gpu_type_count: HashMap<GpuType, u8>,
    has_multiple_gpus: bool,
    gpus_nvidia: GpuNVidia,
    nvml_active: bool,
    gpus_amd: GpuAMD,
}

impl GpuRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        Ok(Self {
            gpus_nvidia: GpuNVidia::new(Arc::clone(&config)),
            gpus_amd: GpuAMD::new(Arc::clone(&config)),
            config,
            devices: HashMap::new(),
            amd_device_infos: HashMap::new(),
            amd_preloaded_statuses: RwLock::new(HashMap::new()),
            gpu_type_count: HashMap::new(),
            has_multiple_gpus: false,
            nvml_active: false,
        })
    }

    async fn detect_gpu_types(&mut self) {
        let nvidia_dev_count =
            if let Some(num_nvml_devices) = self.gpus_nvidia.init_nvml_devices().await {
                self.nvml_active = true;
                num_nvml_devices
            } else {
                self.gpus_nvidia
                    .get_nvidia_smi_status(COMMAND_TIMEOUT_FIRST_TRY)
                    .await
                    .len() as u8
            };
        self.gpu_type_count
            .insert(GpuType::Nvidia, nvidia_dev_count);
        self.gpu_type_count
            .insert(GpuType::AMD, Self::init_amd_devices().await.len() as u8);
        let number_of_gpus = self.gpu_type_count.values().sum::<u8>();
        self.has_multiple_gpus = number_of_gpus > 1;
        if number_of_gpus == 0 {
            warn!("No GPU Devices detected");
        }
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
                Err(err) => error!("Error initializing AMD Hwmon Fans: {}", err),
            };
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(temps),
                Err(err) => error!("Error initializing AMD Hwmon Temps: {}", err),
            };
            if let Some(load_channel) = Self::init_amd_load(&path).await {
                channels.push(load_channel);
            }
            match freqs::init_freqs(&path).await {
                Ok(freqs) => channels.extend(freqs),
                Err(err) => error!("Error initializing AMD Hwmon Freqs: {}", err),
            };
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
        match tokio::fs::read_to_string(base_path.join("device").join("gpu_busy_percent")).await {
            Ok(load) => match fans::check_parsing_8(load) {
                Ok(_) => Some(HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Load,
                    name: GPU_LOAD_NAME.to_string(),
                    label: Some(GPU_LOAD_NAME.to_string()),
                    ..Default::default()
                }),
                Err(err) => {
                    warn!("Error reading AMD busy percent value: {}", err);
                    None
                }
            },
            Err(_) => {
                warn!(
                    "No AMDGPU load found: {:?}/device/gpu_busy_percent",
                    base_path
                );
                None
            }
        }
    }

    async fn get_amd_status(
        &self,
        amd_driver: &HwmonDriverInfo,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = fans::extract_fan_statuses(amd_driver).await;
        status_channels.extend(Self::extract_load_status(amd_driver).await);
        status_channels.extend(freqs::extract_freq_statuses(amd_driver).await);
        let temps = temps::extract_temp_statuses(amd_driver)
            .await
            .iter()
            .map(|temp| TempStatus {
                name: temp.name.clone(),
                temp: temp.temp,
            })
            .collect();
        (status_channels, temps)
    }

    async fn extract_load_status(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
        let mut channels = vec![];
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Load {
                continue;
            }
            let load =
                tokio::fs::read_to_string(driver.path.join("device").join("gpu_busy_percent"))
                    .await
                    .and_then(fans::check_parsing_8)
                    .unwrap_or(0);
            channels.push(ChannelStatus {
                name: channel.name.clone(),
                duty: Some(f64::from(load)),
                ..Default::default()
            });
        }
        channels
    }

    async fn reset_amd_to_default(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let amd_hwmon_info = self
            .amd_device_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let channel_info = amd_hwmon_info
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| format!("Searching for channel name: {channel_name}"))?;
        fans::set_pwm_enable_to_default(&amd_hwmon_info.path, channel_info).await
    }

    async fn set_amd_duty(
        &self,
        device_uid: &UID,
        channel_name: &str,
        fixed_speed: u8,
    ) -> Result<()> {
        let amd_hwmon_info = self
            .amd_device_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let channel_info = amd_hwmon_info
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| "Searching for channel name")?;
        fans::set_pwm_duty(&amd_hwmon_info.path, channel_info, fixed_speed).await
    }

    async fn initialize_amd_devices(&mut self) -> Result<()> {
        for (index, amd_driver) in Self::init_amd_devices().await.into_iter().enumerate() {
            let id = index as u8 + 1;
            let mut channels = HashMap::new();
            for channel in &amd_driver.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Fan => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
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
                    HwmonChannelType::Load => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Freq => {
                        let label_base = channel
                            .label
                            .as_ref()
                            .map(|l| l.to_title_case())
                            .unwrap_or_else(|| channel.name.to_title_case());
                        let channel_info = ChannelInfo {
                            label: Some(format!("{GPU_FREQ_NAME} {label_base}")),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    _ => continue,
                }
            }
            let amd_status = self.get_amd_status(&amd_driver).await;
            self.amd_preloaded_statuses
                .write()
                .await
                .insert(id, amd_status.clone());
            let temps = amd_driver
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
                            label: format!("{GPU_TEMP_NAME} {label_base}"),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut device = Device::new(
                amd_driver.name.clone(),
                DeviceType::GPU,
                id,
                None,
                DeviceInfo {
                    temps,
                    channels,
                    temp_max: 100,
                    model: amd_driver.model.clone(),
                    ..Default::default()
                },
                Some(amd_driver.u_id.clone()),
            );
            let status = Status {
                channels: amd_status.0,
                temps: amd_status.1,
                ..Default::default()
            };
            device.initialize_status_history_with(status);
            let cc_device_setting = self.config.get_cc_settings_for_device(&device.uid).await?;
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!(
                    "Skipping disabled device: {} with UID: {}",
                    device.name, device.uid
                );
                continue; // skip loading this device into the device list
            }
            self.amd_device_infos
                .insert(device.uid.clone(), Arc::new(amd_driver.clone()));
            self.devices
                .insert(device.uid.clone(), Arc::new(RwLock::new(device)));
        }
        Ok(())
    }

    async fn initialize_nvidia_devices(&mut self) -> Result<()> {
        if self.gpu_type_count.get(&GpuType::Nvidia).unwrap_or(&0) == &0 {
            return Ok(()); // skip if no Nvidia devices detected
        }
        let starting_nvidia_index = if self.has_multiple_gpus {
            self.gpu_type_count.get(&GpuType::AMD).unwrap_or(&0) + 1
        } else {
            1
        };
        let nvidia_devices = if self.nvml_active {
            self.gpus_nvidia
                .retrieve_nvml_devices(starting_nvidia_index)
                .await?
        } else {
            self.gpus_nvidia
                .init_nvidia_smi_devices(starting_nvidia_index)
                .await?
        };
        self.devices.extend(nvidia_devices);
        Ok(())
    }

    /// This function is specifically written for multithreaded concurrency
    async fn load_nvml_status(self: Arc<Self>, tasks: &mut Vec<JoinHandle<()>>) {
        for (uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.read().await.type_index;
                let self_ref = Arc::clone(&self);
                let nv_info = Arc::clone(nv_info);
                let join_handle = tokio::task::spawn(async move {
                    let nvml_status = self_ref.gpus_nvidia.request_nvml_status(nv_info).await;
                    self_ref
                        .gpus_nvidia
                        .nvidia_preloaded_statuses
                        .write()
                        .await
                        .insert(
                            type_index,
                            StatusNvidiaDeviceSMI {
                                temps: nvml_status.temps,
                                channels: nvml_status.channels,
                                ..Default::default()
                            },
                        );
                });
                tasks.push(join_handle);
            }
        }
    }

    /// This function is specifically written for multithreaded concurrency
    fn load_nvidia_smi_status(self: Arc<Self>, tasks: &mut Vec<JoinHandle<()>>) {
        let join_handle = tokio::task::spawn(async move {
            let mut nv_status_map = HashMap::new();
            for nv_status in self.gpus_nvidia.try_request_nv_smi_statuses().await {
                nv_status_map.insert(nv_status.index, nv_status);
            }
            for (uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
                if let Some(device_lock) = self.devices.get(uid) {
                    let type_index = device_lock.read().await.type_index;
                    if let Some(nv_status) = nv_status_map.remove(&nv_info.gpu_index) {
                        self.gpus_nvidia
                            .nvidia_preloaded_statuses
                            .write()
                            .await
                            .insert(type_index, nv_status);
                    } else {
                        error!("GPU Index not found in Nvidia status response");
                    }
                }
            }
        });
        tasks.push(join_handle);
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
        self.initialize_amd_devices().await?;
        self.initialize_nvidia_devices().await?;
        let mut init_devices = HashMap::new();
        for (uid, device) in &self.devices {
            init_devices.insert(uid.clone(), device.read().await.clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized GPU Devices: {:?}", init_devices);
            info!("Initialized AMD HwmonInfos: {:?}", self.amd_device_infos);
        } else {
            info!(
                "Initialized GPU Devices: {:?}",
                init_devices
                    .iter()
                    .map(|d| d.1.name.clone())
                    .collect::<Vec<String>>()
            );
        }
        trace!(
            "Time taken to initialize all GPU devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("GPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();

        let mut tasks = Vec::new();
        for (uid, amd_driver) in &self.amd_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.read().await.type_index;
                let self = Arc::clone(&self);
                let amd_driver = Arc::clone(amd_driver);
                let join_handle = tokio::task::spawn(async move {
                    let statuses = self.get_amd_status(&amd_driver).await;
                    self.amd_preloaded_statuses
                        .write()
                        .await
                        .insert(type_index, statuses);
                });
                tasks.push(join_handle);
            }
        }
        if self.nvml_active {
            self.load_nvml_status(&mut tasks).await;
        } else {
            let self = Arc::clone(&self);
            self.load_nvidia_smi_status(&mut tasks);
        }
        for task in tasks {
            if let Err(err) = task.await {
                error!("{}", err);
            }
        }
        trace!(
            "STATUS PRELOAD Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (uid, amd_driver) in &self.amd_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let preloaded_statuses_map = self.amd_preloaded_statuses.read().await;
                let preloaded_statuses =
                    preloaded_statuses_map.get(&device_lock.read().await.type_index);
                if preloaded_statuses.is_none() {
                    error!(
                        "There is no status preloaded for this AMD device: {}",
                        device_lock.read().await.type_index
                    );
                    continue;
                }
                let (channels, temps) = preloaded_statuses.unwrap().clone();
                let status = Status {
                    temps,
                    channels,
                    ..Default::default()
                };
                trace!("Device: {} status updated: {:?}", amd_driver.name, status);
                device_lock.write().await.set_status(status);
            }
        }
        self.gpus_nvidia.update_all_statuses().await;
        trace!(
            "STATUS SNAPSHOT Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (uid, device_lock) in &self.devices {
            let is_amd = self.amd_device_infos.contains_key(uid);
            if is_amd {
                for channel_name in device_lock.read().await.info.channels.keys() {
                    self.reset_amd_to_default(uid, channel_name).await.ok();
                }
            }
        }
        self.gpus_nvidia.reset_devices().await;
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        debug!(
            "Applying GPU device: {} channel: {}; Resetting to Automatic fan control",
            device_uid, channel_name
        );
        let is_amd = self.amd_device_infos.contains_key(device_uid);
        if is_amd {
            self.reset_amd_to_default(device_uid, channel_name).await
        } else {
            self.gpus_nvidia.reset_device(device_uid).await
        }
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        debug!(
            "Applying GPU device: {} channel: {}; Fixed Speed: {}",
            device_uid, channel_name, speed_fixed
        );
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {}", speed_fixed));
        }
        let is_amd = self.amd_device_infos.contains_key(device_uid);
        if is_amd {
            self.set_amd_duty(device_uid, channel_name, speed_fixed)
                .await
        } else {
            self.gpus_nvidia.set_fan_duty(device_uid, speed_fixed).await
        }
    }

    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Speed Profiles are not supported for GPU devices"
        ))
    }

    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Speed Profiles are not supported for GPU devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying LCD settings are not supported for GPU devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying pwm modes are not supported for GPU devices"
        ))
    }
}
