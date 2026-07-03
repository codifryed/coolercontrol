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
//! Single source of truth for custom device and channel names. Resolution
//! is layered: override > detected label > raw name. The file is
//! hand-editable; hand edits are picked up at daemon startup only. Entries
//! are never pruned because hardware is absent, only when the described
//! entity is deliberately deleted.

// Not yet wired into the API; remove once the endpoints land.
#![allow(dead_code)]

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Not;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use log::{info, warn};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item, Value};

use crate::api::CCError;
use crate::cc_fs;
use crate::device::{ChannelName, DeviceName, DeviceUID};
use crate::paths;

/// Matches the UI rename input limit (`DEFAULT_NAME_STRING_LENGTH`).
pub const NAME_LENGTH_MAX: usize = 40;

const BANNER: &str = "\
# CoolerControl display-name overrides.
# Managed by coolercontrold: comments are not preserved on rewrite.
# Hand edits are applied at the next daemon startup.
";

/// Owns the overrides document and resolves display names against it.
pub struct OverridesController {
    path: PathBuf,
    document: RefCell<OverridesDocument>,
}

impl OverridesController {
    /// Loads the overrides file from the standard config location.
    pub async fn init() -> Self {
        Self::init_from(paths::overrides_file().to_path_buf()).await
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
        }
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
        channel_name: &ChannelName,
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

    /// Resolves a channel display label. Layer order: override > detected > raw.
    pub fn resolve_channel_label(
        &self,
        device_uid: &DeviceUID,
        channel_name: &ChannelName,
        detected: Option<&str>,
        raw_name: &str,
    ) -> String {
        self.channel_label_override(device_uid, channel_name)
            .or_else(|| detected.map(str::to_owned))
            .unwrap_or_else(|| raw_name.to_owned())
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
        })
        .await
    }

    /// Sets or removes (`None`) the channel label override.
    /// `device_name_hint` refreshes the hand-editor hint line.
    pub async fn set_channel_label(
        &self,
        device_uid: &DeviceUID,
        device_name_hint: &str,
        channel_name: &ChannelName,
        label: Option<&str>,
    ) -> Result<()> {
        let label = label.map(validate_name).transpose()?;
        self.read_modify_write(|document| {
            let device = document.devices.entry(device_uid.clone()).or_default();
            device.device_name = Some(device_name_hint.to_owned());
            match label {
                Some(label) => {
                    let channel = device.channels.entry(channel_name.clone()).or_default();
                    channel.label = Some(label);
                }
                None => {
                    if let Some(channel) = device.channels.get_mut(channel_name) {
                        channel.label = None;
                    }
                }
            }
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
        })
        .await
    }

    /// Read-modify-write against the file on disk. Re-reading narrows the
    /// window where a daemon write clobbers a concurrent hand-edit.
    async fn read_modify_write<F>(&self, apply: F) -> Result<()>
    where
        F: FnOnce(&mut OverridesDocument),
    {
        let mut document = load(&self.path).await?;
        apply(&mut document);
        prune(&mut document);
        self.save(&document).await?;
        self.document.replace(document);
        Ok(())
    }

    async fn save(&self, document: &OverridesDocument) -> Result<()> {
        let contents = render(document)?;
        assert!(contents.starts_with(BANNER));
        cc_fs::write_string(&self.path, contents)
            .await
            .with_context(|| format!("Writing overrides file {}", self.path.display()))
    }
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct OverridesDocument {
    #[serde(default)]
    devices: BTreeMap<DeviceUID, DeviceOverrides>,
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct DeviceOverrides {
    /// Daemon-written hint for hand-editors, ignored on read.
    #[serde(skip_serializing_if = "Option::is_none")]
    device_name: Option<DeviceName>,
    /// User override for the device display name.
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<DeviceName>,
    /// Temps and channels share one namespace, keyed by raw channel name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    channels: BTreeMap<ChannelName, ChannelOverrides>,
}

/// A table per channel so color/ignore/compute can slot in later.
#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct ChannelOverrides {
    #[serde(skip_serializing_if = "Option::is_none")]
    label: Option<String>,
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

/// Trims and validates a user-supplied name.
fn validate_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(CCError::UserError {
            msg: "Name must not be empty".to_string(),
        }
        .into());
    }
    if trimmed.chars().count() > NAME_LENGTH_MAX {
        return Err(CCError::UserError {
            msg: format!("Name must be at most {NAME_LENGTH_MAX} characters"),
        }
        .into());
    }
    debug_assert!(trimmed.is_empty().not());
    debug_assert_eq!(trimmed, trimmed.trim());
    Ok(trimmed.to_owned())
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
        // Goal: verify the layer order override > detected > raw for both
        // devices and channels, including the negative space (no override).
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let uid = DEVICE_UID.to_string();
            let channel = "fan1".to_string();

            let controller = OverridesController::init_from(overrides_path(&tmp)).await;
            assert_eq!(
                controller.resolve_device_name(&uid, Some("detected"), "raw"),
                "detected"
            );
            assert_eq!(controller.resolve_device_name(&uid, None, "raw"), "raw");
            assert_eq!(
                controller.resolve_channel_label(&uid, &channel, Some("CPU Fan"), "fan1"),
                "CPU Fan"
            );
            assert_eq!(
                controller.resolve_channel_label(&uid, &channel, None, "fan1"),
                "fan1"
            );

            controller
                .set_device_name(&uid, HINT, Some("Motherboard"))
                .await
                .unwrap();
            controller
                .set_channel_label(&uid, HINT, &channel, Some("Front Intake"))
                .await
                .unwrap();
            assert_eq!(
                controller.resolve_device_name(&uid, Some("detected"), "raw"),
                "Motherboard"
            );
            assert_eq!(
                controller.resolve_channel_label(&uid, &channel, Some("CPU Fan"), "fan1"),
                "Front Intake"
            );
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
                .set_channel_label(&uid, HINT, &"fan1".to_string(), Some("Front Intake"))
                .await
                .unwrap();

            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(contents.contains(&format!("device_name = \"{HINT}\"")));
            // No device name override was set, so the hint must not leak in.
            assert_eq!(controller.resolve_device_name(&uid, None, "raw"), "raw");
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
                .set_channel_label(&uid, HINT, &"fan1".to_string(), Some("Front Intake"))
                .await
                .unwrap();
            controller
                .set_channel_label(&uid, HINT, &"temp1".to_string(), Some("Coolant"))
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
            let too_long = "x".repeat(NAME_LENGTH_MAX + 1);
            assert!(controller
                .set_device_name(&uid, HINT, Some(&too_long))
                .await
                .is_err());
            let max_len = "x".repeat(NAME_LENGTH_MAX);
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
                .set_channel_label(&uid, HINT, &sensor1, Some("Avg Coolant"))
                .await
                .unwrap();
            controller
                .set_channel_label(&uid, HINT, &sensor2, Some("Case Ambient"))
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
}
