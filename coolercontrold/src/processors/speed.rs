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

use std::collections::HashMap;
use std::ops::Not;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use tokio::sync::RwLock;

use crate::{AllDevices, utils};
use crate::config::Config;
use crate::device::{DeviceType, UID};
use crate::processors::{NormalizedProfile, Processor, ReposByType, SpeedProfileData};
use crate::processors::function_processors::{FunctionEMAPreProcessor, FunctionIdentityPreProcessor};
use crate::processors::profile_postprocessors::DutyThresholdPostProcessor;
use crate::processors::profile_processors::GraphProfileProcessor;
use crate::setting::{Function, FunctionType, Profile};

struct ProcessorCollection {
    fun_identity_pre: Arc<dyn Processor>,
    fun_ema_pre: Arc<dyn Processor>,
    graph_proc: Arc<dyn Processor>,
    duty_thresh_post: Arc<dyn Processor>,
}

/// This enables the use of a scheduler to automatically set the speed on devices in relation to
/// temperature sources that are not supported on the device itself.
/// For ex. Fan and Pump controls based on CPU Temp,
/// or profile speed settings for devices that only support fixed speeds.
pub struct SpeedProcessor {
    all_devices: AllDevices,
    repos: ReposByType,
    scheduled_settings: RwLock<HashMap<UID, HashMap<String, NormalizedProfile>>>,
    config: Arc<Config>,
    processors: ProcessorCollection,
}

impl SpeedProcessor {
    pub fn new(all_devices: AllDevices, repos: ReposByType, config: Arc<Config>) -> Self {
        Self {
            repos,
            scheduled_settings: RwLock::new(HashMap::new()),
            config,
            processors: ProcessorCollection {
                fun_identity_pre: Arc::new(FunctionIdentityPreProcessor::new(all_devices.clone())),
                fun_ema_pre: Arc::new(FunctionEMAPreProcessor::new(all_devices.clone())),
                graph_proc: Arc::new(GraphProfileProcessor::new()),
                duty_thresh_post: Arc::new(DutyThresholdPostProcessor::new(all_devices.clone())),
            },
            all_devices,
        }
    }

    pub async fn schedule_setting(&self, device_uid: &UID, channel_name: &str, profile: &Profile) -> Result<()> {
        if profile.temp_source.is_none() || profile.speed_profile.is_none() {
            return Err(anyhow!("Not enough info to schedule a manual speed profile"));
        }
        let temp_source = profile.temp_source.as_ref().unwrap();
        let temp_source_device = self.all_devices.get(temp_source.device_uid.as_str())
            .with_context(|| format!("temp_source Device must currently be present to schedule speed: {}", temp_source.device_uid))?;
        let max_temp = temp_source_device.read().await.info.as_ref().map_or(100, |info| info.temp_max) as f64;
        let device_to_schedule = self.all_devices.get(device_uid)
            .with_context(|| format!("Target Device to schedule speed must be present: {}", device_uid))?;
        let max_duty = device_to_schedule.read().await.info.as_ref()
            .with_context(|| format!("Device Info must be present for target device: {}", device_uid))?
            .channels.get(channel_name)
            .with_context(|| format!("Channel Info for channel: {} in setting must be present for target device: {}", channel_name, device_uid))?
            .speed_options.as_ref()
            .with_context(|| format!("Speed Options must be present for target device: {}", device_uid))?
            .max_duty;
        let function = if profile.function_uid.is_empty().not() {
            self.config.get_functions().await?.iter()
                .find(|fun| fun.uid == profile.function_uid)
                .with_context(|| "Function must be present in list of functions to schedule speed settings")?
                .clone()
        } else {
            // this is to handle legacy settings, where no profile_uid is set, but created for backwards compatibility:
            // Deprecated, to be removed later, once speed_profile no longer exists in the base settings
            let temp_source_device_type = temp_source_device.read().await.d_type.clone();
            if self.config.get_settings().await?.handle_dynamic_temps
                && (temp_source_device_type == DeviceType::CPU
                || temp_source_device_type == DeviceType::GPU
                || temp_source_device_type == DeviceType::Composite) {
                Function { f_type: FunctionType::ExponentialMovingAvg, ..Default::default() }
            } else {
                Function { f_type: FunctionType::Identity, ..Default::default() }
            }
        };
        let normalized_speed_profile = utils::normalize_profile(
            profile.speed_profile.as_ref().unwrap(),
            max_temp,
            max_duty,
        );
        let normalized_setting = NormalizedProfile {
            channel_name: channel_name.to_string(),
            p_type: profile.p_type.clone(),
            speed_profile: normalized_speed_profile,
            temp_source: temp_source.clone(),
            function,
            ..Default::default()
        };
        self.scheduled_settings.write().await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(channel_name.to_string(), normalized_setting);
        self.processors.duty_thresh_post.init_state(device_uid, channel_name).await;
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) = self.scheduled_settings.write().await.get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
        self.processors.duty_thresh_post.clear_state(device_uid, channel_name).await;
    }

    pub async fn update_speed(&self) {
        for (device_uid, channel_settings) in self.scheduled_settings.read().await.iter() {
            for (channel_name, normalized_profile) in channel_settings {
                self.process_speed_setting(device_uid, channel_name, normalized_profile).await;
            }
        }
    }

    async fn process_speed_setting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        normalized_profile: &NormalizedProfile,
    ) {
        let mut speed_profile_data = SpeedProfileData {
            temp: None,
            duty: None,
            profile: normalized_profile.clone(),
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_string(),
        };
        let duty_to_set = speed_profile_data
            .apply(&self.processors.fun_identity_pre).await
            .apply(&self.processors.fun_ema_pre).await
            .apply(&self.processors.graph_proc).await
            .apply(&self.processors.duty_thresh_post).await
            .return_processed_duty();
        if duty_to_set.is_none() {
            return;
        }
        self.set_speed(device_uid, channel_name, duty_to_set.unwrap()).await;
    }

    async fn set_speed(&self, device_uid: &UID, channel_name: &str, duty_to_set: u8) {
        // this will block if reference is held, thus clone()
        let device_type = self.all_devices[device_uid].read().await.d_type.clone();
        debug!(
            "Applying scheduled Speed Profile for device: {} channel: {}; DUTY: {}",
            device_uid, channel_name, duty_to_set
        );
        if let Some(repo) = self.repos.get(&device_type) {
            if let Err(err) = repo.apply_setting_speed_fixed(
                device_uid, channel_name, duty_to_set,
            ).await {
                error!("Error applying scheduled speed setting: {}", err);
            }
        }
    }
}
