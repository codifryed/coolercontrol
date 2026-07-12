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
//! UID. Callers are responsible for keeping their assembled uid-keyed device maps unique so that
//! such a collision never silently drops a device.

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
