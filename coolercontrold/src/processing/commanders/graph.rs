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

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use log::{debug, error};
use tokio::sync::RwLock;

use crate::config::Config;
use crate::device::{ChannelName, DeviceType, DeviceUID, Duty, UID};
use crate::processing::processors::functions::{
    FunctionDutyThresholdPostProcessor, FunctionEMAPreProcessor, FunctionIdentityPreProcessor,
    FunctionSafetyLatchProcessor, FunctionStandardPreProcessor,
};
use crate::processing::processors::profiles::GraphProcessor;
use crate::processing::{utils, NormalizedGraphProfile, Processor, ReposByType, SpeedProfileData};
use crate::repositories::repository::DeviceLock;
use crate::setting::{Function, FunctionType, FunctionUID, Profile, ProfileType};
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
    pub scheduled_settings:
        RwLock<HashMap<Arc<NormalizedGraphProfile>, HashSet<(DeviceUID, ChannelName)>>>,
    config: Arc<Config>,
    processors: ProcessorCollection,
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
        }
    }

    /// This is called on both the initial setting of Settings and when a Profile is updated
    pub async fn schedule_setting(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        profile: &Profile,
    ) -> Result<()> {
        if profile.p_type != ProfileType::Graph {
            return Err(anyhow!(
                "Only Graph Profiles are supported for scheduling in the GraphProfileCommander"
            ));
        }
        let normalized_profile_setting = self
            .normalize_profile_setting(device_uid, channel_name, profile)
            .await?;
        let device_channel = (device_uid.clone(), channel_name.to_string());
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
        }
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &DeviceUID, channel_name: &str) {
        // the mix commander will have multiple profiles for the same channel, so we need a Vec:
        let mut profiles_to_remove = Vec::new();
        let device_channel = (device_uid.clone(), channel_name.to_string());
        let mut scheduled_settings_lock = self.scheduled_settings.write().await;
        for (profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel);
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
                profiles_to_remove.push(Arc::clone(profile));
            }
        }
        for profile in profiles_to_remove {
            scheduled_settings_lock.remove(&profile);
        }
    }

    /// Processes and applies the speed of all devices that have a scheduled speed setting.
    /// Normally triggered by a loop/timer.
    pub async fn update_speeds(&self) {
        for (normalized_profile, device_channels) in self.scheduled_settings.read().await.iter() {
            let optional_duty_to_set = self.process_speed_setting(normalized_profile).await;
            if let Some(duty_to_set) = optional_duty_to_set {
                for (device_uid, channel_name) in device_channels {
                    self.set_speed(device_uid, channel_name, duty_to_set).await;
                }
            }
        }
    }

    pub async fn process_speed_setting<'a>(
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

    pub async fn set_speed(&self, device_uid: &UID, channel_name: &str, duty_to_set: u8) {
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
        let max_temp = temp_source_device
            .read()
            .await
            .info
            .as_ref()
            .map_or(100, |info| info.temp_max) as f64;
        let max_duty = self.get_max_device_duty(device_uid, channel_name).await?;
        let function = self
            .get_profiles_function(&profile.function_uid, temp_source_device)
            .await?;
        let normalized_speed_profile =
            utils::normalize_profile(profile.speed_profile.as_ref().unwrap(), max_temp, max_duty);
        Ok(NormalizedGraphProfile {
            profile_uid: profile.uid.clone(),
            speed_profile: normalized_speed_profile,
            temp_source: temp_source.clone(),
            function,
            ..Default::default()
        })
    }

    async fn get_max_device_duty(&self, device_uid: &UID, channel_name: &str) -> Result<Duty> {
        let device_to_schedule = self.all_devices.get(device_uid).with_context(|| {
            format!(
                "Target Device to schedule speed must be present: {}",
                device_uid
            )
        })?;
        let device_lock = device_to_schedule.read().await;
        let device_info = device_lock.info.as_ref().with_context(|| {
            format!(
                "Device Info must be present for target device: {}",
                device_uid
            )
        })?;
        let channel_info = device_info.channels.get(channel_name).with_context(|| {
            format!(
                "Channel Info for channel: {} in setting must be present for target device: {}",
                channel_name, device_uid
            )
        })?;
        let max_duty = channel_info
            .speed_options
            .as_ref()
            .with_context(|| {
                format!(
                    "Speed Options must be present for target device: {}",
                    device_uid
                )
            })?
            .max_duty;
        Ok(max_duty)
    }

    async fn get_profiles_function(
        &self,
        function_uid: &FunctionUID,
        temp_source_device: &DeviceLock,
    ) -> Result<Function> {
        if function_uid.is_empty() {
            // this is to handle legacy settings, where no profile_uid is set, but created for backwards compatibility:
            // Deprecated, to be removed later, once speed_profile no longer exists in the base settings
            let temp_source_device_type = temp_source_device.read().await.d_type.clone();
            let function = if self.config.get_settings().await?.handle_dynamic_temps
                && (temp_source_device_type == DeviceType::CPU
                    || temp_source_device_type == DeviceType::GPU
                    || temp_source_device_type == DeviceType::Composite)
            {
                Function {
                    f_type: FunctionType::ExponentialMovingAvg,
                    ..Default::default()
                }
            } else {
                Function {
                    f_type: FunctionType::Identity,
                    ..Default::default()
                }
            };
            return Ok(function);
        }
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
