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
use std::ops::Not;
use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use const_format::concatcp;
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use uuid::Uuid;

use crate::api::CCError;
use crate::config::{Config, DEFAULT_CONFIG_DIR};
use crate::device::{ChannelName, DeviceUID, UID};
use crate::processing::settings::SettingsController;
use crate::setting::{ProfileUID, Setting, DEFAULT_PROFILE_UID};
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
    active_modes: RwLock<Vec<UID>>,
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
            active_modes: RwLock::new(Vec::new()),
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
            self.determine_active_modes().await;
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

    /// Returns the currently active Modes.
    pub async fn determine_active_modes_uids(&self) -> Vec<UID> {
        self.determine_active_modes().await;
        self.active_modes.read().await.clone()
    }

    /// Determines the active modes and sets them.
    async fn determine_active_modes(&self) {
        let mut active_modes = Vec::new();
        let modes = self.modes.read().await;
        'modes: for (mode_uid, mode) in modes.iter() {
            'currently_present_devices: for device_uid in self.all_devices.keys() {
                let current_channel_settings =
                    self.config.get_device_settings(device_uid).await.unwrap();
                if mode.all_device_settings.contains_key(device_uid).not() {
                    if current_channel_settings.is_empty() {
                        // No ModeSetting and no saved device settings for this device, ignore.
                        continue 'currently_present_devices;
                    }
                    // There are applied settings for this device, but no ModeSetting present.
                    debug!(
                        "Mode {} contains no setting for device UID: {device_uid}.",
                        mode.name
                    );
                    continue 'modes;
                };
                let mode_channel_settings = mode.all_device_settings.get(device_uid).unwrap();
                if mode_channel_settings.iter().any(|(channel_name, setting)| {
                    current_channel_settings
                        .iter()
                        .any(|setting| &setting.channel_name == channel_name)
                        .not()
                        &&
                        // If it's not present in the current settings, but the Mode's setting
                        // is to the default profile, then there's no issue.
                        Self::is_default_profile(setting.profile_uid.as_ref()).not()
                }) {
                    // Make sure to compare Mode channel settings that have been reset - which
                    // don't exist anymore in the current_channel_settings
                    continue 'modes;
                }
                for channel_setting in &current_channel_settings {
                    if mode_channel_settings
                        .get(&channel_setting.channel_name)
                        .is_none()
                    {
                        if Self::is_default_profile(channel_setting.profile_uid.as_ref()) {
                            // if the Mode doesn't have anything set but the channel is set to
                            // the Default Profile, then it's a match. (none == default)
                            continue;
                        }
                        // This shouldn't happen after applying a Mode, as empty is set to default,
                        // but can happen after changing a setting for a channel for which the Mode
                        // has no setting.
                        debug!(
                            "Mode {} contains no setting for channel {} device UID: {}.",
                            mode.name, channel_setting.channel_name, device_uid
                        );
                        continue 'modes;
                    }
                    if channel_setting
                        != mode_channel_settings
                            .get(&channel_setting.channel_name)
                            .unwrap()
                    {
                        // If any channel setting doesn't match, move on to the next mode.
                        continue 'modes;
                    }
                }
            }
            // All applicable device & channel settings are a match
            active_modes.push(mode_uid.clone());
        }
        if active_modes.is_empty() {
            self.active_modes.write().await.clear();
            debug!("No mode is currently active");
            return;
        }
        debug!("Active modes determined: {active_modes:?}");
        self.update_active_modes(active_modes).await;
    }

    fn is_default_profile(profile_uid: Option<&ProfileUID>) -> bool {
        profile_uid.map_or(false, |uid| uid == DEFAULT_PROFILE_UID)
    }

    async fn update_active_modes(&self, mut active_modes: Vec<UID>) {
        let mut active_modes_lock = self.active_modes.write().await;
        active_modes_lock.clear();
        active_modes_lock.append(&mut active_modes);
    }

    /// Takes a Mode UID and applies all it's saved settings, making it the active Mode.
    /// This method handles several edge cases and unknowns.
    pub async fn activate_mode(&self, mode_uid: &UID) -> Result<()> {
        let Some(mode) = self.modes.read().await.get(mode_uid).cloned() else {
            error!("Mode not found: {}", mode_uid);
            return Err(CCError::NotFound {
                msg: format!("Mode not found: {mode_uid}"),
            }
            .into());
        };
        if self.active_modes.read().await.contains(mode_uid) {
            debug!("Mode already active: {} ID:{mode_uid}", mode.name);
            return Ok(());
        }

        let mut apply_settings_tasks = Vec::new();
        for device_uid in self.all_devices.keys() {
            if mode.all_device_settings.contains_key(device_uid).not() {
                self.reset_device_settings(&mut apply_settings_tasks, device_uid)
                    .await?;
                continue;
            }
            let saved_device_settings_map: HashMap<ChannelName, Setting> = self
                .config
                .get_device_settings(device_uid)
                .await?
                .into_iter()
                .map(|setting| (setting.channel_name.clone(), setting))
                .collect();
            let mode_device_settings = mode.all_device_settings.get(device_uid).unwrap();
            self.reset_unset_mode_channels(
                &mut apply_settings_tasks,
                device_uid,
                &saved_device_settings_map,
                mode_device_settings,
            );
            self.apply_mode_channel_settings(
                &mut apply_settings_tasks,
                device_uid,
                &saved_device_settings_map,
                mode_device_settings,
            );
        }
        // Wait for all tasks to complete before saving
        for task in apply_settings_tasks {
            if let Err(err) = task.await {
                error!("{err}");
            }
        }
        self.config.save_config_file().await?;
        debug!("Mode applied: {}", mode.name);
        Ok(())
    }

    async fn reset_device_settings(
        &self,
        apply_settings_tasks: &mut Vec<JoinHandle<()>>,
        device_uid: &DeviceUID,
    ) -> Result<()> {
        let saved_device_settings = self.config.get_device_settings(device_uid).await?;
        for setting in saved_device_settings {
            let settings_controller = Arc::clone(&self.settings_controller);
            let config = Arc::clone(&self.config);
            let device_uid = device_uid.clone();
            let channel_name = setting.channel_name.clone();
            let reset_setting = Setting {
                channel_name: setting.channel_name,
                reset_to_default: Some(true),
                ..Default::default()
            };
            apply_settings_tasks.push(tokio::spawn(async move {
                debug!("Applying RESET Mode Setting: {reset_setting:?} to device: {device_uid}");
                if let Err(err) = settings_controller
                    .set_reset(&device_uid, &channel_name)
                    .await
                {
                    error!("Error setting device setting: {err}");
                }
                config.set_device_setting(&device_uid, &reset_setting).await;
            }));
        }
        Ok(())
    }

    fn reset_unset_mode_channels(
        &self,
        apply_settings_tasks: &mut Vec<JoinHandle<()>>,
        device_uid: &DeviceUID,
        saved_device_settings_map: &HashMap<ChannelName, Setting>,
        mode_device_settings: &HashMap<ChannelName, Setting>,
    ) {
        for saved_setting_channel_name in saved_device_settings_map.keys() {
            if mode_device_settings
                .contains_key(saved_setting_channel_name)
                .not()
            {
                // There are settings applied to a channel that the Mode doesn't contain.
                // We reset these settings - as no setting in a Mode == default settings.
                let settings_controller = Arc::clone(&self.settings_controller);
                let config = Arc::clone(&self.config);
                let device_uid = device_uid.clone();
                let channel_name = saved_setting_channel_name.clone();
                let reset_setting = Setting {
                    channel_name: channel_name.clone(),
                    reset_to_default: Some(true),
                    ..Default::default()
                };
                apply_settings_tasks.push(tokio::spawn(async move {
                    debug!("Applying Mode Setting: {reset_setting:?} to device: {device_uid}");
                    if let Err(err) = settings_controller
                        .set_reset(&device_uid, &channel_name)
                        .await
                    {
                        error!("Error setting device setting: {err}");
                    }
                    config.set_device_setting(&device_uid, &reset_setting).await;
                }));
            }
        }
    }

    fn apply_mode_channel_settings(
        &self,
        apply_settings_tasks: &mut Vec<JoinHandle<()>>,
        device_uid: &DeviceUID,
        saved_device_settings_map: &HashMap<ChannelName, Setting>,
        mode_device_settings: &HashMap<ChannelName, Setting>,
    ) {
        for (channel_name, setting) in mode_device_settings {
            if saved_device_settings_map
                .get(channel_name)
                .map_or(false, |saved_setting| saved_setting == setting)
            {
                continue; // no need to apply if the setting is the same
            }
            let settings_controller = Arc::clone(&self.settings_controller);
            let config = Arc::clone(&self.config);
            let device_uid = device_uid.clone();
            let setting = setting.clone();
            apply_settings_tasks.push(tokio::spawn(async move {
                debug!("Applying Mode Setting: {setting:?} to device: {device_uid}");
                if let Err(err) = settings_controller
                    .set_config_setting(&device_uid, &setting)
                    .await
                {
                    error!("Error setting device setting: {err}");
                    return; // don't save setting if it wasn't successfully applied
                }
                debug!("Device Setting Applied: {setting:?}");
                config.set_device_setting(&device_uid, &setting).await;
            }));
        }
    }

    /// Creates a new Mode with the given name and all current device settings.
    /// This will also essentially duplicate a currently active Mode.
    pub async fn create_mode(&self, name: String) -> Result<Mode> {
        let all_device_settings = self.get_all_device_settings().await?;
        let mode_uid = Uuid::new_v4().to_string();
        let mode = Mode {
            uid: mode_uid.clone(),
            name,
            all_device_settings,
        };
        {
            // force a lock release after inserting
            self.modes
                .write()
                .await
                .insert(mode_uid.clone(), mode.clone());
            self.mode_order.write().await.push(mode_uid);
        }
        self.save_modes_data().await?;
        Ok(mode)
    }

    /// Duplicates a Mode with the given Mode UID.
    pub async fn duplicate_mode(&self, mode_uid_to_dup: &UID) -> Result<Mode> {
        let new_mode = {
            let modes_lock = self.modes.read().await;
            let mode_to_dup = modes_lock
                .get(mode_uid_to_dup)
                .ok_or_else(|| CCError::NotFound {
                    msg: format!("Mode not found: {mode_uid_to_dup}"),
                })?;
            Mode {
                uid: Uuid::new_v4().to_string(),
                name: format!("{} (copy)", mode_to_dup.name),
                all_device_settings: mode_to_dup.all_device_settings.clone(),
            }
        };
        {
            // force a lock release after inserting
            self.modes
                .write()
                .await
                .insert(new_mode.uid.clone(), new_mode.clone());
            self.mode_order.write().await.push(new_mode.uid.clone());
        }
        self.save_modes_data().await?;
        Ok(new_mode)
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
        let mode = {
            let mut modes_lock = self.modes.write().await;
            let mode = modes_lock
                .get_mut(mode_uid)
                .ok_or_else(|| CCError::NotFound {
                    msg: format!("Mode not found: {mode_uid}"),
                })?;
            mode.all_device_settings = self.get_all_device_settings().await?;
            mode.clone()
        };
        self.save_modes_data().await?;
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
        if self.modes.read().await.contains_key(mode_uid).not() {
            return Err(CCError::NotFound {
                msg: format!("Mode not found: {mode_uid}"),
            }
            .into());
        }
        self.modes.write().await.remove(mode_uid);
        self.mode_order.write().await.retain(|uid| uid != mode_uid);
        self.save_modes_data().await?;
        Ok(())
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

    /// Handles the deletion of a profile by removing references to it from other modes.
    ///
    /// This function takes the UID of the deleted profile and removes any settings that reference
    /// it from all modes.
    ///
    /// # Parameters
    ///
    /// * `profile_uid`: The `ProfileUID` of the profile that was deleted.
    ///
    /// # Returns
    ///
    /// A `Result` containing `()`, indicating that the deletion was successful.
    pub async fn profile_deleted(&self, profile_uid: &ProfileUID) -> Result<()> {
        let settings_to_delete = self.search_for_deleted_profile(profile_uid).await;
        self.remove_affected_settings(settings_to_delete).await;
        self.save_modes_data().await?;
        Ok(())
    }

    /// Removes settings that reference a deleted profile from all modes.
    ///
    /// This function takes a vector of tuples, where each tuple contains the mode UID, device UID,
    /// and channel name of a setting that references a deleted profile. It then removes these
    /// settings from the corresponding modes.
    ///
    /// # Parameters
    ///
    /// * `settings_to_delete`: A vector of tuples containing the mode UID, device UID, and channel name of settings
    ///   to remove.
    ///
    /// # Behavior
    ///
    /// This function iterates over the `settings_to_delete` vector and removes the corresponding
    /// settings from the modes. If a mode's device settings become empty after removing a setting,
    /// the device settings are also removed.
    async fn remove_affected_settings(&self, settings_to_delete: Vec<(String, String, String)>) {
        let mut modes = self.modes.write().await;
        for (mode_uid, device_uid, channel_name) in settings_to_delete {
            let device_settings = modes
                .get_mut(&mode_uid)
                .unwrap()
                .all_device_settings
                .get_mut(&device_uid)
                .unwrap();
            device_settings.remove(&channel_name);
            if device_settings.is_empty() {
                modes
                    .get_mut(&mode_uid)
                    .unwrap()
                    .all_device_settings
                    .remove(&device_uid);
            }
        }
    }

    /// Searches for and returns a list of tuples containing the mode UID, device UID,
    /// and channel name for settings that reference a deleted profile UID.
    ///
    /// # Arguments
    ///
    /// * `profile_uid` - A reference to the `ProfileUID` that has been deleted.
    ///
    /// # Returns
    ///
    /// A vector of tuples, where each tuple contains:
    /// - The UID of the mode.
    /// - The UID of the device.
    /// - The name of the channel.
    ///
    /// This function traverses all modes and their device settings, looking for any settings that
    /// reference the given profile UID. When such a setting is found, it adds a tuple containing the
    /// mode UID, device UID, and channel name to the results. This allows for easy identification and
    /// removal of settings associated with a deleted profile.
    async fn search_for_deleted_profile(
        &self,
        profile_uid: &ProfileUID,
    ) -> Vec<(String, String, String)> {
        let mut settings_to_delete = Vec::new();
        let modes = self.modes.read().await;
        for mode in modes.values() {
            for (device_uid, device_settings) in &mode.all_device_settings {
                for (channel_name, setting) in device_settings {
                    if setting
                        .profile_uid
                        .as_ref()
                        .is_some_and(|p_uid| p_uid == profile_uid)
                    {
                        settings_to_delete.push((
                            mode.uid.clone(),
                            device_uid.clone(),
                            channel_name.clone(),
                        ));
                    }
                }
            }
        }
        settings_to_delete
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
