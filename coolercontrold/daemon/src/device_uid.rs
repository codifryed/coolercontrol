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
//! so this logic is high-stakes and rarely changed: altering it re-keys existing configs. It lives
//! here, separate from the `Device` struct, so the reasoning is in one place.
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

use crate::device::{DeviceType, UID};

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
        // this should be pretty unique to the device itself, such as a serial number or device path
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
/// `identifier` is the device's preferred identity (a serial or path; it may be empty or shared).
/// `distinguisher` is a stable per-device fallback (its sysfs path) used only on collision.
/// `assigned` accumulates the UIDs handed out so far; pass the same set for every device in a batch.
///
/// Returns the identifier to actually store (so a later `create_uid_from` reproduces the same UID)
/// and that UID. The first device to claim a UID keeps it, so a stable processing order (for
/// example path-sorted) yields a stable, reboot-independent result.
pub fn assign_unique(
    assigned: &mut HashSet<UID>,
    d_type: DeviceType,
    name: &str,
    identifier: &str,
    distinguisher: &str,
) -> (String, UID) {
    let natural = create_uid_from(name, d_type, 0, Some(&identifier.to_owned()));
    if assigned.insert(natural.clone()) {
        return (identifier.to_owned(), natural);
    }
    // The preferred identifier is empty or shared with an earlier device. Fall back to the stable
    // per-device distinguisher, salting with an ordinal only if that also collides. The loop is
    // bounded by the number of already-assigned UIDs, since each salt is a distinct string.
    let mut ordinal: u16 = 0;
    loop {
        let candidate = if ordinal == 0 {
            distinguisher.to_owned()
        } else {
            format!("{distinguisher}#{ordinal}")
        };
        let uid = create_uid_from(name, d_type, 0, Some(&candidate));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_identifiers_keep_natural_uids() {
        // Goal: two devices with different identifiers each keep their natural UID unchanged.
        // Method: assign two distinct serials and assert the identifiers and UIDs pass through.
        let mut assigned = HashSet::new();
        let (id_a, uid_a) = assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "dev",
            "serial-a",
            "/path/a",
        );
        let (id_b, uid_b) = assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "dev",
            "serial-b",
            "/path/b",
        );

        assert_eq!(id_a, "serial-a");
        assert_eq!(id_b, "serial-b");
        assert_ne!(uid_a, uid_b);
        assert_eq!(
            uid_a,
            create_uid_from("dev", DeviceType::Hwmon, 0, Some(&"serial-a".to_owned()))
        );
    }

    #[test]
    fn colliding_identifier_disambiguates_by_distinguisher() {
        // Goal: the reported case, two devices with the same (blank) identifier. The first keeps the
        // natural UID and its settings; the second is disambiguated by its distinct sysfs path.
        // Method: assign two blank-identifier devices with different paths.
        let mut assigned = HashSet::new();
        let (id_a, uid_a) = assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "poweradjust3",
            "",
            "/sys/devices/hwmon2",
        );
        let (id_b, uid_b) = assign_unique(
            &mut assigned,
            DeviceType::Hwmon,
            "poweradjust3",
            "",
            "/sys/devices/hwmon3",
        );

        // the incumbent keeps the natural blank-identifier UID:
        assert_eq!(id_a, "");
        assert_eq!(
            uid_a,
            create_uid_from("poweradjust3", DeviceType::Hwmon, 0, Some(&String::new()))
        );
        // the second is given its path as its identity, producing a distinct UID:
        assert_eq!(id_b, "/sys/devices/hwmon3");
        assert_ne!(uid_a, uid_b);
    }

    #[test]
    fn indistinguishable_devices_salt_with_ordinal() {
        // Goal: even devices sharing BOTH identifier and distinguisher (truly indistinguishable)
        // still get distinct UIDs via an ordinal salt, never a collision.
        // Method: assign three devices all with empty identifier and empty distinguisher.
        let mut assigned = HashSet::new();
        let (_, uid_a) = assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", "");
        let (_, uid_b) = assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", "");
        let (_, uid_c) = assign_unique(&mut assigned, DeviceType::Hwmon, "dev", "", "");

        assert_ne!(uid_a, uid_b);
        assert_ne!(uid_b, uid_c);
        assert_ne!(uid_a, uid_c);
        assert_eq!(assigned.len(), 3);
    }
}
