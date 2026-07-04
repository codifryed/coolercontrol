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

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::settings::{CoolerControlDeviceSettingsDto, CoolerControlSettingsDto};
use crate::api::CCError;
use crate::config::Config;
use crate::device::{ChannelName, DeviceInfo, DeviceName, DeviceType, DeviceUID};
use crate::overrides::{OverridesController, OverridesDocument};
use crate::setting::{
    CCChannelSettings, CCDeviceSettings, CoolerControlSettings, CustomSensor, DeviceExtensions,
    Profile, ProfileType, Setting, SettingKind, TempSource,
};
use crate::AllDevices;
use anyhow::Result;
use moro_local::Scope;
use std::collections::{HashMap, HashSet};
use std::default::Default;
use std::fmt::Write;
use std::ops::Not;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct SettingActor {
    receiver: mpsc::Receiver<SettingMessage>,
    all_devices: AllDevices,
    config: Rc<Config>,
    overrides: Rc<OverridesController>,
}

enum SettingMessage {
    GetCC {
        respond_to: oneshot::Sender<Result<CoolerControlSettings>>,
    },
    UpdateCC {
        update: CoolerControlSettingsDto,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetAllCCDevices {
        respond_to: oneshot::Sender<Result<Vec<CoolerControlDeviceSettingsDto>>>,
    },
    GetCCDevice {
        device_uid: DeviceUID,
        respond_to: oneshot::Sender<Result<CoolerControlDeviceSettingsDto>>,
    },
    UpdateCCDevice {
        device_uid: DeviceUID,
        update: CCDeviceSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetUI {
        respond_to: oneshot::Sender<Result<String>>,
    },
    UpdateUI {
        settings: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetOverrides {
        respond_to: oneshot::Sender<Result<OverridesDocument>>,
    },
    SetDeviceNameOverride {
        device_uid: DeviceUID,
        name: Option<String>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    SetChannelLabelOverride {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        label: Option<String>,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl SettingActor {
    pub fn new(
        receiver: mpsc::Receiver<SettingMessage>,
        all_devices: AllDevices,
        config: Rc<Config>,
        overrides: Rc<OverridesController>,
    ) -> Self {
        Self {
            receiver,
            all_devices,
            config,
            overrides,
        }
    }

    /// The live detected device name, when the device is registered.
    fn live_device_name(&self, device_uid: &DeviceUID) -> Option<DeviceName> {
        let device_lock = self.all_devices.get(device_uid)?;
        let lock = device_lock.borrow();
        Some(lock.info.model.clone().unwrap_or_else(|| lock.name.clone()))
    }

    /// The device must be known live, via saved settings, or via the config
    /// devices list (which retains devices no longer detected); the returned
    /// detected name refreshes the hand-editor hint in `overrides.toml`.
    fn device_name_hint(&self, device_uid: &DeviceUID) -> Result<DeviceName> {
        if let Some(name) = self.live_device_name(device_uid) {
            return Ok(name);
        }
        if let Some(settings) = self.config.get_cc_settings_for_device(device_uid)? {
            return Ok(settings.name);
        }
        if let Some(name) = self.config.device_name(device_uid) {
            return Ok(name);
        }
        Err(CCError::NotFound {
            msg: "Device not found".to_string(),
        }
        .into())
    }

    /// Sets or removes the user-defined device name override.
    async fn set_device_name_override(
        &self,
        device_uid: DeviceUID,
        name: Option<String>,
    ) -> Result<()> {
        let hint = self.device_name_hint(&device_uid)?;
        self.overrides
            .set_device_name(&device_uid, &hint, name.as_deref())
            .await
    }

    /// Sets or removes the user-defined channel label override.
    async fn set_channel_label_override(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        label: Option<String>,
    ) -> Result<()> {
        let device_name_hint = self.device_name_hint(&device_uid)?;
        let channel_label_hint = self.detected_channel_label_hint(&device_uid, &channel_name);
        self.overrides
            .set_channel_label(
                &device_uid,
                &device_name_hint,
                &channel_name,
                channel_label_hint.as_deref(),
                label.as_deref(),
            )
            .await
    }

    /// The detected channel label for the hand-editor hint: live device
    /// info first (domain labels are never override-mutated), then the
    /// saved detection memo.
    fn detected_channel_label_hint(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
    ) -> Option<String> {
        if let Some(device_lock) = self.all_devices.get(device_uid) {
            let lock = device_lock.borrow();
            if let Some(label) = detected_channel_label(&lock.info, channel_name) {
                return Some(label);
            }
        }
        self.config
            .get_cc_settings_for_device(device_uid)
            .ok()
            .flatten()
            .and_then(|settings| {
                settings
                    .channel_settings
                    .get(channel_name)
                    .and_then(|channel| channel.label.clone())
            })
    }

    /// Applies boundary resolution to a CC device settings DTO: the name and
    /// channel labels carry resolver output instead of raw config snapshots.
    fn resolve_cc_device_dto(&self, dto: &mut CoolerControlDeviceSettingsDto) {
        let live_name = self.live_device_name(&dto.uid);
        dto.name = resolve_cc_device_name(&self.overrides, &dto.uid, &dto.name, live_name);
        if let Some(device_lock) = self.all_devices.get(&dto.uid) {
            let lock = device_lock.borrow();
            resolve_channel_setting_labels(
                &self.overrides,
                &dto.uid,
                Some(&lock.info),
                &mut dto.channel_settings,
            );
        } else {
            resolve_channel_setting_labels(
                &self.overrides,
                &dto.uid,
                None,
                &mut dto.channel_settings,
            );
        }
    }
}

impl ApiActor<SettingMessage> for SettingActor {
    fn name(&self) -> &'static str {
        "SettingActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<SettingMessage> {
        &mut self.receiver
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_message(&mut self, msg: SettingMessage) {
        match msg {
            SettingMessage::GetCC { respond_to } => {
                let result = self.config.get_settings();
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateCC { update, respond_to } => {
                let result = async {
                    let current_settings = self.config.get_settings()?;
                    let settings_to_set = update.merge(current_settings);
                    self.config.set_settings(&settings_to_set);
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetAllCCDevices { respond_to } => {
                let result = async {
                    let mut devices_settings = HashMap::new();
                    let mut saved_settings_map = self.config.get_all_cc_devices_settings()?;
                    for (device_uid, device_lock) in self.all_devices.iter() {
                        // use saved settings if present, otherwise use default
                        if let Some(settings) = saved_settings_map.remove(device_uid) {
                            devices_settings.insert(
                                device_uid.clone(),
                                CoolerControlDeviceSettingsDto {
                                    uid: device_uid.clone(),
                                    name: settings.name, // detection memo, resolved below
                                    disable: settings.disable,
                                    extensions: settings.extensions,
                                    channel_settings: settings.channel_settings,
                                },
                            );
                        } else {
                            let device_name = {
                                let lock = device_lock.borrow();
                                if lock.d_type == DeviceType::CustomSensors {
                                    // custom sensors is handled differently than hardware devices
                                    continue;
                                }
                                lock.info.model.clone().unwrap_or_else(|| lock.name.clone())
                            };
                            devices_settings.insert(
                                device_uid.clone(),
                                CoolerControlDeviceSettingsDto {
                                    uid: device_uid.clone(),
                                    name: device_name,
                                    disable: false,
                                    extensions: DeviceExtensions::default(),
                                    channel_settings: HashMap::with_capacity(0),
                                },
                            );
                        }
                    }
                    // This adds the remaining devices which are currently not present, (i.e. blacklisted devices)
                    for (device_uid, settings) in saved_settings_map {
                        devices_settings.insert(
                            device_uid.clone(),
                            CoolerControlDeviceSettingsDto {
                                uid: device_uid,
                                name: settings.name,
                                disable: settings.disable,
                                extensions: settings.extensions,
                                channel_settings: settings.channel_settings,
                            },
                        );
                    }
                    let mut cc_devices_settings = devices_settings
                        .into_values()
                        .collect::<Vec<CoolerControlDeviceSettingsDto>>();
                    for dto in &mut cc_devices_settings {
                        self.resolve_cc_device_dto(dto);
                    }
                    Ok(cc_devices_settings)
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetCCDevice {
                device_uid,
                respond_to,
            } => {
                let result = async {
                    let settings_option = self.config.get_cc_settings_for_device(&device_uid)?;
                    let mut dto = if let Some(settings) = settings_option {
                        CoolerControlDeviceSettingsDto {
                            uid: device_uid,
                            name: settings.name, // detection memo, resolved below
                            disable: settings.disable,
                            extensions: settings.extensions,
                            channel_settings: settings.channel_settings,
                        }
                    } else {
                        // Default settings
                        let current_device_name = {
                            if let Some(device_lock) = self.all_devices.get(&device_uid) {
                                let lock = device_lock.borrow();
                                let device_name =
                                    lock.info.model.clone().unwrap_or_else(|| lock.name.clone());
                                Some(device_name)
                            } else {
                                None
                            }
                        }
                        .ok_or_else(|| CCError::NotFound {
                            msg: "Device not found".to_string(),
                        })?;
                        CoolerControlDeviceSettingsDto {
                            uid: device_uid,
                            name: current_device_name,
                            disable: false,
                            extensions: DeviceExtensions::default(),
                            channel_settings: HashMap::with_capacity(0),
                        }
                    };
                    self.resolve_cc_device_dto(&mut dto);
                    Ok(dto)
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateCCDevice {
                device_uid,
                mut update,
                respond_to,
            } => {
                let result = async {
                    let current_settings = self
                        .config
                        .get_cc_settings_for_device(&device_uid)?
                        .unwrap_or_default();
                    // Detection memos: the daemon stamps the last-detected names.
                    // Client-supplied values are ignored (write-deprecated); user
                    // names live in overrides.toml.
                    if let Some(device_lock) = self.all_devices.get(&device_uid) {
                        let lock = device_lock.borrow();
                        let detected = lock.info.model.clone().unwrap_or_else(|| lock.name.clone());
                        stamp_detection_memos(
                            &mut update,
                            &current_settings,
                            Some((&detected, &lock.info)),
                        );
                    } else {
                        stamp_detection_memos(&mut update, &current_settings, None);
                    }
                    // Reject the update before any mutation if it would orphan a Profile
                    // temp_source or a Custom Sensor source on a newly-disabled channel/device.
                    let profiles = self.config.get_profiles().await?;
                    let custom_sensors = self.config.get_custom_sensors()?;
                    let channel_labels =
                        build_temp_channel_labels(&self.all_devices, &device_uid, &update);
                    verify_disable_does_not_orphan_temp_sources(
                        &device_uid,
                        &update,
                        &current_settings,
                        &profiles,
                        &custom_sensors,
                        &channel_labels,
                    )?;
                    self.config.set_cc_settings_for_device(&device_uid, &update);
                    // check for disabled devices and channels and remove their settings:
                    if update.channel_settings.is_empty().not() {
                        for setting in self.config.get_device_settings(&device_uid)? {
                            if update
                                .channel_settings
                                .get(&setting.channel_name)
                                .is_some_and(|s| s.disabled)
                            {
                                let reset_setting = Setting {
                                    channel_name: setting.channel_name,
                                    kind: SettingKind::Reset {
                                        reset_to_default: true,
                                    },
                                };
                                self.config.set_device_setting(&device_uid, &reset_setting);
                            }
                        }
                    }
                    if update.disable {
                        self.config.clear_device_settings(&device_uid);
                    }
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetUI { respond_to } => {
                let result = self.config.load_ui_config_file().await;
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateUI {
                settings,
                respond_to,
            } => {
                let result = self.config.save_ui_config_file(settings).await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetOverrides { respond_to } => {
                let _ = respond_to.send(Ok(self.overrides.document()));
            }
            SettingMessage::SetDeviceNameOverride {
                device_uid,
                name,
                respond_to,
            } => {
                let result = self.set_device_name_override(device_uid, name).await;
                let _ = respond_to.send(result);
            }
            SettingMessage::SetChannelLabelOverride {
                device_uid,
                channel_name,
                label,
                respond_to,
            } => {
                let result = self
                    .set_channel_label_override(device_uid, channel_name, label)
                    .await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct SettingHandle {
    sender: mpsc::Sender<SettingMessage>,
}

impl SettingHandle {
    pub fn new<'s>(
        all_devices: AllDevices,
        config: Rc<Config>,
        overrides: Rc<OverridesController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = SettingActor::new(receiver, all_devices, config, overrides);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get_cc(&self) -> Result<CoolerControlSettings> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetCC { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_cc(&self, update: CoolerControlSettingsDto) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::UpdateCC {
            update,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_all_cc_devices(&self) -> Result<Vec<CoolerControlDeviceSettingsDto>> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetAllCCDevices { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_cc_device(
        &self,
        device_uid: DeviceUID,
    ) -> Result<CoolerControlDeviceSettingsDto> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetCCDevice {
            device_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_cc_device(
        &self,
        device_uid: DeviceUID,
        update: CCDeviceSettings,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::UpdateCCDevice {
            device_uid,
            update,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_ui(&self) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetUI { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_ui(&self, settings: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::UpdateUI {
            settings,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_overrides(&self) -> Result<OverridesDocument> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetOverrides { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn set_device_name_override(
        &self,
        device_uid: DeviceUID,
        name: Option<String>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::SetDeviceNameOverride {
            device_uid,
            name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn set_channel_label_override(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        label: Option<String>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::SetChannelLabelOverride {
            device_uid,
            channel_name,
            label,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}

/// The detected label for a channel from live device info, when present.
fn detected_channel_label(info: &DeviceInfo, channel_name: &str) -> Option<String> {
    if let Some(temp_info) = info.temps.get(channel_name) {
        return Some(temp_info.label.clone());
    }
    info.channels
        .get(channel_name)
        .and_then(|channel| channel.label.clone())
}

/// Resolves a CC device settings name: override > saved memo > live detected.
fn resolve_cc_device_name(
    overrides: &OverridesController,
    device_uid: &DeviceUID,
    memo_name: &str,
    live_name: Option<DeviceName>,
) -> DeviceName {
    if let Some(name) = overrides.device_name_override(device_uid) {
        return name;
    }
    if memo_name.is_empty().not() {
        return memo_name.to_string();
    }
    live_name.unwrap_or_else(|| memo_name.to_string())
}

/// Resolves each channel setting's label: override > saved memo > live detected.
fn resolve_channel_setting_labels(
    overrides: &OverridesController,
    device_uid: &DeviceUID,
    live_info: Option<&DeviceInfo>,
    channel_settings: &mut HashMap<ChannelName, CCChannelSettings>,
) {
    assert!(device_uid.is_empty().not());
    for (channel_name, settings) in channel_settings.iter_mut() {
        let memo = settings.label.take().filter(|label| label.is_empty().not());
        let live = live_info.and_then(|info| detected_channel_label(info, channel_name));
        settings.label = overrides
            .channel_label_override(device_uid, channel_name)
            .or(memo)
            .or(live);
    }
    debug_assert!(channel_settings
        .iter()
        .all(|(channel_name, settings)| overrides
            .channel_label_override(device_uid, channel_name)
            .is_none_or(|over| settings.label.as_deref() == Some(over.as_str()))));
}

/// Stamps `CCDeviceSettings.name` and channel labels as last-detected-name
/// memos. Client-supplied values are ignored (write-deprecated): live info
/// wins, then the currently saved memo. The client value survives only for
/// devices with no live record and no saved memo (nothing better exists).
fn stamp_detection_memos(
    update: &mut CCDeviceSettings,
    current: &CCDeviceSettings,
    live: Option<(&str, &DeviceInfo)>,
) {
    if let Some((detected_name, _)) = live {
        update.name = detected_name.to_string();
    } else if current.name.is_empty().not() {
        update.name.clone_from(&current.name);
    }
    for (channel_name, settings) in &mut update.channel_settings {
        let live_label = live.and_then(|(_, info)| detected_channel_label(info, channel_name));
        let memo = current
            .channel_settings
            .get(channel_name)
            .and_then(|channel| channel.label.clone());
        if let Some(stamped) = live_label.or(memo) {
            settings.label = Some(stamped);
        }
    }
    if let Some((detected_name, _)) = live {
        debug_assert_eq!(update.name, detected_name);
    }
    debug_assert!(update.name.is_empty().not() || current.name.is_empty());
}

/// Build a `temp_name` -> human label map for the device being acted upon.
/// Pulls from the live device's `TempInfo` / `ChannelInfo`, then overlays any
/// user-supplied label coming in on the update so the most recent label wins.
/// Falls back to an empty map when the device is not registered (blacklisted
/// devices may still own settings without a live record).
fn build_temp_channel_labels(
    all_devices: &AllDevices,
    device_uid: &DeviceUID,
    update: &CCDeviceSettings,
) -> HashMap<String, String> {
    let mut labels = HashMap::with_capacity(16);
    if let Some(device_lock) = all_devices.get(device_uid) {
        let lock = device_lock.borrow();
        for (name, info) in &lock.info.temps {
            labels.insert(name.clone(), info.label.clone());
        }
        for (name, info) in &lock.info.channels {
            if let Some(label) = info.label.as_ref() {
                labels.insert(name.clone(), label.clone());
            }
        }
    }
    for (name, settings) in &update.channel_settings {
        if let Some(label) = settings.label.as_ref().filter(|l| l.is_empty().not()) {
            labels.insert(name.clone(), label.clone());
        }
    }
    labels
}

/// Reject a `CCDeviceSettings` update if applying it would orphan any saved
/// Graph Profile `temp_source` or Custom Sensor source.
///
/// Delta semantics: only references made *newly* broken by this update are
/// reported. References already pointing at a previously-disabled channel are
/// the user's existing problem and do not block unrelated edits.
///
/// `channel_labels` maps each `temp_name` on the device being disabled to its
/// human label; the error message uses the label when present and falls back
/// to the raw name otherwise.
fn verify_disable_does_not_orphan_temp_sources(
    device_uid: &DeviceUID,
    update: &CCDeviceSettings,
    current: &CCDeviceSettings,
    profiles: &[Profile],
    custom_sensors: &[CustomSensor],
    channel_labels: &HashMap<String, String>,
) -> Result<(), CCError> {
    let newly_disabled_device = update.disable && current.disable.not();
    let already_disabled: HashSet<&str> = current
        .channel_settings
        .iter()
        .filter_map(|(name, settings)| settings.disabled.then_some(name.as_str()))
        .collect();
    let newly_disabled_channels: HashSet<&str> = update
        .channel_settings
        .iter()
        .filter_map(|(name, settings)| {
            (settings.disabled && already_disabled.contains(name.as_str()).not())
                .then_some(name.as_str())
        })
        .collect();
    if newly_disabled_device.not() && newly_disabled_channels.is_empty() {
        return Ok(());
    }
    let is_broken = |source: &TempSource| -> bool {
        if &source.device_uid != device_uid {
            return false;
        }
        newly_disabled_device || newly_disabled_channels.contains(source.temp_name.as_str())
    };
    let label_for = |temp_name: &str| -> String {
        channel_labels
            .get(temp_name)
            .cloned()
            .unwrap_or_else(|| temp_name.to_string())
    };
    let mut broken_profiles: Vec<(String, String)> = Vec::with_capacity(profiles.len());
    for profile in profiles {
        if profile.p_type() != ProfileType::Graph {
            continue;
        }
        let Some(source) = profile.temp_source() else {
            continue;
        };
        if is_broken(source) {
            broken_profiles.push((profile.name.clone(), label_for(&source.temp_name)));
        }
    }
    let mut broken_sensors: Vec<(String, String)> = Vec::with_capacity(custom_sensors.len());
    for sensor in custom_sensors {
        for sensor_source in sensor.sources() {
            if is_broken(&sensor_source.temp_source) {
                broken_sensors.push((
                    sensor.id.clone(),
                    label_for(&sensor_source.temp_source.temp_name),
                ));
            }
        }
    }
    if broken_profiles.is_empty() && broken_sensors.is_empty() {
        return Ok(());
    }
    Err(CCError::UserError {
        msg: build_orphan_error_message(&broken_profiles, &broken_sensors),
    })
}

fn build_orphan_error_message(
    broken_profiles: &[(String, String)],
    broken_sensors: &[(String, String)],
) -> String {
    let mut msg = String::with_capacity(256);
    msg.push_str("Cannot disable: the following references would break. Update them first.");
    if broken_profiles.is_empty().not() {
        msg.push_str("\n  Profiles:");
        for (name, channel) in broken_profiles {
            let _ = write!(msg, "\n    - \"{name}\" -> channel \"{channel}\"");
        }
    }
    if broken_sensors.is_empty().not() {
        msg.push_str("\n  Custom Sensors:");
        for (id, channel) in broken_sensors {
            let _ = write!(msg, "\n    - \"{id}\" -> channel \"{channel}\"");
        }
    }
    msg
}

#[cfg(test)]
mod tests {
    use super::{
        build_orphan_error_message, resolve_cc_device_name, resolve_channel_setting_labels,
        stamp_detection_memos, verify_disable_does_not_orphan_temp_sources, CCDeviceSettings,
        CCError, CustomSensor, Profile, TempSource,
    };
    use crate::device::{ChannelInfo, ChannelKind, DeviceInfo, TempInfo};
    use crate::overrides::OverridesController;
    use crate::setting::{
        CCChannelSettings, CustomSensorKind, CustomSensorMixFunctionType, CustomTempSourceData,
        DeviceExtensions, ProfileKind,
    };
    use std::collections::HashMap;
    use std::ops::Not;
    use std::path::PathBuf;

    const DEVICE_A: &str = "uid_a";
    const DEVICE_B: &str = "uid_b";

    fn settings(channels: &[(&str, bool)], device_disabled: bool) -> CCDeviceSettings {
        let mut channel_settings = HashMap::with_capacity(channels.len());
        for (name, disabled) in channels {
            channel_settings.insert(
                (*name).to_string(),
                CCChannelSettings {
                    label: None,
                    disabled: *disabled,
                    extension: None,
                },
            );
        }
        CCDeviceSettings {
            name: "test_device".to_string(),
            disable: device_disabled,
            extensions: DeviceExtensions::default(),
            channel_settings,
        }
    }

    fn graph_profile(name: &str, source_uid: &str, source_temp: &str) -> Profile {
        Profile {
            uid: format!("profile-{name}"),
            name: name.to_string(),
            kind: ProfileKind::Graph {
                speed_profile: None,
                temp_source: Some(TempSource {
                    temp_name: source_temp.to_string(),
                    device_uid: source_uid.to_string(),
                }),
                temp_min: None,
                temp_max: None,
            },
            ..Default::default()
        }
    }

    fn mix_sensor(id: &str, source_uid: &str, source_temp: &str) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            kind: CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Min,
                sources: vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        temp_name: source_temp.to_string(),
                        device_uid: source_uid.to_string(),
                    },
                }],
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    fn no_labels() -> HashMap<String, String> {
        HashMap::new()
    }

    fn assert_user_error_contains(result: Result<(), CCError>, needle: &str) {
        match result {
            Err(CCError::UserError { msg }) => assert!(
                msg.contains(needle),
                "expected msg to contain {needle:?}, got {msg:?}"
            ),
            other => panic!("expected UserError containing {needle:?}, got {other:?}"),
        }
    }

    #[test]
    fn disable_channel_referenced_by_graph_profile_returns_err() {
        // A Graph Profile points to (DEVICE_A, "Tctl"). Disabling "Tctl" must reject
        // the update so the user can rewire the Profile first.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![graph_profile("CPU Curve", DEVICE_A, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        assert_user_error_contains(result.clone(), "CPU Curve");
        assert_user_error_contains(result, "Tctl");
    }

    #[test]
    fn disable_device_referenced_by_graph_profile_returns_err() {
        // A device-level disable nukes every channel; any Graph Profile pointing at
        // this device's UID is orphaned regardless of which temp it referenced.
        let current = settings(&[], false);
        let update = settings(&[], true);
        let profiles = vec![graph_profile("GPU Aggressive", DEVICE_A, "edge")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        assert_user_error_contains(result, "GPU Aggressive");
    }

    #[test]
    fn disable_channel_referenced_by_custom_sensor_returns_err() {
        // A Mix Custom Sensor sources from (DEVICE_A, "Tctl"). Disabling "Tctl"
        // would break the sensor's mix calculation; reject and name the sensor.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let sensors = vec![mix_sensor("MyMix", DEVICE_A, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &[],
            &sensors,
            &no_labels(),
        );
        assert_user_error_contains(result, "MyMix");
    }

    #[test]
    fn disable_device_referenced_by_custom_sensor_returns_err() {
        // Device-wide disable also breaks any Custom Sensor sourcing from this device.
        let current = settings(&[], false);
        let update = settings(&[], true);
        let sensors = vec![mix_sensor("MyMix", DEVICE_A, "edge")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &[],
            &sensors,
            &no_labels(),
        );
        assert_user_error_contains(result, "MyMix");
    }

    #[test]
    fn error_message_lists_both_profiles_and_custom_sensors() {
        // Combined breakage produces a single message with both sections present.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![graph_profile("CPU Curve", DEVICE_A, "Tctl")];
        let sensors = vec![mix_sensor("MyMix", DEVICE_A, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &sensors,
            &no_labels(),
        );
        let Err(CCError::UserError { msg }) = result else {
            panic!("expected UserError, got {result:?}");
        };
        assert!(msg.contains("Profiles:"), "missing Profiles section: {msg}");
        assert!(
            msg.contains("Custom Sensors:"),
            "missing Custom Sensors section: {msg}"
        );
        assert!(msg.contains("CPU Curve"));
        assert!(msg.contains("MyMix"));
    }

    #[test]
    fn disable_unrelated_channel_succeeds() {
        // The Profile and Custom Sensor reference "Tctl"; the update disables a
        // different channel ("fan2"). Nothing breaks, validation must pass.
        let current = settings(&[("fan2", false)], false);
        let update = settings(&[("fan2", true)], false);
        let profiles = vec![graph_profile("CPU Curve", DEVICE_A, "Tctl")];
        let sensors = vec![mix_sensor("MyMix", DEVICE_A, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &sensors,
            &no_labels(),
        );
        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[test]
    fn unrelated_device_uid_is_ignored() {
        // We're disabling channels on DEVICE_A; Profile points at DEVICE_B.
        // Different device, different problem, must pass.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![graph_profile("Other", DEVICE_B, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[test]
    fn disable_already_disabled_channel_is_noop() {
        // The channel was already disabled in current state. A Profile already
        // referenced it (existing broken state). Resubmitting the same disable
        // introduces no NEW breakage, so delta semantics return Ok.
        let current = settings(&[("Tctl", true)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![graph_profile("CPU Curve", DEVICE_A, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[test]
    fn non_graph_profile_with_no_temp_source_is_ignored() {
        // Mix/Overlay/Fixed/Default Profiles either lack temp_source or don't
        // resolve it directly. They must never block a disable.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let mut fixed_profile = graph_profile("Fixed", DEVICE_A, "Tctl");
        fixed_profile.kind = ProfileKind::Fixed { speed_fixed: None };
        let mut mix_profile = graph_profile("Mix", DEVICE_A, "Tctl");
        mix_profile.kind = ProfileKind::Mix {
            member_profile_uids: Vec::new(),
            mix_function_type: None,
        };
        let profiles = vec![fixed_profile, mix_profile];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[test]
    fn file_type_custom_sensor_is_ignored() {
        // File-type Custom Sensors always have empty `sources`. They cannot
        // reference any hardware temp, so a disable must not block on them.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let file_sensor = CustomSensor {
            id: "FromFile".to_string(),
            kind: CustomSensorKind::File {
                file_path: PathBuf::from("/tmp/from_file"),
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &[],
            &[file_sensor],
            &no_labels(),
        );
        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[test]
    fn multiple_broken_profiles_listed_in_message() {
        // Two Graph Profiles share the same temp source. The error message must
        // list both so the user can fix them in one pass.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![
            graph_profile("Profile_One", DEVICE_A, "Tctl"),
            graph_profile("Profile_Two", DEVICE_A, "Tctl"),
        ];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        let Err(CCError::UserError { msg }) = result else {
            panic!("expected UserError, got {result:?}");
        };
        assert!(msg.contains("Profile_One"), "missing Profile_One: {msg}");
        assert!(msg.contains("Profile_Two"), "missing Profile_Two: {msg}");
    }

    #[test]
    fn error_message_substitutes_human_label_for_raw_temp_name() {
        // When channel_labels has an entry for the broken temp_name, the error
        // message must show the label (e.g. "CPU Tctl Temperature") instead of
        // the raw kernel name ("Tctl") that most users do not recognize. The
        // raw name should NOT appear in the message when a label is available.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![graph_profile("CPU Curve", DEVICE_A, "Tctl")];
        let sensors = vec![mix_sensor("MyMix", DEVICE_A, "Tctl")];
        let mut labels = HashMap::new();
        labels.insert("Tctl".to_string(), "CPU Tctl Temperature".to_string());
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &sensors,
            &labels,
        );
        let Err(CCError::UserError { msg }) = result else {
            panic!("expected UserError, got {result:?}");
        };
        assert!(
            msg.contains("CPU Tctl Temperature"),
            "label missing from msg: {msg}"
        );
        assert!(
            msg.contains("\"Tctl\"").not(),
            "raw temp_name leaked into msg: {msg}"
        );
    }

    #[test]
    fn error_message_falls_back_to_raw_name_when_label_missing() {
        // No labels supplied: the error message uses the raw temp_name.
        // This is the path hit when the device is blacklisted / not registered
        // and the actor cannot resolve labels from `info.temps`.
        let current = settings(&[("Tctl", false)], false);
        let update = settings(&[("Tctl", true)], false);
        let profiles = vec![graph_profile("CPU Curve", DEVICE_A, "Tctl")];
        let result = verify_disable_does_not_orphan_temp_sources(
            &DEVICE_A.to_string(),
            &update,
            &current,
            &profiles,
            &[],
            &no_labels(),
        );
        let Err(CCError::UserError { msg }) = result else {
            panic!("expected UserError, got {result:?}");
        };
        assert!(msg.contains("\"Tctl\""), "raw temp_name missing: {msg}");
    }

    #[test]
    fn empty_inputs_produce_empty_only_header() {
        // Sanity-check the message builder directly: with no broken references,
        // only the header is emitted and no section markers appear.
        let msg = build_orphan_error_message(&[], &[]);
        assert!(msg.starts_with("Cannot disable:"));
        assert!(msg.contains("Profiles:").not());
        assert!(msg.contains("Custom Sensors:").not());
    }

    /// Live device info with one temp and one fan channel, both labeled.
    fn live_info() -> DeviceInfo {
        let mut info = DeviceInfo::default();
        info.temps.insert(
            "temp1".to_string(),
            TempInfo {
                label: "Live Temp Label".to_string(),
                number: 1,
            },
        );
        info.channels.insert(
            "fan1".to_string(),
            ChannelInfo {
                label: Some("Live Fan Label".to_string()),
                kind: ChannelKind::default(),
            },
        );
        info
    }

    async fn empty_overrides() -> (tempfile::TempDir, OverridesController) {
        let tmp = tempfile::tempdir().unwrap();
        let controller = OverridesController::init_from(tmp.path().join("overrides.toml")).await;
        (tmp, controller)
    }

    #[test]
    fn stamp_ignores_client_values_when_device_is_live() {
        // Write deprecation: the client sends its own name and label, but the
        // daemon stamps the live detected values over both.
        let current = settings(&[], false);
        let mut update = settings(&[("temp1", false), ("fan1", false)], false);
        update.name = "Client Name".to_string();
        update.channel_settings.get_mut("temp1").unwrap().label = Some("Client Label".to_string());

        let info = live_info();
        stamp_detection_memos(&mut update, &current, Some(("Detected Device", &info)));

        assert_eq!(update.name, "Detected Device");
        assert_eq!(
            update.channel_settings.get("temp1").unwrap().label,
            Some("Live Temp Label".to_string())
        );
        assert_eq!(
            update.channel_settings.get("fan1").unwrap().label,
            Some("Live Fan Label".to_string())
        );
    }

    #[test]
    fn stamp_falls_back_to_saved_memo_when_not_live() {
        // A blacklisted device has no live record: the saved memos win over
        // whatever the client sends, so names survive daemon restarts.
        let mut current = settings(&[("temp1", false)], false);
        current.name = "Saved Memo Name".to_string();
        current.channel_settings.get_mut("temp1").unwrap().label =
            Some("Saved Memo Label".to_string());
        let mut update = settings(&[("temp1", false)], false);
        update.name = "Client Name".to_string();
        update.channel_settings.get_mut("temp1").unwrap().label = Some("Client Label".to_string());

        stamp_detection_memos(&mut update, &current, None);

        assert_eq!(update.name, "Saved Memo Name");
        assert_eq!(
            update.channel_settings.get("temp1").unwrap().label,
            Some("Saved Memo Label".to_string())
        );
    }

    #[test]
    fn stamp_keeps_client_value_only_as_last_resort() {
        // No live record and no saved memo: nothing better exists, so the
        // client-supplied values are kept rather than blanked.
        let mut current = settings(&[], false);
        current.name = String::new();
        let mut update = settings(&[("temp1", false)], false);
        update.name = "Client Name".to_string();
        update.channel_settings.get_mut("temp1").unwrap().label = Some("Client Label".to_string());

        stamp_detection_memos(&mut update, &current, None);

        assert_eq!(update.name, "Client Name");
        assert_eq!(
            update.channel_settings.get("temp1").unwrap().label,
            Some("Client Label".to_string())
        );
    }

    #[test]
    fn cc_dto_name_resolves_override_then_memo_then_live() {
        // Boundary resolution for the settings DTO name walks the layers:
        // override > saved memo > live detected name.
        crate::rt::test_runtime(async {
            let (_tmp, overrides) = empty_overrides().await;
            let uid = "uid_a".to_string();

            let resolved =
                resolve_cc_device_name(&overrides, &uid, "Memo", Some("Live".to_string()));
            assert_eq!(resolved, "Memo");
            let resolved = resolve_cc_device_name(&overrides, &uid, "", Some("Live".to_string()));
            assert_eq!(resolved, "Live");

            overrides
                .set_device_name(&uid, "hint", Some("Override"))
                .await
                .unwrap();
            let resolved =
                resolve_cc_device_name(&overrides, &uid, "Memo", Some("Live".to_string()));
            assert_eq!(resolved, "Override");
        });
    }

    #[test]
    fn cc_dto_channel_labels_resolve_override_then_memo_then_live() {
        // Channel labels in the settings DTO walk the same layers; a channel
        // with no override and no memo shows the live detected label.
        crate::rt::test_runtime(async {
            let (_tmp, overrides) = empty_overrides().await;
            let uid = "uid_a".to_string();
            let info = live_info();
            let mut channel_settings =
                settings(&[("temp1", false), ("fan1", false)], false).channel_settings;
            channel_settings.get_mut("fan1").unwrap().label = Some("Memo Label".to_string());
            overrides
                .set_channel_label(
                    &uid,
                    "hint",
                    &"temp1".to_string(),
                    None,
                    Some("Override Label"),
                )
                .await
                .unwrap();

            resolve_channel_setting_labels(&overrides, &uid, Some(&info), &mut channel_settings);

            assert_eq!(
                channel_settings.get("temp1").unwrap().label,
                Some("Override Label".to_string())
            );
            assert_eq!(
                channel_settings.get("fan1").unwrap().label,
                Some("Memo Label".to_string())
            );

            // Negative space: without override or memo, the live label lands.
            let mut plain = settings(&[("temp1", false)], false).channel_settings;
            resolve_channel_setting_labels(
                &overrides,
                &"uid_b".to_string(),
                Some(&info),
                &mut plain,
            );
            assert_eq!(
                plain.get("temp1").unwrap().label,
                Some("Live Temp Label".to_string())
            );
        });
    }
}
