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

use std::collections::HashSet;

use log::info;
use sha2::{Digest, Sha256};

use crate::device::{Device, DeviceType, UID};

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
    /// `identifier` is the device's preferred identity (a serial or path; may be empty or shared).
    /// `distinguisher` is a stable per-device fallback (its sysfs path) used only on collision.
    /// `assigned` accumulates the UIDs handed out so far; pass the same set for a whole batch.
    ///
    /// Returns the identifier to actually store (so a later `create_uid_from` reproduces the same
    /// UID) and that UID. The first device to claim a UID keeps it, so a stable processing order
    /// (for example path-sorted) yields a stable, reboot-independent result.
    pub fn assign_unique(
        assigned: &mut HashSet<UID>,
        d_type: DeviceType,
        name: &str,
        identifier: &str,
        distinguisher: &str,
    ) -> (String, UID) {
        let natural = Self::create_uid_from(name, d_type, 0, Some(&identifier.to_owned()));
        if assigned.insert(natural.clone()) {
            return (identifier.to_owned(), natural);
        }
        // The preferred identifier is empty or shared with an earlier device. Fall back to the
        // stable per-device distinguisher, salting with an ordinal only if that also collides. The
        // loop is bounded by the number of already-assigned UIDs, since each salt is distinct.
        let mut ordinal: u16 = 0;
        loop {
            let candidate = if ordinal == 0 {
                distinguisher.to_owned()
            } else {
                format!("{distinguisher}#{ordinal}")
            };
            let uid = Self::create_uid_from(name, d_type, 0, Some(&candidate));
            if assigned.insert(uid.clone()) {
                info!(
                    "Device '{name}' computed a UID already used by another device; \
                     assigned it a distinct UID from its location instead"
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
            "/path/x",
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
            "/sys/devices/hwmon10",
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
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "serial-a", "/a");
        let (id_b, uid_b) =
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "serial-b", "/b");

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
            "/sys/hwmon2",
        );
        let (id_b, uid_b) = Device::assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "poweradjust3",
            "",
            "/sys/hwmon3",
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
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "shared", "/a");
        let (id_b, uid_b) =
            Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "shared", "/b");

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

    #[test]
    fn indistinguishable_devices_salt_with_ordinal() {
        // Goal: even devices sharing BOTH identifier and distinguisher (truly indistinguishable)
        // still get distinct UIDs via an ordinal salt, never a collision.
        // Method: assign three devices all with empty identifier and empty distinguisher.
        let mut assigned = HashSet::new();
        let (_, uid_a) = Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", "");
        let (_, uid_b) = Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", "");
        let (_, uid_c) = Device::assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", "");

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
            Device::assign_unique(&mut forward, DeviceType::Hwmon, "dev", "", "/a").0;
        let bumped_forward =
            Device::assign_unique(&mut forward, DeviceType::Hwmon, "dev", "", "/b").0;
        assert_eq!(kept_forward, "");
        assert_eq!(bumped_forward, "/b");

        let mut reverse = HashSet::new();
        let kept_reverse =
            Device::assign_unique(&mut reverse, DeviceType::Hwmon, "dev", "", "/b").0;
        let bumped_reverse =
            Device::assign_unique(&mut reverse, DeviceType::Hwmon, "dev", "", "/a").0;
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
                &format!("/p/{i}"),
            );
            uids.push(uid);
        }
        for i in 0..3 {
            let (_, uid) = Device::assign_unique(
                &mut assigned,
                DeviceType::Liquidctl,
                "other-model",
                "GENERIC-SERIAL",
                &format!("/dev/hidraw{i}"),
            );
            uids.push(uid);
        }
        let distinct: HashSet<&UID> = uids.iter().collect();
        assert_eq!(distinct.len(), uids.len());
        assert_eq!(assigned.len(), uids.len());
    }
}
