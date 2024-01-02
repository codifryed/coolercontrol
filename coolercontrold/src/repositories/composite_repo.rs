/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
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
 */

use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, info, trace};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{Device, DeviceInfo, DeviceType, Status, TempStatus, UID};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{LcdSettings, LightingSettings, TempSource};

const AVG_ALL: &str = "Average All";
const MAX_ALL: &str = "Max All";

type AllTemps = Vec<(String, f64, u8)>;

/// A Repository for Composite Temperatures of other repositories
//#[deprecated(since="0.18.0", note="To be removed after deployment of custom sensors")]
pub struct CompositeRepo {
    config: Arc<Config>,
    composite_device: Option<DeviceLock>,
    other_devices: DeviceList,
    liquid_temp_names: Vec<String>,
}

impl CompositeRepo {
    pub fn new(config: Arc<Config>, devices_for_composite: DeviceList) -> Self {
        Self {
            config,
            composite_device: None,
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

    fn get_max_all_temps(&self, all_temps: &AllTemps) -> Vec<TempStatus> {
        let max_all_temps: f64 = all_temps.iter()
            .map(|(_, temp, _)| temp)
            .max_by(|a, b| a.total_cmp(b))
            .map(|temp| temp.clone())
            .unwrap_or_default();
        let temp_status = TempStatus {
            name: MAX_ALL.to_string(),
            temp: max_all_temps,
            frontend_name: MAX_ALL.to_string(),
            external_name: MAX_ALL.to_string(),
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

    fn get_max_cpu_gpu_temps(&self, all_temps: &AllTemps) -> Vec<TempStatus> {
        let mut cpu_gpu_maximums = Vec::new();
        let base_cpu_temps = all_temps.iter()
            .filter(|(external_name, _, _)|
                external_name.contains("CPU") &&
                    (external_name.contains("Package") || external_name.contains("Tctl"))
            ).collect::<Vec<&(String, f64, u8)>>();
        all_temps.iter()
            .filter(|(external_name, _, _)| external_name.contains("GPU"))
            .for_each(|(gpu_name, gpu_temp, _)| {
                for (cpu_name, cpu_temp, _) in &base_cpu_temps {
                    let max_temp_name = format!("Max {} {}", cpu_name, gpu_name);
                    cpu_gpu_maximums.push(
                        TempStatus {
                            name: max_temp_name.clone(),
                            temp: cpu_temp.clone().max(gpu_temp.clone()),
                            frontend_name: max_temp_name.clone(),
                            external_name: max_temp_name,
                        }
                    );
                }
            });
        cpu_gpu_maximums
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
        let composite_device = Device::new(
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
        );
        let cc_device_setting = self.config.get_cc_settings_for_device(
            &composite_device.uid
        ).await?;
        if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
            info!("Skipping disabled composite device with UID: {}", composite_device.uid);
        } else if self.other_devices.len() > 1 {
            self.composite_device = Some(Arc::new(RwLock::new(composite_device)));
        }
        self.update_statuses().await?;
        if self.composite_device.is_some() {
            let recent_status = self.composite_device
            .as_ref()
            .unwrap()
            .read()
            .await
            .status_current()
            .unwrap();
            self.composite_device
            .as_ref()
            .unwrap()
            .write()
            .await
            .initialize_status_history_with(recent_status);
        }
        if log::max_level() == log::LevelFilter::Debug {
            if let Some(composite_device) = self.composite_device.as_ref() {
                info!("Initialized Composite Device: {:?}", composite_device.read().await);
            } else {
                info!("Initialized Composite Device: None");
            }
        }
        trace!(
            "Time taken to initialize COMPOSITE device: {:?}", start_initialization.elapsed()
        );
        debug!("COMPOSITE Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        if let Some(device) = self.composite_device.as_ref() {
            vec![device.clone()]
        } else {
            Vec::new()
        }
    }

    /// For composite repos, there is no need to preload as other device statuses
    /// have already been updated.
    async fn preload_statuses(self: Arc<Self>) {}

    async fn update_statuses(&self) -> Result<()> {
        if let Some(composite_device) = self.composite_device.as_ref() {
            let start_update = Instant::now();
            let all_temps = self.collect_all_temps().await;
            if all_temps.len() > 1 {
                let mut composite_temps = Vec::new();
                composite_temps.append(&mut self.get_avg_all_temps(&all_temps));
                composite_temps.append(&mut self.get_max_all_temps(&all_temps));
                composite_temps.append(&mut self.get_delta_cpu_liquid_temps(&all_temps));
                composite_temps.append(&mut self.get_delta_gpu_liquid_temps(&all_temps));
                composite_temps.append(&mut self.get_max_cpu_gpu_temps(&all_temps));
                composite_device.write().await.set_status(
                    Status {
                        temps: composite_temps,
                        ..Default::default()
                    }
                )
            }
            trace!(
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

    async fn apply_setting_reset(&self, _device_uid: &UID, _channel_name: &str) -> Result<()> {
        Ok(())
    }
    async fn apply_setting_speed_fixed(&self, _device_uid: &UID, _channel_name: &str, _speed_fixed: u8) -> Result<()> {
        Err(anyhow!("Applying settings Speed Fixed is not supported for COMPOSITE devices"))
    }
    async fn apply_setting_speed_profile(&self, _device_uid: &UID, _channel_name: &str, _temp_source: &TempSource, _speed_profile: &Vec<(f64, u8)>) -> Result<()> {
        Err(anyhow!("Applying settings Speed Profile is not supported for COMPOSITE devices"))
    }
    async fn apply_setting_lighting(&self, _device_uid: &UID, _channel_name: &str, _lighting: &LightingSettings) -> Result<()> {
        Err(anyhow!("Applying settings Lighting is not supported for COMPOSITE devices"))
    }
    async fn apply_setting_lcd(&self, _device_uid: &UID, _channel_name: &str, _lcd: &LcdSettings) -> Result<()> {
        Err(anyhow!("Applying settings LCD is not supported for COMPOSITE devices"))
    }
    async fn apply_setting_pwm_mode(&self, _device_uid: &UID, _channel_name: &str, _pwm_mode: u8) -> Result<()> {
        Err(anyhow!("Applying settings pwm_mode is not supported for COMPOSITE devices"))
    }
}
