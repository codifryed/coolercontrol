// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The shared shape of every repository's "Initialized ... Devices" log line.
//!
//! This is the first line a maintainer reads in a bug report, and it used to be a JSON object
//! keyed by device name. Two devices of the same model share a name, so the second silently
//! overwrote the first: a user with two identical AIOs got a line listing one of them, which read
//! as a device having failed to connect and cost real diagnosis time. An array of objects cannot
//! collapse, and carrying the UID is what correlates an entry with the user's saved settings and
//! with every other line in the journal.

use std::collections::HashMap;

use serde::Serialize;

use crate::device::{Device, DeviceUID};

/// One device's entry in an "Initialized ... Devices" line.
#[derive(Debug, Serialize)]
pub struct DeviceSummary {
    pub name: String,
    pub uid: DeviceUID,
    #[serde(rename = "driver name")]
    pub driver_name: String,
    #[serde(rename = "driver version")]
    pub driver_version: String,
    pub locations: Vec<String>,
    pub channels: Vec<String>,
    pub temps: Vec<String>,
    /// Hwmon only, omitted entirely for every other repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chip: Option<String>,
}

impl DeviceSummary {
    fn new(device: &Device, chip: Option<String>) -> Self {
        let mut channels: Vec<String> = device.info.channels.keys().cloned().collect();
        channels.sort();
        let mut temps: Vec<String> = device.info.temps.keys().cloned().collect();
        temps.sort();
        Self {
            name: device.name.clone(),
            uid: device.uid.clone(),
            driver_name: device.info.driver_info.name.clone().unwrap_or_default(),
            driver_version: device.info.driver_info.version.clone().unwrap_or_default(),
            locations: device.info.driver_info.locations.clone(),
            channels,
            temps,
            chip,
        }
    }
}

/// Renders devices as a JSON array for the info-level init summary.
pub fn summarize_devices<'d>(devices: impl IntoIterator<Item = &'d Device>) -> String {
    summarize(devices, None)
}

/// As `summarize_devices`, with each hwmon device's chip name attached.
pub fn summarize_devices_with_chips<'d>(
    devices: impl IntoIterator<Item = &'d Device>,
    chips: &HashMap<DeviceUID, String>,
) -> String {
    summarize(devices, Some(chips))
}

/// Ordered by name then UID so two runs of the same machine produce comparable lines, which a
/// `HashMap` iteration order never did.
fn summarize<'d>(
    devices: impl IntoIterator<Item = &'d Device>,
    chips: Option<&HashMap<DeviceUID, String>>,
) -> String {
    let mut summaries: Vec<DeviceSummary> = devices
        .into_iter()
        .map(|device| {
            let chip = chips.and_then(|chips| chips.get(&device.uid).cloned());
            DeviceSummary::new(device, chip)
        })
        .collect();
    summaries.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.uid.cmp(&right.uid))
    });
    serde_json::to_string(&summaries).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{DeviceInfo, DeviceType};
    use std::ops::Not;

    fn device_named(name: &str, uid: &str) -> Device {
        let mut device = Device::new(
            name.to_string(),
            DeviceType::Liquidctl,
            1,
            None,
            DeviceInfo::default(),
            Some(uid.to_string()),
            1.0,
        );
        // The UID is normally a hash; the tests need it readable and distinct.
        device.uid = uid.to_string();
        device
    }

    #[test]
    fn identically_named_devices_both_appear() {
        // Goal: the defect this module exists for. Two AIOs of the same model share a name, and
        // the old name-keyed map dropped one of them, so a user's log read as if only one device
        // had connected. Method: two devices with the same name and different UIDs.
        let first = device_named("Asetek 690LC", "uid-a");
        let second = device_named("Asetek 690LC", "uid-b");
        let json = summarize_devices([&first, &second]);
        assert!(json.contains("uid-a"), "first device missing from {json}");
        assert!(json.contains("uid-b"), "second device missing from {json}");
    }

    #[test]
    fn devices_are_ordered_by_name_then_uid() {
        // Goal: two runs of the same machine produce comparable lines, which HashMap order did
        // not. Method: devices supplied out of order.
        let zebra = device_named("Zebra", "uid-z");
        let alpha_second = device_named("Alpha", "uid-b");
        let alpha_first = device_named("Alpha", "uid-a");
        let json = summarize_devices([&zebra, &alpha_second, &alpha_first]);
        let position_of = |uid: &str| json.find(uid).expect("uid must be present");
        assert!(position_of("uid-a") < position_of("uid-b"));
        assert!(position_of("uid-b") < position_of("uid-z"));
    }

    #[test]
    fn the_chip_field_is_present_only_for_hwmon() {
        // Goal: hwmon's extra field rides along without appearing as a null for the other four
        // repositories. Method: the same device summarized both ways.
        let device = device_named("nct6779", "uid-a");
        assert!(summarize_devices([&device]).contains("chip").not());
        let mut chips = HashMap::new();
        chips.insert("uid-a".to_string(), "nct6779".to_string());
        let json = summarize_devices_with_chips([&device], &chips);
        assert!(json.contains("\"chip\":\"nct6779\""), "got {json}");
    }

    #[test]
    fn no_devices_renders_an_empty_array() {
        // Goal: the negative space. A machine with no devices of this type must still log valid
        // JSON. Method: an empty iterator.
        assert_eq!(summarize_devices(Vec::<&Device>::new()), "[]");
    }
}
