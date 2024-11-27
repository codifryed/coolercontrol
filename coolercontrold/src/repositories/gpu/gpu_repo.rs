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
use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, error, info, trace, warn};
use moro_local::Scope;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{DeviceType, UID};
use crate::repositories::gpu::amd::GpuAMD;
use crate::repositories::gpu::nvidia::{GpuNVidia, StatusNvidiaDeviceSMI};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{LcdSettings, LightingSettings, TempSource};

pub const GPU_TEMP_NAME: &str = "GPU Temp";
pub const GPU_FREQ_NAME: &str = "GPU Freq";
pub const GPU_LOAD_NAME: &str = "GPU Load";
pub const COMMAND_TIMEOUT_DEFAULT: Duration = Duration::from_millis(800);
pub const COMMAND_TIMEOUT_FIRST_TRY: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    devices: HashMap<UID, DeviceLock>,
    gpu_type_count: HashMap<GpuType, u8>,
    gpus_nvidia: GpuNVidia,
    nvml_active: bool,
    gpus_amd: GpuAMD,
    force_nvidia_cli: bool,
}

impl GpuRepo {
    pub async fn new(config: Rc<Config>, nvidia_cli: bool) -> Result<Self> {
        Ok(Self {
            gpus_nvidia: GpuNVidia::new(Rc::clone(&config)),
            gpus_amd: GpuAMD::new(config),
            devices: HashMap::new(),
            gpu_type_count: HashMap::new(),
            nvml_active: false,
            force_nvidia_cli: nvidia_cli,
        })
    }

    async fn detect_gpu_types(&mut self) {
        let nvidia_dev_count = if self.force_nvidia_cli {
            self.gpus_nvidia
                .get_nvidia_smi_status(COMMAND_TIMEOUT_FIRST_TRY)
                .await
                .len() as u8
        } else if let Some(num_nvml_devices) = self.gpus_nvidia.init_nvml_devices().await {
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
            .insert(GpuType::AMD, self.gpus_amd.init_devices().await.len() as u8);
        let number_of_gpus = self.gpu_type_count.values().sum::<u8>();
        if number_of_gpus == 0 {
            warn!("No GPU Devices detected");
        }
    }

    pub async fn load_amd_statuses<'s>(self: &'s Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        for (uid, amd_driver) in &self.gpus_amd.amd_driver_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.borrow().type_index;
                scope.spawn(async move {
                    let statuses = self.gpus_amd.get_amd_status(amd_driver).await;
                    self.gpus_amd
                        .amd_preloaded_statuses
                        .borrow_mut()
                        .insert(type_index, statuses);
                });
            }
        }
    }

    async fn load_nvml_status<'s>(self: &'s Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        for (uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.borrow().type_index;
                scope.spawn(async move {
                    let nvml_status = self.gpus_nvidia.request_nvml_status(nv_info).await;
                    self.gpus_nvidia
                        .nvidia_preloaded_statuses
                        .borrow_mut()
                        .insert(
                            type_index,
                            StatusNvidiaDeviceSMI {
                                temps: nvml_status.temps,
                                channels: nvml_status.channels,
                                ..Default::default()
                            },
                        );
                });
            }
        }
    }

    fn load_nvidia_smi_status<'s>(self: Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        scope.spawn(async move {
            let mut nv_status_map = HashMap::new();
            for nv_status in self.gpus_nvidia.try_request_nv_smi_statuses().await {
                nv_status_map.insert(nv_status.index, nv_status);
            }
            for (uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
                if let Some(device_lock) = self.devices.get(uid) {
                    let type_index = device_lock.borrow().type_index;
                    if let Some(nv_status) = nv_status_map.remove(&nv_info.gpu_index) {
                        self.gpus_nvidia
                            .nvidia_preloaded_statuses
                            .borrow_mut()
                            .insert(type_index, nv_status);
                    } else {
                        error!("GPU Index not found in Nvidia status response");
                    }
                }
            }
        });
    }
}

#[async_trait(?Send)]
impl Repository for GpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::GPU
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.detect_gpu_types().await;
        let amd_devices = self.gpus_amd.initialize_amd_devices().await?;
        self.devices.extend(amd_devices);
        let has_nvidia_devices = self.gpu_type_count.get(&GpuType::Nvidia).unwrap_or(&0) > &0;
        if has_nvidia_devices {
            let starting_nvidia_index = self.gpu_type_count.get(&GpuType::AMD).unwrap_or(&0) + 1;
            let nvidia_devices = self
                .gpus_nvidia
                .initialize_nvidia_devices(starting_nvidia_index)
                .await?;
            self.devices.extend(nvidia_devices);
        };
        let mut init_devices = HashMap::new();
        for (uid, device) in &self.devices {
            init_devices.insert(uid.clone(), device.borrow().clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized GPU Devices: {:?}", init_devices);
        } else {
            let device_map: HashMap<_, _> = init_devices
                .iter()
                .map(|d| {
                    (
                        d.1.name.clone(),
                        HashMap::from([
                            (
                                "driver name",
                                vec![d.1.info.driver_info.name.clone().unwrap_or_default()],
                            ),
                            (
                                "driver version",
                                vec![d.1.info.driver_info.version.clone().unwrap_or_default()],
                            ),
                            ("locations", d.1.info.driver_info.locations.clone()),
                        ]),
                    )
                })
                .collect();
            info!(
                "Initialized GPU Devices: {}",
                serde_json::to_string(&device_map).unwrap_or_default()
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

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        let self_c = Rc::clone(&self);
        moro_local::async_scope!(|scope| {
            if self.devices.is_empty().not() {
                self_c.load_amd_statuses(scope).await;
                if self.nvml_active {
                    self_c.load_nvml_status(scope).await;
                } else {
                    self.load_nvidia_smi_status(scope);
                }
            }
        })
        .await;
        trace!(
            "STATUS PRELOAD Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        self.gpus_amd.update_all_statuses().await;
        self.gpus_nvidia.update_all_statuses().await;
        trace!(
            "STATUS SNAPSHOT Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.gpus_amd.reset_devices().await;
        self.gpus_nvidia.reset_devices().await;
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        debug!(
            "Applying GPU device: {} channel: {}; Resetting to Automatic fan control",
            device_uid, channel_name
        );
        let is_amd = self.gpus_amd.amd_driver_infos.contains_key(device_uid);
        if is_amd {
            self.gpus_amd
                .reset_amd_to_default(device_uid, channel_name)
                .await
        } else {
            self.gpus_nvidia
                .reset_device(device_uid, channel_name)
                .await
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
        let is_amd = self.gpus_amd.amd_driver_infos.contains_key(device_uid);
        if is_amd {
            self.gpus_amd
                .set_amd_duty(device_uid, channel_name, speed_fixed)
                .await
        } else {
            self.gpus_nvidia
                .set_fan_duty(device_uid, channel_name, speed_fixed)
                .await
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

    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }
}
