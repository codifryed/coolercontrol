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

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Not;
use std::path::Path;
use std::rc::Rc;

use anyhow::{Context, Result};
use const_format::concatcp;
use log::{debug, error, info, trace, warn};
use moro_local::Scope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::actor::ModeHandle;
use crate::api::modes::ActiveModesDto;
use crate::api::CCError;
use crate::config::{Config, DEFAULT_CONFIG_DIR};
use crate::device::{ChannelName, DeviceUID, UID};
use crate::engine::main::Engine;
use crate::setting::{ProfileUID, Setting};
use crate::{cc_fs, AllDevices};

const DEFAULT_MODE_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/modes.json");

/// The `ModeController` is responsible for managing mode snapshots of all the device settings and
/// applying them when appropriate.
pub struct ModeController {
    config: Rc<Config>,
    all_devices: AllDevices,
    engine: Rc<Engine>,
    modes: RefCell<HashMap<UID, Mode>>,
    mode_order: RefCell<Vec<UID>>,
    active_modes: RefCell<ActiveModes>,
    mode_handle: RefCell<Option<ModeHandle>>,
}

impl ModeController {
    /// Initializes the `ModeController` and fills it with data from the Mode configuration file.
    pub async fn init(
        config: Rc<Config>,
        all_devices: AllDevices,
        engine: Rc<Engine>,
    ) -> Result<Self> {
        let mode_controller = Self {
            config,
            all_devices,
            engine,
            modes: RefCell::new(HashMap::new()),
            mode_order: RefCell::new(Vec::new()),
            active_modes: RefCell::new(ActiveModes::new()),
            mode_handle: RefCell::new(None),
        };
        mode_controller.fill_data_from_mode_config_file().await?;
        Ok(mode_controller)
    }

    /// Sets the `ModeHandle` for the `ModeController`.
    ///
    /// The `ModeHandle` is used to broadcast notifications when a mode is activated.
    pub fn set_mode_handle(&self, mode_handle: ModeHandle) {
        self.mode_handle.replace(Some(mode_handle));
    }

    /// Apply all saved device settings to the devices if the `apply_on_boot` setting is true
    pub async fn handle_settings_at_boot(&self) {
        if self
            .config
            .get_settings()
            .expect("config settings should be verified by this point")
            .apply_on_boot
        {
            let all_successful = self.apply_all_saved_device_settings().await;
            if !all_successful {
                self.clear_active_modes().await;
            }
        }
    }

    /// Apply all saved device settings to the devices
    pub async fn apply_all_saved_device_settings(&self) -> bool {
        info!("Applying all saved device settings");
        // we loop through all currently present devices so that we don't apply settings
        //  to devices that are no longer there.
        let mut all_successful = true;
        for uid in self.all_devices.keys() {
            match self.config.get_device_settings(uid) {
                Ok(settings) => {
                    trace!("Settings for device: {uid} loaded from config file: {settings:?}");
                    for setting in &settings {
                        if let Err(err) = self.engine.set_config_setting(uid, setting).await {
                            error!("Error setting device setting: {err}");
                            all_successful = false;
                        }
                    }
                }
                Err(err) => {
                    error!("Error trying to read device settings from config file: {err}");
                    all_successful = false;
                }
            }
        }
        all_successful
    }

    /// Reads the Mode configuration file and fills the Modes `HashMap` and Mode Order Vec.
    async fn fill_data_from_mode_config_file(&self) -> Result<()> {
        let config_dir = Path::new(DEFAULT_CONFIG_DIR);
        if !config_dir.exists() {
            info!("config directory doesn't exist. Attempting to create it: {DEFAULT_CONFIG_DIR}");
            cc_fs::create_dir_all(config_dir)?;
        }
        let path = Path::new(DEFAULT_MODE_CONFIG_FILE_PATH).to_path_buf();
        let config_contents = if let Ok(contents) = cc_fs::read_txt(&path).await {
            contents
        } else {
            info!("Writing a new Modes configuration file");
            let default_mode_config = serde_json::to_string(&ModeConfigFile {
                modes: Vec::new(),
                order: Vec::new(),
                current_active_mode: None,
                previous_active_mode: None,
            })?;
            cc_fs::write_string(&path, default_mode_config)
                .await
                .with_context(|| format!("Writing new configuration file: {}", path.display()))?;
            // make sure the file is readable:
            cc_fs::read_txt(&path)
                .await
                .with_context(|| format!("Reading configuration file {}", path.display()))?
        };
        let mode_config: ModeConfigFile = serde_json::from_str(&config_contents)
            .with_context(|| format!("Parsing Mode configuration file {}", path.display()))?;
        {
            let mut modes_lock = self.modes.borrow_mut();
            modes_lock.clear();
            for mode in mode_config.modes {
                modes_lock.insert(mode.uid.clone(), mode);
            }
        }
        {
            let mut mode_order_lock = self.mode_order.borrow_mut();
            mode_order_lock.clear();
            mode_order_lock.extend(mode_config.order);
        }
        {
            let mut active_modes_lock = self.active_modes.borrow_mut();
            active_modes_lock.current = mode_config.current_active_mode;
            active_modes_lock.previous = mode_config.previous_active_mode;
        }
        Ok(())
    }

    pub fn get_modes(&self) -> Vec<Mode> {
        let modes_lock = self.modes.borrow();
        self.mode_order
            .borrow()
            .iter()
            .filter_map(|uid| modes_lock.get(uid).cloned())
            .collect()
    }

    pub fn get_mode(&self, mode_uid: &UID) -> Option<Mode> {
        self.modes.borrow().get(mode_uid).cloned()
    }

    /// Returns the currently active Modes.
    pub fn get_active_modes(&self) -> ActiveModesDto {
        ActiveModesDto {
            current_mode_uid: self.active_modes.borrow().current.clone(),
            previous_mode_uid: self.active_modes.borrow().previous.clone(),
        }
    }

    pub async fn clear_active_modes(&self) {
        self.active_modes.borrow_mut().clear();
        if let Err(err) = self.save_modes_data().await {
            error!("Error saving mode data: {err}");
        }
        self.broadcast_active_mode(None, None, None);
    }

    fn update_active_modes(&self, mode_uid: UID) {
        self.active_modes.borrow_mut().mode_activated(mode_uid);
    }

    /// Takes a Mode UID and applies all it's saved settings, making it the active Mode.
    /// This method handles several edge cases and unknowns.
    pub async fn activate_mode(&self, mode_uid: &UID) -> Result<()> {
        let Some(mode) = self.modes.borrow().get(mode_uid).cloned() else {
            error!("Mode not found: {mode_uid}");
            return Err(CCError::NotFound {
                msg: format!("Mode not found: {mode_uid}"),
            }
            .into());
        };
        {
            let active_modes_lock = self.active_modes.borrow();
            if active_modes_lock.current.as_ref() == Some(mode_uid) {
                debug!("Mode already active: {} ID:{mode_uid}", mode.name);
                self.broadcast_active_mode(
                    Some(&mode.uid),
                    Some(&mode.name),
                    self.active_modes.borrow().previous.as_ref(),
                );
                return Ok(());
            }
        }
        debug!("Activating mode: {} ID:{mode_uid}", mode.name);
        moro_local::async_scope!(|scope| -> Result<()> {
            // devices that have been disabled are simply skipped.
            for device_uid in self.all_devices.keys() {
                if mode.all_device_settings.contains_key(device_uid).not() {
                    self.reset_device_settings(device_uid, scope)?;
                    continue;
                }
                let mut saved_device_settings_map: HashMap<ChannelName, Setting> = HashMap::new();
                for setting in self.config.get_device_settings(device_uid)? {
                    saved_device_settings_map.insert(setting.channel_name.clone(), setting);
                }
                let mode_device_settings = mode.all_device_settings.get(device_uid).unwrap();
                self.reset_unset_mode_channels(
                    device_uid,
                    &saved_device_settings_map,
                    mode_device_settings,
                    scope,
                );
                self.apply_mode_channel_settings(
                    device_uid,
                    &saved_device_settings_map,
                    mode_device_settings,
                    scope,
                );
            }
            Ok(())
        })
        .await?;
        self.config.save_config_file().await?;
        self.update_active_modes(mode_uid.to_string());
        self.save_modes_data().await?;
        self.broadcast_active_mode(
            Some(&mode.uid),
            Some(&mode.name),
            self.active_modes.borrow().previous.as_ref(),
        );
        info!("Successfully applied:: Mode: {}", mode.name);
        Ok(())
    }

    fn reset_device_settings<'s>(
        &self,
        device_uid: &DeviceUID,
        scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Result<()> {
        let saved_device_settings = self.config.get_device_settings(device_uid)?;
        for setting in saved_device_settings {
            let engine = Rc::clone(&self.engine);
            let config = Rc::clone(&self.config);
            let device_uid = device_uid.clone();
            let channel_name = setting.channel_name.clone();
            let reset_setting = Setting {
                channel_name: setting.channel_name,
                reset_to_default: Some(true),
                ..Default::default()
            };
            scope.spawn(async move {
                debug!("Applying RESET Mode Setting: {reset_setting:?} to device: {device_uid}");
                if let Err(err) = engine.set_reset(&device_uid, &channel_name).await {
                    error!("Error setting device setting: {err}");
                }
                config.set_device_setting(&device_uid, &reset_setting);
            });
        }
        Ok(())
    }

    fn reset_unset_mode_channels<'s>(
        &self,
        device_uid: &DeviceUID,
        saved_device_settings_map: &HashMap<ChannelName, Setting>,
        mode_device_settings: &HashMap<ChannelName, Setting>,
        scope: &'s Scope<'s, 's, Result<()>>,
    ) {
        for saved_setting_channel_name in saved_device_settings_map.keys() {
            if mode_device_settings
                .contains_key(saved_setting_channel_name)
                .not()
            {
                // There are settings applied to a channel that the Mode doesn't contain.
                // We reset these settings - as no setting in a Mode == default settings.
                let engine = Rc::clone(&self.engine);
                let config = Rc::clone(&self.config);
                let device_uid = device_uid.clone();
                let channel_name = saved_setting_channel_name.clone();
                let reset_setting = Setting {
                    channel_name: channel_name.clone(),
                    reset_to_default: Some(true),
                    ..Default::default()
                };
                scope.spawn(async move {
                    debug!("Applying Mode Setting: {reset_setting:?} to device: {device_uid}");
                    if let Err(err) = engine.set_reset(&device_uid, &channel_name).await {
                        error!("Error resetting device setting for Mode: {err}");
                    }
                    config.set_device_setting(&device_uid, &reset_setting);
                });
            }
        }
    }

    fn apply_mode_channel_settings<'s>(
        &self,
        device_uid: &DeviceUID,
        saved_device_settings_map: &HashMap<ChannelName, Setting>,
        mode_device_settings: &HashMap<ChannelName, Setting>,
        scope: &'s Scope<'s, 's, Result<()>>,
    ) {
        let cc_device_settings = self
            .config
            .get_cc_settings_for_device(device_uid)
            .unwrap_or_default();
        for (channel_name, setting) in mode_device_settings {
            if saved_device_settings_map.get(channel_name) == Some(setting) {
                continue; // no need to apply if the setting is the same
            }
            if let Some(cc_settings) = &cc_device_settings {
                if cc_settings.disable_channels.contains(channel_name) {
                    warn!(
                        "This Mode contains a Channel: {channel_name} that has been disabled. \
                        Please update your Mode to remove this channel."
                    );
                    continue; // do not attempt to apply a setting for a disabled channel
                }
            }
            let engine = Rc::clone(&self.engine);
            let config = Rc::clone(&self.config);
            let device_uid = device_uid.clone();
            let setting = setting.clone();
            scope.spawn(async move {
                debug!("Applying Mode Setting: {setting:?} to device: {device_uid}");
                if let Err(err) = engine.set_config_setting(&device_uid, &setting).await {
                    error!("Error applying setting device setting for Mode: {err}");
                    return; // don't save setting if it wasn't successfully applied
                }
                debug!("Device Setting Applied: {setting:?}");
                config.set_device_setting(&device_uid, &setting);
            });
        }
    }

    fn broadcast_active_mode(
        &self,
        mode_uid: Option<&UID>,
        mode_name: Option<&String>,
        previous_mode_uid: Option<&UID>,
    ) {
        if let Some(mode_handle) = self.mode_handle.borrow().as_ref() {
            mode_handle.broadcast_active_mode(mode_uid, mode_name, previous_mode_uid);
        }
    }

    /// Creates a new Mode with the given name and all current device settings.
    /// This will also essentially duplicate a currently active Mode.
    pub async fn create_mode(&self, name: String) -> Result<Mode> {
        let all_device_settings = self.get_all_device_settings()?;
        let mode_uid = Uuid::new_v4().to_string();
        let mode = Mode {
            uid: mode_uid.clone(),
            name,
            all_device_settings,
        };
        {
            // force a lock release after inserting
            self.modes
                .borrow_mut()
                .insert(mode_uid.clone(), mode.clone());
            self.mode_order.borrow_mut().push(mode_uid.clone());
        }
        self.update_active_modes(mode_uid);
        self.save_modes_data().await?;
        self.broadcast_active_mode(
            Some(&mode.uid),
            Some(&mode.name),
            self.active_modes.borrow().previous.as_ref(),
        );
        Ok(mode)
    }

    /// Duplicates a Mode with the given Mode UID.
    pub async fn duplicate_mode(&self, mode_uid_to_dup: &UID) -> Result<Mode> {
        let new_mode = {
            let modes_lock = self.modes.borrow();
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
                .borrow_mut()
                .insert(new_mode.uid.clone(), new_mode.clone());
            self.mode_order.borrow_mut().push(new_mode.uid.clone());
        }
        self.save_modes_data().await?;
        Ok(new_mode)
    }

    /// Returns a Mode-style `HashMap` of all current device settings.
    fn get_all_device_settings(&self) -> Result<HashMap<UID, HashMap<ChannelName, Setting>>> {
        let mut all_device_settings = HashMap::new();
        let all_current_device_settings = self.config.get_all_devices_settings()?;
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
            let mut modes_lock = self.modes.borrow_mut();
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
            let mut modes_lock = self.modes.borrow_mut();
            let mode = modes_lock
                .get_mut(mode_uid)
                .ok_or_else(|| CCError::NotFound {
                    msg: format!("Mode not found: {mode_uid}"),
                })?;
            mode.all_device_settings = self.get_all_device_settings()?;
            mode.clone()
        };
        self.update_active_modes(mode_uid.to_string());
        self.save_modes_data().await?;
        self.broadcast_active_mode(
            Some(&mode.uid),
            Some(&mode.name),
            self.active_modes.borrow().previous.as_ref(),
        );
        Ok(mode)
    }

    /// Updates the Mode order with the given list of Mode UIDs.
    pub async fn update_mode_order(&self, mode_uids: Vec<UID>) -> Result<()> {
        {
            let mut mode_order_lock = self.mode_order.borrow_mut();
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
        if self.modes.borrow().contains_key(mode_uid).not() {
            return Err(CCError::NotFound {
                msg: format!("Mode not found: {mode_uid}"),
            }
            .into());
        }
        let active_mode_name = {
            if self.active_modes.borrow().current.as_ref() == Some(mode_uid) {
                self.modes.borrow().get(mode_uid).map(|m| m.name.clone())
            } else {
                None
            }
        };
        let active_mode_changed = {
            self.modes.borrow_mut().remove(mode_uid);
            self.mode_order.borrow_mut().retain(|uid| uid != mode_uid);
            self.active_modes.borrow_mut().mode_deleted(mode_uid)
        };
        if active_mode_changed {
            let active_modes_lock = self.active_modes.borrow();
            self.broadcast_active_mode(
                active_modes_lock.current.as_ref(),
                active_mode_name.as_ref(),
                active_modes_lock.previous.as_ref(),
            );
        }
        self.save_modes_data().await?;
        Ok(())
    }

    /// Saves the current Modes data to the Mode configuration file.
    async fn save_modes_data(&self) -> Result<()> {
        let mode_config = ModeConfigFile {
            modes: self.modes.borrow().values().cloned().collect(),
            order: self.mode_order.borrow().clone(),
            current_active_mode: self.active_modes.borrow().current.clone(),
            previous_active_mode: self.active_modes.borrow().previous.clone(),
        };
        let mode_config_json = serde_json::to_string(&mode_config)?;
        cc_fs::write_string(DEFAULT_MODE_CONFIG_FILE_PATH, mode_config_json)
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
        let settings_to_delete = self.search_for_deleted_profile(profile_uid);
        self.remove_affected_settings(settings_to_delete);
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
    fn remove_affected_settings(&self, settings_to_delete: Vec<(String, String, String)>) {
        let mut modes = self.modes.borrow_mut();
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
    fn search_for_deleted_profile(
        &self,
        profile_uid: &ProfileUID,
    ) -> Vec<(String, String, String)> {
        let mut settings_to_delete = Vec::new();
        let modes = self.modes.borrow();
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
    current_active_mode: Option<UID>,
    previous_active_mode: Option<UID>,
}

#[derive(Debug, Clone)]
struct ActiveModes {
    current: Option<UID>,
    previous: Option<UID>,
}

impl ActiveModes {
    fn new() -> Self {
        ActiveModes {
            current: None,
            previous: None,
        }
    }

    fn mode_deleted(&mut self, mode_uid: &UID) -> bool {
        let mut active_mode_changed = false;
        if self.current.as_ref() == Some(mode_uid) {
            self.current = None;
            active_mode_changed = true;
        }
        if self.previous.as_ref() == Some(mode_uid) {
            self.previous = None;
            active_mode_changed = true;
        }
        active_mode_changed
    }

    fn mode_activated(&mut self, mode_uid: UID) {
        self.previous = self.current.replace(mode_uid);
    }

    fn clear(&mut self) {
        self.current = None;
        self.previous = None;
    }
}
