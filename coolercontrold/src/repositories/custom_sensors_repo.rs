/*
 *  CoolerControl - monitor and control your cooling and other devices
 *  Copyright (c) 2023  Guy Boldon
 *
 *  This program is free software: you can redistribute it and/or modify
 *  it under the terms of the GNU General Public License as published by
 *  the Free Software Foundation, either version 3 of the License, or
 *  (at your option) any later version.
 *
 *  This program is distributed in the hope that it will be useful,
 *  but WITHOUT ANY WARRANTY; without even the implied warranty of
 *  MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 *  GNU General Public License for more details.
 *
 *  You should have received a copy of the GNU General Public License
 *  along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::api::CCError;
use crate::config::Config;
use crate::device::{Device, DeviceInfo, DeviceType, Status, TempStatus, UID};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{
    CustomSensor, CustomSensorMixFunctionType, CustomSensorType, LcdSettings, LightingSettings,
    TempSource,
};

type CustomSensors = Arc<RwLock<Vec<CustomSensor>>>;

/// A Repository for Custom Sensors defined by the user
pub struct CustomSensorsRepo {
    config: Arc<Config>,
    custom_sensor_device: Option<DeviceLock>,
    all_devices: HashMap<UID, DeviceLock>,
    sensors: CustomSensors,
}

impl CustomSensorsRepo {
    pub async fn new(config: Arc<Config>, all_other_devices: DeviceList) -> Self {
        let mut all_devices = HashMap::new();
        for device in all_other_devices.into_iter() {
            let uid = device.read().await.uid.clone();
            all_devices.insert(uid, device);
        }
        Self {
            config,
            custom_sensor_device: None,
            all_devices,
            sensors: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn get_device_uid(&self) -> UID {
        self.custom_sensor_device
            .as_ref()
            .expect("Custom Sensor Device should always be present after initialization")
            .read()
            .await
            .uid
            .clone()
    }

    pub async fn get_custom_sensor(&self, custom_sensor_id: &str) -> Result<CustomSensor> {
        self.sensors
            .read()
            .await
            .iter()
            .find(|cs| cs.id == custom_sensor_id)
            .map(|cs| cs.clone())
            .ok_or_else(|| {
                CCError::NotFound {
                    msg: format!("Custom Sensor not found: {}", custom_sensor_id),
                }
                .into()
            })
    }

    pub async fn get_custom_sensors(&self) -> Result<Vec<CustomSensor>> {
        Ok(self.sensors.read().await.clone())
    }

    pub async fn set_custom_sensors_order(&self, custom_sensors: &Vec<CustomSensor>) -> Result<()> {
        self.config.set_custom_sensor_order(custom_sensors).await?;
        self.sensors.write().await.clear();
        self.sensors.write().await.extend(custom_sensors.clone());
        Ok(())
    }

    pub async fn set_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        self.config.set_custom_sensor(custom_sensor.clone()).await?;
        if let Err(err) = self
            .fill_status_history_for_new_sensor(&custom_sensor)
            .await
        {
            error!("Failed to fill status history for new Custom Sensor, removing sensor.: {err}");
            self.config.delete_custom_sensor(&custom_sensor.id).await?;
            return Err(err);
        }
        self.sensors.write().await.push(custom_sensor);
        Ok(())
    }

    pub async fn update_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        self.config
            .update_custom_sensor(custom_sensor.clone())
            .await?;
        let mut sensors = self.sensors.write().await;
        // find check is done in the config update
        let pos = sensors
            .iter()
            .position(|s| s.id == custom_sensor.id)
            .expect("Custom Sensor not found");
        sensors[pos] = custom_sensor;
        // temp status will be handled automatically on next status update
        Ok(())
    }

    pub async fn delete_custom_sensor(&self, custom_sensor_id: &str) -> Result<()> {
        self.config.delete_custom_sensor(custom_sensor_id).await?;
        Self::remove_status_history_for_sensor(&self, custom_sensor_id).await;
        self.sensors
            .write()
            .await
            .retain(|cs| cs.id != custom_sensor_id);
        Ok(())
    }

    /// The function `fill_status_history_for_new_sensor` updates the status history of the
    /// custom sensor device.
    ///
    /// Arguments:
    ///
    /// * `sensor`: The `sensor` parameter is of type `CustomSensor`, which is a struct representing a
    /// custom sensor.
    ///
    /// Returns: a `Result<()>`.
    async fn fill_status_history_for_new_sensor(&self, sensor: &CustomSensor) -> Result<()> {
        let mut status_history = self
            .custom_sensor_device
            .as_ref()
            .unwrap()
            .read()
            .await
            .status_history
            .clone();
        match sensor.cs_type {
            CustomSensorType::Mix => {
                for (index, status) in status_history.iter_mut().enumerate() {
                    let temp_status = self
                        .process_custom_sensor_data_mix_indexed(sensor, index)
                        .await?;
                    status.temps.push(temp_status);
                }
            }
        }
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .write()
            .await
            .status_history = status_history;
        Ok(())
    }

    /// The function `process_custom_sensor_data_mix_indexed` processes custom sensor data by retrieving
    /// temperature values from different sources and applying a mixing function to calculate a custom
    /// temperature value.
    ///
    /// Arguments:
    ///
    /// * `sensor`: A reference to a `CustomSensor` object, which contains information about the sensor
    /// and its sources.
    /// * `index`: The `index` parameter represents the index of the status history that you want to
    /// retrieve the temperature data from. It is used to access the temperature data at a specific
    /// point in time.
    ///
    /// Returns: a `Result<TempStatus>`.
    async fn process_custom_sensor_data_mix_indexed(
        &self,
        sensor: &CustomSensor,
        index: usize,
    ) -> Result<TempStatus> {
        let mut temp_data = Vec::new();
        for custom_temp_source_data in sensor.sources.iter() {
            let temp_source = &custom_temp_source_data.temp_source;
            if let Some(temp_source_device) = self.all_devices.get(&temp_source.device_uid) {
                let some_temp = temp_source_device
                    .read()
                    .await
                    .status_history
                    .get(index)
                    .and_then(|status| {
                        status
                            .temps
                            .iter()
                            .filter(|temp_status| temp_status.name == temp_source.temp_name)
                            .map(|temp_status| temp_status.temp)
                            .last()
                    });
                if let None = some_temp {
                    let msg = format!(
                        "Temp not found for Custom Sensor: {}:{}",
                        temp_source.device_uid, temp_source.temp_name
                    );
                    return Err(CCError::InternalError { msg }.into());
                }
                temp_data.push(TempData {
                    temp: some_temp.unwrap(),
                    weight: custom_temp_source_data.weight as f64,
                })
            }
        }
        if temp_data.is_empty() {
            temp_data.push(TempData {
                temp: 0.,
                weight: 1.,
            });
            debug!(
                "No temp data found for Custom Sensor: {}. Filling with zeros",
                sensor.id
            );
        }
        let custom_temp = match sensor.mix_function {
            CustomSensorMixFunctionType::Min => Self::process_mix_min(&temp_data),
            CustomSensorMixFunctionType::Max => Self::process_mix_max(&temp_data),
            CustomSensorMixFunctionType::Avg => Self::process_mix_avg(&temp_data),
            CustomSensorMixFunctionType::WeightedAvg => Self::process_mix_weighted_avg(&temp_data),
        };
        Ok(TempStatus {
            name: sensor.id.clone(),
            temp: custom_temp,
            frontend_name: sensor.id.to_title_case(),
            external_name: sensor.id.to_title_case(),
        })
    }

    /// The function processes current sensor data by mixing the current temperature values from
    /// different sources based on a specified mixing function.
    ///
    /// Arguments:
    ///
    /// * `sensor`: The `sensor` parameter is of type `&CustomSensor`, which is a reference to a
    /// `CustomSensor` object.
    ///
    /// Returns: an `TempStatus`
    async fn process_custom_sensor_data_mix_current(&self, sensor: &CustomSensor) -> TempStatus {
        let mut temp_data = Vec::new();
        for custom_temp_source_data in sensor.sources.iter() {
            let temp_source = &custom_temp_source_data.temp_source;
            if let Some(temp_source_device) = self.all_devices.get(&temp_source.device_uid) {
                let some_temp =
                    temp_source_device
                        .read()
                        .await
                        .status_current()
                        .and_then(|status| {
                            status
                                .temps
                                .iter()
                                .filter(|temp_status| temp_status.name == temp_source.temp_name)
                                .map(|temp_status| temp_status.temp)
                                .last()
                        });
                if let None = some_temp {
                    error!(
                        "Temp not found for Custom Sensor: {}:{}",
                        temp_source.device_uid, temp_source.temp_name
                    );
                    continue;
                }
                temp_data.push(TempData {
                    temp: some_temp.unwrap(),
                    weight: custom_temp_source_data.weight as f64,
                })
            }
        }
        if temp_data.is_empty() {
            temp_data.push(TempData {
                temp: 0.,
                weight: 1.,
            });
            debug!(
                "No temp data found for Custom Sensor: {}. Filling with zeros",
                sensor.id
            );
        }
        let custom_temp = match sensor.mix_function {
            CustomSensorMixFunctionType::Min => Self::process_mix_min(&temp_data),
            CustomSensorMixFunctionType::Max => Self::process_mix_max(&temp_data),
            CustomSensorMixFunctionType::Avg => Self::process_mix_avg(&temp_data),
            CustomSensorMixFunctionType::WeightedAvg => Self::process_mix_weighted_avg(&temp_data),
        };
        TempStatus {
            name: sensor.id.clone(),
            temp: custom_temp,
            frontend_name: sensor.id.clone(),
            external_name: sensor.id.clone(),
        }
    }

    fn process_mix_min(temp_data: &Vec<TempData>) -> f64 {
        temp_data.iter().fold(254., |acc, data| data.temp.min(acc))
    }

    fn process_mix_max(temp_data: &Vec<TempData>) -> f64 {
        temp_data.iter().fold(0., |acc, data| data.temp.max(acc))
    }

    fn process_mix_avg(temp_data: &Vec<TempData>) -> f64 {
        temp_data.iter().fold(0., |acc, data| acc + data.temp) / temp_data.len() as f64
    }

    fn process_mix_weighted_avg(temp_data: &Vec<TempData>) -> f64 {
        temp_data
            .iter()
            .fold(
                TempData {
                    temp: 0.,
                    weight: 0.,
                },
                |mut acc, data| {
                    let total_weight = acc.weight + data.weight;
                    acc.temp = (acc.temp * acc.weight + data.temp * data.weight) / total_weight;
                    acc.weight = total_weight;
                    acc
                },
            )
            .temp
    }

    async fn remove_status_history_for_sensor(&self, sensor_id: &str) {
        let mut status_history = self
            .custom_sensor_device
            .as_ref()
            .unwrap()
            .read()
            .await
            .status_history
            .clone();
        for status in status_history.iter_mut() {
            status
                .temps
                .retain(|temp_status| temp_status.name != sensor_id);
        }
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .write()
            .await
            .status_history = status_history;
    }
}

#[async_trait]
impl Repository for CustomSensorsRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::CustomSensors
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        let custom_sensor_device = Arc::new(RwLock::new(Device::new(
            "Custom Sensors".to_string(),
            DeviceType::CustomSensors,
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
        )));
        // not allowed to blacklist this device, otherwise things can get strange
        self.custom_sensor_device = Some(custom_sensor_device);
        self.config.update_deprecated_custom_sensor_temp_sources(&self.all_devices).await?;
        let custom_sensors = self.config.get_custom_sensors().await?;
        self.sensors.write().await.extend(custom_sensors);
        self.update_statuses().await?;
        let recent_status = self
            .custom_sensor_device
            .as_ref()
            .unwrap()
            .read()
            .await
            .status_current()
            .unwrap();
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .write()
            .await
            .initialize_status_history_with(recent_status);
        if log::max_level() == log::LevelFilter::Debug {
            info!(
                "Initialized Custom Sensors Device: {:?}",
                self.custom_sensor_device.as_ref().unwrap().read().await
            );
        } else {
            info!(
                "Initialized Custom Sensors: {:?}",
                self.sensors
                    .read()
                    .await
                    .iter()
                    .map(|d| d.id.clone())
                    .collect::<Vec<String>>()
            );
        }
        trace!(
            "Time taken to initialize CUSTOM_SENSORS device: {:?}",
            start_initialization.elapsed()
        );
        debug!("CUSTOM_SENSOR Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        if let Some(device) = self.custom_sensor_device.as_ref() {
            vec![device.clone()]
        } else {
            Vec::new()
        }
    }

    /// For composite/sensor repos, there is no need to preload as other device statuses
    /// have already been updated.
    async fn preload_statuses(self: Arc<Self>) {}

    async fn update_statuses(&self) -> Result<()> {
        if let None = self.custom_sensor_device {
            return Ok(());
        }
        let start_update = Instant::now();
        let mut custom_temps = Vec::new();
        for sensor in self.sensors.read().await.iter() {
            match sensor.cs_type {
                CustomSensorType::Mix => {
                    let temp_status = self.process_custom_sensor_data_mix_current(sensor).await;
                    custom_temps.push(temp_status)
                }
            }
        }
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .write()
            .await
            .set_status(Status {
                temps: custom_temps,
                ..Default::default()
            });
        trace!(
            "STATUS SNAPSHOT Time taken for CUSTOM_SENSORS device: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("CUSTOM_SENSORS Repository shutdown");
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
            "Applying settings Speed Fixed is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &Vec<(f64, u8)>,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings Speed Profile is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings Lighting is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings LCD is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings pwm_mode is not supported for CUSTOMER_SENSORS devices"
        ))
    }
}

struct TempData {
    temp: f64,
    weight: f64,
}
