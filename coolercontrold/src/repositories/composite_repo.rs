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

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, info};
use tokio::sync::RwLock;
use tokio::time::Instant;
use crate::config::Config;

use crate::device::{Device, DeviceInfo, DeviceType, Status, TempStatus, UID};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

const AVG_ALL: &str = "Average All";

type AllTemps = Vec<(String, f64, u8)>;

/// A Repository for Composite Temperatures of other respositories
pub struct CompositeRepo {
    config: Arc<Config>,
    composite_device: DeviceLock,
    other_devices: DeviceList,
    should_compose: bool,
    liquid_temp_names: Vec<String>,
}

impl CompositeRepo {
    pub fn new(config: Arc<Config>, devices_for_composite: DeviceList) -> Self {
        Self {
            config,
            composite_device: Arc::new(RwLock::new(Device::new(
                "Composite".to_string(),
                DeviceType::Composite,
                1,
                None,
                Some(DeviceInfo {
                    temp_min: 0,
                    temp_max: 100,
                    temp_ext_available: true,
                    profile_max_length: 21,
                    ..Default::default()
                }),
                None,
                None,
            ))),
            should_compose: devices_for_composite.len() > 1,
            other_devices: devices_for_composite,
            liquid_temp_names: vec!["Liquid".to_string(), "Water".to_string()],
        }
    }

    async fn collect_all_temps(&self) -> AllTemps {
        let mut all_temps = Vec::new();
        for device in self.other_devices.iter() {
            let type_index = device.read().await.type_index;
            let current_status = &device.read().await.status_current();
            if let Some(status) = current_status {
                for temp_status in &status.temps {
                    all_temps.push((temp_status.external_name.clone(), temp_status.temp, type_index));
                }
            }
        }
        all_temps
    }

    fn get_avg_all_temps(&self, all_temps: &AllTemps) -> Vec<TempStatus> {
        let total_all_temps: f64 = all_temps.iter()
            .map(|(_, temp, _)| temp)
            .sum();
        let average = (total_all_temps / all_temps.len() as f64 * 100.0).round() / 100.0;
        let temp_status = TempStatus {
            name: AVG_ALL.to_string(),
            temp: average,
            frontend_name: AVG_ALL.to_string(),
            external_name: AVG_ALL.to_string(),
        };
        vec![temp_status]
    }

    fn get_delta_cpu_liquid_temps(&self, all_temps: &AllTemps) -> Vec<TempStatus> {
        let mut deltas = Vec::new();
        let cpu_temps = all_temps.iter()
            .filter(|(external_name, _, _)| external_name.contains("CPU"))
            .collect::<Vec<&(String, f64, u8)>>();
        all_temps.iter()
            .filter(|(name, _, _)|
                self.liquid_temp_names.iter().any(|liquid_temp_name| name.contains(liquid_temp_name))
            ).for_each(|(liquid_name, liquid_temp, _)| {
            for (cpu_name, cpu_temp, _) in &cpu_temps {
                let delta_temp_name = format!("Δ {} {}", cpu_name, liquid_name);
                deltas.push(
                    TempStatus {
                        name: delta_temp_name.clone(),
                        temp: ((cpu_temp - liquid_temp).abs() * 100.0).round() / 100.0,
                        frontend_name: delta_temp_name.clone(),
                        external_name: delta_temp_name,
                    }
                );
            }
        });
        deltas
    }

    fn get_delta_gpu_liquid_temps(&self, all_temps: &AllTemps) -> Vec<TempStatus> {
        let mut deltas = Vec::new();
        let gpu_temps = all_temps.iter()
            .filter(|(external_name, _, _)| external_name.contains("GPU"))
            .collect::<Vec<&(String, f64, u8)>>();
        all_temps.iter()
            .filter(|(name, _, _)|
                self.liquid_temp_names.iter().any(|liquid_temp_name| name.contains(liquid_temp_name))
            ).for_each(|(liquid_name, liquid_temp, _)| {
            for (gpu_name, gpu_temp, _) in &gpu_temps {
                let delta_temp_name = format!("Δ {} {}", gpu_name, liquid_name);
                deltas.push(
                    TempStatus {
                        name: delta_temp_name.clone(),
                        temp: ((gpu_temp - liquid_temp).abs() * 100.0).round() / 100.0,
                        frontend_name: delta_temp_name.clone(),
                        external_name: delta_temp_name,
                    }
                );
            }
        });
        deltas
    }
}

#[async_trait]
impl Repository for CompositeRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Composite
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        let cc_device_setting = self.config.get_cc_settings_for_device(
            &self.composite_device.read().await.uid
        ).await?;
        if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
            info!("Skipping updates for disabled composite device with UID: {}", self.composite_device.read().await.uid);
            self.should_compose = false;
        }
        self.update_statuses().await?;
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Devices: {:#?}", self.composite_device.read().await);  // pretty output for easy reading
        } else {
            info!("Initialized Devices: {:?}", self.composite_device.read().await);
        }
        debug!(
            "Time taken to initialize COMPOSITE device: {:?}", start_initialization.elapsed()
        );
        info!("COMPOSITE Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        vec![self.composite_device.clone()]
    }

    /// For composite repos, there is no need to preload as other device statuses
    /// have already been updated.
    async fn preload_statuses(&self) {}

    async fn update_statuses(&self) -> Result<()> {
        if self.should_compose {
            let start_update = Instant::now();
            let all_temps = self.collect_all_temps().await;
            if all_temps.len() > 1 {
                let mut composite_temps = Vec::new();
                composite_temps.append(&mut self.get_avg_all_temps(&all_temps));
                composite_temps.append(&mut self.get_delta_cpu_liquid_temps(&all_temps));
                composite_temps.append(&mut self.get_delta_gpu_liquid_temps(&all_temps));
                self.composite_device.write().await.set_status(
                    Status {
                        temps: composite_temps,
                        ..Default::default()
                    }
                )
            }
            debug!(
                "STATUS SNAPSHOT Time taken for COMPOSITE device: {:?}",
                start_update.elapsed()
            );
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("COMPOSITE Repository shutdown");
        Ok(())
    }

    async fn apply_setting(&self, _device_uid: &UID, _setting: &Setting) -> Result<()> {
        Err(anyhow!("Applying settings is not supported for COMPOSITE devices"))
    }
}
