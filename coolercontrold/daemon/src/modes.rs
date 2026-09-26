// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Not;
use std::rc::Rc;

use anyhow::{Context, Result};
use log::{debug, error, info, trace, warn};
use moro_local::Scope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::actor::ModeHandle;
use crate::api::modes::ActiveModesDto;
use crate::api::CCError;
use crate::config::Config;
use crate::device::{ChannelName, DeviceUID, UID};
use crate::engine::main::Engine;
use crate::paths;
use crate::power_profile_listener::PowerProfiles;
use crate::setting::{ProfileUID, Setting, SettingKind};
use crate::{cc_fs, AllDevices};

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
    power_profiles: PowerProfiles,
}

impl ModeController {
    /// Initializes the `ModeController` and fills it with data from the Mode configuration file.
    pub async fn init(
        config: Rc<Config>,
        all_devices: AllDevices,
        engine: Rc<Engine>,
        power_profiles: PowerProfiles,
    ) -> Result<Self> {
        let mode_controller = Self {
            config,
            all_devices,
            engine,
            modes: RefCell::new(HashMap::new()),
            mode_order: RefCell::new(Vec::new()),
            active_modes: RefCell::new(ActiveModes::new()),
            mode_handle: RefCell::new(None),
            power_profiles,
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

    /// The `ModeHandle`, once the API has set it. `None` before `start_server` has run.
    ///
    /// The handle is `Send` while the controller is not, so this is how off-thread callers such
    /// as the power profile listener activate a Mode through the actor rather than reaching into
    /// the controller directly.
    pub fn mode_handle(&self) -> Option<ModeHandle> {
        self.mode_handle.borrow().clone()
    }

    /// Apply all saved device settings to the devices if the `apply_on_boot` setting is true
    pub async fn handle_settings_at_boot(&self) {
        if self
            .config
            .get_settings()
            .expect("config settings should be verified by this point")
            .apply_on_boot
            .not()
        {
            return;
        }
        let all_successful =
            moro_local::async_scope!(|scope| self.apply_all_saved_device_settings(scope)).await;
        if all_successful.get().not() {
            self.clear_active_modes().await;
        }
    }

    /// Spawns one task per device onto `scope` that applies that device's
    /// saved settings sequentially. Per-device parallelism: settings on the
    /// same device serialize at the device's permit, so spawning per channel
    /// would not gain throughput. The caller awaits its scope, then reads
    /// `all_successful` to decide whether to clear active modes.
    pub fn apply_all_saved_device_settings<'s>(
        &'s self,
        scope: &'s Scope<'s, 's, Rc<Cell<bool>>>,
    ) -> Rc<Cell<bool>> {
        info!("Applying all saved device settings");
        // we loop through all currently present devices so that we don't apply settings
        //  to devices that are no longer there.
        let all_successful = Rc::new(Cell::new(true));
        for uid in self.all_devices.keys() {
            match self.config.get_device_settings(uid) {
                Ok(settings) => {
                    trace!("Settings for device: {uid} loaded from config file: {settings:?}");
                    let successful = Rc::clone(&all_successful);
                    scope.spawn(async move {
                        for setting in &settings {
                            if channel_is_available(&self.all_devices, uid, &setting.channel_name)
                                .not()
                            {
                                info!(
                                    "Skipping the saved setting for a channel the device no \
                                    longer offers: {}",
                                    self.engine.log_device_channel(uid, &setting.channel_name)
                                );
                                continue;
                            }
                            if let Err(err) = self.engine.set_config_setting(uid, setting).await {
                                error!("Error setting device setting: {err}");
                                successful.set(false);
                            }
                        }
                    });
                }
                Err(err) => {
                    error!("Error trying to read device settings from config file: {err}");
                    all_successful.set(false);
                }
            }
        }
        all_successful
    }

    /// Reads the Mode configuration file and fills the Modes `HashMap` and Mode Order Vec.
    async fn fill_data_from_mode_config_file(&self) -> Result<()> {
        let config_dir = paths::config_dir();
        if !config_dir.exists() {
            info!(
                "config directory doesn't exist. Attempting to create it: {}",
                config_dir.display()
            );
            cc_fs::create_dir_all(config_dir).await?;
        }
        let path = paths::mode_config_file().to_path_buf();
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

    /// Clears the active Modes. Nothing is saved or broadcast when none was active, since every
    /// setting write lands here and each broadcast makes all UIs reload their device settings.
    pub async fn clear_active_modes(&self) {
        let active_modes_changed = self.active_modes.borrow_mut().clear();
        if active_modes_changed.not() {
            return;
        }
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
            return Err(CCError::NotFound {
                msg: "Mode not found".to_string(),
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
                let Some(mode_device_settings) = mode.all_device_settings.get(device_uid) else {
                    self.reset_device_settings(device_uid, scope)?;
                    continue;
                };
                let mut saved_device_settings_map: HashMap<ChannelName, Setting> = HashMap::new();
                for setting in self.config.get_device_settings(device_uid)? {
                    saved_device_settings_map.insert(setting.channel_name.clone(), setting);
                }
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
        self.update_active_modes(mode_uid.clone());
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
                kind: SettingKind::Reset {
                    reset_to_default: true,
                },
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
                    kind: SettingKind::Reset {
                        reset_to_default: true,
                    },
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
        for (channel_name, setting) in mode_device_settings {
            if saved_device_settings_map.get(channel_name) == Some(setting) {
                continue; // no need to apply if the setting is the same
            }
            if channel_is_available(&self.all_devices, device_uid, channel_name).not() {
                warn!(
                    "This Mode contains a Channel: {channel_name} that the device no longer \
                    offers. Please update your Mode to remove this channel."
                );
                continue; // do not attempt to apply a setting for a channel that isn't there
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
                    msg: "Mode not found.".to_string(),
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
                    msg: "Mode not found".to_string(),
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
                    msg: "Mode not found".to_string(),
                })?;
            mode.all_device_settings = self.get_all_device_settings()?;
            mode.clone()
        };
        self.update_active_modes(mode_uid.clone());
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
                msg: "Mode not found".to_string(),
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
        self.prune_power_profile_modes(mode_uid).await?;
        self.save_modes_data().await?;
        Ok(())
    }

    /// Drops any power profile mapping that pointed at the deleted Mode.
    ///
    /// The API rejects a mapping to a Mode that does not exist, but that only holds at write
    /// time. Without this, deleting a mapped Mode leaves an entry that silently does nothing the
    /// next time the system switches to that profile.
    async fn prune_power_profile_modes(&self, mode_uid: &UID) -> Result<()> {
        let mut modes = self.config.get_power_profile_modes();
        let before = modes.len();
        modes.retain(|_, mapped_uid| mapped_uid != mode_uid);
        if modes.len() == before {
            return Ok(());
        }
        debug_assert!(
            modes.values().all(|mapped_uid| mapped_uid != mode_uid),
            "Every mapping to the deleted Mode must be gone"
        );
        debug_assert!(modes.len() < before, "Pruning must only remove entries");
        self.config.set_power_profile_modes(&modes);
        self.config.save_config_file().await?;
        // Only after the write succeeds, so a failed save cannot leave the listener acting on a
        // mapping that is still persisted.
        self.power_profiles.set_modes(modes);
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
        cc_fs::write_string(paths::mode_config_file(), mode_config_json)
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
            let Some(mode) = modes.get_mut(&mode_uid) else {
                continue;
            };
            let Some(device_settings) = mode.all_device_settings.get_mut(&device_uid) else {
                continue;
            };
            device_settings.remove(&channel_name);
            if device_settings.is_empty() {
                mode.all_device_settings.remove(&device_uid);
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
                    if let SettingKind::Profile { profile_uid: p_uid } = &setting.kind {
                        if p_uid == profile_uid {
                            settings_to_delete.push((
                                mode.uid.clone(),
                                device_uid.clone(),
                                channel_name.clone(),
                            ));
                        }
                    }
                }
            }
        }
        settings_to_delete
    }
}

/// Whether the device currently offers the channel.
///
/// A saved setting can name a channel that is not there: the user disabled it, their lm-sensors
/// configuration ignores it, or the driver stopped reporting it. Such settings are kept in the
/// config, since the channel can come back, but they cannot be applied while it is gone.
fn channel_is_available(
    all_devices: &AllDevices,
    device_uid: &DeviceUID,
    channel_name: &str,
) -> bool {
    all_devices
        .get(device_uid)
        .is_some_and(|device| device.borrow().info.channels.contains_key(channel_name))
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

/// Validates that `contents` parse as a modes config file.
pub fn validate(contents: &str) -> anyhow::Result<()> {
    use anyhow::Context;
    serde_json::from_str::<ModeConfigFile>(contents)
        .context("Parsing modes configuration")
        .map(|_| ())
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

    /// Returns whether an active Mode was cleared.
    fn clear(&mut self) -> bool {
        let active_mode_changed = self.current.take().is_some();
        let previous_mode_changed = self.previous.take().is_some();
        debug_assert!(self.current.is_none());
        debug_assert!(self.previous.is_none());
        active_mode_changed || previous_mode_changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::modes::ActiveMode;
    use crate::calibration::{CalibrationStore, FanStateMap};
    use crate::device::{ChannelInfo, ChannelKind, Device, DeviceInfo, DeviceType, SpeedOptions};
    use crate::overrides::OverridesController;
    use crate::repositories::repository::Repositories;
    use crate::setting::Setting;
    use serial_test::serial;
    use tokio_util::sync::CancellationToken;

    /// One Hwmon device offering exactly `channel_name`.
    fn devices_offering(channel_name: &str) -> (AllDevices, DeviceUID) {
        let mut info = DeviceInfo::default();
        info.channels.insert(
            channel_name.to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    fixed_enabled: true,
                    ..Default::default()
                }),
            },
        );
        let device = Rc::new(RefCell::new(Device::new(
            "Test Device".to_string(),
            DeviceType::Hwmon,
            0,
            None,
            info,
            None,
            1.0,
        )));
        let device_uid = device.borrow().uid.clone();
        let mut devices = HashMap::new();
        devices.insert(device_uid.clone(), device);
        (Rc::new(devices), device_uid)
    }

    /// A controller with no repositories: every setting that is actually applied fails, which
    /// is what makes a skipped setting distinguishable from an applied one.
    fn mode_controller(all_devices: &AllDevices, config: &Rc<Config>) -> ModeController {
        let engine = Rc::new(Engine::new(
            Rc::clone(all_devices),
            &Rc::new(Repositories::default()),
            Rc::clone(config),
            Rc::new(CalibrationStore::empty()),
            Rc::new(FanStateMap::new()),
            Rc::new(OverridesController::empty()),
        ));
        ModeController {
            config: Rc::clone(config),
            all_devices: Rc::clone(all_devices),
            engine,
            modes: RefCell::new(HashMap::new()),
            mode_order: RefCell::new(Vec::new()),
            active_modes: RefCell::new(ActiveModes::new()),
            mode_handle: RefCell::new(None),
            power_profiles: PowerProfiles::default(),
        }
    }

    /// Runs `clear_active_modes` with a subscribed `ModeHandle` and returns what it broadcast.
    async fn clear_and_receive_broadcast(controller: &Rc<ModeController>) -> Option<ActiveMode> {
        let cancel_token = CancellationToken::new();
        let mut received = None;
        moro_local::async_scope!(|scope| -> Result<()> {
            let mode_handle = ModeHandle::new(Rc::clone(controller), cancel_token.clone(), scope);
            // Subscribe first: the handle skips broadcasts that have no receivers.
            let mut receiver = mode_handle.broadcaster().subscribe();
            controller.clear_active_modes().await;
            received = receiver.try_recv().ok();
            // Stops the actor so the scope can finish.
            cancel_token.cancel();
            Ok(())
        })
        .await
        .unwrap();
        received
    }

    fn fixed_speed(channel_name: &str) -> Setting {
        Setting {
            channel_name: channel_name.to_string(),
            kind: SettingKind::SpeedFixed { speed_fixed: 50 },
        }
    }

    /// Goal: deleting a Mode must take its power profile mapping with it, in both the config
    /// and the live map the listener reads, or the next switch to that profile would silently
    /// do nothing.
    /// Methodology: map two profiles, delete the Mode one of them points at, then read both the
    /// persisted mapping and the shared one back.
    #[test]
    #[serial(modes_file)]
    fn deleting_a_mode_prunes_its_power_profile_mapping() {
        cc_fs::test_runtime(async {
            let (all_devices, _) = devices_offering("fan1");
            let config = Rc::new(Config::init_default_config().unwrap());
            let controller = mode_controller(&all_devices, &config);
            let deleted = "mode-to-delete".to_string();
            let kept = "mode-to-keep".to_string();
            for mode_uid in [&deleted, &kept] {
                controller.modes.borrow_mut().insert(
                    mode_uid.clone(),
                    Mode {
                        uid: mode_uid.clone(),
                        name: mode_uid.clone(),
                        all_device_settings: HashMap::new(),
                    },
                );
                controller.mode_order.borrow_mut().push(mode_uid.clone());
            }
            let mapping = HashMap::from([
                ("performance".to_string(), deleted.clone()),
                ("balanced".to_string(), kept.clone()),
            ]);
            config.set_power_profile_modes(&mapping);
            controller.power_profiles.set_modes(mapping);

            controller.delete_mode(&deleted).await.unwrap();

            let persisted = config.get_power_profile_modes();
            assert_eq!(
                persisted.get("performance"),
                None,
                "The mapping to the deleted Mode is gone"
            );
            assert_eq!(
                persisted.get("balanced"),
                Some(&kept),
                "Mappings to surviving Modes are untouched"
            );
            assert_eq!(
                controller.power_profiles.mode_for("performance"),
                None,
                "The listener must not act on the pruned mapping"
            );
            assert_eq!(controller.power_profiles.mode_for("balanced"), Some(kept));
        });
    }

    /// Goal: deleting an unmapped Mode must leave the mapping alone, so an unrelated delete
    /// never rewrites the config.
    /// Methodology: delete a Mode no profile points at.
    #[test]
    #[serial(modes_file)]
    fn deleting_an_unmapped_mode_leaves_the_mapping_alone() {
        cc_fs::test_runtime(async {
            let (all_devices, _) = devices_offering("fan1");
            let config = Rc::new(Config::init_default_config().unwrap());
            let controller = mode_controller(&all_devices, &config);
            let unmapped = "unmapped-mode".to_string();
            controller.modes.borrow_mut().insert(
                unmapped.clone(),
                Mode {
                    uid: unmapped.clone(),
                    name: unmapped.clone(),
                    all_device_settings: HashMap::new(),
                },
            );
            controller.mode_order.borrow_mut().push(unmapped.clone());
            let mapping = HashMap::from([("balanced".to_string(), "another-mode".to_string())]);
            config.set_power_profile_modes(&mapping);

            controller.delete_mode(&unmapped).await.unwrap();

            assert_eq!(config.get_power_profile_modes(), mapping);
        });
    }

    /// Goal: `ActiveModes::clear` reports whether it changed anything, so callers can skip
    /// saving and broadcasting a no-op.
    /// Methodology: clear an empty state, then states with only current, only previous, and
    /// both set, checking the result and that both slots end up empty.
    #[test]
    fn active_modes_clear_reports_whether_anything_changed() {
        let mut active_modes = ActiveModes::new();
        assert!(active_modes.clear().not(), "An empty state is unchanged");
        assert!(active_modes.current.is_none());
        assert!(active_modes.previous.is_none());

        let cases = [
            (Some("current".to_string()), None),
            (None, Some("previous".to_string())),
            (Some("current".to_string()), Some("previous".to_string())),
        ];
        for (current, previous) in cases {
            let mut active_modes = ActiveModes { current, previous };
            assert!(active_modes.clear(), "A set Mode is a change");
            assert!(active_modes.current.is_none());
            assert!(active_modes.previous.is_none());
            assert!(active_modes.clear().not(), "A second clear is a no-op");
        }
    }

    /// Goal: clearing with no active Mode must neither broadcast nor rewrite the Modes file.
    /// Every setting write clears the Modes, and each broadcast makes all UIs reload.
    /// Methodology: seed the Modes file with a sentinel, clear with a subscribed handle, then
    /// check nothing was received and the sentinel is intact.
    #[test]
    #[serial(modes_file)]
    fn clearing_without_an_active_mode_is_silent() {
        cc_fs::test_runtime(async {
            let (all_devices, _) = devices_offering("fan1");
            let config = Rc::new(Config::init_default_config().unwrap());
            let controller = Rc::new(mode_controller(&all_devices, &config));
            let sentinel = "sentinel: not written by the daemon";
            std::fs::write(paths::mode_config_file(), sentinel).unwrap();

            let received = clear_and_receive_broadcast(&controller).await;

            assert!(received.is_none(), "No broadcast for a no-op clear");
            let contents = std::fs::read_to_string(paths::mode_config_file()).unwrap();
            assert_eq!(contents, sentinel, "The Modes file is not rewritten");
        });
    }

    /// Goal: clearing an active Mode still saves and broadcasts the all-empty state.
    /// Methodology: activate a Mode in memory, clear with a subscribed handle, then check the
    /// broadcast and the saved file.
    #[test]
    #[serial(modes_file)]
    fn clearing_an_active_mode_saves_and_broadcasts() {
        cc_fs::test_runtime(async {
            let (all_devices, _) = devices_offering("fan1");
            let config = Rc::new(Config::init_default_config().unwrap());
            let controller = Rc::new(mode_controller(&all_devices, &config));
            controller.update_active_modes("active-mode".to_string());
            std::fs::write(paths::mode_config_file(), "sentinel").unwrap();

            let received = clear_and_receive_broadcast(&controller).await;

            let active_mode = received.expect("Clearing an active Mode is broadcast");
            assert!(active_mode.uid.is_none());
            assert!(active_mode.name.is_none());
            assert!(active_mode.previous_uid.is_none());
            let contents = std::fs::read_to_string(paths::mode_config_file()).unwrap();
            let saved: ModeConfigFile = serde_json::from_str(&contents).unwrap();
            assert!(saved.current_active_mode.is_none());
            assert!(saved.previous_active_mode.is_none());
        });
    }

    #[test]
    /// Goal: the availability check answers for the channel, not the device.
    fn channel_availability_follows_the_device_info() {
        let (all_devices, device_uid) = devices_offering("fan1");
        assert!(channel_is_available(&all_devices, &device_uid, "fan1"));
        assert!(channel_is_available(&all_devices, &device_uid, "fan8").not());
        assert!(channel_is_available(&all_devices, &"unknown".to_string(), "fan1").not());
    }

    #[test]
    /// Goal: a saved setting for a channel the device no longer offers (disabled, ignored by
    /// the lm-sensors configuration, or gone) is skipped instead of failing, so that boot does
    /// not log an error and does not clear the active modes.
    fn saved_setting_for_a_missing_channel_is_skipped() {
        cc_fs::test_runtime(async {
            let (all_devices, device_uid) = devices_offering("fan1");
            let config = Rc::new(Config::init_default_config().unwrap());
            config.set_device_setting(&device_uid, &fixed_speed("fan8"));
            let controller = mode_controller(&all_devices, &config);

            let all_successful =
                moro_local::async_scope!(|scope| controller.apply_all_saved_device_settings(scope))
                    .await;

            assert!(all_successful.get());
        });
    }

    #[test]
    /// Goal: the skip is limited to missing channels. An offered channel is still applied, and
    /// its failure still marks the run unsuccessful.
    fn saved_setting_for_an_offered_channel_is_still_applied() {
        cc_fs::test_runtime(async {
            let (all_devices, device_uid) = devices_offering("fan1");
            let config = Rc::new(Config::init_default_config().unwrap());
            config.set_device_setting(&device_uid, &fixed_speed("fan1"));
            let controller = mode_controller(&all_devices, &config);

            let all_successful =
                moro_local::async_scope!(|scope| controller.apply_all_saved_device_settings(scope))
                    .await;

            // No repository is registered, so applying it fails.
            assert!(all_successful.get().not());
        });
    }
}
