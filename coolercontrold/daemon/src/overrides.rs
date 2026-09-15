// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! User-defined display-name overrides, stored in `overrides.toml`.
//!
//! Resolution is layered, highest first:
//!
//! 1. these overrides, which the user set in `CoolerControl`
//! 2. the user's lm-sensors configuration, see [`crate::sensors_conf`]
//! 3. the label the driver reports
//! 4. the raw channel name
//!
//! This controller owns the top two layers and answers the whole chain. The layers are applied
//! at different points and cannot be collapsed into one: an override can change while the daemon
//! runs, so it is applied per request at the DTO boundary, while the lm-sensors layer is fixed at
//! startup and is folded into the label a repository detects. Baking an override in at detection
//! time would leave a stale name behind when the user later clears it.
//!
//! Resolution needs the detected labels, which only exist once devices have been detected, so the
//! device map is bound after the fact by [`OverridesController::set_device_context`]. Log lines
//! resolve through the same chain as the DTO boundary, in the form `Resolved (raw)`, keeping the
//! raw name a log line can be grepped by.
//!
//! The overrides file is hand-editable; edits are read only at startup. Entries are pruned only on
//! deliberate entity deletion, never on hardware absence.

use std::borrow::Cow;
use std::cell::{Cell, OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Value};

use crate::api::{is_forbidden_name_char, validate_name_string};
use crate::cc_fs;
use crate::config::Config;
use crate::device::{ChannelName, DeviceName, DeviceUID};
use crate::paths;
use crate::repositories::hwmon::chip_name::ChipName;
use crate::sensors_conf::SensorsConf;
use crate::AllDevices;

const BANNER: &str = "\
# CoolerControl display-name overrides.
# Managed by coolercontrold: comments are not preserved on rewrite.
# Hand edits are applied at the next daemon startup.
";

/// Upper bound on channel overrides stored per device. Real hardware has far
/// fewer channels, so this only caps accidental or abusive growth of the file.
const MAX_CHANNEL_OVERRIDES_PER_DEVICE: usize = 512;

/// Owns the overrides document and the lm-sensors layer below it, and resolves display names
/// against both.
pub struct OverridesController {
    path: PathBuf,
    /// The layer under our own overrides. Read-only and fixed at startup.
    sensors_conf: Rc<SensorsConf>,
    document: RefCell<OverridesDocument>,
    /// Serializes read-modify-write cycles. Multiple actors write (settings
    /// renames, the custom sensor delete cascade) and a cycle spans await
    /// points, so unserialized writes could interleave and lose one.
    write_lock: tokio::sync::Mutex<()>,
    /// The layers below lm-sensors, captured at startup. See [`Self::capture_detected_names`].
    detected: OnceCell<DetectedNames>,
}

/// The detected layers of the chain, copied once devices have been detected.
///
/// Values rather than a handle on the device map, because every layer below the user's
/// overrides is fixed at detection: a device's name, the driver's channel labels, and the
/// lm-sensors labels already folded into them. Copying buys two things a live handle cannot.
/// It keeps this controller free of any reference back into a device, which would otherwise
/// close a cycle: a device owns its status augmenter, the calibration augmenter owns the
/// calibration store, and the store resolves names through here. And it keeps resolution off
/// the device `RefCell`s, so a log line can never collide with a status update mid-borrow.
///
/// The one thing that does move afterwards is a custom sensor added or removed at runtime
/// (`CustomSensorsRepo::update_device_info_temps`). Its label is the sensor id in title case,
/// so a sensor missing from here logs `sensor9` instead of `Sensor 9 (sensor9)`, and a user
/// override still names it either way.
#[derive(Default)]
struct DetectedNames {
    /// Layer 3 for devices: the detected name, falling back to the config `devices` list, which
    /// is never pruned and so still names hardware that is no longer present.
    devices: HashMap<DeviceUID, DeviceName>,
    /// Layer 3 for channels: the label the driver reports, with any lm-sensors label already
    /// folded in by the repository that detected it.
    channels: HashMap<DeviceUID, HashMap<ChannelName, String>>,
}

impl OverridesController {
    /// Loads the overrides file from the standard config location, running
    /// the one-time `config-ui.json` name migration when the file is absent.
    pub async fn init() -> Self {
        Self::init_with_migration(
            paths::overrides_file().to_path_buf(),
            paths::ui_config_file(),
        )
        .await
    }

    /// Runs the one-time migration gate, then loads: when the overrides file
    /// is absent, user-defined names are imported from the UI settings blob
    /// and written out, closing the gate for subsequent boots. Migration is
    /// tolerant: any failure degrades to an empty store, never blocks
    /// startup, and never overwrites an existing overrides file.
    pub async fn init_with_migration(path: PathBuf, ui_config_path: &Path) -> Self {
        if path.exists().not() {
            if let Some(document) = migrate_from_ui_settings(ui_config_path).await {
                info!(
                    "Migrated name overrides for {} device(s) from UI settings",
                    document.devices.len()
                );
                if let Err(err) = save(&path, &document).await {
                    warn!("Failed to write migrated overrides file: {err:#}");
                }
                return Self {
                    path,
                    sensors_conf: Rc::new(SensorsConf::default()),
                    document: RefCell::new(document),
                    write_lock: tokio::sync::Mutex::new(()),
                    detected: OnceCell::new(),
                };
            }
        }
        Self::init_from(path).await
    }

    /// Loads the overrides file at `path`. An absent file is an empty store;
    /// the file is only created on first write. A malformed file logs a
    /// warning and resolves nothing, and writes then fail on the re-read so
    /// a hand-edited file is never silently clobbered.
    pub async fn init_from(path: PathBuf) -> Self {
        let document = match load(&path).await {
            Ok(document) => document,
            Err(err) => {
                warn!("Ignoring overrides file {}: {err:#}", path.display());
                OverridesDocument::default()
            }
        };
        if document.devices.is_empty().not() {
            info!(
                "Loaded name overrides for {} device(s)",
                document.devices.len()
            );
        }
        Self {
            path,
            sensors_conf: Rc::new(SensorsConf::default()),
            document: RefCell::new(document),
            write_lock: tokio::sync::Mutex::new(()),
            detected: OnceCell::new(),
        }
    }

    /// Attaches the lm-sensors layer. Separate from the constructors because those load the
    /// overrides document, which has nothing to say about lm-sensors.
    #[must_use]
    pub fn with_sensors_conf(mut self, sensors_conf: Rc<SensorsConf>) -> Self {
        self.sensors_conf = sensors_conf;
        self
    }

    /// An empty store not backed by a file, for tests of components that
    /// hold the controller but do not exercise name persistence.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            sensors_conf: Rc::new(SensorsConf::default()),
            document: RefCell::new(OverridesDocument::default()),
            write_lock: tokio::sync::Mutex::new(()),
            detected: OnceCell::new(),
        }
    }

    /// A copy of the raw, sparse overrides document.
    pub fn document(&self) -> OverridesDocument {
        self.document.borrow().clone()
    }

    /// The label the user's lm-sensors configuration gives a channel, which is the layer directly
    /// under our own overrides, paired with the file that set it so the choice can be reported.
    ///
    /// Takes the chip rather than a device UID because lm-sensors names chips, not our devices.
    /// A device we could not identify a chip for has nothing to match against.
    pub fn sensors_conf_label_source(
        &self,
        chip: Option<&ChipName>,
        channel_name: &str,
    ) -> Option<(&str, &Path)> {
        self.sensors_conf.label_source(chip?, channel_name)
    }

    /// The lm-sensors configuration file that hides a channel, if one does.
    ///
    /// Hiding is not a name, but it comes from the same file and follows the same rule that our
    /// own settings win, so it is answered here too.
    pub fn sensors_conf_ignore_source(
        &self,
        chip: Option<&ChipName>,
        channel_name: &str,
    ) -> Option<&Path> {
        self.sensors_conf.ignore_source(chip?, channel_name)
    }

    /// The user-set device name override, if any.
    pub fn device_name_override(&self, device_uid: &DeviceUID) -> Option<DeviceName> {
        self.document
            .borrow()
            .devices
            .get(device_uid)
            .and_then(|device| device.name.clone())
    }

    /// The user-set channel label override, if any.
    pub fn channel_label_override(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
    ) -> Option<String> {
        self.document
            .borrow()
            .devices
            .get(device_uid)
            .and_then(|device| device.channels.get(channel_name))
            .and_then(|channel| channel.label.clone())
    }

    /// Copies the detected layers in. They cannot be constructor arguments: the repositories
    /// consume this controller to build the very labels being copied, so nothing has been
    /// detected yet when it is built. Called once at startup, right after the device map is
    /// created. Until then, and in tests, resolution simply skips the layers it cannot see.
    pub fn capture_detected_names(&self, all_devices: &AllDevices, config: &Config) {
        let mut detected = DetectedNames::default();
        // The config list first: it holds hardware that is no longer present, and the live
        // devices below overwrite it for everything detected this boot.
        for (device_uid, name) in config.device_names() {
            detected.devices.insert(device_uid, name);
        }
        for (device_uid, device_lock) in all_devices.iter() {
            let device = device_lock.borrow();
            detected
                .devices
                .insert(device_uid.clone(), device.name.clone());
            let mut labels = HashMap::new();
            for (channel_name, channel) in &device.info.channels {
                if let Some(label) = channel.label.clone() {
                    labels.insert(channel_name.clone(), label);
                }
            }
            // Temps last: they win the same name, matching `DeviceInfo::detected_channel_label`.
            for (temp_name, temp) in &device.info.temps {
                labels.insert(temp_name.clone(), temp.label.clone());
            }
            if labels.is_empty().not() {
                detected.channels.insert(device_uid.clone(), labels);
            }
        }
        let captured = self.detected.set(detected);
        debug_assert!(
            captured.is_ok(),
            "detected names must be captured exactly once"
        );
    }

    /// The label the driver reports for a channel, layer 3. `None` when the device is unknown or
    /// reports no label for it.
    fn detected_channel_label(&self, device_uid: &DeviceUID, channel_name: &str) -> Option<String> {
        self.detected
            .get()?
            .channels
            .get(device_uid)?
            .get(channel_name)
            .cloned()
    }

    /// The device's detected name, or the one the config list remembers for hardware that is no
    /// longer present.
    fn detected_device_name(&self, device_uid: &DeviceUID) -> Option<DeviceName> {
        self.detected.get()?.devices.get(device_uid).cloned()
    }

    /// The chain's answer for a device, or `None` when no layer names it: not overridden, not
    /// detected, and absent from the config device list. Callers that must render something
    /// either way want [`Self::log_device_name`] instead.
    pub fn known_device_name(&self, device_uid: &DeviceUID) -> Option<DeviceName> {
        self.device_name_override(device_uid)
            .or_else(|| self.detected_device_name(device_uid))
    }

    /// The whole chain for a device, highest layer first. `detected` is the caller's own answer
    /// for layer 3, which callers holding the device pass to save a map lookup.
    fn chain_device_name(
        &self,
        device_uid: &DeviceUID,
        detected: Option<&str>,
        raw_name: &str,
    ) -> DeviceName {
        self.device_name_override(device_uid)
            .or_else(|| detected.map(str::to_owned))
            .or_else(|| self.detected_device_name(device_uid))
            .unwrap_or_else(|| raw_name.to_owned())
    }

    /// The whole chain for a channel, highest layer first. The lm-sensors layer sits inside
    /// `detected`: a repository folds it into the label it reports at detection time.
    fn chain_channel_label(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        detected: Option<String>,
    ) -> String {
        self.channel_label_override(device_uid, channel_name)
            .or(detected)
            .or_else(|| self.detected_channel_label(device_uid, channel_name))
            .unwrap_or_else(|| channel_name.to_owned())
    }

    /// Resolves a device display name. Layer order: override > detected > raw.
    pub fn resolve_device_name(
        &self,
        device_uid: &DeviceUID,
        detected: Option<&str>,
        raw_name: &str,
    ) -> DeviceName {
        self.chain_device_name(device_uid, detected, raw_name)
    }

    /// Channel display label: override > detected > raw. Plain, not the
    /// `Resolved (raw)` log form, and sanitized because it reaches log lines.
    pub fn resolve_channel_label(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        detected: Option<String>,
    ) -> String {
        let label = self.chain_channel_label(device_uid, channel_name, detected);
        sanitize_for_log(&label).into_owned()
    }

    /// Log display form of a device name: `Resolved (raw)` when the chain answers something
    /// other than the raw name, plain raw otherwise.
    pub fn log_device_name(&self, device_uid: &DeviceUID, raw_name: &str) -> String {
        format_log_name(
            Some(self.chain_device_name(device_uid, None, raw_name)),
            raw_name,
        )
    }

    /// Log display form of a channel name: `Resolved (raw)` when the chain answers something
    /// other than the raw channel key, plain raw otherwise. The raw key is kept so a log line
    /// still names the channel the way the config files and sysfs do.
    pub fn log_channel_name(&self, device_uid: &DeviceUID, channel_name: &str) -> String {
        format_log_name(
            Some(self.chain_channel_label(device_uid, channel_name, None)),
            channel_name,
        )
    }

    /// Log display form of a device and channel pair: `Device (raw) | Channel (raw)`. The one
    /// call sites use when they hold nothing but the pair of identifiers.
    pub fn log_device_channel(&self, device_uid: &DeviceUID, channel_name: &str) -> String {
        let raw_device_name = self
            .detected_device_name(device_uid)
            .unwrap_or_else(|| unnamed_device(device_uid));
        format!(
            "{} | {}",
            self.log_device_name(device_uid, &raw_device_name),
            self.log_channel_name(device_uid, channel_name)
        )
    }

    /// Sets or removes (`None`) the device name override.
    /// `device_name_hint` refreshes the hand-editor hint line.
    pub async fn set_device_name(
        &self,
        device_uid: &DeviceUID,
        device_name_hint: &str,
        name: Option<&str>,
    ) -> Result<()> {
        let name = name.map(validate_name).transpose()?;
        self.read_modify_write(|document| {
            let device = document.devices.entry(device_uid.clone()).or_default();
            device.device_name = Some(device_name_hint.to_owned());
            device.name = name;
            Ok(())
        })
        .await
    }

    /// Sets or removes (`None`) the channel label override. The two hints
    /// refresh the hand-editor hint fields; a `None` hint keeps whatever
    /// hint is already stored.
    pub async fn set_channel_label(
        &self,
        device_uid: &DeviceUID,
        device_name_hint: &str,
        channel_name: &ChannelName,
        channel_label_hint: Option<&str>,
        label: Option<&str>,
    ) -> Result<()> {
        // The channel name is a TOML key not bounded by a liveness check, so
        // hold it to the same character and length policy as a name value.
        validate_name_string(channel_name)?;
        let label = label.map(validate_name).transpose()?;
        self.read_modify_write(|document| {
            let device = document.devices.entry(device_uid.clone()).or_default();
            device.device_name = Some(device_name_hint.to_owned());
            match label {
                Some(label) => {
                    let is_new = device.channels.contains_key(channel_name).not();
                    if is_new && device.channels.len() >= MAX_CHANNEL_OVERRIDES_PER_DEVICE {
                        return Err(anyhow!(
                            "device {device_uid} has reached the maximum of \
                             {MAX_CHANNEL_OVERRIDES_PER_DEVICE} channel overrides"
                        ));
                    }
                    let channel = device.channels.entry(channel_name.clone()).or_default();
                    if let Some(hint) = channel_label_hint {
                        channel.channel_label = Some(hint.to_owned());
                    }
                    channel.label = Some(label);
                }
                None => {
                    if let Some(channel) = device.channels.get_mut(channel_name) {
                        channel.label = None;
                    }
                }
            }
            Ok(())
        })
        .await
    }

    /// Removes every override for `channel_name`. Cascade for deliberate
    /// entity deletion: custom sensor IDs are recycled, so a new sensor
    /// must not inherit a deleted sensor's overrides.
    pub async fn remove_channel(
        &self,
        device_uid: &DeviceUID,
        channel_name: &ChannelName,
    ) -> Result<()> {
        self.read_modify_write(|document| {
            if let Some(device) = document.devices.get_mut(device_uid) {
                device.channels.remove(channel_name);
            }
            Ok(())
        })
        .await
    }

    /// An entry already under `current` is left alone: merging would pick a winner. Reports
    /// what the on-disk pass did, not the in-memory probe: a hand-edit can land in between.
    pub async fn migrate_device_uid(
        &self,
        legacy: &DeviceUID,
        current: &DeviceUID,
    ) -> Result<bool> {
        {
            let document = self.document.borrow();
            if document.devices.contains_key(legacy).not() {
                return Ok(false);
            }
            if document.devices.contains_key(current) {
                return Ok(false);
            }
        }
        let moved = Cell::new(false);
        self.read_modify_write(|document| {
            if document.devices.contains_key(current) {
                return Ok(());
            }
            if let Some(device_overrides) = document.devices.remove(legacy) {
                document.devices.insert(current.clone(), device_overrides);
                moved.set(true);
            }
            Ok(())
        })
        .await?;
        Ok(moved.get())
    }

    /// Read-modify-write against the file on disk. Re-reading narrows the
    /// window where a daemon write clobbers a concurrent hand-edit. An `apply`
    /// that returns an error aborts before any write, leaving the file intact.
    async fn read_modify_write<F>(&self, apply: F) -> Result<()>
    where
        F: FnOnce(&mut OverridesDocument) -> Result<()>,
    {
        let _write_guard = self.write_lock.lock().await;
        let mut document = load(&self.path).await?;
        apply(&mut document)?;
        prune(&mut document);
        debug_assert!(is_pruned(&document));
        save(&self.path, &document).await?;
        debug_assert!(self.document.try_borrow_mut().is_ok());
        self.document.replace(document);
        Ok(())
    }
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OverridesDocument {
    #[serde(default)]
    pub devices: BTreeMap<DeviceUID, DeviceOverrides>,
}

#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceOverrides {
    /// Daemon-written hint for hand-editors, ignored on read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_name: Option<DeviceName>,
    /// User override for the device display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<DeviceName>,
    /// Temps and channels share one namespace, keyed by raw channel name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub channels: BTreeMap<ChannelName, ChannelOverrides>,
}

/// A table per channel so color/ignore/compute can slot in later.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelOverrides {
    /// Daemon-written detected-label hint for hand-editors and rename
    /// dialogs, ignored on read (never a resolution layer).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

async fn save(path: &Path, document: &OverridesDocument) -> Result<()> {
    let contents = render(document)?;
    assert!(contents.starts_with(BANNER));
    // Write to a sibling temp file then rename over the target: a crash
    // mid-write cannot leave a partial file that would then permanently fail
    // every read-modify-write (which refuses to clobber an unparseable file).
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp_path = PathBuf::from(tmp);
    cc_fs::write_string(&tmp_path, contents)
        .await
        .with_context(|| format!("Writing overrides temp file {}", tmp_path.display()))?;
    cc_fs::rename(&tmp_path, path)
        .await
        .with_context(|| format!("Publishing overrides file {}", path.display()))
}

/// Reads and parses the file on disk. An absent file is an empty document.
async fn load(path: &Path) -> Result<OverridesDocument> {
    if path.exists().not() {
        return Ok(OverridesDocument::default());
    }
    let contents = cc_fs::read_txt(path)
        .await
        .with_context(|| format!("Reading overrides file {}", path.display()))?;
    let document = toml_edit::de::from_str::<OverridesDocument>(&contents)
        .with_context(|| format!("Parsing overrides file {}", path.display()))?;
    Ok(document)
}

/// Validates that `contents` parse as an overrides document.
pub fn validate(contents: &str) -> Result<()> {
    toml_edit::de::from_str::<OverridesDocument>(contents)
        .with_context(|| "Parsing overrides configuration")
        .map(|_| ())
}

/// One place owns the log format so it cannot drift per call site. Names are
/// sanitized here because hand-edited overrides bypass intake validation, so
/// the log boundary re-applies the injection-character policy.
fn format_log_name(resolved_name: Option<String>, raw_name: &str) -> String {
    let raw = sanitize_for_log(raw_name);
    match resolved_name {
        Some(name) if name != raw_name => format!("{} ({raw})", sanitize_for_log(&name)),
        _ => raw.into_owned(),
    }
}

/// UID characters kept when nothing names a device. Enough to stay unique in practice and to
/// grep the config files with, without pasting a 64 character hash into a log line.
const UNNAMED_DEVICE_UID_CHARS: usize = 12;

/// Stands in for a device no layer can name: not detected, and absent from the config device
/// list. Rare, and the full UID is not worth a log line when it happens.
fn unnamed_device(device_uid: &DeviceUID) -> String {
    let short: String = device_uid.chars().take(UNNAMED_DEVICE_UID_CHARS).collect();
    format!("unknown device ({short})")
}

/// Drops injection-capable characters from a name destined for a log line.
fn sanitize_for_log(name: &str) -> Cow<'_, str> {
    if name.chars().any(is_forbidden_name_char) {
        Cow::Owned(
            name.chars()
                .filter(|c| is_forbidden_name_char(*c).not())
                .collect(),
        )
    } else {
        Cow::Borrowed(name)
    }
}

/// Trims a user-supplied name and validates it with the daemon's
/// canonical name rules (non-empty, length cap, no control characters).
fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    validate_name_string(trimmed)?;
    debug_assert!(trimmed.is_empty().not());
    debug_assert_eq!(trimmed, trimmed.trim());
    Ok(trimmed.to_owned())
}

/// One-time import of user-defined names from the UI settings blob
/// (`config-ui.json`). Returns `None` when there is nothing to import or
/// the blob is unreadable; the caller then starts with an empty store and
/// the gate (overrides file absence) stays open for the next boot.
async fn migrate_from_ui_settings(ui_config_path: &Path) -> Option<OverridesDocument> {
    if ui_config_path.exists().not() {
        return None;
    }
    let contents = match cc_fs::read_txt(ui_config_path).await {
        Ok(contents) => contents,
        Err(err) => {
            warn!("Skipping name migration, could not read UI settings: {err:#}");
            return None;
        }
    };
    let mirror = match serde_json::from_str::<UiSettingsMirror>(&contents) {
        Ok(mirror) => mirror,
        Err(err) => {
            warn!("Skipping name migration, could not parse UI settings: {err}");
            return None;
        }
    };
    let mut document = OverridesDocument::default();
    for (device_uid, settings) in mirror.devices.iter().zip(mirror.device_settings.iter()) {
        let mut device = DeviceOverrides {
            name: migrated_name(settings.user_name.as_deref()),
            ..Default::default()
        };
        let channels = settings
            .names
            .iter()
            .zip(settings.sensor_and_channel_settings.iter());
        for (channel_name, channel) in channels {
            if let Some(label) = migrated_name(channel.user_name.as_deref()) {
                device.channels.insert(
                    channel_name.clone(),
                    ChannelOverrides {
                        channel_label: None,
                        label: Some(label),
                    },
                );
            }
        }
        document.devices.insert(device_uid.clone(), device);
    }
    prune(&mut document);
    if document.devices.is_empty() {
        return None;
    }
    debug_assert!(is_pruned(&document));
    debug_assert!(document.devices.len() <= mirror.devices.len());
    Some(document)
}

/// Prepares a migrated name: trims, drops empties, and rejects
/// injection-capable characters (debug-escaped in the log). Unlike API
/// intake there is no length cap: pre-existing user data is preserved.
fn migrated_name(name: Option<&str>) -> Option<DeviceName> {
    let trimmed = name?.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.chars().any(is_forbidden_name_char) {
        warn!("Skipping migration of unsafe name {trimmed:?}");
        return None;
    }
    Some(trimmed.to_owned())
}

/// Minimal tolerant mirror of the UI settings blob: only the name fields.
/// The blob stores maps as parallel arrays (`devices[i]` pairs with
/// `device_settings[i]`, `names[j]` with `sensor_and_channel_settings[j]`).
#[derive(Deserialize)]
struct UiSettingsMirror {
    #[serde(default)]
    devices: Vec<DeviceUID>,
    #[serde(default, rename = "deviceSettings")]
    device_settings: Vec<UiDeviceSettingsMirror>,
}

#[derive(Deserialize)]
struct UiDeviceSettingsMirror {
    #[serde(default, rename = "userName")]
    user_name: Option<String>,
    #[serde(default)]
    names: Vec<ChannelName>,
    #[serde(default, rename = "sensorAndChannelSettings")]
    sensor_and_channel_settings: Vec<UiChannelSettingsMirror>,
}

#[derive(Deserialize)]
struct UiChannelSettingsMirror {
    #[serde(default, rename = "userName")]
    user_name: Option<String>,
}

/// True when every entry carries user data (the `prune` postcondition).
fn is_pruned(document: &OverridesDocument) -> bool {
    document
        .devices
        .values()
        .all(|device| device.name.is_some() || device.channels.is_empty().not())
}

/// Drops entries that no longer carry user data (a hint alone is not data).
fn prune(document: &mut OverridesDocument) {
    for device in document.devices.values_mut() {
        device.channels.retain(|_, channel| channel.label.is_some());
    }
    document
        .devices
        .retain(|_, device| device.name.is_some() || device.channels.is_empty().not());
}

/// Renders the document with the banner and inline channel tables.
fn render(document: &OverridesDocument) -> Result<String> {
    let mut doc =
        toml_edit::ser::to_document(document).context("Serializing overrides document")?;
    expand_layout(&mut doc);
    let rendered = format!("{BANNER}\n{doc}");
    debug_assert!(
        toml_edit::de::from_str::<OverridesDocument>(&rendered)
            .is_ok_and(|reparsed| reparsed == *document),
        "rendered overrides must parse back identically"
    );
    Ok(rendered)
}

/// The serializer emits one nested inline value; expand the outer levels so
/// each device gets a `[devices.<uid>]` header and a
/// `[devices.<uid>.channels]` header, while each channel entry stays an
/// inline `fan1 = { label = "..." }` table.
fn expand_layout(doc: &mut DocumentMut) {
    let Some(devices) = doc.get_mut("devices") else {
        return;
    };
    expand_to_table(devices);
    let Some(devices) = devices.as_table_mut() else {
        return;
    };
    devices.set_implicit(true);
    for (_, device) in devices.iter_mut() {
        expand_to_table(device);
        let Some(channels) = device.get_mut("channels") else {
            continue;
        };
        expand_to_table(channels);
    }
}

/// Converts an inline table item into a standard table with a header.
fn expand_to_table(item: &mut Item) {
    if let Item::Value(Value::InlineTable(inline)) = item {
        let table = std::mem::take(inline).into_table();
        *item = Item::Table(table);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE_UID: &str = "abc123";
    const HINT: &str = "Nuvoton NCT6798-D";

    fn overrides_path(tmp: &tempfile::TempDir) -> PathBuf {
        tmp.path().join("overrides.toml")
    }

    const LEGACY_UID: &str = "8ef08515338cc1f7727415a1d6bc45e84d5a818e94bd2072ac34b9d8d87f4210";
    const CURRENT_UID: &str = "67ada747f6820bb2e4a9b5b639657e0971998a6f5ca5f50480ecf36b46c7ec29";

    #[test]
    fn migrate_device_uid_moves_overrides_and_persists() {
        // Goal: a drive's label overrides follow it onto the wwid-derived UID, and the
        // move survives a fresh load, so the rename is on disk and not just in memory.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let legacy = LEGACY_UID.to_string();
            let current = CURRENT_UID.to_string();
            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_device_name(&legacy, HINT, Some("SSD OS"))
                .await
                .unwrap();

            let moved = controller
                .migrate_device_uid(&legacy, &current)
                .await
                .unwrap();

            assert!(moved);
            assert_eq!(
                controller.device_name_override(&current),
                Some("SSD OS".to_string())
            );
            assert_eq!(controller.device_name_override(&legacy), None);
            let reloaded = OverridesController::init_from(path).await;
            assert_eq!(
                reloaded.device_name_override(&current),
                Some("SSD OS".to_string())
            );
        });
    }

    #[test]
    fn migrate_device_uid_does_not_clobber_existing_overrides() {
        // Goal: negative space. Real overrides already stored under the new UID must win,
        // since merging two entries would silently discard one of the user's names.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let legacy = LEGACY_UID.to_string();
            let current = CURRENT_UID.to_string();
            let controller = OverridesController::init_from(path).await;
            controller
                .set_device_name(&legacy, HINT, Some("old"))
                .await
                .unwrap();
            controller
                .set_device_name(&current, HINT, Some("keep me"))
                .await
                .unwrap();

            let moved = controller
                .migrate_device_uid(&legacy, &current)
                .await
                .unwrap();

            assert!(moved.not());
            assert_eq!(
                controller.device_name_override(&current),
                Some("keep me".to_string())
            );
        });
    }

    #[test]
    fn migrate_device_uid_is_a_noop_without_overrides() {
        // Goal: the common case. Almost no device has overrides, so the migration must
        // cost nothing and must not create the file (which would close the migration gate).
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let controller = OverridesController::init_from(path.clone()).await;

            let moved = controller
                .migrate_device_uid(&LEGACY_UID.to_string(), &CURRENT_UID.to_string())
                .await
                .unwrap();

            assert!(moved.not());
            assert!(path.exists().not());
        });
    }

    #[test]
    fn absent_file_starts_empty_and_is_not_created() {
        // Goal: an absent file yields an empty store and stays absent, so the
        // config-ui.json migration gate (file absence) is not broken by init.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);

            let controller = OverridesController::init_from(path.clone()).await;

            let uid = DEVICE_UID.to_string();
            assert_eq!(controller.device_name_override(&uid), None);
            assert_eq!(controller.resolve_device_name(&uid, None, "raw"), "raw");
            assert!(path.exists().not());
        });
    }

    #[test]
    fn set_device_name_round_trips_through_disk() {
        // Goal: a set override resolves immediately and survives a fresh
        // load from the same file (write path and read path agree).
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();
            assert_eq!(
                controller.resolve_device_name(&uid, Some("detected"), "raw"),
                "Motherboard"
            );

            let reloaded = OverridesController::init_from(path).await;
            assert_eq!(
                reloaded.device_name_override(&uid),
                Some("Motherboard".to_string())
            );
        });
    }

    #[test]
    fn resolution_layers_apply_in_order() {
        // Goal: verify the layer order override > detected > raw for the
        // device resolver, including the negative space (no override).
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(overrides_path(&tmp)).await;
            assert_eq!(
                controller.resolve_device_name(&uid, Some("detected"), "raw"),
                "detected"
            );
            assert_eq!(controller.resolve_device_name(&uid, None, "raw"), "raw");

            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();
            assert_eq!(
                controller.resolve_device_name(&uid, Some("detected"), "raw"),
                "Motherboard"
            );
        });
    }

    /// A one-device map plus the config device list, the two layers bound after detection.
    fn device_context(
        name: &str,
        channels: &[(&str, Option<&str>)],
    ) -> (DeviceUID, AllDevices, Rc<Config>) {
        let mut info = crate::device::DeviceInfo::default();
        for (channel_name, label) in channels {
            info.channels.insert(
                (*channel_name).to_string(),
                crate::device::ChannelInfo {
                    label: label.map(ToString::to_string),
                    ..Default::default()
                },
            );
        }
        let device = Rc::new(RefCell::new(crate::device::Device::new(
            name.to_string(),
            crate::device::DeviceType::Hwmon,
            0,
            None,
            info,
            None,
            1.0,
        )));
        let uid = device.borrow().uid.clone();
        let all_devices: AllDevices = Rc::new(HashMap::from([(uid.clone(), device)]));
        let config = Rc::new(Config::init_default_config().unwrap());
        config.create_device_list(&all_devices);
        (uid, all_devices, config)
    }

    #[test]
    fn channel_log_name_falls_back_to_the_driver_label() {
        // Goal: a log line names a channel the driver labelled, without the user having to
        // rename it first. The raw key is kept so the line still maps to sysfs.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let (uid, all_devices, config) =
                device_context("nct6798", &[("fan1", Some("CPU Fan")), ("fan2", None)]);
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;
            controller.capture_detected_names(&all_devices, &config);

            assert_eq!(controller.log_channel_name(&uid, "fan1"), "CPU Fan (fan1)");
            // Negative space: a channel the driver does not label stays the bare key rather
            // than growing an empty parenthetical.
            assert_eq!(controller.log_channel_name(&uid, "fan2"), "fan2");

            controller
                .set_channel_label(&uid, HINT, &"fan1".to_string(), None, Some("Front Intake"))
                .await
                .unwrap();
            assert_eq!(
                controller.log_channel_name(&uid, "fan1"),
                "Front Intake (fan1)"
            );
        });
    }

    #[test]
    fn resolution_does_not_touch_a_borrowed_device() {
        // Goal: names are copied at startup, not read live, so a log line emitted while a
        // repository holds a device borrowed for a status update cannot panic the daemon.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let (uid, all_devices, config) =
                device_context("nct6798", &[("fan1", Some("CPU Fan"))]);
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;
            controller.capture_detected_names(&all_devices, &config);

            let device = all_devices.get(&uid).unwrap();
            let borrowed = device.borrow_mut();
            assert_eq!(
                controller.log_device_channel(&uid, "fan1"),
                "nct6798 | CPU Fan (fan1)"
            );
            drop(borrowed);
        });
    }

    #[test]
    fn device_log_name_falls_back_to_the_persisted_list() {
        // Goal: a calibration or setting outlives its hardware, so a device absent at boot is
        // still named from the config device list, which is never pruned.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let (uid, all_devices, config) = device_context("nct6798", &[("fan1", None)]);
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;
            controller.capture_detected_names(&all_devices, &config);

            assert_eq!(
                controller.log_device_channel(&uid, "fan1"),
                "nct6798 | fan1"
            );

            // The same UID with the device gone: the persisted list still names it.
            let absent: AllDevices = Rc::new(HashMap::new());
            let absent_controller =
                OverridesController::init_from(overrides_path(&tmp).with_extension("two")).await;
            absent_controller.capture_detected_names(&absent, &config);
            assert_eq!(
                absent_controller.log_device_channel(&uid, "fan1"),
                "nct6798 | fan1"
            );
        });
    }

    #[test]
    fn device_log_name_marks_a_uid_no_layer_knows() {
        // Goal: the one case nothing can name reads as an unknown device, and never pastes a
        // 64 character hash into the log line.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let (_, all_devices, config) = device_context("nct6798", &[("fan1", None)]);
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;
            controller.capture_detected_names(&all_devices, &config);

            let stranger = CURRENT_UID.to_string();
            let logged = controller.log_device_channel(&stranger, "fan1");
            assert_eq!(
                logged,
                format!(
                    "unknown device ({}) | fan1",
                    &CURRENT_UID[..UNNAMED_DEVICE_UID_CHARS]
                )
            );
            assert!(logged.contains(CURRENT_UID).not());
        });
    }

    #[test]
    fn resolution_without_a_device_context_uses_the_layers_it_has() {
        // Goal: the detected layers are bound after detection, so every resolution before that
        // point, and every test that never binds them, still answers from the layers it has.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let uid = DEVICE_UID.to_string();
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;

            assert_eq!(controller.log_channel_name(&uid, "fan1"), "fan1");
            assert_eq!(controller.log_device_name(&uid, "nct6798"), "nct6798");
            assert_eq!(controller.known_device_name(&uid), None);

            controller
                .set_channel_label(&uid, HINT, &"fan1".to_string(), None, Some("Front Intake"))
                .await
                .unwrap();
            assert_eq!(
                controller.log_channel_name(&uid, "fan1"),
                "Front Intake (fan1)"
            );
        });
    }

    #[test]
    fn channel_labels_resolve_override_then_detected_then_raw() {
        // Goal: alert text shows the app's label and leaks no control characters.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let uid = DEVICE_UID.to_string();
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;

            assert_eq!(
                controller.resolve_channel_label(&uid, "temp1", None),
                "temp1"
            );
            assert_eq!(
                controller.resolve_channel_label(&uid, "temp1", Some("CPU Temp Tctl".to_string())),
                "CPU Temp Tctl"
            );

            controller
                .set_channel_label(&uid, HINT, &"temp1".to_string(), None, Some("Coolant"))
                .await
                .unwrap();
            assert_eq!(
                controller.resolve_channel_label(&uid, "temp1", Some("CPU Temp Tctl".to_string())),
                "Coolant"
            );

            // A detected label comes from sysfs unvalidated.
            assert_eq!(
                controller.resolve_channel_label(&uid, "temp2", Some("Pump\u{1b}[31m".to_string())),
                "Pump[31m"
            );
        });
    }

    #[test]
    fn log_names_show_override_with_raw_in_parens() {
        // Goal: the single log helper renders `Override (raw)` when an
        // override differs, and plain raw otherwise, for devices and
        // channels alike.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let uid = DEVICE_UID.to_string();
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;

            assert_eq!(controller.log_device_name(&uid, "nct6798"), "nct6798");
            assert_eq!(controller.log_channel_name(&uid, "fan1"), "fan1");

            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();
            controller
                .set_channel_label(&uid, HINT, &"fan1".to_string(), None, Some("Front Intake"))
                .await
                .unwrap();
            assert_eq!(
                controller.log_device_name(&uid, "nct6798"),
                "Motherboard (nct6798)"
            );
            assert_eq!(
                controller.log_channel_name(&uid, "fan1"),
                "Front Intake (fan1)"
            );

            // An override equal to the raw name renders plain, not doubled.
            controller
                .set_channel_label(&uid, HINT, &"fan2".to_string(), None, Some("fan2"))
                .await
                .unwrap();
            assert_eq!(controller.log_channel_name(&uid, "fan2"), "fan2");
        });
    }

    #[test]
    fn hint_is_stamped_but_never_resolves() {
        // Goal: the device_name hint is written for hand-editors but is not
        // a resolution layer; a file with only a hint resolves to fallbacks.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_channel_label(&uid, HINT, &"fan1".to_string(), None, Some("Front Intake"))
                .await
                .unwrap();

            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains(&format!("device_name = \"{HINT}\"")));
            // No device name override was set, so the hint must not leak in.
            assert_eq!(controller.resolve_device_name(&uid, None, "raw"), "raw");

            // The channel hint is stamped alongside the label when provided,
            // and is not a resolution layer either.
            controller
                .set_channel_label(
                    &uid,
                    HINT,
                    &"fan2".to_string(),
                    Some("Fan 2 Detected"),
                    Some("Side Intake"),
                )
                .await
                .unwrap();
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains("channel_label = \"Fan 2 Detected\""));
            assert_eq!(
                controller.channel_label_override(&uid, "fan2"),
                Some("Side Intake".to_string())
            );
        });
    }

    #[test]
    fn channels_render_as_inline_tables_with_banner() {
        // Goal: the written file matches the settled schema, with the banner
        // on top and one inline table per channel under [devices.*.channels].
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_channel_label(&uid, HINT, &"fan1".to_string(), None, Some("Front Intake"))
                .await
                .unwrap();
            controller
                .set_channel_label(&uid, HINT, &"temp1".to_string(), None, Some("Coolant"))
                .await
                .unwrap();

            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.starts_with(BANNER));
            assert!(contents.contains(&format!("[devices.{DEVICE_UID}.channels]")));
            assert!(contents.contains("fan1 = { label = \"Front Intake\" }"));
            assert!(contents.contains("temp1 = { label = \"Coolant\" }"));
        });
    }

    #[test]
    fn validation_trims_and_rejects_bad_names() {
        // Goal: whitespace is trimmed, and both empty and over-length names
        // are rejected without touching the file.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_device_name(&uid, HINT, Some("  My Board  "))
                .await
                .unwrap();
            assert_eq!(
                controller.device_name_override(&uid),
                Some("My Board".to_string())
            );

            assert!(controller
                .set_device_name(&uid, HINT, Some("   "))
                .await
                .is_err());
            assert!(controller
                .set_device_name(&uid, HINT, Some("Tab\there"))
                .await
                .is_err());
            let too_long = "x".repeat(51);
            assert!(controller
                .set_device_name(&uid, HINT, Some(&too_long))
                .await
                .is_err());
            let max_len = "x".repeat(50);
            assert!(controller
                .set_device_name(&uid, HINT, Some(&max_len))
                .await
                .is_ok());
        });
    }

    #[test]
    fn removing_last_override_prunes_the_device_entry() {
        // Goal: setting None removes the override, and an entry left with
        // only the hint is dropped from the file entirely.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();
            controller.set_device_name(&uid, HINT, None).await.unwrap();

            assert_eq!(controller.device_name_override(&uid), None);
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains(DEVICE_UID).not());
        });
    }

    #[test]
    fn remove_channel_cascades_only_that_channel() {
        // Goal: the deliberate-deletion cascade removes exactly the deleted
        // channel's entry and leaves sibling overrides intact.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();
            let sensor1 = "sensor1".to_string();
            let sensor2 = "sensor2".to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_channel_label(&uid, HINT, &sensor1, None, Some("Avg Coolant"))
                .await
                .unwrap();
            controller
                .set_channel_label(&uid, HINT, &sensor2, None, Some("Case Ambient"))
                .await
                .unwrap();
            controller.remove_channel(&uid, &sensor1).await.unwrap();

            assert_eq!(controller.channel_label_override(&uid, &sensor1), None);
            assert_eq!(
                controller.channel_label_override(&uid, &sensor2),
                Some("Case Ambient".to_string())
            );
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains("sensor1").not());
            assert!(contents.contains("Case Ambient"));
        });
    }

    #[test]
    fn writes_preserve_concurrent_hand_edits() {
        // Goal: mutations read-modify-write against the disk file, so an
        // entry hand-added after init survives a daemon write.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            let hand_edit = "[devices.hand-edited-uid]\nname = \"Hand Edited\"\n";
            std::fs::write(&path, hand_edit).unwrap();

            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();

            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains("Hand Edited"));
            assert!(contents.contains("Motherboard"));
        });
    }

    #[test]
    fn malformed_file_is_ignored_and_never_clobbered() {
        // Goal: a file with a TOML error degrades to an empty store at init,
        // and writes fail on the re-read instead of overwriting the file.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();
            let malformed = "[devices.abc123\nname = broken";
            std::fs::write(&path, malformed).unwrap();

            let controller = OverridesController::init_from(path.clone()).await;
            assert_eq!(controller.device_name_override(&uid), None);

            let result = controller.set_device_name(&uid, HINT, Some("Board")).await;
            assert!(result.is_err());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), malformed);
        });
    }

    /// A realistic config-ui.json excerpt: two devices (one is the custom
    /// sensors device), parallel arrays, unrelated fields present.
    fn ui_settings_fixture() -> &'static str {
        r##"{
            "devices": ["uid-a", "uid-custom"],
            "deviceSettings": [
                {
                    "userName": "Motherboard",
                    "userColor": "#ff0000",
                    "names": ["fan1", "temp1"],
                    "sensorAndChannelSettings": [
                        { "userName": "Front Intake", "viewType": "Control" },
                        { "viewType": "Control" }
                    ]
                },
                {
                    "names": ["sensor1"],
                    "sensorAndChannelSettings": [ { "userName": "Avg Coolant" } ]
                }
            ],
            "themeMode": "system",
            "dashboards": []
        }"##
    }

    #[test]
    fn migration_imports_user_names_and_writes_the_file() {
        // Goal: with no overrides file, userNames from config-ui.json are
        // imported for devices, channels, and custom sensors, and the file
        // is written so the gate closes for subsequent boots.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let ui_path = tmp.path().join("config-ui.json");
            std::fs::write(&ui_path, ui_settings_fixture()).unwrap();

            let controller = OverridesController::init_with_migration(path.clone(), &ui_path).await;

            let uid_a = "uid-a".to_string();
            let uid_custom = "uid-custom".to_string();
            assert_eq!(
                controller.device_name_override(&uid_a),
                Some("Motherboard".to_string())
            );
            assert_eq!(
                controller.channel_label_override(&uid_a, "fan1"),
                Some("Front Intake".to_string())
            );
            // temp1 had no userName and must not gain an entry.
            assert_eq!(controller.channel_label_override(&uid_a, "temp1"), None);
            assert_eq!(
                controller.channel_label_override(&uid_custom, "sensor1"),
                Some("Avg Coolant".to_string())
            );
            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.starts_with(BANNER));
            assert!(contents.contains("Motherboard"));

            // Second boot: the file exists, migration must not run again.
            std::fs::write(&ui_path, "garbage now").unwrap();
            let reloaded = OverridesController::init_with_migration(path, &ui_path).await;
            assert_eq!(
                reloaded.device_name_override(&uid_a),
                Some("Motherboard".to_string())
            );
        });
    }

    #[test]
    fn migration_gate_respects_existing_overrides_file() {
        // Goal: an existing overrides file, even an empty one, blocks the
        // migration entirely; user data is never overwritten.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let ui_path = tmp.path().join("config-ui.json");
            std::fs::write(&path, "").unwrap();
            std::fs::write(&ui_path, ui_settings_fixture()).unwrap();

            let controller = OverridesController::init_with_migration(path, &ui_path).await;

            assert_eq!(controller.device_name_override(&"uid-a".to_string()), None);
        });
    }

    #[test]
    fn migration_tolerates_garbage_and_absence() {
        // Goal: an unparseable or absent blob degrades to an empty store
        // and creates no file, leaving the gate open for the next boot.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let ui_path = tmp.path().join("config-ui.json");

            let controller = OverridesController::init_with_migration(path.clone(), &ui_path).await;
            assert_eq!(controller.device_name_override(&"uid-a".to_string()), None);
            assert!(path.exists().not());

            std::fs::write(&ui_path, "{ not json").unwrap();
            let controller = OverridesController::init_with_migration(path.clone(), &ui_path).await;
            assert_eq!(controller.device_name_override(&"uid-a".to_string()), None);
            assert!(path.exists().not());
        });
    }

    #[test]
    fn migration_without_user_names_creates_no_file() {
        // Goal: a blob whose entries carry no userNames yields nothing; no
        // file is written so a later rename still starts from a clean gate.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let ui_path = tmp.path().join("config-ui.json");
            let blob = r#"{ "devices": ["uid-a"], "deviceSettings": [
                { "names": ["fan1"], "sensorAndChannelSettings": [ {} ] } ] }"#;
            std::fs::write(&ui_path, blob).unwrap();

            let _controller =
                OverridesController::init_with_migration(path.clone(), &ui_path).await;

            assert!(path.exists().not());
        });
    }

    #[test]
    fn migration_skips_unsafe_names_and_trims() {
        // Goal: injection-capable names are dropped, whitespace is trimmed,
        // and over-cap lengths are preserved (existing user data).
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let ui_path = tmp.path().join("config-ui.json");
            let long_name = "x".repeat(60);
            let blob = format!(
                r#"{{ "devices": ["uid-a"], "deviceSettings": [
                    {{ "userName": "  {long_name}  ",
                       "names": ["fan1", "fan2"],
                       "sensorAndChannelSettings": [
                           {{ "userName": "bad\u202ename" }},
                           {{ "userName": "  Rear Exhaust  " }}
                       ] }} ] }}"#
            );
            std::fs::write(&ui_path, blob).unwrap();

            let controller = OverridesController::init_with_migration(path, &ui_path).await;

            let uid = "uid-a".to_string();
            assert_eq!(controller.device_name_override(&uid), Some(long_name));
            assert_eq!(controller.channel_label_override(&uid, "fan1"), None);
            assert_eq!(
                controller.channel_label_override(&uid, "fan2"),
                Some("Rear Exhaust".to_string())
            );
        });
    }

    #[test]
    fn unknown_keys_are_tolerated_on_read() {
        // Goal: forward compatibility; a file written by a newer daemon with
        // extra fields (e.g. color) still loads and resolves labels.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let contents = format!(
                "[future]\nsetting = 1\n\n[devices.{DEVICE_UID}]\nname = \"Motherboard\"\n\
                future_field = true\n\n[devices.{DEVICE_UID}.channels]\n\
                fan1 = {{ label = \"Front Intake\", color = \"#ff0000\" }}\n"
            );
            std::fs::write(&path, contents).unwrap();

            let controller = OverridesController::init_from(path).await;
            let uid = DEVICE_UID.to_string();
            assert_eq!(
                controller.device_name_override(&uid),
                Some("Motherboard".to_string())
            );
            assert_eq!(
                controller.channel_label_override(&uid, "fan1"),
                Some("Front Intake".to_string())
            );
        });
    }

    #[test]
    fn format_log_name_strips_control_characters() {
        // Goal: hand-edited override names reach the log helper unvalidated,
        // so the helper drops injection-capable characters (ESC here) itself.
        let tainted = "Pump\u{1b}[31m".to_string();
        let formatted = format_log_name(Some(tainted), "hwmon2");
        assert!(formatted.chars().any(is_forbidden_name_char).not());
        assert_eq!(formatted, "Pump[31m (hwmon2)");
        assert_eq!(
            format_log_name(Some("Pump".to_string()), "hwmon2"),
            "Pump (hwmon2)"
        );
    }

    #[test]
    fn save_publishes_atomically_without_leaving_a_temp_file() {
        // Goal: writes go through a temp file + rename, so a crash cannot
        // leave a partial file and no temp sibling remains afterwards.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();

            let controller = OverridesController::init_from(path.clone()).await;
            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();

            let mut temp_sibling = path.as_os_str().to_owned();
            temp_sibling.push(".tmp");
            assert!(PathBuf::from(temp_sibling).exists().not());
            let reloaded = OverridesController::init_from(path).await;
            assert_eq!(
                reloaded.device_name_override(&uid),
                Some("Motherboard".to_string())
            );
        });
    }

    #[test]
    fn channel_overrides_are_capped_per_device() {
        // Goal: bound file growth. A device accepts up to the cap of distinct
        // channel overrides; a further new channel is rejected, while updating
        // an already-stored channel still succeeds.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let uid = DEVICE_UID.to_string();
            let controller = OverridesController::init_from(overrides_path(&tmp)).await;

            for i in 0..MAX_CHANNEL_OVERRIDES_PER_DEVICE {
                let channel = format!("chan{i}");
                controller
                    .set_channel_label(&uid, HINT, &channel, None, Some("Label"))
                    .await
                    .unwrap();
            }
            let overflow = format!("chan{MAX_CHANNEL_OVERRIDES_PER_DEVICE}");
            assert!(controller
                .set_channel_label(&uid, HINT, &overflow, None, Some("Label"))
                .await
                .is_err());
            controller
                .set_channel_label(&uid, HINT, &"chan0".to_string(), None, Some("Renamed"))
                .await
                .unwrap();
            assert_eq!(
                controller.channel_label_override(&uid, "chan0"),
                Some("Renamed".to_string())
            );
        });
    }

    #[test]
    fn channel_label_rejects_forbidden_key() {
        // Goal: the channel name becomes a TOML key with no liveness check, so
        // an injection-capable key is rejected before anything is written.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let path = overrides_path(&tmp);
            let uid = DEVICE_UID.to_string();
            let controller = OverridesController::init_from(path.clone()).await;

            let bad_key = "fan1\ninjected".to_string();
            assert!(controller
                .set_channel_label(&uid, HINT, &bad_key, None, Some("Label"))
                .await
                .is_err());
            assert!(path.exists().not());
            assert_eq!(controller.channel_label_override(&uid, &bad_key), None);
        });
    }
}
