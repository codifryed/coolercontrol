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
use std::ops::Deref;

use anyhow::Result;
use async_trait::async_trait;
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::process::Command;
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::device::{ChannelStatus, Device, DeviceInfo, DeviceType, Status, TempStatus};
use crate::repositories::repository::Repository;
use crate::setting::Setting;

const GPU_TEMP_NAME: &str = "GPU Temp";
const GPU_LOAD_NAME: &str = "GPU Load";
const GPU_FAN_NAME: &str = "GPU Fan";
const DEFAULT_AMD_GPU_NAME: &str = "Radeon Graphics";


#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    devices: RwLock<Vec<Device>>,
    gpu_type_count: RwLock<HashMap<GpuType, u8>>,
    has_multiple_gpus: RwLock<bool>,
}

impl GpuRepo {
    pub async fn new() -> Result<Self> {
        Ok(Self {
            devices: RwLock::new(Vec::new()),
            gpu_type_count: RwLock::new(HashMap::new()),
            has_multiple_gpus: RwLock::new(false),
        })
    }

    async fn detect_gpu_types(&self) {
        {
            // todo: AMD (use hwmon, it has all we need)
            //  see: https://www.kernel.org/doc/html/v5.0/gpu/amdgpu.html#gpu-power-thermal-controls-and-monitoring
            let mut type_map = self.gpu_type_count.write().await;
            type_map.insert(GpuType::Nvidia, self.get_nvidia_status().await.len() as u8);
        }
        let number_of_gpus = self.gpu_type_count.read().await.values().sum::<u8>();
        let mut has_multiple_gpus = self.has_multiple_gpus.write().await;
        *has_multiple_gpus = number_of_gpus > 1;
        if number_of_gpus == 0 {
            warn!("No GPU Devices detected")
        }
    }

    async fn request_statuses(&self) -> Vec<(Status, String)> {
        let mut statuses = vec![];
        // todo: AMD
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
            let mut temps = vec![];
            let mut channels = vec![];
            if let Some(temp) = nvidia_status.temp {
                let gpu_temp_name_prefix = if has_multiple_gpus {
                    format!("#{} ", starting_gpu_index + (index as u8))
                } else {
                    "".to_string()
                };
                temps.push(
                    TempStatus {
                        name: GPU_TEMP_NAME.to_string(),
                        temp,
                        frontend_name: GPU_TEMP_NAME.to_string(),
                        external_name: gpu_temp_name_prefix + GPU_TEMP_NAME,
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
                        name: GPU_FAN_NAME.to_string(),
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
                    debug!("Nvidia raw status output: {}", out_str);
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
                    error!("Error communicating with nvidia-smi: {}", out_err)
                }
            }
            Err(err) => warn!("Nvidia driver not found: {}", err)
        }
        vec![]
    }
}

#[async_trait]
impl Repository for GpuRepo {
    async fn initialize_devices(&self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.detect_gpu_types().await;
        for (index, (status, gpu_name)) in self.request_statuses().await.iter().enumerate() {
            let mut device = Device {
                name: gpu_name.to_string(),
                d_type: DeviceType::GPU,
                type_id: (index + 1) as u8,
                info: Some(DeviceInfo {
                    temp_max: 100,
                    temp_ext_available: true,
                    ..Default::default()
                }),
                ..Default::default()
            };
            device.set_status(status.clone());
            self.devices.write().await.push(device);
        }
        debug!("Initialized Devices: {:?}", self.devices.read().await);
        debug!(
            "Time taken to initialize all GPU devices: {:?}", start_initialization.elapsed()
        );
        info!("All GPU devices initialized");
        Ok(())
    }

    async fn devices(&self) -> Vec<Device> {
        self.devices.read().await.deref().iter()
            .map(|device| device.clone())
            .collect()
    }

    async fn update_statuses(&self) -> Result<()> {
        debug!("Updating all GPU device statuses");
        let start_update = Instant::now();
        for (index, (status, gpu_name)) in self.request_statuses().await.iter().enumerate() {
            let mut devices = self.devices.write().await;
            if let Some(device) = devices.get_mut(index) {
                device.set_status(status.clone());
                debug!("Device: {} status updated: {:?}", gpu_name, status);
            }
        }
        debug!(
            "Time taken to get status for all GPU devices: {:?}",
            start_update.elapsed()
        );
        info!("All GPU device statuses updated");
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        self.devices.write().await.clear();
        debug!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting(&self, device_type_id: u8, setting: Setting) -> Result<()> {
        // todo: change nvidia fan
        // todo: amd? (is hwmon currently, but perhaps we move it in here (check the crates)
        todo!()
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