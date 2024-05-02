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

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use const_format::concatcp;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::api::CCError;
use crate::config::{Config, DEFAULT_CONFIG_DIR};
use crate::device::{ChannelName, UID};
use crate::processing::settings::SettingsController;
use crate::setting::{Setting, DEFAULT_PROFILE_UID};
use crate::AllDevices;

const DEFAULT_MODE_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/modes.json");

/// The `ModeController` is responsible for managing mode snapshots of all the device settings and
/// applying them when appropriate.
pub struct ModeController {
    config: Arc<Config>,
    all_devices: AllDevices,
    settings_controller: Arc<SettingsController>,
    modes: RwLock<HashMap<UID, Mode>>,
    mode_order: RwLock<Vec<UID>>,
    active_mode: RwLock<Option<UID>>,
}

impl ModeController {
    /// Initializes the `ModeController` and fills it with data from the Mode configuration file.
    pub async fn init(
        config: Arc<Config>,
        all_devices: AllDevices,
        settings_controller: Arc<SettingsController>,
    ) -> Result<Self> {
        let mode_controller = Self {
            config,
            all_devices,
            settings_controller,
            modes: RwLock::new(HashMap::new()),
            mode_order: RwLock::new(Vec::new()),
            active_mode: RwLock::new(None),
        };
        mode_controller.fill_data_from_mode_config_file().await?;
        Ok(mode_controller)
    }

    /// Apply all saved device settings to the devices if the `apply_on_boot` setting is true
    pub async fn handle_settings_at_boot(&self) {
        if self
            .config
            .get_settings()
            .await
            .expect("config settings should be verified by this point")
            .apply_on_boot
        {
            self.apply_all_saved_device_settings().await;
            self.determine_active_mode().await;
        }
    }

    /// Apply all saved device settings to the devices
    pub async fn apply_all_saved_device_settings(&self) {
        info!("Applying all saved device settings");
        // we loop through all currently present devices so that we don't apply settings
        //  to devices that are no longer there.
        for uid in self.all_devices.keys() {
            match self.config.get_device_settings(uid).await {
                Ok(settings) => {
                    trace!(
                        "Settings for device: {} loaded from config file: {:?}",
                        uid,
                        settings
                    );
                    for setting in &settings {
                        if let Err(err) = self
                            .settings_controller
                            .set_config_setting(uid, setting)
                            .await
                        {
                            error!("Error setting device setting: {}", err);
                        }
                    }
                }
                Err(err) => error!(
                    "Error trying to read device settings from config file: {}",
                    err
                ),
            }
        }
    }

    /// Reads the Mode configuration file and fills the Modes `HashMap` and Mode Order Vec.
    async fn fill_data_from_mode_config_file(&self) -> Result<()> {
        let config_dir = Path::new(DEFAULT_CONFIG_DIR);
        if !config_dir.exists() {
            warn!(
                "config directory doesn't exist. Attempting to create it: {}",
                DEFAULT_CONFIG_DIR
            );
            tokio::fs::create_dir_all(&config_dir).await?;
        }
        let path = Path::new(DEFAULT_MODE_CONFIG_FILE_PATH).to_path_buf();
        let config_contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(_) => {
                info!("Writing a new Modes configuration file");
                let default_mode_config = serde_json::to_string(&ModeConfigFile {
                    modes: Vec::new(),
                    order: Vec::new(),
                })?;
                tokio::fs::write(&path, default_mode_config.into_bytes())
                    .await
                    .with_context(|| format!("Writing new configuration file: {path:?}"))?;
                // make sure the file is readable:
                tokio::fs::read_to_string(&path)
                    .await
                    .with_context(|| format!("Reading configuration file {path:?}"))?
            }
        };
        let mode_config: ModeConfigFile = serde_json::from_str(&config_contents)
            .with_context(|| format!("Parsing Mode configuration file {path:?}"))?;
        {
            let mut modes_lock = self.modes.write().await;
            modes_lock.clear();
            for mode in mode_config.modes {
                modes_lock.insert(mode.uid.clone(), mode);
            }
        }
        {
            let mut mode_order_lock = self.mode_order.write().await;
            mode_order_lock.clear();
            mode_order_lock.extend(mode_config.order);
        }
        Ok(())
    }

    pub async fn get_modes(&self) -> Vec<Mode> {
        let modes_lock = self.modes.read().await;
        self.mode_order
            .read()
            .await
            .iter()
            .filter_map(|uid| modes_lock.get(uid).cloned())
            .collect()
    }

    pub async fn get_mode(&self, mode_uid: &UID) -> Option<Mode> {
        self.modes.read().await.get(mode_uid).cloned()
    }

    /// Returns the currently active Mode.
    pub async fn get_active_mode_uid(&self) -> Option<UID> {
        self.determine_active_mode().await;
        self.active_mode.read().await.clone()
    }

    /// Determines the active mode and sets it.
    async fn determine_active_mode(&self) {
        let modes = self.modes.read().await;
        'modes: for (mode_uid, mode) in modes.iter() {
            'present_devices: for device_uid in self.all_devices.keys() {
                let Some(mode_channel_settings) = mode.all_device_settings.get(device_uid) else {
                    if self
                        .config
                        .get_device_settings(device_uid)
                        .await
                        .expect("config settings should be verified by this point")
                        .is_empty()
                    {
                        // there is no ModeSetting and no saved device settings for this device (NEW)
                        continue 'present_devices; // continue matching with the next device
                    } else {
                        // there is a setting for this device, but no ModeSetting
                        // the mode should be updated and will not be considered active
                        warn!(
                            "Mode contains no setting for device UID: {device_uid}. Please update your mode: {}.",
                            mode.name
                        );
                        continue 'modes;
                    }
                };
                let channel_settings = self
                    .config
                    .get_device_settings(device_uid)
                    .await
                    .expect("config settings should be verified by this point");
                for channel_setting in &channel_settings {
                    let Some(mode_channel_setting) =
                        mode_channel_settings.get(&channel_setting.channel_name)
                    else {
                        if channel_setting.profile_uid.is_some()
                            && channel_setting.profile_uid.as_ref().unwrap() == DEFAULT_PROFILE_UID
                        {
                            // if the Mode doesn't have anything set but the channel is set to
                            // the Default Profile, then it's technically a match. (none == default)
                            continue;
                        }
                        error!(
                            "The Mode doesn't contain a setting for the channel {} device UID: {}. Please update your mode: {}.",
                            channel_setting.channel_name, device_uid, mode.name
                        );
                        continue 'modes;
                    };
                    if channel_setting != mode_channel_setting {
                        // not a match for this channel, move on to next mode.
                        continue 'modes;
                    }
                }
            }
            // if we get here, all applicable device & channel settings are a match
            self.active_mode.write().await.replace(mode_uid.clone());
            debug!("Active mode determined: {}", mode.name);
            return;
        }
        self.active_mode.write().await.take();
        debug!("No mode is currently active");
    }

    /// Takes a Mode UID and applies all it's saved settings, making it the active Mode.
    pub async fn activate_mode(&self, mode_uid: &UID) -> Result<()> {
        let Some(mode) = self.modes.read().await.get(mode_uid).cloned() else {
            error!("Mode not found: {}", mode_uid);
            return Err(CCError::NotFound {
                msg: format!("Mode not found: {mode_uid}"),
            }
            .into());
        };
        if let Some(active_mode_uid) = self.active_mode.read().await.as_ref() {
            if active_mode_uid == mode_uid {
                debug!("Mode already active: {} ID:{mode_uid}", mode.name);
                return Ok(());
            }
        }
        for (device_uid, mode_device_settings) in &mode.all_device_settings {
            if self.all_devices.get(device_uid).is_none() {
                warn!(
                    "Mode: {} contains a setting for a device that isn't currently present. Device UID: {device_uid}",
                    mode.name
                );
                continue;
            }
            let saved_device_settings = self.config.get_device_settings(device_uid).await?;
            let saved_device_settings_map: HashMap<ChannelName, Setting> = saved_device_settings
                .into_iter()
                .map(|setting| (setting.channel_name.clone(), setting))
                .collect();
            for (channel_name, setting) in mode_device_settings {
                if let Some(saved_setting) = saved_device_settings_map.get(channel_name) {
                    if saved_setting == setting {
                        continue; // no need to apply if the setting is the same
                    }
                } // apply if there is no saved setting for this channel or setting is different
                if let Err(err) = self
                    .settings_controller
                    .set_config_setting(device_uid, setting)
                    .await
                {
                    error!("Error setting device setting: {}", err);
                }
                self.config.set_device_setting(device_uid, setting).await;
            }
        }
        self.config.save_config_file().await?;
        self.active_mode.write().await.replace(mode_uid.clone());
        debug!("Mode applied: {}", mode.name);
        Ok(())
    }

    /// Creates a new Mode with the given name and all current device settings.
    pub async fn create_mode(&self, name: String) -> Result<Mode> {
        if self.get_active_mode_uid().await.is_some() {
            return Err(CCError::UserError {
                msg: "A Mode already exists with these Device Settings. Please change your settings to create a new Mode.".to_string(),
            }
                .into());
        }
        let all_device_settings = self.get_all_device_settings().await?;
        let mode_uid = Uuid::new_v4().to_string();
        let mode = Mode {
            uid: mode_uid.clone(),
            name,
            all_device_settings,
        };
        {
            self.modes
                .write()
                .await
                .insert(mode_uid.clone(), mode.clone());
            self.mode_order.write().await.push(mode_uid);
        }
        self.save_modes_data().await?;
        Ok(mode)
    }

    /// Returns a Mode-style `HashMap` of all current device settings.
    async fn get_all_device_settings(&self) -> Result<HashMap<UID, HashMap<ChannelName, Setting>>> {
        let mut all_device_settings = HashMap::new();
        let all_current_device_settings = self.config.get_all_devices_settings().await?;
        for (device_uid, channel_settings) in all_current_device_settings {
            let mut channel_settings_map = HashMap::new();
            for setting in channel_settings {
                channel_settings_map.insert(setting.channel_name.clone(), setting);
            }
            all_device_settings.insert(device_uid.clone(), channel_settings_map);
        }
        Ok(all_device_settings)
    }

    /// Updates the Mode's name (currently)
    pub async fn update_mode(&self, mode_uid: &UID, name: String) -> Result<()> {
        {
            let mut modes_lock = self.modes.write().await;
            let mode = modes_lock
                .get_mut(mode_uid)
                .ok_or_else(|| CCError::NotFound {
                    msg: format!("Mode not found: {mode_uid}"),
                })?;
            mode.name = name;
        }
        self.save_modes_data().await?;
        Ok(())
    }

    /// Updates the Mode with the given UID with all current device settings.
    pub async fn update_mode_with_current_settings(&self, mode_uid: &UID) -> Result<Mode> {
        {
            if self.get_active_mode_uid().await.is_some() {
                return Err(CCError::UserError {
                    msg: "There is already a Mode with these Device Settings. Please change the settings or use that mode.".to_string(),
                }
                    .into());
            }
            let mut modes_lock = self.modes.write().await;
            let mode = modes_lock
                .get_mut(mode_uid)
                .ok_or_else(|| CCError::NotFound {
                    msg: format!("Mode not found: {mode_uid}"),
                })?;
            mode.all_device_settings = self.get_all_device_settings().await?;
        }
        self.save_modes_data().await?;
        let mode = self.modes.read().await.get(mode_uid).cloned().unwrap();
        Ok(mode)
    }

    /// Updates the Mode order with the given list of Mode UIDs.
    pub async fn update_mode_order(&self, mode_uids: Vec<UID>) -> Result<()> {
        {
            let mut mode_order_lock = self.mode_order.write().await;
            if mode_order_lock.len() != mode_uids.len() {
                return Err(CCError::UserError {
                    msg: "Mode order list length doesn't match the number of modes".to_string(),
                }
                .into());
            }
            mode_order_lock.clear();
            mode_order_lock.extend(mode_uids);
        }
        self.save_modes_data().await?;
        Ok(())
    }

    /// Deletes a mode from the `ModeController` with the given Mode UID.
    pub async fn delete_mode(&self, mode_uid: &UID) -> Result<()> {
        if self.modes.read().await.contains_key(mode_uid) {
            {
                self.modes.write().await.remove(mode_uid);
                self.mode_order.write().await.retain(|uid| uid != mode_uid);
            }
            self.save_modes_data().await?;
            Ok(())
        } else {
            Err(CCError::NotFound {
                msg: format!("Mode not found: {mode_uid}"),
            }
            .into())
        }
    }

    /// Saves the current Modes data to the Mode configuration file.
    async fn save_modes_data(&self) -> Result<()> {
        let modes = self.modes.read().await;
        let mode_order = self.mode_order.read().await;
        let mode_config = ModeConfigFile {
            modes: modes.values().cloned().collect(),
            order: mode_order.clone(),
        };
        let mode_config_json = serde_json::to_string(&mode_config)?;
        tokio::fs::write(DEFAULT_MODE_CONFIG_FILE_PATH, mode_config_json)
            .await
            .with_context(|| "Writing Modes Configuration File")?;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mode {
    pub uid: UID,
    pub name: String,
    pub all_device_settings: HashMap<UID, HashMap<ChannelName, Setting>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModeConfigFile {
    modes: Vec<Mode>,
    order: Vec<UID>,
}
