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

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, error, info};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::{AllDevices, utils};
use crate::config::Config;
use crate::device::{DeviceType, UID};
use crate::settings_processor::ReposByType;
use crate::repositories::cpu_repo::{CPU_TEMP_BASE_LABEL_NAMES_ORDERED, CPU_TEMP_NAME};
use crate::repositories::gpu_repo::GPU_TEMP_NAME;
use crate::setting::Setting;

const MAX_SAMPLE_SIZE: usize = 20;
const APPLY_DUTY_THRESHOLD: u8 = 2;
const MAX_UNDER_THRESHOLD_COUNTER: usize = 5;
const MAX_UNDER_THRESHOLD_CURRENT_DUTY_COUNTER: usize = 2;

/// This enables the use of a scheduler to automatically set the speed on devices in relation to
/// temperature sources that are not supported on the device itself.
/// For ex. Fan and Pump controls based on CPU Temp,
/// or profile speed settings for devices that only support fixed speeds.
pub struct SpeedScheduler {
    all_devices: AllDevices,
    repos: ReposByType,
    scheduled_settings: RwLock<HashMap<UID, HashMap<String, Setting>>>,
    scheduled_settings_metadata: RwLock<HashMap<UID, HashMap<String, SettingMetadata>>>,
    config: Arc<Config>,
}

impl SpeedScheduler {
    pub fn new(all_devices: AllDevices, repos: ReposByType, config: Arc<Config>) -> Self {
        Self {
            all_devices,
            repos,
            scheduled_settings: RwLock::new(HashMap::new()),
            scheduled_settings_metadata: RwLock::new(HashMap::new()),
            config,
        }
    }

    pub async fn schedule_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
        if setting.temp_source.is_none() || setting.speed_profile.is_none() {
            return Err(anyhow!("Not enough info to schedule a manual speed profile"));
        }
        let temp_source = setting.temp_source.as_ref().unwrap();
        let temp_source_device = self.all_devices.get(temp_source.device_uid.as_str())
            .with_context(|| format!("temp_source Device must currently be present to schedule speed: {}", temp_source.device_uid))?;
        let max_temp = temp_source_device.read().await.info.as_ref().map_or(100, |info| info.temp_max) as f64;
        let device_to_schedule = self.all_devices.get(device_uid)
            .with_context(|| format!("Target Device to schedule speed must be present: {}", device_uid))?;
        let max_duty = device_to_schedule.read().await.info.as_ref()
            .with_context(|| format!("Device Info must be present for target device: {}", device_uid))?
            .channels.get(setting.channel_name.as_str())
            .with_context(|| format!("Channel Info for channel: {} in setting must be present for target device: {}", setting.channel_name, device_uid))?
            .speed_options.as_ref()
            .with_context(|| format!("Speed Options must be present for target device: {}", device_uid))?
            .max_duty;
        let normalized_profile = utils::normalize_profile(
            setting.speed_profile.as_ref().unwrap(),
            max_temp,
            max_duty,
        );
        let normalized_setting = Setting {
            channel_name: setting.channel_name.clone(),
            speed_profile: Some(normalized_profile),
            temp_source: Some(temp_source.clone()),
            ..Default::default()
        };
        self.scheduled_settings.write().await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(setting.channel_name.clone(), normalized_setting);
        self.scheduled_settings_metadata.write().await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(setting.channel_name.clone(), SettingMetadata::new());
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) = self.scheduled_settings.write().await.get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
        if let Some(device_channel_settings) = self.scheduled_settings_metadata.write().await.get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
    }

    pub async fn update_speed(&self) {
        debug!("SPEED SCHEDULER triggered");
        for (device_uid, channel_settings) in self.scheduled_settings.read().await.iter() {
            for (channel_name, scheduler_setting) in channel_settings {
                if scheduler_setting.temp_source.is_none() {
                    continue;
                }
                if let Some(current_source_temp) = self.get_source_temp(scheduler_setting).await {
                    let duty_to_set = utils::interpolate_profile(scheduler_setting.speed_profile.as_ref().unwrap(), current_source_temp);
                    if self.duty_is_above_threshold(device_uid, scheduler_setting, duty_to_set).await {
                        self.set_speed(device_uid, scheduler_setting, duty_to_set).await;
                    } else {
                        self.scheduled_settings_metadata.write().await.get_mut(device_uid).unwrap()
                            .get_mut(channel_name).unwrap().under_threshold_counter += 1;
                        debug!("Duty not above threshold to be applied to device. Skipping");
                        debug!("Last applied duties: {:?}",
                            self.scheduled_settings_metadata.read().await.get(device_uid).unwrap()
                            .get(channel_name).unwrap().last_manual_speeds_set)
                    }
                } else {
                    error!(
                        "Temp sensor name was not found in the Temp Source Device: {}",
                        scheduler_setting.temp_source.as_ref().expect("Is previously checked").temp_name
                    )
                }
            }
        }
    }

    async fn get_source_temp(&self, setting: &Setting) -> Option<f64> {
        if let Some(temp_source_device_lock) = self.all_devices
            .get(setting.temp_source.as_ref().unwrap().device_uid.as_str()) {
            let temp_source_device = temp_source_device_lock.read().await;
            let temp_source_temp_name = &setting.temp_source.as_ref().unwrap().temp_name;
            let mut temps = temp_source_device.status_history.iter().rev()
                // we only need the last (sample_size ) temps for EMA:
                .take(utils::SAMPLE_SIZE as usize)
                .flat_map(|status| status.temps.as_slice())
                .filter(|temp_status| &temp_status.name == temp_source_temp_name)
                .map(|temp_status| temp_status.temp)
                .collect::<Vec<f64>>();
            temps.reverse(); // re-order temps so last is last
            if temps.is_empty() {
                // Workaround for GPU Temp & CPU Temp backward compatibility (<0.16.0)
                //  As soon as the user sets the profile again in the UI this won't be used anymore
                let temp_source_temp_name = temp_source_temp_name.to_lowercase();
                if temp_source_temp_name.starts_with(&CPU_TEMP_NAME.to_lowercase())
                    || temp_source_temp_name.starts_with(&GPU_TEMP_NAME.to_lowercase()
                ) {
                    temps = temp_source_device.status_history.iter().rev()
                        .take(utils::SAMPLE_SIZE as usize)
                        .flat_map(|status| status.temps.as_slice())
                        .filter(|temp_status| {
                            let temp_status_lower = temp_status.name.to_lowercase();
                            // check for the previous temps only available for CPU and GPU:
                            temp_status_lower.starts_with(&temp_source_temp_name)
                                && (CPU_TEMP_BASE_LABEL_NAMES_ORDERED.iter()
                                // cpu & amdgpu with single temp:
                                .any(|base_label| temp_status_lower.contains(base_label))
                                // amdgpu with multiple temps
                                || temp_status_lower.contains("edge"))
                        })
                        .map(|temp_status| temp_status.temp)
                        .collect::<Vec<f64>>();
                    temps.reverse();
                }
                if temps.is_empty() {
                    return None;
                }
            }
            let temp_source_device_type = &temp_source_device.d_type;
            match self.config.get_settings().await {
                Ok(cooler_control_settings) => {
                    if cooler_control_settings.handle_dynamic_temps
                        // in the future this will be controllable by config settings:
                        && (temp_source_device_type == &DeviceType::CPU
                        || temp_source_device_type == &DeviceType::GPU
                        || temp_source_device_type == &DeviceType::Composite)
                    {
                        Some(utils::current_temp_from_exponential_moving_average(&temps))
                    } else {
                        Some(temps.last().unwrap().clone())
                    }
                }
                Err(err) => {
                    error!("Could not read CoolerControl configuration settings: {}", err);
                    None
                }
            }
        } else {
            error!("Temperature Source Device for Speed Scheduler is currently not present: {}",
                setting.temp_source.as_ref().unwrap().device_uid);
            None
        }
    }

    async fn duty_is_above_threshold(&self, device_uid: &UID, scheduler_setting: &Setting, duty_to_set: u8) -> bool {
        if self.scheduled_settings_metadata.read().await[device_uid][&scheduler_setting.channel_name]
            .last_manual_speeds_set.is_empty() {
            return true;
        }
        let last_duty = self.get_appropriate_last_duty(device_uid, scheduler_setting).await;
        let diff_to_last_duty = duty_to_set.abs_diff(last_duty);
        let under_threshold_counter = self.scheduled_settings_metadata.read()
            .await[device_uid][&scheduler_setting.channel_name]
            .under_threshold_counter;
        let threshold = if under_threshold_counter < MAX_UNDER_THRESHOLD_COUNTER {
            APPLY_DUTY_THRESHOLD
        } else { 0 };
        diff_to_last_duty > threshold
    }

    /// This either uses the last applied duty as a comparison or the actual current duty.
    /// This handles situations where the last applied duty is not what the actual duty is
    /// in some circumstances, such as some when external programs are also trying to manipulate the duty.
    /// There needs to be a delay here (currently 2 seconds), as the device's duty often doesn't change instantaneously.
    async fn get_appropriate_last_duty(&self, device_uid: &UID, scheduler_setting: &Setting) -> u8 {
        let metadata = &self.scheduled_settings_metadata.read()
            .await[device_uid][&scheduler_setting.channel_name];
        if metadata.under_threshold_counter < MAX_UNDER_THRESHOLD_CURRENT_DUTY_COUNTER {
            metadata.last_manual_speeds_set.back().unwrap().clone()  // already checked to exist
        } else {
            let current_duty = self.all_devices[device_uid].read().await
                .status_history.iter().rev()
                .flat_map(|status| status.channels.as_slice())
                .filter(|channel_status| channel_status.name == scheduler_setting.channel_name)
                .find_map(|channel_status| channel_status.duty);
            if let Some(duty) = current_duty {
                duty.round() as u8
            } else {
                metadata.last_manual_speeds_set.back().unwrap().clone()
            }
        }
    }

    async fn set_speed(&self, device_uid: &UID, scheduler_setting: &Setting, duty_to_set: u8) {
        let fixed_setting = Setting {
            channel_name: scheduler_setting.channel_name.clone(),
            speed_fixed: Some(duty_to_set),
            temp_source: scheduler_setting.temp_source.clone(),
            pwm_mode: scheduler_setting.pwm_mode.clone(),
            ..Default::default()
        };
        {
            let mut metadata_lock = self.scheduled_settings_metadata.write().await;
            let metadata = metadata_lock.get_mut(device_uid).unwrap()
                .get_mut(&scheduler_setting.channel_name).unwrap();
            metadata.last_manual_speeds_set.push_back(duty_to_set);
            metadata.under_threshold_counter = 0;
            if metadata.last_manual_speeds_set.len() > MAX_SAMPLE_SIZE {
                metadata.last_manual_speeds_set.pop_front();
            }
        }
        // this will block if reference is held, thus clone()
        let device_type = self.all_devices[device_uid].read().await.d_type.clone();
        info!("Applying scheduled speed setting for device: {}", device_uid);
        debug!("Applying scheduled speed setting: {:?}", fixed_setting);
        if let Some(repo) = self.repos.get(&device_type) {
            if let Err(err) = repo.apply_setting(device_uid, &fixed_setting).await {
                error!("Error applying scheduled speed setting: {}", err);
            }
        }
    }
}

/// This is used by the SpeedScheduler for help in deciding exactly when to apply a setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingMetadata {
    /// (internal use) the last duty speeds that we set manually. This keeps track of applied settings
    /// to not re-apply the same setting over and over again needlessly. eg: [20, 25, 30]
    #[serde(skip_serializing, skip_deserializing)]
    pub last_manual_speeds_set: VecDeque<u8>,

    /// (internal use) a counter to be able to know how many times the to-be-applied duty was under
    /// the apply-threshold. This helps mitigate issues where the duty is 1% off target for a long time.
    #[serde(skip_serializing, skip_deserializing)]
    pub under_threshold_counter: usize,
}

impl SettingMetadata {
    pub fn new() -> Self {
        Self {
            last_manual_speeds_set: VecDeque::with_capacity(MAX_SAMPLE_SIZE + 1),
            under_threshold_counter: 0,
        }
    }
}