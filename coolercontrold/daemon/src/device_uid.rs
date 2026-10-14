// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Device UID derivation.
//!
//! A Device UID is the stable identifier used as the key for all of a device's persisted settings,
//! so this logic is high-stakes and rarely changed: altering it re-keys existing configs. The
//! functions are associated with [`Device`] (call them as `Device::create_uid_from` /
//! `Device::assign_unique`), but their logic lives in this module so the reasoning is in one place.
//!
//! `create_uid_from` produces the UID as a sha256 hex hash of the `DeviceType` combined with a
//! caller-provided identifier:
//!
//!   - When the caller has a device-specific identifier (a hardware serial number or a stable sysfs
//!     path), it is passed as `device_id` and the hash is `DeviceType + device_id`. This is name-
//!     and order-independent, so the UID follows the same physical device across renames and
//!     reordering.
//!   - When no such identifier exists, the caller passes `None` and the hash falls back to
//!     `DeviceType + name + type_index`. This is a last resort: `type_index` is assigned in
//!     detection order, so the UID is not stable across hardware add/remove.
//!
//! The identifier priority (serial, then realpath, then name, etc.) is chosen by each repository,
//! since only it knows the hardware. This module owns how a chosen identifier becomes a UID.
//!
//! Including `DeviceType` in the hash means the same physical device seen through two subsystems
//! (for example the kernel hwmon driver and liquidctl) gets two distinct UIDs, matching how the
//! daemon models them as separate devices.
//!
//! Uniqueness is best-effort. We cannot always obtain a true per-device identifier, so two
//! indistinguishable devices (identical model, no serial, no distinct path) can derive the same
//! UID. `assign_unique` enforces uniqueness within a detection batch so that such a collision never
//! silently drops a device from a uid-keyed map.
//!
//! When a repository does move a device onto a better identifier, `migrate_uid_in_file` re-keys
//! the JSON stores that name the old UID.

use std::collections::HashSet;
use std::ops::Not;
use std::path::Path;

use anyhow::Result;
use log::info;
use sha2::{Digest, Sha256};

use crate::cc_fs;
use crate::device::{Device, DeviceType, UID};

/// Repoints every reference to `legacy` in a JSON store that has not been loaded yet, so no
/// reload is needed. A textual replacement is sound: a UID is 64 hex chars and cannot be a
/// substring of anything else in these files. A file that already names `current` is left
/// alone: merging would pick a winner.
pub async fn migrate_uid_in_file(path: &Path, legacy: &UID, current: &UID) -> Result<bool> {
    debug_assert_eq!(legacy.len(), 64);
    debug_assert_eq!(current.len(), 64);
    debug_assert_ne!(legacy, current);
    let Ok(contents) = cc_fs::read_txt(path).await else {
        return Ok(false);
    };
    if contents.contains(legacy.as_str()).not() {
        return Ok(false);
    }
    if contents.contains(current.as_str()) {
        return Ok(false);
    }
    let migrated = contents.replace(legacy.as_str(), current.as_str());
    cc_fs::write(path, migrated.into_bytes()).await?;
    Ok(true)
}

impl Device {
    /// Returns a sha256 hash string of an attempted unique identifier for a device.
    ///
    /// Unique in the sense that we try to follow the same device even if, for example:
    ///   - another device has been removed and the order has changed.
    ///   - the device has been swapped with another device plugged into the system.
    ///
    /// See the module docs for how `device_id` and the `None` fallback are chosen.
    pub fn create_uid_from(
        name: &str,
        d_type: DeviceType,
        type_index: u8,
        device_id: Option<&String>,
    ) -> UID {
        let mut hasher = Sha256::new();
        hasher.update(d_type.clone().to_string());
        if let Some(d_id) = device_id {
            // this should be pretty unique to the device itself, such as a serial or device path
            hasher.update(d_id);
        } else {
            // non-optimal fallback if needed:
            hasher.update(name);
            hasher.update([type_index]);
        }
        crate::hashutil::to_lower_hex(&hasher.finalize())
    }

    /// Assigns a device a UID that is unique within a detection batch, so a device is never silently
    /// dropped by colliding with another in a uid-keyed map.
    ///
    /// `distinguishers` are fallbacks in descending strength, tried only on a collision; the last
    /// is salted with an ordinal, so it must be one the caller can always supply (a sysfs path).
    /// Order matters for stability, not just uniqueness: a hardware identifier follows the device
    /// while a sysfs path is reassigned every boot. Returns the identifier to store, and its UID.
    pub fn assign_unique(
        assigned: &mut HashSet<UID>,
        d_type: DeviceType,
        name: &str,
        identifier: &str,
        distinguishers: &[&str],
    ) -> (String, UID) {
        debug_assert!(
            distinguishers.is_empty().not(),
            "at least one distinguisher is required, since the last one is salted"
        );
        let natural = Self::create_uid_from(name, d_type, 0, Some(&identifier.to_owned()));
        if assigned.insert(natural.clone()) {
            return (identifier.to_owned(), natural);
        }
        // Shared or empty. Walk the rest strongest first; a blank carries no identity.
        for candidate in distinguishers {
            if candidate.is_empty() {
                continue;
            }
            let uid = Self::create_uid_from(name, d_type, 0, Some(&(*candidate).to_owned()));
            if assigned.insert(uid.clone()) {
                info!(
                    "Device '{name}' computed a UID already used by another device; \
                     assigned it a distinct UID from a weaker identifier instead"
                );
                return ((*candidate).to_owned(), uid);
            }
        }
        // All shared. Salt the weakest one. Bounded by the assigned count, each salt distinct.
        let base = distinguishers.last().copied().unwrap_or(identifier);
        let mut ordinal: u16 = 1;
        loop {
            let candidate = format!("{base}#{ordinal}");
            let uid = Self::create_uid_from(name, d_type, 0, Some(&candidate));
            if assigned.insert(uid.clone()) {
                info!(
                    "Device '{name}' shares every available identifier with another device; \
                     assigned it a synthesised UID that will not survive a hardware change"
                );
                return (candidate, uid);
            }
            ordinal += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- create_uid_from: the hashing contract -----------------------------------------------

    #[test]
    fn create_uid_from_matches_a_known_sha256_vector() {
        // Goal: pin the exact UID bytes. UIDs key every persisted setting, so a change in the
        // hashing crate or in what gets fed to it would silently orphan a user's whole config.
        // Method: compare against a sha256 of "Hwmon" + the device id, computed independently.
        let device_id = "/sys/devices/platform/test/hwmon/hwmon0".to_owned();
        let uid = Device::create_uid_from("ignored", DeviceType::Hwmon, 0, Some(&device_id));
        assert_eq!(
            uid,
            "091c1c7e743cc5404f3476bfde4e489fab977a93127bb319bf9f68393bdb028a"
        );
    }

    #[test]
    fn create_uid_from_produces_64_char_lower_hex() {
        // Goal: the UID is always a 64-char lowercase sha256 hex string, and derivation is
        // deterministic. Method: hash the same inputs twice and inspect the output shape.
        let uid = Device::create_uid_from("dev", DeviceType::Hwmon, 1, Some(&"serial".to_owned()));
        assert_eq!(uid.len(), 64);
        assert!(uid
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(
            uid,
            Device::create_uid_from("dev", DeviceType::Hwmon, 1, Some(&"serial".to_owned()))
        );
    }

    #[test]
    fn create_uid_from_some_ignores_name_and_type_index() {
        // Goal: with a device_id present, the name and type_index do not affect the UID, so a device
        // is followed across renames and reordering. Method: vary name and type_index, keep the id.
        let a = Device::create_uid_from(
            "name-one",
            DeviceType::Hwmon,
            0,
            Some(&"same-id".to_owned()),
        );
        let b = Device::create_uid_from(
            "name-two",
            DeviceType::Hwmon,
            9,
            Some(&"same-id".to_owned()),
        );
        assert_eq!(a, b);
    }

    #[test]
    fn create_uid_from_none_varies_with_name_and_type_index() {
        // Goal: the None fallback folds in name and type_index, so it is order-dependent.
        // Method: change name, then type_index, and assert each yields a different UID.
        let base = Device::create_uid_from("dev", DeviceType::Hwmon, 0, None);
        assert_ne!(
            base,
            Device::create_uid_from("other", DeviceType::Hwmon, 0, None)
        );
        assert_ne!(
            base,
            Device::create_uid_from("dev", DeviceType::Hwmon, 1, None)
        );
    }

    #[test]
    fn create_uid_from_empty_identifier_collides_across_names() {
        // Goal: two different devices with an empty identifier hash to the same UID (DeviceType
        // only). This is exactly the collision the dedup guard exists to catch.
        // Method: hash an empty id under two different names/indexes and assert equality.
        let a = Device::create_uid_from("device-a", DeviceType::Hwmon, 0, Some(&String::new()));
        let b = Device::create_uid_from("device-b", DeviceType::Hwmon, 3, Some(&String::new()));
        assert_eq!(a, b);
    }

    #[test]
    fn create_uid_from_device_type_distinguishes() {
        // Goal: the same identifier under a different DeviceType yields a different UID, so the same
        // physical device seen via two subsystems keeps two identities. Method: hash id across types.
        let hwmon = Device::create_uid_from("dev", DeviceType::Hwmon, 0, Some(&"id".to_owned()));
        let gpu = Device::create_uid_from("dev", DeviceType::GPU, 0, Some(&"id".to_owned()));
        assert_ne!(hwmon, gpu);
    }

    #[test]
    fn blank_hwmon_uid_matches_historical_constant() {
        // Goal: lock the exact historical UID of a blank-identifier hwmon device, so the guard-only
        // fix stays backward compatible (a lone serial-less hwmon device keeps this UID and its
        // settings). Method: assert the known sha256("Hwmon") value.
        assert_eq!(
            Device::create_uid_from("poweradjust3", DeviceType::Hwmon, 0, Some(&String::new())),
            "94c00dd53c8b0eb972b101a6a22c7d9e0f62270af245638a7b6cfda4be6d2c8e"
        );
    }

    // -- assign_unique: the dedup guard ------------------------------------------------------

    #[test]
    fn single_device_keeps_its_identifier() {
        // Goal: with no collision, the identifier and its natural UID pass through unchanged.
        // Method: assign one device and assert the returned identifier and UID.
        let mut assigned = HashSet::new();
        let (id, uid) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "dev",
            "serial-x",
            &["/path/x"],
        );
        assert_eq!(id, "serial-x");
        assert_eq!(
            uid,
            Device::create_uid_from("dev", DeviceType::Hwmon, 0, Some(&"serial-x".to_owned()))
        );
        assert_eq!(assigned.len(), 1);
    }

    #[test]
    fn lone_blank_identifier_keeps_natural_uid() {
        // Goal: a single device with no usable identifier keeps its natural (blank-derived) UID; the
        // distinguisher is consulted only on collision. This is the compat guarantee for a lone
        // serial-less device. Method: assign one blank-identifier device with a path present.
        let mut assigned = HashSet::new();
        let (id, uid) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "psu",
            "",
            &["/sys/devices/hwmon10"],
        );
        assert_eq!(id, "");
        assert_eq!(
            uid,
            Device::create_uid_from("psu", DeviceType::Hwmon, 0, Some(&String::new()))
        );
    }

    #[test]
    fn distinct_identifiers_keep_natural_uids() {
        // Goal: two devices with different identifiers each keep their natural UID unchanged.
        // Method: assign two distinct serials and assert the identifiers and UIDs pass through.
        let mut assigned = HashSet::new();
        let (id_a, uid_a) =
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "serial-a", &["/a"]);
        let (id_b, uid_b) =
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "serial-b", &["/b"]);

        assert_eq!(id_a, "serial-a");
        assert_eq!(id_b, "serial-b");
        assert_ne!(uid_a, uid_b);
        assert_eq!(
            uid_a,
            Device::create_uid_from("dev", DeviceType::Hwmon, 0, Some(&"serial-a".to_owned()))
        );
    }

    #[test]
    fn colliding_identifier_disambiguates_by_distinguisher() {
        // Goal: the reported case, two devices with the same (blank) identifier. The first keeps the
        // natural UID and its settings; the second is disambiguated by its distinct sysfs path.
        // Method: assign two blank-identifier devices with different paths.
        let mut assigned = HashSet::new();
        let (id_a, uid_a) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "poweradjust3",
            "",
            &["/sys/hwmon2"],
        );
        let (id_b, uid_b) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "poweradjust3",
            "",
            &["/sys/hwmon3"],
        );

        // the incumbent keeps the natural blank-identifier UID:
        assert_eq!(id_a, "");
        assert_eq!(
            uid_a,
            Device::create_uid_from("poweradjust3", DeviceType::Hwmon, 0, Some(&String::new()))
        );
        // the second is given its path as its identity, producing a distinct UID:
        assert_eq!(id_b, "/sys/hwmon3");
        assert_ne!(uid_a, uid_b);
    }

    #[test]
    fn returned_identifier_reproduces_uid_regardless_of_type_index() {
        // Goal: the returned identifier must reproduce the same UID when fed back through
        // create_uid_from by Device::new, which passes the real type_index (ignored for the Some
        // branch). Method: assign a colliding pair and re-derive both UIDs with arbitrary indexes.
        let mut assigned = HashSet::new();
        let (id_a, uid_a) =
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "shared", &["/a"]);
        let (id_b, uid_b) =
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "shared", &["/b"]);

        assert_eq!(id_a, "shared");
        assert_eq!(id_b, "/b");
        assert_eq!(
            uid_a,
            Device::create_uid_from("dev", DeviceType::Hwmon, 42, Some(&id_a))
        );
        assert_eq!(
            uid_b,
            Device::create_uid_from("dev", DeviceType::Hwmon, 7, Some(&id_b))
        );
        assert_ne!(uid_a, uid_b);
    }

    // Goal: two drives whose firmware reports the same wwid must still both get a STABLE
    // identity when their serials differ. Before the serial rung existed the second fell
    // straight to its sysfs path, which is reassigned in probe order on exactly the
    // hardware this matters for, so its UID would churn every boot.
    #[test]
    fn colliding_wwid_falls_back_to_serial_not_location() {
        let mut assigned = HashSet::new();
        let shared_wwid = "naa.5000c500a9068285";

        let (id_a, uid_a) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "ST2000LX001",
            shared_wwid,
            &[
                "WCC34KL1",
                "/sys/devices/pci0000:00/host6/target6:0:0/6:0:0:0",
            ],
        );
        let (id_b, uid_b) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "ST2000LX001",
            shared_wwid,
            &[
                "WCC34KL2",
                "/sys/devices/pci0000:00/host6/target6:0:1/6:0:1:0",
            ],
        );

        assert_eq!(id_a, shared_wwid, "the incumbent keeps the wwid");
        assert_eq!(
            id_b, "WCC34KL2",
            "the collider takes its serial, not its path"
        );
        assert_ne!(uid_a, uid_b);
        // Neither UID may be derived from a sysfs path.
        assert!(id_b.starts_with("/sys").not());
    }

    // Goal: negative space for the rung. A blank serial carries no identity, so it must be
    // skipped rather than claimed, otherwise every serial-less duplicate would collapse
    // onto one UID and a device would be silently dropped from a uid-keyed map.
    #[test]
    fn blank_serial_rung_is_skipped_for_the_path() {
        let mut assigned = HashSet::new();
        let shared_wwid = "shared-wwid";

        let (id_a, _) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "drive",
            shared_wwid,
            &["", "/sys/a"],
        );
        let (id_b, uid_b) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "drive",
            shared_wwid,
            &["", "/sys/b"],
        );

        assert_eq!(id_a, shared_wwid);
        assert_eq!(id_b, "/sys/b", "a blank rung is skipped, not claimed");
        assert_eq!(
            uid_b,
            Device::create_uid_from("drive", DeviceType::Hwmon, 0, Some(&"/sys/b".to_owned()))
        );
    }

    // Goal: when every rung is shared (a bridge reporting one canned identity for all its
    // units) the salt still guarantees distinct UIDs, so no device is silently dropped.
    #[test]
    fn every_rung_shared_still_yields_distinct_uids() {
        let mut assigned = HashSet::new();
        let mut uids = Vec::new();
        for _ in 0..3 {
            let (_, uid) = Device::assign_unique(
                &mut assigned,
                DeviceType::Hwmon,
                "clone",
                "canned-wwid",
                &["canned-serial", "/sys/same"],
            );
            uids.push(uid);
        }

        let distinct: HashSet<&UID> = uids.iter().collect();
        assert_eq!(distinct.len(), uids.len());
    }

    #[test]
    fn indistinguishable_devices_salt_with_ordinal() {
        // Goal: even devices sharing BOTH identifier and distinguisher (truly indistinguishable)
        // still get distinct UIDs via an ordinal salt, never a collision.
        // Method: assign three devices all with empty identifier and empty distinguisher.
        let mut assigned = HashSet::new();
        let (_, uid_a) = Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", &[""]);
        let (_, uid_b) = Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", &[""]);
        let (_, uid_c) = Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", &[""]);

        assert_ne!(uid_a, uid_b);
        assert_ne!(uid_b, uid_c);
        assert_ne!(uid_a, uid_c);
        assert_eq!(assigned.len(), 3);
    }

    #[test]
    fn processing_order_determines_uid_keeper() {
        // Goal: which colliding device keeps the natural UID depends on processing order, so callers
        // sort by a stable key (e.g. sysfs path) for a stable result.
        // Method: assign the same two blank devices in both orders and inspect who kept the natural.
        let mut forward = HashSet::new();
        let kept_forward =
            Device::assign_unique(&mut forward, DeviceType::Hwmon, "dev", "", &["/a"]).0;
        let bumped_forward =
            Device::assign_unique(&mut forward, DeviceType::Hwmon, "dev", "", &["/b"]).0;
        assert_eq!(kept_forward, "");
        assert_eq!(bumped_forward, "/b");

        let mut reverse = HashSet::new();
        let kept_reverse =
            Device::assign_unique(&mut reverse, DeviceType::Hwmon, "dev", "", &["/b"]).0;
        let bumped_reverse =
            Device::assign_unique(&mut reverse, DeviceType::Hwmon, "dev", "", &["/a"]).0;
        assert_eq!(kept_reverse, "");
        assert_eq!(bumped_reverse, "/a");
    }

    #[test]
    fn batch_of_colliding_devices_all_get_distinct_uids() {
        // Goal: a batch mixing blank-identifier and shared-generic-serial devices yields all-distinct
        // UIDs and never drops one. Method: assign several colliding groups and count distinct UIDs.
        let mut assigned = HashSet::new();
        let mut uids = Vec::new();
        for i in 0..5 {
            let (_, uid) = Device::assign_unique(
                &mut assigned,
                DeviceType::Hwmon,
                "same-model",
                "",
                &[format!("/p/{i}").as_str()],
            );
            uids.push(uid);
        }
        for i in 0..3 {
            let (_, uid) = Device::assign_unique(
                &mut assigned,
                DeviceType::Liquidctl,
                "other-model",
                "GENERIC-SERIAL",
                &[format!("/dev/hidraw{i}").as_str()],
            );
            uids.push(uid);
        }
        let distinct: HashSet<&UID> = uids.iter().collect();
        assert_eq!(distinct.len(), uids.len());
        assert_eq!(assigned.len(), uids.len());
    }

    // -- migrate_uid_in_file: the JSON store re-key ------------------------------------------

    const LEGACY_UID: &str = "8ef08515338cc1f7727415a1d6bc45e84d5a818e94bd2072ac34b9d8d87f4210";
    const CURRENT_UID: &str = "67ada747f6820bb2e4a9b5b639657e0971998a6f5ca5f50480ecf36b46c7ec29";

    async fn write_store(tmp: &tempfile::TempDir, contents: &str) -> std::path::PathBuf {
        let path = tmp.path().join("store.json");
        cc_fs::write(&path, contents.as_bytes().to_vec())
            .await
            .unwrap();
        path
    }

    async fn migrate_store(path: &Path) -> bool {
        migrate_uid_in_file(path, &LEGACY_UID.to_string(), &CURRENT_UID.to_string())
            .await
            .unwrap()
    }

    // Goal: an alert watching a device follows it onto its new UID.
    // Method: write a realistic alerts file naming the legacy UID in both the
    // downgrade-compat `channel_source` and the `channel_sources` array, then migrate.
    #[test]
    #[serial_test::serial]
    fn migrate_uid_in_file_repoints_every_reference() {
        crate::rt::test_runtime(async {
            let contents = format!(
                r#"[{{"uid":"a1","name":"Drive hot","channel_source":{{"device_uid":"{LEGACY_UID}","channel_name":"temp1"}},"channel_sources":[{{"device_uid":"{LEGACY_UID}","channel_name":"temp1"}}]}}]"#
            );
            let tmp = tempfile::tempdir().unwrap();
            let path = write_store(&tmp, &contents).await;

            let migrated = migrate_store(&path).await;

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated);
            assert!(after.contains(LEGACY_UID).not());
            assert_eq!(after.matches(CURRENT_UID).count(), 2);
            assert!(after.contains(r#""name":"Drive hot""#));
        });
    }

    // Goal: modes.json keys a device's settings by its UID, so the re-key must reach map keys
    // and leave a file the real type still loads. Method: serialize a Mode holding a setting
    // under the legacy UID, migrate, then deserialize it again.
    #[test]
    #[serial_test::serial]
    fn migrate_uid_in_file_repoints_a_uid_used_as_a_map_key() {
        use crate::modes::Mode;
        use crate::setting::{Setting, SettingKind};
        use std::collections::HashMap;

        crate::rt::test_runtime(async {
            let setting = Setting {
                channel_name: "fan1".to_string(),
                kind: SettingKind::SpeedFixed { speed_fixed: 42 },
            };
            let mode = Mode {
                uid: "m1".to_string(),
                name: "Quiet".to_string(),
                all_device_settings: HashMap::from([(
                    LEGACY_UID.to_string(),
                    HashMap::from([("fan1".to_string(), setting)]),
                )]),
            };
            let contents = serde_json::json!({
                "modes": [mode],
                "order": ["m1"],
                "current_active_mode": null,
                "previous_active_mode": null,
            })
            .to_string();
            let tmp = tempfile::tempdir().unwrap();
            let path = write_store(&tmp, &contents).await;

            let migrated = migrate_store(&path).await;

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated);
            crate::modes::validate(&after).unwrap();
            let mut file: serde_json::Value = serde_json::from_str(&after).unwrap();
            let mode: Mode = serde_json::from_value(file["modes"][0].take()).unwrap();
            assert!(mode.all_device_settings.contains_key(LEGACY_UID).not());
            assert_eq!(mode.all_device_settings[CURRENT_UID].len(), 1);
        });
    }

    // Goal: negative space. A file that already names the new UID is left untouched, so a
    // half-applied migration is never merged into an ambiguous state. Method: seed both UIDs
    // and assert the file is unchanged.
    #[test]
    #[serial_test::serial]
    fn migrate_uid_in_file_skips_when_current_present() {
        crate::rt::test_runtime(async {
            let contents = format!(
                r#"[{{"uid":"a1","channel_source":{{"device_uid":"{LEGACY_UID}"}}}},{{"uid":"a2","channel_source":{{"device_uid":"{CURRENT_UID}"}}}}]"#
            );
            let tmp = tempfile::tempdir().unwrap();
            let path = write_store(&tmp, &contents).await;

            let migrated = migrate_store(&path).await;

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated.not());
            assert_eq!(after, contents);
        });
    }

    // Goal: the overwhelmingly common case. Nothing references this device, so the file
    // must be left byte-identical rather than rewritten on every boot. Method: seed another UID
    // and assert the file is unchanged.
    #[test]
    #[serial_test::serial]
    fn migrate_uid_in_file_leaves_unrelated_entries_alone() {
        crate::rt::test_runtime(async {
            let contents = r#"[{"uid":"a1","channel_source":{"device_uid":"other"}}]"#;
            let tmp = tempfile::tempdir().unwrap();
            let path = write_store(&tmp, contents).await;

            let migrated = migrate_store(&path).await;

            let after = cc_fs::read_txt(&path).await.unwrap();
            assert!(migrated.not());
            assert_eq!(after, contents);
        });
    }

    // Goal: a store that was never written is the first-run case, not an error. Method: migrate
    // a path that does not exist and assert nothing moved.
    #[test]
    #[serial_test::serial]
    fn migrate_uid_in_file_ignores_a_missing_file() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();

            let migrated = migrate_store(&tmp.path().join("absent.json")).await;

            assert!(migrated.not());
        });
    }
}
