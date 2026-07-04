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

//! User-defined display-name overrides, stored in `overrides.toml`.
//!
//! Resolution is layered (override > detected label > raw name). The file is
//! hand-editable; edits are read only at startup. Entries are pruned only on
//! deliberate entity deletion, never on hardware absence.

use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Not;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use log::{info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Value};

use crate::api::{is_forbidden_name_char, validate_name_string};
use crate::cc_fs;
use crate::device::{ChannelName, DeviceName, DeviceUID};
use crate::paths;

const BANNER: &str = "\
# CoolerControl display-name overrides.
# Managed by coolercontrold: comments are not preserved on rewrite.
# Hand edits are applied at the next daemon startup.
";

/// Upper bound on channel overrides stored per device. Real hardware has far
/// fewer channels, so this only caps accidental or abusive growth of the file.
const MAX_CHANNEL_OVERRIDES_PER_DEVICE: usize = 512;

/// Owns the overrides document and resolves display names against it.
pub struct OverridesController {
    path: PathBuf,
    document: RefCell<OverridesDocument>,
    /// Serializes read-modify-write cycles. Multiple actors write (settings
    /// renames, the custom sensor delete cascade) and a cycle spans await
    /// points, so unserialized writes could interleave and lose one.
    write_lock: tokio::sync::Mutex<()>,
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
                    document: RefCell::new(document),
                    write_lock: tokio::sync::Mutex::new(()),
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
            document: RefCell::new(document),
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// An empty store not backed by a file, for tests of components that
    /// hold the controller but do not exercise name persistence.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self {
            path: PathBuf::new(),
            document: RefCell::new(OverridesDocument::default()),
            write_lock: tokio::sync::Mutex::new(()),
        }
    }

    /// A copy of the raw, sparse overrides document.
    pub fn document(&self) -> OverridesDocument {
        self.document.borrow().clone()
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

    /// Resolves a device display name. Layer order: override > detected > raw.
    pub fn resolve_device_name(
        &self,
        device_uid: &DeviceUID,
        detected: Option<&str>,
        raw_name: &str,
    ) -> DeviceName {
        self.device_name_override(device_uid)
            .or_else(|| detected.map(str::to_owned))
            .unwrap_or_else(|| raw_name.to_owned())
    }

    /// Log display form of a device name: `Override (raw)` when a user
    /// override exists and differs, plain raw otherwise.
    pub fn log_device_name(&self, device_uid: &DeviceUID, raw_name: &str) -> String {
        format_log_name(self.device_name_override(device_uid), raw_name)
    }

    /// Log display form of a channel name: `Override (raw)` when a user
    /// override exists and differs, plain raw otherwise.
    pub fn log_channel_name(&self, device_uid: &DeviceUID, channel_name: &str) -> String {
        format_log_name(
            self.channel_label_override(device_uid, channel_name),
            channel_name,
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

/// One place owns the log format so it cannot drift per call site. Names are
/// sanitized here because hand-edited overrides bypass intake validation, so
/// the log boundary re-applies the injection-character policy.
fn format_log_name(override_name: Option<String>, raw_name: &str) -> String {
    let raw = sanitize_for_log(raw_name);
    match override_name {
        Some(name) if name != raw_name => format!("{} ({raw})", sanitize_for_log(&name)),
        _ => raw.into_owned(),
    }
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
                controller.channel_label_override(&uid_a, &"fan1".to_string()),
                Some("Front Intake".to_string())
            );
            // temp1 had no userName and must not gain an entry.
            assert_eq!(
                controller.channel_label_override(&uid_a, &"temp1".to_string()),
                None
            );
            assert_eq!(
                controller.channel_label_override(&uid_custom, &"sensor1".to_string()),
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
            assert_eq!(
                controller.channel_label_override(&uid, &"fan1".to_string()),
                None
            );
            assert_eq!(
                controller.channel_label_override(&uid, &"fan2".to_string()),
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
                controller.channel_label_override(&uid, &"fan1".to_string()),
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
                controller.channel_label_override(&uid, &"chan0".to_string()),
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
