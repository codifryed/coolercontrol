/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

use crate::api::CCError;
use crate::config::{Config, DEFAULT_CONFIG_DIR};
use crate::device::{ChannelStatus, DeviceType, DeviceUID, Duty, Status, TempStatus, UID};
use crate::engine::commanders::graph::GraphProfileCommander;
use crate::engine::commanders::lcd::LcdCommander;
use crate::engine::commanders::mix::MixProfileCommander;
use crate::engine::commanders::overlay::OverlayProfileCommander;
use crate::engine::{processors, DeviceChannelProfileSetting};
use crate::repositories::repository::{DeviceLock, Repository};
use crate::setting::{
    FunctionType, FunctionUID, LcdSettings, LightingSettings, Profile, ProfileType, ProfileUID,
    Setting, DEFAULT_FUNCTION_UID,
};
use crate::{cc_fs, repositories, AllDevices, Repos};
use anyhow::{anyhow, Context, Result};
use chrono::Local;
use log::{error, info, trace};
use mime::Mime;
use moro_local::Scope;
use tokio::time::Instant;

const IMAGE_FILENAME_PNG: &str = "lcd_image.png";
const IMAGE_FILENAME_GIF: &str = "lcd_image.gif";
const SYNC_CHANNEL_NAME: &str = "sync";

pub type ReposByType = HashMap<DeviceType, Rc<dyn Repository>>;

pub struct Engine {
    all_devices: AllDevices,
    repos: ReposByType,
    config: Rc<Config>,
    graph_commander: Rc<GraphProfileCommander>,
    mix_commander: Rc<MixProfileCommander>,
    overlay_commander: Rc<OverlayProfileCommander>,
    pub lcd_commander: Rc<LcdCommander>,
}

impl Engine {
    pub fn new(all_devices: AllDevices, repos: &Repos, config: Rc<Config>) -> Self {
        let mut repos_by_type = HashMap::new();
        for repo in repos.iter() {
            match repo.device_type() {
                DeviceType::CPU => repos_by_type.insert(DeviceType::CPU, Rc::clone(repo)),
                DeviceType::GPU => repos_by_type.insert(DeviceType::GPU, Rc::clone(repo)),
                DeviceType::Liquidctl => {
                    repos_by_type.insert(DeviceType::Liquidctl, Rc::clone(repo))
                }
                DeviceType::Hwmon => repos_by_type.insert(DeviceType::Hwmon, Rc::clone(repo)),
                DeviceType::CustomSensors => {
                    repos_by_type.insert(DeviceType::CustomSensors, Rc::clone(repo))
                }
            };
        }
        let graph_commander = Rc::new(GraphProfileCommander::new(
            all_devices.clone(),
            repos_by_type.clone(),
            config.clone(),
        ));
        let mix_commander = Rc::new(MixProfileCommander::new(Rc::clone(&graph_commander)));
        let overlay_commander = Rc::new(OverlayProfileCommander::new(
            Rc::clone(&graph_commander),
            Rc::clone(&mix_commander),
        ));
        let lcd_commander = Rc::new(LcdCommander::new(
            all_devices.clone(),
            repos_by_type.clone(),
        ));
        Engine {
            all_devices,
            repos: repos_by_type,
            config,
            graph_commander,
            mix_commander,
            overlay_commander,
            lcd_commander,
        }
    }

    /// This is used to set the config Setting model configuration.
    /// Currently used at startup to apply saved settings and by Modes.
    pub async fn set_config_setting(&self, device_uid: &String, setting: &Setting) -> Result<()> {
        if let Some(true) = setting.reset_to_default {
            self.set_reset(device_uid, &setting.channel_name).await
        } else if setting.speed_fixed.is_some() {
            self.set_fixed_speed(
                device_uid,
                &setting.channel_name,
                setting.speed_fixed.unwrap(),
            )
            .await
        } else if setting.lighting.is_some() {
            self.set_lighting(
                device_uid,
                &setting.channel_name,
                setting.lighting.as_ref().unwrap(),
            )
            .await
        } else if setting.lcd.is_some() {
            self.set_lcd(
                device_uid,
                &setting.channel_name,
                setting.lcd.as_ref().unwrap(),
            )
            .await
        } else if setting.profile_uid.is_some() {
            self.set_profile(
                device_uid,
                &setting.channel_name,
                setting.profile_uid.as_ref().unwrap(),
            )
            .await
        } else {
            Err(anyhow!("Invalid Setting combination: {setting:?}"))
        }
    }

    fn get_device_repo(&self, device_uid: &UID) -> Result<(&DeviceLock, &Rc<dyn Repository>)> {
        if let Some(device_lock) = self.all_devices.get(device_uid) {
            let device_type = device_lock.borrow().d_type.clone();
            if let Some(repo) = self.repos.get(&device_type) {
                Ok((device_lock, repo))
            } else {
                Err(anyhow!(
                    "Repository: {:?} for device is currently not running!",
                    device_type
                ))
            }
        } else {
            Err(anyhow!("Device Not Found: {}", device_uid))
        }
    }

    pub async fn set_fixed_speed(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        speed_fixed: Duty,
    ) -> Result<()> {
        match self.get_device_repo(device_uid) {
            Ok((device_lock, repo)) => {
                self.overlay_commander
                    .clear_channel_setting_all_commanders(device_uid, channel_name);
                repo.apply_setting_manual_control(device_uid, channel_name)
                    .await?;
                repo.apply_setting_speed_fixed(device_uid, channel_name, speed_fixed)
                    .await
                    .inspect(|()| {
                        info!(
                            "Successfully applied:: {} | {channel_name} | Fixed Speed: {speed_fixed}",
                            device_lock.borrow().name
                        );
                    })
            }
            Err(err) => Err(err),
        }
    }

    pub async fn set_profile(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        profile_uid: &ProfileUID,
    ) -> Result<()> {
        let profile = self
            .config
            .get_profiles()
            .await?
            .iter()
            .find(|p| &p.uid == profile_uid)
            .with_context(|| "Profile should be present")?
            .clone();
        match profile.p_type {
            ProfileType::Default => self.set_reset(device_uid, channel_name).await,
            ProfileType::Fixed => {
                self.set_fixed_speed(
                    device_uid,
                    channel_name,
                    profile
                        .speed_fixed
                        .with_context(|| "Speed Fixed should be preset for Fixed Profiles")?,
                )
                .await
            }
            ProfileType::Graph => {
                self.set_graph_profile(device_uid, channel_name, &profile)
                    .await
            }
            ProfileType::Mix => {
                self.set_mix_profile(device_uid, channel_name, &profile)
                    .await
            }
            ProfileType::Overlay => {
                self.set_overlay_profile(device_uid, channel_name, &profile)
                    .await
            }
        }
        .inspect(|()| {
            info!(
                "Successfully applied:: {} | {channel_name} | Profile: {}",
                self.all_devices.get(device_uid).unwrap().borrow().name,
                profile.name
            );
        })
    }

    async fn set_graph_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        profile: &Profile,
    ) -> Result<()> {
        if profile.speed_profile.is_none() || profile.temp_source.is_none() {
            return Err(anyhow!(
                "Speed Profile and Temp Source must be present for a Graph Profile"
            ));
        }
        let (device_lock, repo) = self.get_device_repo(device_uid)?;
        let speed_options = device_lock
            .borrow()
            .info
            .channels
            .get(channel_name)
            .with_context(|| "Looking for Channel Info")?
            .speed_options
            .clone()
            .with_context(|| "Looking for Channel Speed Options")?;
        let temp_source = profile.temp_source.as_ref().unwrap();
        let profile_function = self
            .config
            .get_functions()
            .await?
            .into_iter()
            .find(|f| f.uid == profile.function_uid)
            .with_context(|| "Function should be present")?;
        // clear any profile setting for this channel first:
        self.overlay_commander
            .clear_channel_setting_all_commanders(device_uid, channel_name);
        // For internal temps, if the device firmware supports speed profiles and settings
        // match, let's use it: (device firmwares only support Identity Functions)
        if speed_options.auto_hw_curve
            && &temp_source.device_uid == device_uid
            && profile_function.f_type == FunctionType::Identity
        {
            repo.apply_setting_speed_profile(
                device_uid,
                channel_name,
                temp_source,
                profile.speed_profile.as_ref().unwrap(),
            )
            .await
        } else if speed_options.fixed_enabled {
            repo.apply_setting_manual_control(device_uid, channel_name)
                .await?;
            self.graph_commander.schedule_setting(
                DeviceChannelProfileSetting::Graph {
                    device_uid: device_uid.clone(),
                    channel_name: channel_name.to_string(),
                },
                profile,
            )
        } else {
            Err(anyhow!(
                "Speed Profiles not enabled for this device: {device_uid}"
            ))
        }
    }

    async fn set_mix_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        profile: &Profile,
    ) -> Result<()> {
        if profile.member_profile_uids.is_empty() {
            return Err(anyhow!("Mix Profile should have member profiles"));
        }
        if profile.mix_function_type.is_none() {
            return Err(anyhow!("Mix Profile should have a mix function type"));
        }
        let (device_lock, repo) = self.get_device_repo(device_uid)?;
        let speed_options = device_lock
            .borrow()
            .info
            .channels
            .get(channel_name)
            .with_context(|| "Looking for Channel Info")?
            .speed_options
            .clone()
            .with_context(|| "Looking for Channel Speed Options")?;
        let member_profiles = self
            .get_ordered_member_profiles(&profile.member_profile_uids)
            .await?;
        let all_function_uids = self
            .config
            .get_functions()
            .await?
            .into_iter()
            .map(|f| f.uid)
            .collect::<Vec<FunctionUID>>();
        let member_profile_functions_all_present = member_profiles
            .iter()
            .all(|p| all_function_uids.contains(&p.function_uid));
        if member_profile_functions_all_present.not() {
            return Err(anyhow!("All Member Profile Functions should be present"));
        }
        if speed_options.fixed_enabled {
            // clear any profile setting for this channel first:
            self.overlay_commander
                .clear_channel_setting_all_commanders(device_uid, channel_name);
            // This could potentially take significant time for slow devices:
            repo.apply_setting_manual_control(device_uid, channel_name)
                .await
                .inspect_err(|err| {
                    error!(
                        "Failed to enable manual control for Mix Profile: {}. \
                        Profile scheduling has been disabled for {channel_name} | {err}",
                        profile.name
                    );
                })?;
            self.mix_commander.schedule_setting(
                DeviceChannelProfileSetting::Mix {
                    device_uid: device_uid.clone(),
                    channel_name: channel_name.to_string(),
                },
                profile,
                member_profiles,
            )
        } else {
            Err(anyhow!(
                "Device Control not enabled for this device: {device_uid}"
            ))
        }
    }

    async fn set_overlay_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        profile: &Profile,
    ) -> Result<()> {
        if profile.member_profile_uids.len() != 1 {
            return Err(anyhow!("Overlay Profile should one member profile"));
        }
        if profile.offset_profile.is_none() {
            return Err(anyhow!("Overlay Profile should have an offset profile"));
        }
        if profile.offset_profile.as_ref().unwrap().is_empty() {
            return Err(anyhow!(
                "Overlay Profile offset profiles should have at least one duty/offset pair"
            ));
        }
        let (device_lock, repo) = self.get_device_repo(device_uid)?;
        let speed_options = device_lock
            .borrow()
            .info
            .channels
            .get(channel_name)
            .with_context(|| "Looking for Channel Info")?
            .speed_options
            .clone()
            .with_context(|| "Looking for Channel Speed Options")?;
        let member_profile = self
            .config
            .get_profiles()
            .await?
            .into_iter()
            .find(|p| profile.member_profile_uids.contains(&p.uid))
            .ok_or(anyhow!("Overlay Member Profile should be present"))?;
        let member_profile_members = self
            .get_ordered_member_profiles(&member_profile.member_profile_uids)
            .await?;
        if speed_options.fixed_enabled {
            // clear any profile setting for this channel first:
            self.overlay_commander
                .clear_channel_setting_all_commanders(device_uid, channel_name);
            // This could potentially take significant time for slow devices:
            repo.apply_setting_manual_control(device_uid, channel_name)
                .await
                .inspect_err(|err| {
                    error!(
                        "Failed to enable manual control for Overlay Profile: {}. \
                        Profile scheduling has been disabled for {channel_name} | {err}",
                        profile.name
                    );
                })?;
            self.overlay_commander.schedule_setting(
                DeviceChannelProfileSetting::Overlay {
                    device_uid: device_uid.clone(),
                    channel_name: channel_name.to_string(),
                },
                profile,
                &member_profile,
                member_profile_members,
            )
        } else {
            Err(anyhow!(
                "Device Control not enabled for this device: {device_uid}"
            ))
        }
    }
    /// Handles applying LCD Settings for all `LcdModes`.
    pub async fn set_lcd(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd_settings: &LcdSettings,
    ) -> Result<()> {
        let (device_lock, repo) = self.get_device_repo(device_uid)?;
        let lcd_not_enabled = device_lock
            .borrow()
            .info
            .channels
            .get(channel_name)
            .with_context(|| "Looking for Channel Info")?
            .lcd_modes
            .is_empty();
        if lcd_not_enabled {
            return Err(anyhow!(
                "LCD Screen modes not enabled for this device: {}",
                device_uid
            ));
        }
        let result = if lcd_settings.mode == "temp" {
            if lcd_settings.temp_source.is_none() {
                return Err(anyhow!(
                    "A Temp Source must be set when scheduling a LCD Temperature display for this device: {device_uid}"
                ));
            }
            self.lcd_commander
                .schedule_single_temp(device_uid, channel_name, lcd_settings)
        } else if lcd_settings.mode == "carousel" {
            self.lcd_commander
                .schedule_carousel(device_uid, channel_name, lcd_settings)
                .await
        } else {
            self.lcd_commander
                .clear_channel_setting(device_uid, channel_name);
            repo.apply_setting_lcd(device_uid, channel_name, lcd_settings)
                .await
        };
        result.inspect(|()| {
            info!(
                "Successfully applied:: {} | {channel_name} | LCD Mode: {}",
                device_lock.borrow().name,
                lcd_settings.mode
            );
        })
    }

    /// This function processes the image file for the specified device channel.
    pub async fn process_lcd_image(
        &self,
        device_uid: &String,
        channel_name: &str,
        files: &mut Vec<(Mime, Vec<u8>)>,
    ) -> Result<(Mime, Vec<u8>)> {
        let lcd_info = self
            .all_devices
            .get(device_uid)
            .ok_or_else(|| CCError::NotFound {
                msg: format!("Device with UID:{device_uid}"),
            })?
            .borrow()
            .info
            .channels
            .get(channel_name)
            .ok_or_else(|| CCError::NotFound {
                msg: format!("Channel info; UID:{device_uid}; Channel Name: {channel_name}"),
            })?
            .lcd_info
            .clone()
            .ok_or_else(|| CCError::NotFound {
                msg: format!("LCD INFO; UID:{device_uid}; Channel Name: {channel_name}"),
            })?;
        let (content_type, file_data) = files.pop().unwrap();
        processors::image::process_image(
            content_type,
            file_data,
            lcd_info.screen_width,
            lcd_info.screen_height,
        )
        .await
        .and_then(|(content_type, image_data)| {
            if image_data.len() > lcd_info.max_image_size_bytes as usize {
                Err(CCError::UserError {
                    msg: format!(
                        "Image file after processing still too large. Max Size: {}MBs",
                        lcd_info.max_image_size_bytes / 1_000_000
                    ),
                }
                .into())
            } else {
                Ok((content_type, image_data))
            }
        })
    }

    pub async fn save_lcd_image(&self, content_type: &Mime, file_data: Vec<u8>) -> Result<String> {
        let image_path = if content_type == &mime::IMAGE_GIF {
            std::path::Path::new(DEFAULT_CONFIG_DIR).join(IMAGE_FILENAME_GIF)
        } else {
            std::path::Path::new(DEFAULT_CONFIG_DIR).join(IMAGE_FILENAME_PNG)
        };
        cc_fs::write(&image_path, file_data).await?;
        let image_location = image_path
            .to_str()
            .map(ToString::to_string)
            .ok_or_else(|| CCError::InternalError {
                msg: "Path to str conversion".to_string(),
            })?;
        Ok(image_location)
    }

    /// Retrieves the saved image file
    pub async fn get_lcd_image(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<(Mime, Vec<u8>)> {
        let setting = self
            .config
            .get_device_channel_settings(device_uid, channel_name)?;
        let lcd_setting = setting.lcd.ok_or_else(|| CCError::NotFound {
            msg: "LCD Settings".to_string(),
        })?;
        let image_path = lcd_setting
            .image_file_processed
            .ok_or_else(|| CCError::NotFound {
                msg: "LCD Image File Path".to_string(),
            })?;
        let image_data = cc_fs::read_image(std::path::Path::new(&image_path))
            .await
            .map_err(|err| CCError::InternalError {
                msg: err.to_string(),
            })?;
        let content_type = if image_path.ends_with(IMAGE_FILENAME_GIF) {
            mime::IMAGE_GIF
        } else {
            mime::IMAGE_PNG
        };
        Ok((content_type, image_data))
    }

    pub async fn set_lighting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lighting_settings: &LightingSettings,
    ) -> Result<()> {
        let (device_lock, repo) = self.get_device_repo(device_uid)?;
        let lighting_channels = device_lock
            .borrow()
            .info
            .channels
            .iter()
            .filter(|&(_ch_name, ch_info)| ch_info.lighting_modes.is_empty().not())
            .map(|(ch_name, _ch_info)| ch_name.clone())
            .collect::<Vec<String>>();
        if lighting_channels.contains(&SYNC_CHANNEL_NAME.to_string()) {
            if channel_name == SYNC_CHANNEL_NAME {
                for ch in &lighting_channels {
                    if ch == SYNC_CHANNEL_NAME {
                        continue;
                    }
                    let reset_setting = Setting {
                        channel_name: ch.to_string(),
                        reset_to_default: Some(true),
                        ..Default::default()
                    };
                    self.config.set_device_setting(device_uid, &reset_setting);
                }
            } else {
                let reset_setting = Setting {
                    channel_name: SYNC_CHANNEL_NAME.to_string(),
                    reset_to_default: Some(true),
                    ..Default::default()
                };
                self.config.set_device_setting(device_uid, &reset_setting);
            }
        }
        repo.apply_setting_lighting(device_uid, channel_name, lighting_settings)
            .await
            .inspect(|()| {
                info!(
                    "Successfully applied:: {} | {channel_name} | Lighting Mode: {}",
                    device_lock.borrow().name,
                    lighting_settings.mode
                );
            })
    }

    pub async fn set_pwm_mode(
        &self,
        device_uid: &UID,
        channel_name: &str,
        pwm_mode: u8,
    ) -> Result<()> {
        match self.get_device_repo(device_uid) {
            Ok((_device_lock, repo)) => {
                repo.apply_setting_pwm_mode(device_uid, channel_name, pwm_mode)
                    .await
            }
            Err(err) => Err(err),
        }
    }

    pub async fn set_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        match self.get_device_repo(device_uid) {
            Ok((_device_lock, repo)) => {
                self.overlay_commander
                    .clear_channel_setting_all_commanders(device_uid, channel_name);
                self.lcd_commander
                    .clear_channel_setting(device_uid, channel_name);
                repo.apply_setting_reset(device_uid, channel_name).await
            }
            Err(err) => Err(err),
        }
    }

    /// Processes and applies the speed of all devices that have a scheduled setting.
    /// Normally triggered by a loop/timer.
    pub fn process_scheduled_speeds<'s>(&'s self, scope: &'s Scope<'s, 's, Result<()>>) {
        let start = Instant::now();
        self.graph_commander.process_all_profiles();
        self.mix_commander.process_all_profiles();
        self.overlay_commander.process_all_profiles();
        trace!(
            "Processing time taken for all profiles: {:?}",
            start.elapsed()
        );
        self.graph_commander.update_speeds(scope);
        self.mix_commander.update_speeds(scope);
        self.overlay_commander.update_speeds(scope);
        trace!("Update and Processing time taken: {:?}", start.elapsed());
    }

    /// This is used to reinitialize liquidctl devices after waking from sleep
    pub async fn reinitialize_devices(&self) {
        if let Some(liquidctl_repo) = self.repos.get(&DeviceType::Liquidctl) {
            liquidctl_repo.reinitialize_devices().await;
        }
    }

    /// This reinitialized the status history for all devices. This is helpful for example when
    /// waking from sleep, as the status history is no longer sequential.
    pub fn reinitialize_all_status_histories(&self) -> Result<()> {
        let poll_rate = self.config.get_settings()?.poll_rate;
        for (_uid, device) in self.all_devices.iter() {
            let most_recent_status = device.borrow().status_current().unwrap();
            let adjusted_recent_status = Status {
                // The next snapshot will most likely be after another loop interval:
                timestamp: Local::now(),
                temps: most_recent_status
                    .temps
                    .into_iter()
                    .map(|t| TempStatus { temp: 0.0, ..t })
                    .collect::<Vec<TempStatus>>(),
                channels: most_recent_status
                    .channels
                    .into_iter()
                    .map(|c| ChannelStatus {
                        rpm: if c.rpm.is_some() { Some(0) } else { None },
                        duty: if c.duty.is_some() { Some(0.0) } else { None },
                        ..c
                    })
                    .collect::<Vec<ChannelStatus>>(),
            };
            device
                .borrow_mut()
                .initialize_status_history_with(adjusted_recent_status, poll_rate);
        }
        Ok(())
    }

    pub async fn thinkpad_fan_control(&self, enable: &bool) -> Result<()> {
        repositories::utils::thinkpad_fan_control(enable)
            .await
            .map(|()| info!("Successfully enabled ThinkPad Fan Control"))
            .inspect_err(|err| error!("Error attempting to enable ThinkPad Fan Control: {err}"))
    }

    /// This function finds out if the give Profile UID is in use, and if so, updates
    /// the settings for those devices.
    pub async fn profile_updated(&self, profile_uid: &ProfileUID) {
        let affected_mix_profiles = self
            .get_profiles_affected_by(profile_uid, ProfileType::Mix)
            .await;
        let affected_overlay_profiles = self
            .get_profiles_affected_by(profile_uid, ProfileType::Overlay)
            .await;
        for (device_uid, _device) in self.all_devices.iter() {
            if let Ok(config_settings) = self.config.get_device_settings(device_uid) {
                for setting in config_settings {
                    if setting.profile_uid.is_none() {
                        continue;
                    }
                    let setting_profile_uid = setting.profile_uid.as_ref().unwrap();
                    if setting_profile_uid == profile_uid {
                        // If this device channel setting contains the updated Profile:
                        self.set_profile(device_uid, &setting.channel_name, profile_uid)
                            .await
                            .ok();
                    } else if affected_mix_profiles
                        .iter()
                        .any(|p| &p.uid == setting_profile_uid)
                    {
                        // otherwise, IF the device channel setting contains an affected Mix Profile
                        self.set_profile(device_uid, &setting.channel_name, setting_profile_uid)
                            .await
                            .ok();
                    } else if affected_overlay_profiles
                        .iter()
                        .any(|p| &p.uid == setting_profile_uid)
                    {
                        // otherwise, IF the device channel setting contains an affected Overlay Profile
                        self.set_profile(device_uid, &setting.channel_name, setting_profile_uid)
                            .await
                            .ok();
                    }
                }
            }
        }
    }

    /// This function finds out if the give Profile UID is in use, and if so resets
    /// the settings for those devices to the default profile.
    pub async fn profile_deleted(&self, profile_uid: &UID) -> Result<()> {
        let mut affected_mix_profiles = self
            .config
            .get_profiles()
            .await?
            .into_iter()
            .filter(|profile| {
                profile.p_type == ProfileType::Mix
                    && profile.member_profile_uids.contains(profile_uid)
            })
            .collect::<Vec<_>>();
        if self
            .config
            .get_profiles()
            .await?
            .into_iter()
            .any(|profile| {
                profile.p_type == ProfileType::Overlay
                    && profile.member_profile_uids.contains(profile_uid)
            })
        {
            return Err(CCError::UserError {
                msg: "In use by an Overlay Profile. Please remove from the overlay profile first"
                    .to_string(),
            }
            .into());
        }
        if affected_mix_profiles
            .iter()
            .any(|p| p.member_profile_uids.len() < 2)
        {
            return Err(CCError::UserError {
                msg: "Mix Profiles must have at least 1 member profiles".to_string(),
            }
            .into());
        }
        for mix_profile in &mut affected_mix_profiles {
            mix_profile
                .member_profile_uids
                .retain(|p_uid| p_uid != profile_uid);
            self.config.update_profile(mix_profile.clone())?;
        }
        for (device_uid, _device) in self.all_devices.iter() {
            if let Ok(config_settings) = self.config.get_device_settings(device_uid) {
                for setting in config_settings {
                    if setting.profile_uid.is_none() {
                        continue;
                    }
                    let setting_profile_uid = setting.profile_uid.as_ref().unwrap();
                    if setting_profile_uid == profile_uid {
                        let default_setting = Setting {
                            channel_name: setting.channel_name,
                            reset_to_default: Some(true),
                            ..Default::default()
                        };
                        self.config.set_device_setting(device_uid, &default_setting);
                        self.set_reset(device_uid, &default_setting.channel_name)
                            .await
                            .ok();
                    } else if affected_mix_profiles
                        .iter()
                        .any(|p| &p.uid == setting_profile_uid)
                    {
                        self.set_profile(device_uid, &setting.channel_name, setting_profile_uid)
                            .await?;
                    }
                }
            }
        }
        Ok(())
    }

    /// This function finds out if the given Function UID is in use, and if so updates
    /// the settings for those devices with the associated profile.
    pub async fn function_updated(&self, function_uid: &UID) {
        let affected_profiles = self
            .config
            .get_profiles()
            .await
            .unwrap_or(Vec::new())
            .into_iter()
            .filter(|profile| &profile.function_uid == function_uid)
            .collect::<Vec<Profile>>();
        if affected_profiles.is_empty() {
            return;
        }
        let affected_mix_profiles = self
            .config
            .get_profiles()
            .await
            .unwrap_or_else(|_| Vec::new())
            .into_iter()
            .filter(|profile| {
                profile.p_type == ProfileType::Mix
                    && profile
                        .member_profile_uids
                        .iter()
                        .any(|p_uid| affected_profiles.iter().any(|p| &p.uid == p_uid))
            })
            .collect::<Vec<_>>();
        for (device_uid, _device) in self.all_devices.iter() {
            if let Ok(config_settings) = self.config.get_device_settings(device_uid) {
                for setting in config_settings {
                    let Some(setting_profile_uid) = setting.profile_uid else {
                        continue;
                    };

                    if affected_profiles
                        .iter()
                        .chain(affected_mix_profiles.iter())
                        .any(|p| p.uid == setting_profile_uid)
                    {
                        self.set_profile(device_uid, &setting.channel_name, &setting_profile_uid)
                            .await
                            .ok();
                    }
                }
            }
        }
    }

    /// This function finds out if the given Function UID is in use, and if so resets
    /// the Function for those Profiles to the default Function (Identity).
    pub async fn function_deleted(&self, function_uid: &UID) {
        let mut affected_profiles = self
            .config
            .get_profiles()
            .await
            .unwrap_or_else(|_| Vec::new())
            .into_iter()
            .filter(|profile| &profile.function_uid == function_uid)
            .collect::<Vec<Profile>>();
        for profile in &mut affected_profiles {
            profile.function_uid = DEFAULT_FUNCTION_UID.to_string(); // the default function
            if let Err(err) = self.config.update_profile(profile.clone()) {
                error!("Error updating Profile: {profile:?} {err}");
                continue;
            }
            // This handles affected Mix Profiles:
            self.profile_updated(&profile.uid).await;
        }
    }

    pub async fn custom_sensor_deleted(
        &self,
        cs_device_uid: &str,
        custom_sensor_id: &str,
    ) -> Result<()> {
        let affects_profiles = self
            .config
            .get_profiles()
            .await
            .unwrap_or(Vec::new())
            .iter()
            .any(|profile| {
                profile.temp_source.is_some()
                    && profile.temp_source.as_ref().unwrap().temp_name == custom_sensor_id
            });
        let affects_lcd_settings =
            self.config
                .get_device_settings(cs_device_uid)?
                .iter()
                .any(|setting| {
                    setting.lcd.is_some()
                        && setting.lcd.as_ref().unwrap().temp_source.is_some()
                        && setting
                            .lcd
                            .as_ref()
                            .unwrap()
                            .temp_source
                            .as_ref()
                            .unwrap()
                            .device_uid
                            == cs_device_uid
                        && setting
                            .lcd
                            .as_ref()
                            .unwrap()
                            .temp_source
                            .as_ref()
                            .unwrap()
                            .temp_name
                            == custom_sensor_id
                });
        if affects_profiles || affects_lcd_settings {
            Err(CCError::UserError {
                msg: format!(
                    "Custom Sensor with ID:{custom_sensor_id} is being used by another setting.
                    Please remove the custom sensor from your settings before deleting."
                ),
            }
            .into())
        } else {
            Ok(())
        }
    }

    async fn get_ordered_member_profiles(
        &self,
        member_profile_uids: &Vec<UID>,
    ) -> Result<Vec<Profile>> {
        let mut all_profiles = self.config.get_profiles().await?;
        let mut member_profiles = Vec::new();
        for member_profile_uid in member_profile_uids {
            let Some(index) = all_profiles
                .iter()
                .position(|m| &m.uid == member_profile_uid)
            else {
                return Err(anyhow!(
                    "Member Profile UID: {member_profile_uid} could not be found!"
                ));
            };
            member_profiles.push(all_profiles.swap_remove(index));
        }
        Ok(member_profiles)
    }

    async fn get_profiles_affected_by(
        &self,
        changed_profile_uid: &UID,
        profile_type: ProfileType,
    ) -> Vec<Profile> {
        self.config
            .get_profiles()
            .await
            .unwrap_or_else(|_| Vec::new())
            .into_iter()
            .filter(|profile| {
                profile.p_type == profile_type
                    && profile.member_profile_uids.contains(changed_profile_uid)
            })
            .collect::<Vec<_>>()
    }
}
