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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use tokio::sync::RwLock;

use crate::config::Config;
use crate::device::{DeviceUID, Duty, UID};
use crate::processing::processors::functions::{
    FunctionDutyThresholdPostProcessor, FunctionEMAPreProcessor, FunctionIdentityPreProcessor,
    FunctionSafetyLatchProcessor, FunctionStandardPreProcessor,
};
use crate::processing::processors::profiles::GraphProcessor;
use crate::processing::settings::ReposByType;
use crate::processing::{
    utils, DeviceChannelProfileSetting, NormalizedGraphProfile, Processor, SpeedProfileData,
};
use crate::setting::{Function, FunctionUID, Profile, ProfileType, ProfileUID};
use crate::AllDevices;

struct ProcessorCollection {
    fun_safety_latch: Arc<dyn Processor>,
    fun_identity_pre: Arc<dyn Processor>,
    fun_ema_pre: Arc<dyn Processor>,
    fun_std_pre: Arc<dyn Processor>,
    graph_proc: Arc<dyn Processor>,
    fun_duty_thresh_post: Arc<dyn Processor>,
}

/// This is the commander for Graph Profile Processing.
/// This enables the use of a scheduler to automatically set the speed on devices in relation to
/// temperature sources that are not supported on the device itself.
/// For ex. Fan and Pump controls based on CPU Temp,
/// or profile speed settings for devices that only support fixed speeds.
pub struct GraphProfileCommander {
    all_devices: AllDevices,
    repos: ReposByType,
    scheduled_settings:
        RwLock<HashMap<Arc<NormalizedGraphProfile>, HashSet<DeviceChannelProfileSetting>>>,
    config: Arc<Config>,
    processors: ProcessorCollection,
    pub process_output_cache: RwLock<HashMap<ProfileUID, Option<Duty>>>,
}

impl GraphProfileCommander {
    pub fn new(all_devices: AllDevices, repos: ReposByType, config: Arc<Config>) -> Self {
        Self {
            repos,
            scheduled_settings: RwLock::new(HashMap::new()),
            config,
            processors: ProcessorCollection {
                fun_safety_latch: Arc::new(FunctionSafetyLatchProcessor::new()),
                fun_identity_pre: Arc::new(FunctionIdentityPreProcessor::new(all_devices.clone())),
                fun_ema_pre: Arc::new(FunctionEMAPreProcessor::new(all_devices.clone())),
                fun_std_pre: Arc::new(FunctionStandardPreProcessor::new(all_devices.clone())),
                graph_proc: Arc::new(GraphProcessor::new()),
                fun_duty_thresh_post: Arc::new(FunctionDutyThresholdPostProcessor::new()),
            },
            all_devices,
            process_output_cache: RwLock::new(HashMap::new()),
        }
    }

    /// This is called on both the initial setting of Settings and when a Profile is updated
    pub async fn schedule_setting(
        &self,
        device_channel: DeviceChannelProfileSetting,
        profile: &Profile,
    ) -> Result<()> {
        if profile.p_type != ProfileType::Graph {
            return Err(anyhow!(
                "Only Graph Profiles are supported for scheduling in the GraphProfileCommander"
            ));
        }
        let normalized_profile_setting = self
            .normalize_profile_setting(
                device_channel.device_uid(),
                device_channel.channel_name(),
                profile,
            )
            .await?;
        let mut settings_lock = self.scheduled_settings.write().await;
        if let Some(mut existing_device_channels) =
            settings_lock.remove(&normalized_profile_setting)
        {
            // We replace the existing NormalizedGraphProfile if it exists to make sure it's
            // internal settings are up-to-date
            existing_device_channels.insert(device_channel);
            settings_lock.insert(
                Arc::new(normalized_profile_setting),
                existing_device_channels,
            );
        } else {
            let mut new_device_channels = HashSet::new();
            new_device_channels.insert(device_channel);
            settings_lock.insert(Arc::new(normalized_profile_setting), new_device_channels);
            self.processors
                .fun_safety_latch
                .init_state(&profile.uid)
                .await;
            self.processors
                .fun_duty_thresh_post
                .init_state(&profile.uid)
                .await;
            self.processors.fun_std_pre.init_state(&profile.uid).await;
            self.process_output_cache
                .write()
                .await
                .insert(profile.uid.clone(), None);
        }
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &DeviceUID, channel_name: &str) {
        // the mix commander will have multiple profiles for the same channel, so we need a Vec:
        let mut profiles_to_remove = Vec::new();
        let device_channel_setting = DeviceChannelProfileSetting::Graph {
            // device_uid and channel_name are used to identify the setting, the
            // DeviceChannelProfileSetting variant is irrelevant for the hash.
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_string(),
        };
        let mut scheduled_settings_lock = self.scheduled_settings.write().await;
        for (profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel_setting);
            if device_channels.is_empty() {
                self.processors
                    .fun_safety_latch
                    .clear_state(&profile.profile_uid)
                    .await;
                self.processors
                    .fun_duty_thresh_post
                    .clear_state(&profile.profile_uid)
                    .await;
                self.processors
                    .fun_std_pre
                    .clear_state(&profile.profile_uid)
                    .await;
                self.process_output_cache
                    .write()
                    .await
                    .remove(&profile.profile_uid);
                profiles_to_remove.push(Arc::clone(profile));
            }
        }
        for profile in profiles_to_remove {
            scheduled_settings_lock.remove(&profile);
        }
    }

    /// This method processes all scheduled profiles and updates the output cache.
    /// This should be called first and only once per update cycle.
    pub async fn process_all_profiles(&self) {
        let mut output_cache_lock = self.process_output_cache.write().await;
        for normalized_profile in self.scheduled_settings.read().await.keys() {
            let optional_duty_to_set = self.process_speed_setting(normalized_profile).await;
            if let Some(cache) = output_cache_lock.get_mut(&normalized_profile.profile_uid) {
                *cache = optional_duty_to_set;
            }
        }
    }

    /// Applies the speed of all devices that have a scheduled Graph Profile setting.
    /// Normally triggered by a loop/timer.
    pub async fn update_speeds(&self) {
        for (normalized_profile, device_channels) in self.scheduled_settings.read().await.iter() {
            let optional_duty_to_set =
                &self.process_output_cache.read().await[&normalized_profile.profile_uid];
            let Some(duty_to_set) = optional_duty_to_set else {
                continue;
            };
            for device_channel in device_channels {
                if let DeviceChannelProfileSetting::Graph {
                    device_uid,
                    channel_name,
                } = device_channel
                {
                    self.set_device_speed(device_uid, channel_name, *duty_to_set)
                        .await;
                }
            }
        }
    }

    async fn process_speed_setting<'a>(
        &'a self,
        normalized_profile: &Arc<NormalizedGraphProfile>,
    ) -> Option<Duty> {
        SpeedProfileData {
            temp: None,
            duty: None,
            profile: Arc::clone(normalized_profile),
            processing_started: false,
            safety_latch_triggered: false,
        }
        .apply(&self.processors.fun_safety_latch)
        .await
        .apply(&self.processors.fun_identity_pre)
        .await
        .apply(&self.processors.fun_ema_pre)
        .await
        .apply(&self.processors.fun_std_pre)
        .await
        .apply(&self.processors.graph_proc)
        .await
        .apply(&self.processors.fun_duty_thresh_post)
        .await
        .apply(&self.processors.fun_safety_latch)
        .await
        .return_processed_duty()
    }

    pub async fn set_device_speed(&self, device_uid: &UID, channel_name: &str, duty_to_set: u8) {
        // this will block if reference is held, thus clone()
        let device_type = self.all_devices[device_uid].read().await.d_type.clone();
        debug!(
            "Applying scheduled Speed Profile for device: {} channel: {}; DUTY: {}",
            device_uid, channel_name, duty_to_set
        );
        if let Some(repo) = self.repos.get(&device_type) {
            if let Err(err) = repo
                .apply_setting_speed_fixed(device_uid, channel_name, duty_to_set)
                .await
            {
                error!("Error applying scheduled speed setting: {}", err);
            }
        }
    }

    async fn normalize_profile_setting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        profile: &Profile,
    ) -> Result<NormalizedGraphProfile> {
        if profile.temp_source.is_none() || profile.speed_profile.is_none() {
            return Err(anyhow!(
                "Not enough info to schedule a manual speed profile"
            ));
        }
        let temp_source = profile.temp_source.as_ref().unwrap();
        let temp_source_device = self
            .all_devices
            .get(temp_source.device_uid.as_str())
            .with_context(|| {
                format!(
                    "temp_source Device must currently be present to schedule speed: {}",
                    temp_source.device_uid
                )
            })?;
        let max_temp = f64::from(temp_source_device.read().await.info.temp_max);
        let max_duty = self.get_max_device_duty(device_uid, channel_name).await?;
        let function = self.get_profiles_function(&profile.function_uid).await?;
        let normalized_speed_profile =
            utils::normalize_profile(profile.speed_profile.as_ref().unwrap(), max_temp, max_duty);
        Ok(NormalizedGraphProfile {
            profile_uid: profile.uid.clone(),
            speed_profile: normalized_speed_profile,
            temp_source: temp_source.clone(),
            function,
        })
    }

    async fn get_max_device_duty(&self, device_uid: &UID, channel_name: &str) -> Result<Duty> {
        let device_to_schedule = self.all_devices.get(device_uid).with_context(|| {
            format!("Target Device to schedule speed must be present: {device_uid}")
        })?;
        let device_lock = device_to_schedule.read().await;
        let channel_info = device_lock.info.channels.get(channel_name).with_context(|| {
            format!(
                "Channel Info for channel: {channel_name} in setting must be present for target device: {device_uid}"
            )
        })?;
        let max_duty = channel_info
            .speed_options
            .as_ref()
            .with_context(|| {
                format!("Speed Options must be present for target device: {device_uid}")
            })?
            .max_duty;
        Ok(max_duty)
    }

    async fn get_profiles_function(&self, function_uid: &FunctionUID) -> Result<Function> {
        let function = self
            .config
            .get_functions()
            .await?
            .into_iter()
            .find(|fun| &fun.uid == function_uid)
            .with_context(|| {
                "Function must be present in list of functions to schedule speed settings"
            })?;
        Ok(function)
    }
}
