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
use async_trait::async_trait;
use log::{error, info};
use mime::Mime;
use serde::{Deserialize, Serialize};

use crate::{AllDevices, Repos, thinkpad_utils};
use crate::api::CCError;
use crate::config::{Config, DEFAULT_CONFIG_DIR};
use crate::device::{DeviceType, UID};
use crate::processors::lcd::LcdProcessor;
use crate::processors::speed::SpeedProcessor;
use crate::repositories::repository::{DeviceLock, Repository};
use crate::setting::{Function, LcdSettings, LightingSettings, Profile, ProfileType, Setting, TempSource};

mod speed;
mod lcd;
mod function_processors;
mod profile_processors;
mod profile_postprocessors;
pub mod lcd_image;

const IMAGE_FILENAME_PNG: &'static str = "lcd_image.png";
const IMAGE_FILENAME_GIF: &'static str = "lcd_image.gif";
const SYNC_CHANNEL_NAME: &'static str = "sync";

pub type ReposByType = HashMap<DeviceType, Arc<dyn Repository>>;

pub struct SettingsProcessor {
    all_devices: AllDevices,
    repos: ReposByType,
    config: Arc<Config>,
    pub speed_processor: Arc<SpeedProcessor>,
    pub lcd_processor: Arc<LcdProcessor>,
}

impl SettingsProcessor {
    pub fn new(all_devices: AllDevices, repos: Repos, config: Arc<Config>) -> Self {
        let mut repos_by_type = HashMap::new();
        for repo in repos.iter() {
            match repo.device_type() {
                DeviceType::CPU => repos_by_type.insert(DeviceType::CPU, Arc::clone(repo)),
                DeviceType::GPU => repos_by_type.insert(DeviceType::GPU, Arc::clone(repo)),
                DeviceType::Liquidctl => repos_by_type.insert(DeviceType::Liquidctl, Arc::clone(repo)),
                DeviceType::Hwmon => repos_by_type.insert(DeviceType::Hwmon, Arc::clone(repo)),
                DeviceType::Composite => repos_by_type.insert(DeviceType::Composite, Arc::clone(repo)),
            };
        }
        let speed_processor = Arc::new(SpeedProcessor::new(
            all_devices.clone(),
            repos_by_type.clone(),
            config.clone(),
        ));
        let lcd_processor = Arc::new(LcdProcessor::new(
            all_devices.clone(),
            repos_by_type.clone(),
        ));
        SettingsProcessor { all_devices, repos: repos_by_type, config, speed_processor, lcd_processor }
    }

    /// This is used to set the config Setting model configuration.
    /// Currently used at startup to apply saved settings, and for the legacy endpoint
    pub async fn set_config_setting(&self, device_uid: &String, setting: &Setting) -> Result<()> {
        if let Some(true) = setting.reset_to_default {
            self.set_reset(device_uid, &setting.channel_name).await
        } else if setting.speed_fixed.is_some() {
            self.set_fixed_speed(device_uid, &setting.channel_name, setting.speed_fixed.unwrap()).await
        } else if setting.lighting.is_some() {
            self.set_lighting(device_uid, &setting.channel_name, setting.lighting.as_ref().unwrap()).await
        } else if setting.speed_profile.is_some() {
            let profile = Profile {
                uid: "".to_string(),
                p_type: ProfileType::Graph,
                name: "".to_string(),
                speed_profile: setting.speed_profile.clone(),
                temp_source: setting.temp_source.clone(),
                function_uid: "".to_string(),
                ..Default::default()
            };
            self.set_graph_profile(device_uid, &setting.channel_name, &profile).await
        } else if setting.lcd.is_some() {
            let lcd_settings = if setting.temp_source.is_some() {
                LcdSettings {
                    temp_source: setting.temp_source.clone(),
                    ..setting.lcd.clone().unwrap()
                }
            } else {
                setting.lcd.clone().unwrap()
            };
            self.set_lcd(device_uid, &setting.channel_name, &lcd_settings).await
        } else if setting.profile_uid.is_some() {
            self.set_profile(device_uid, &setting.channel_name, setting.profile_uid.as_ref().unwrap()).await
        } else {
            Err(anyhow!("Invalid Setting combination: {:?}", setting))
        }
    }

    async fn get_device_repo(&self, device_uid: &UID) -> Result<(&DeviceLock, &Arc<dyn Repository>)> {
        if let Some(device_lock) = self.all_devices.get(device_uid) {
            let device_type = device_lock.read().await.d_type.clone();
            if let Some(repo) = self.repos.get(&device_type) {
                Ok((device_lock, repo))
            } else {
                Err(anyhow!("Repository: {:?} for device is currently not running!", device_type))
            }
        } else {
            Err(anyhow!("Device Not Found: {}", device_uid))
        }
    }

    pub async fn set_fixed_speed(&self, device_uid: &UID, channel_name: &str, speed_fixed: u8) -> Result<()> {
        match self.get_device_repo(device_uid).await {
            Ok((_device_lock, repo)) => {
                self.speed_processor.clear_channel_setting(device_uid, channel_name).await;
                repo.apply_setting_speed_fixed(device_uid, channel_name, speed_fixed).await
            }
            Err(err) => Err(err)
        }
    }

    pub async fn set_profile(&self, device_uid: &UID, channel_name: &str, profile_uid: &UID) -> Result<()> {
        let profile = self.config.get_profiles().await?
            .iter().find(|p| &p.uid == profile_uid)
            .with_context(|| "Profile should be present")?
            .clone();
        match profile.p_type {
            ProfileType::Default => self.set_reset(device_uid, channel_name).await,
            ProfileType::Fixed => self.set_fixed_speed(
                device_uid, channel_name,
                profile.speed_fixed.with_context(|| "Speed Fixed should be preset for Fixed Profiles")?,
            ).await,
            ProfileType::Graph => self.set_graph_profile(device_uid, channel_name, &profile).await,
            ProfileType::Mix => Err(anyhow!("MIX Profiles not yet supported")),
        }
    }

    async fn set_graph_profile(&self, device_uid: &UID, channel_name: &str, profile: &Profile) -> Result<()> {
        if profile.speed_profile.is_none() || profile.temp_source.is_none() {
            return Err(anyhow!("Speed Profile and Temp Source must be present for a Graph Profile"));
        }
        match self.get_device_repo(device_uid).await {
            Ok((device_lock, repo)) => {
                let speed_options = device_lock.read().await
                    .info.as_ref().with_context(|| "Looking for Device Info")?
                    .channels.get(channel_name).with_context(|| "Looking for Channel Info")?
                    .speed_options.clone().with_context(|| "Looking for Channel Speed Options")?;
                let temp_source = profile.temp_source.as_ref().unwrap();
                if speed_options.profiles_enabled && &temp_source.device_uid == device_uid {
                    self.speed_processor.clear_channel_setting(device_uid, channel_name).await;
                    repo.apply_setting_speed_profile(
                        device_uid,
                        channel_name,
                        temp_source,
                        profile.speed_profile.as_ref().unwrap(),
                    ).await
                } else if
                (speed_options.manual_profiles_enabled && &temp_source.device_uid == device_uid)
                    || (speed_options.fixed_enabled && &temp_source.device_uid != device_uid) {
                    self.speed_processor.schedule_setting(device_uid, channel_name, profile).await
                } else {
                    Err(anyhow!("Speed Profiles not enabled for this device: {}", device_uid))
                }
            }
            Err(err) => Err(err)
        }
    }

    /// Sets LCD Settings for all LcdModes except Image.
    pub async fn set_lcd(&self, device_uid: &UID, channel_name: &str, lcd_settings: &LcdSettings) -> Result<()> {
        match self.get_device_repo(device_uid).await {
            Ok((device_lock, repo)) => {
                let lcd_not_enabled = device_lock.read().await
                    .info.as_ref().with_context(|| "Looking for Device Info")?
                    .channels.get(channel_name).with_context(|| "Looking for Channel Info")?
                    .lcd_modes.is_empty();
                if lcd_not_enabled {
                    return Err(anyhow!("LCD Screen modes not enabled for this device: {}", device_uid));
                }
                if lcd_settings.mode == "temp" {
                    if lcd_settings.temp_source.is_none() {
                        return Err(anyhow!("A Temp Source must be set when scheduling a LCD Temperature display for this device: {}", device_uid));
                    }
                    self.lcd_processor.schedule_setting(device_uid, channel_name, lcd_settings).await
                } else {
                    self.lcd_processor.clear_channel_setting(device_uid, channel_name).await;
                    repo.apply_setting_lcd(device_uid, channel_name, lcd_settings).await
                }
            }
            Err(err) => Err(err)
        }
    }

    /// This function processes the image file for the specified device channel.
    pub async fn process_lcd_images(
        &self,
        device_uid: &String,
        channel_name: &str,
        files: &mut Vec<(&Mime, Vec<u8>)>,
    ) -> Result<(Mime, Vec<u8>)> {
        let lcd_info =
            self.all_devices.get(device_uid)
                .ok_or_else(|| CCError::NotFound { msg: format!("Device with UID:{device_uid}") })?
                .read().await
                .info.clone().ok_or_else(|| CCError::NotFound { msg: format!("Device Info with UID:{device_uid}") })?
                .channels.get(channel_name).ok_or_else(|| CCError::NotFound { msg: format!("Channel info; UID:{device_uid}; Channel Name: {channel_name}") })?
                .lcd_info.clone().ok_or_else(|| CCError::NotFound { msg: format!("LCD INFO; UID:{device_uid}; Channel Name: {channel_name}") })?;
        let (content_type, file_data) = files.pop().unwrap();
        lcd_image::process_image(content_type, file_data, lcd_info.screen_width, lcd_info.screen_height).await
            .and_then(|(content_type, image_data)|
                if image_data.len() > lcd_info.max_image_size_bytes as usize {
                    Err(CCError::UserError {
                        msg: format!("Image file after processing still too large. Max Size: {}MBs", lcd_info.max_image_size_bytes / 1_000_000)
                    }.into())
                } else {
                    Ok((content_type, image_data))
                }
            )
    }

    pub async fn save_lcd_image(&self, content_type: &Mime, file_data: Vec<u8>) -> Result<String> {
        let image_path = if content_type == &mime::IMAGE_GIF {
            std::path::Path::new(DEFAULT_CONFIG_DIR).join(IMAGE_FILENAME_GIF)
        } else {
            std::path::Path::new(DEFAULT_CONFIG_DIR).join(IMAGE_FILENAME_PNG)
        };
        tokio::fs::write(&image_path, file_data).await?;
        let image_location = image_path.to_str()
            .map(|s| s.to_string())
            .ok_or_else(|| CCError::InternalError { msg: "Path to str conversion".to_string() })?;
        Ok(image_location)
    }

    /// Retrieves the saved image file
    pub async fn get_lcd_image(&self, device_uid: &UID, channel_name: &str) -> Result<(Mime, Vec<u8>)> {
        let setting = self.config.get_device_channel_settings(&device_uid, &channel_name).await?;
        let lcd_setting = setting.lcd.ok_or_else(|| CCError::NotFound { msg: "LCD Settings".to_string() })?;
        let image_path = lcd_setting.image_file_processed.ok_or_else(|| CCError::NotFound { msg: "LCD Image File Path".to_string() })?;
        let image_data = tokio::fs::read(std::path::Path::new(&image_path)).await
            .map_err(|err| CCError::InternalError { msg: err.to_string() })?;
        let content_type = if image_path.ends_with(IMAGE_FILENAME_GIF) {
            mime::IMAGE_GIF
        } else { mime::IMAGE_PNG };
        Ok((content_type, image_data))
    }

    pub async fn set_lighting(&self, device_uid: &UID, channel_name: &str, lighting_settings: &LightingSettings) -> Result<()> {
        match self.get_device_repo(device_uid).await {
            Ok((device_lock, repo)) => {
                let lighting_channels = device_lock.read().await
                    .info.as_ref().with_context(|| "Device Info")?
                    .channels.iter()
                    .filter_map(|(ch_name, ch_info)|
                        ch_info.lighting_modes.is_empty().not().then(|| ch_name.clone())
                    ).collect::<Vec<String>>();
                if lighting_channels.contains(&SYNC_CHANNEL_NAME.to_string()) {
                    if channel_name == SYNC_CHANNEL_NAME {
                        for ch in lighting_channels.iter() {
                            if ch == SYNC_CHANNEL_NAME {
                                continue;
                            }
                            let reset_setting = Setting {
                                channel_name: ch.to_string(),
                                reset_to_default: Some(true),
                                ..Default::default()
                            };
                            self.config.set_device_setting(device_uid, &reset_setting).await;
                        }
                    } else {
                        let reset_setting = Setting {
                            channel_name: SYNC_CHANNEL_NAME.to_string(),
                            reset_to_default: Some(true),
                            ..Default::default()
                        };
                        self.config.set_device_setting(device_uid, &reset_setting).await;
                    }
                }
                repo.apply_setting_lighting(device_uid, channel_name, lighting_settings).await
            }
            Err(err) => Err(err)
        }
    }

    pub async fn set_pwm_mode(&self, device_uid: &UID, channel_name: &str, pwm_mode: u8) -> Result<()> {
        match self.get_device_repo(device_uid).await {
            Ok((_device_lock, repo)) =>
                repo.apply_setting_pwm_mode(device_uid, channel_name, pwm_mode).await,
            Err(err) => Err(err)
        }
    }

    pub async fn set_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        match self.get_device_repo(device_uid).await {
            Ok((_device_lock, repo)) => {
                self.speed_processor.clear_channel_setting(device_uid, channel_name).await;
                self.lcd_processor.clear_channel_setting(device_uid, channel_name).await;
                repo.apply_setting_reset(device_uid, channel_name).await
            }
            Err(err) => Err(err)
        }
    }

    /// This is used to reinitialize liquidctl devices after waking from sleep
    pub async fn reinitialize_devices(&self) {
        if let Some(liquidctl_repo) = self.repos.get(&DeviceType::Liquidctl) {
            liquidctl_repo.reinitialize_devices().await;
        }
    }

    /// This clears the status history for all devices. This is helpful for example when
    /// waking from sleep, as the status history is no longer sequential.
    pub async fn clear_all_status_histories(&self) {
        for (_uid, device) in self.all_devices.iter() {
            device.write().await.status_history.clear()
        }
    }

    pub async fn thinkpad_fan_control(&self, enable: &bool) -> Result<()> {
        thinkpad_utils::thinkpad_fan_control(enable).await
            .map(|_| info!("Successfully enabled ThinkPad Fan Control"))
            .map_err(|err| {
                error!("Error attempting to enable ThinkPad Fan Control: {}", err);
                err
            })
    }

    /// This function finds out if the the give Profile UID is in use, and if so updates
    /// the settings for those devices.
    pub async fn profile_updated(&self, profile_uid: &UID) {
        for (device_uid, _device) in self.all_devices.iter() {
            if let Ok(config_settings) = self.config.get_device_settings(device_uid).await {
                for setting in config_settings {
                    if setting.profile_uid.is_none() || setting.profile_uid.as_ref().unwrap() != profile_uid {
                        continue;
                    }
                    self.set_profile(device_uid, &setting.channel_name, profile_uid).await.ok();
                }
            }
        }
    }

    /// This function finds out if the the give Profile UID is in use, and if so resets
    /// the settings for those devices to the default profile.
    pub async fn profile_deleted(&self, profile_uid: &UID) {
        for (device_uid, _device) in self.all_devices.iter() {
            if let Ok(config_settings) = self.config.get_device_settings(device_uid).await {
                for setting in config_settings {
                    if setting.profile_uid.is_none() || setting.profile_uid.as_ref().unwrap() != profile_uid {
                        continue;
                    }
                    self.set_reset(device_uid, &setting.channel_name).await.ok();
                }
            }
        }
    }

    /// This function finds out if the the give Function UID is in use, and if so updates
    /// the settings for those devices with the associated profile.
    pub async fn function_updated(&self, function_uid: &UID) {
        let affected_profiles = self.config.get_profiles().await
            .unwrap_or(Vec::new()).into_iter()
            .filter(|profile| &profile.function_uid == function_uid)
            .collect::<Vec<Profile>>();
        if affected_profiles.is_empty() {
            return;
        }
        for (device_uid, _device) in self.all_devices.iter() {
            if let Ok(config_settings) = self.config.get_device_settings(device_uid).await {
                for setting in config_settings {
                    if setting.profile_uid.is_none() ||
                        affected_profiles.iter()
                            .any(|profile| &profile.uid == setting.profile_uid.as_ref().unwrap())
                            .not() {
                        continue;
                    }
                    self.set_profile(device_uid, &setting.channel_name, &setting.profile_uid.unwrap()).await.ok();
                }
            }
        }
    }

    /// This function finds out if the the give Function UID is in use, and if so resets
    /// the Function for those Profiles to the default Function (Identity).
    pub async fn function_deleted(&self, function_uid: &UID) {
        let mut affected_profiles = self.config.get_profiles().await
            .unwrap_or(Vec::new()).into_iter()
            .filter(|profile| &profile.function_uid == function_uid)
            .collect::<Vec<Profile>>();
        for profile in affected_profiles.iter_mut() {
            profile.function_uid = "0".to_string(); // the default function
            if let Err(_) = self.config.set_profile(profile.clone()).await {
                continue;
            }
            self.profile_updated(&profile.uid).await;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct NormalizedProfile {
    channel_name: String,
    p_type: ProfileType,
    speed_profile: Vec<(f64, u8)>,
    temp_source: TempSource,
    function: Function,
    member_profiles: Vec<NormalizedProfile>,
}

impl Default for NormalizedProfile {
    fn default() -> Self {
        Self {
            channel_name: "".to_string(),
            p_type: ProfileType::Graph,
            speed_profile: Vec::new(),
            temp_source: TempSource { temp_name: "".to_string(), device_uid: "".to_string() },
            function: Default::default(),
            member_profiles: Vec::new(),
        }
    }
}

#[async_trait]
trait Processor: Send + Sync {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool;
    async fn init_state(&self, device_uid: &UID, channel_name: &str);
    async fn clear_state(&self, device_uid: &UID, channel_name: &str);
    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData;
}

struct SpeedProfileData {
    temp: Option<f64>,
    duty: Option<u8>,
    profile: NormalizedProfile,
    device_uid: UID,
    channel_name: String,
}

impl SpeedProfileData {
    async fn apply<'a>(&'a mut self, processor: &'a Arc<dyn Processor>) -> &mut Self {
        if processor.is_applicable(self).await {
            processor.process(self).await
        } else {
            self
        }
    }

    fn return_processed_duty(&self) -> Option<u8> {
        self.duty
    }

    // could use in future for special cases:
    // async fn apply_if(&mut self, processor: Arc<dyn Processor>, predicate: impl Fn(&Self) -> bool) -> Self {
    //     if predicate() {
    //         processor.process(self).await
    //     } else {
    //         self
    //     }
    // }
}
