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

//! Persistence and in-memory cache for per-channel calibrations.
//!
//! Mirrors the `alerts.rs` pattern: a top-level `RefCell<IndexMap<...>>`
//! is populated from a JSON file at startup, mutated via CRUD methods that
//! save after each change, and serialized through a `Vec<Entry>` wrapper
//! so the on-disk format does not depend on the in-memory key type.

use super::curve::Calibration;
use super::ChannelKey;
use crate::cc_fs;
use crate::device::{ChannelName, DeviceUID, Duty, RPM};
use crate::paths;
use anyhow::{anyhow, Result};
use indexmap::IndexMap;
use log::{info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::ops::Not;

/// On-disk shape of `/etc/coolercontrol/calibrations.json`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationConfigFile {
    pub calibrations: Vec<CalibrationEntry>,
}

/// One persisted calibration record. The on-disk format is a flat list
/// of these because JSON object keys cannot natively express tuples.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CalibrationEntry {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub calibration: Calibration,
}

/// In-memory cache of calibrations, keyed by `(device_uid, channel_name)`.
///
/// All mutating methods persist to disk before returning, mirroring the
/// alerts controller. Reads are cheap clones of the cached values.
pub struct CalibrationStore {
    calibrations: RefCell<IndexMap<ChannelKey, Calibration>>,
}

impl CalibrationStore {
    /// Load the store from disk, creating an empty file on first run.
    pub async fn init() -> Result<Self> {
        let store = Self::empty();
        store.load_from_disk().await?;
        Ok(store)
    }

    /// Construct an empty store without touching disk.
    ///
    /// Public so unit tests and future Phase 1 consumers can build a
    /// store in-process without a filesystem dependency.
    pub fn empty() -> Self {
        Self {
            calibrations: RefCell::new(IndexMap::new()),
        }
    }

    /// Number of stored calibrations.
    pub fn len(&self) -> usize {
        self.calibrations.borrow().len()
    }

    /// Whether the store holds a calibration for the given channel.
    pub fn has(&self, key: &ChannelKey) -> bool {
        self.calibrations.borrow().contains_key(key)
    }

    /// Returns a clone of the stored calibration for the given channel.
    pub fn get(&self, key: &ChannelKey) -> Option<Calibration> {
        self.calibrations.borrow().get(key).cloned()
    }

    /// Returns clones of every stored `(key, calibration)` pair.
    pub fn all(&self) -> Vec<(ChannelKey, Calibration)> {
        let map = self.calibrations.borrow();
        let mut out = Vec::with_capacity(map.len());
        for (k, v) in map.iter() {
            out.push((k.clone(), v.clone()));
        }
        out
    }

    /// Map a measured RPM to its true-duty equivalent for the given
    /// channel. Returns `None` when the channel is uncalibrated, when
    /// the curve is stepped (mapping disabled), or when the channel's
    /// calibration would not produce a meaningful value.
    ///
    /// Used by the status-ingestion pipeline as the sanity cross-check
    /// against the device-duty-derived value: when the two disagree by
    /// more than a small threshold, the RPM-derived value wins so a
    /// stuck or dead fan does not stay hidden behind the device-duty
    /// the daemon wrote.
    pub fn rpm_to_true_duty(&self, key: &ChannelKey, rpm: RPM) -> Option<Duty> {
        let map = self.calibrations.borrow();
        let calibration = map.get(key)?;
        calibration.rpm_to_true_duty(rpm)
    }

    /// Map a cached **device-duty** (raw PWM percent currently being
    /// driven) to its true-duty equivalent for the given channel. Same
    /// `None` semantics as `rpm_to_true_duty`. Used by the status
    /// pipeline as the **stable** source of the displayed true-duty,
    /// cross-checked against the RPM-derived value.
    pub fn device_to_true_duty(&self, key: &ChannelKey, device_duty: Duty) -> Option<Duty> {
        let map = self.calibrations.borrow();
        let calibration = map.get(key)?;
        calibration.device_to_true_duty(device_duty)
    }

    /// Insert or replace a calibration. Persists to disk on success.
    pub async fn insert(&self, key: ChannelKey, calibration: Calibration) -> Result<()> {
        self.calibrations.borrow_mut().insert(key, calibration);
        self.save_to_disk().await
    }

    /// Insert without persisting. Reserved for the diagnoser path so it
    /// can atomically build a batch of changes and save once at the end.
    pub fn insert_unsaved(&self, key: ChannelKey, calibration: Calibration) {
        self.calibrations.borrow_mut().insert(key, calibration);
    }

    /// Remove the calibration for a channel. Persists on success when the
    /// key existed; otherwise returns `Ok(())` without touching disk.
    pub async fn remove(&self, key: &ChannelKey) -> Result<()> {
        let existed = self.calibrations.borrow_mut().shift_remove(key).is_some();
        if existed {
            self.save_to_disk().await?;
        }
        Ok(())
    }

    /// Explicit disk save. Public so the shutdown hook can flush pending
    /// in-memory changes (mirrors `Alerts::watch_for_shutdown`).
    pub async fn save_to_disk(&self) -> Result<()> {
        let entries = self.snapshot_entries();
        let file = CalibrationConfigFile {
            calibrations: entries,
        };
        let json = serde_json::to_string(&file)?;
        cc_fs::write_string(paths::calibration_config_file(), json)
            .await
            .map_err(|err| anyhow!("Writing Calibration Configuration File - {err}"))
    }

    /// Reads the JSON file at the configured path and fills the cache.
    ///
    /// On a fresh install the file does not exist; we write a default
    /// empty file and read it back so subsequent saves overwrite atomic
    /// well-formed JSON. Same approach as the alerts controller.
    async fn load_from_disk(&self) -> Result<()> {
        ensure_config_dir().await?;
        let path = paths::calibration_config_file().to_path_buf();
        let contents = if let Ok(c) = cc_fs::read_txt(&path).await {
            c
        } else {
            info!("Writing a new Calibration configuration file");
            let default = serde_json::to_string(&CalibrationConfigFile {
                calibrations: Vec::with_capacity(0),
            })?;
            cc_fs::write_string(&path, default).await.map_err(|err| {
                anyhow!("Writing new configuration file: {} - {err}", path.display())
            })?;
            cc_fs::read_txt(&path)
                .await
                .map_err(|err| anyhow!("Reading configuration file {} - {err}", path.display()))?
        };
        // Parse entries one at a time so a single corrupted/legacy
        // entry (e.g. pre-variable-resolution schema) does not torch
        // the whole store. Failures are logged and skipped; the user
        // re-calibrates the affected channel.
        let raw: serde_json::Value = serde_json::from_str(&contents).map_err(|err| {
            anyhow!(
                "Parsing Calibration configuration file {} - {err}",
                path.display()
            )
        })?;
        let entries_json = raw
            .get("calibrations")
            .and_then(serde_json::Value::as_array)
            .cloned()
            .unwrap_or_default();
        let mut calibrations = Vec::with_capacity(entries_json.len());
        for entry_json in entries_json {
            match serde_json::from_value::<CalibrationEntry>(entry_json.clone()) {
                Ok(entry) => calibrations.push(entry),
                Err(err) => {
                    let key = entry_json
                        .get("device_uid")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("<unknown>");
                    let channel = entry_json
                        .get("channel_name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("<unknown>");
                    warn!(
                        "Dropping incompatible calibration entry for {key}:{channel} - {err}. \
                         Re-calibrate the channel from the UI to restore mapping."
                    );
                }
            }
        }
        self.replace_cache(CalibrationConfigFile { calibrations });
        Ok(())
    }

    /// Build a sorted entries list for serialization. Sorting by key keeps
    /// the on-disk format stable across saves so diffs are reviewable.
    fn snapshot_entries(&self) -> Vec<CalibrationEntry> {
        let map = self.calibrations.borrow();
        let mut entries: Vec<CalibrationEntry> = Vec::with_capacity(map.len());
        for ((device_uid, channel_name), calibration) in map.iter() {
            entries.push(CalibrationEntry {
                device_uid: device_uid.clone(),
                channel_name: channel_name.clone(),
                calibration: calibration.clone(),
            });
        }
        entries.sort_by(|a, b| {
            a.device_uid
                .cmp(&b.device_uid)
                .then_with(|| a.channel_name.cmp(&b.channel_name))
        });
        entries
    }

    fn replace_cache(&self, parsed: CalibrationConfigFile) {
        let mut lock = self.calibrations.borrow_mut();
        lock.clear();
        for entry in parsed.calibrations {
            lock.insert((entry.device_uid, entry.channel_name), entry.calibration);
        }
    }
}

/// Ensures the config directory exists, creating it if necessary.
async fn ensure_config_dir() -> Result<()> {
    let dir = paths::config_dir();
    if dir.exists().not() {
        info!(
            "config directory doesn't exist. Attempting to create it: {}",
            dir.display()
        );
        cc_fs::create_dir_all(dir).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::curve::{CurveKind, DutySample};
    use super::*;
    use chrono::Local;

    fn sample_calibration() -> Calibration {
        // Build a deterministic Calibration suitable for serde round-trips
        // and CRUD assertions. Uniform 5%-step samples are sufficient for
        // store-level tests; the diagnoser tests cover variable spacing.
        let up: Vec<DutySample> = (0..21usize)
            .map(|i| DutySample {
                duty: u8::try_from(i).expect("fits in u8") * 5,
                rpm: 100 * u32::try_from(i).expect("fits in u32"),
            })
            .collect();
        let down = up.clone();
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 750,
            min_start_duty: 5,
            min_sustain_duty: 5,
            min_stable_duty: 5,
            max_eff_duty: 95,
            rpm_max: 2000,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            was_rpm_only: false,
            timestamp: Local::now(),
        }
    }

    #[test]
    fn calibration_round_trips_through_json() {
        // Goal: serde must round-trip Calibration losslessly, since the
        // on-disk JSON is the canonical persisted form of every store
        // record. Any drift would corrupt user data on first save.
        let original = sample_calibration();
        let json = serde_json::to_string(&original).expect("serializes");
        let recovered: Calibration = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(recovered, original);
    }

    #[test]
    fn config_file_round_trips_with_multiple_entries() {
        // Goal: a config file holding several entries must round-trip with
        // exact ordering and content. Tests the wrapper layer plus the
        // entry struct in combination.
        let entries = vec![
            CalibrationEntry {
                device_uid: "dev-a".to_string(),
                channel_name: "fan1".to_string(),
                calibration: sample_calibration(),
            },
            CalibrationEntry {
                device_uid: "dev-b".to_string(),
                channel_name: "pump".to_string(),
                calibration: sample_calibration(),
            },
        ];
        let file = CalibrationConfigFile {
            calibrations: entries.clone(),
        };
        let json = serde_json::to_string(&file).expect("serializes");
        let recovered: CalibrationConfigFile = serde_json::from_str(&json).expect("deserializes");
        assert_eq!(recovered.calibrations, entries);
    }

    #[test]
    fn empty_store_is_empty() {
        // Goal: a freshly constructed store has no records and reports
        // accurate len/has/get for an unseen channel.
        let store = CalibrationStore::empty();
        assert_eq!(store.len(), 0);
        assert!(store.all().is_empty());
        let key: ChannelKey = ("dev-x".to_string(), "fan1".to_string());
        assert!(store.has(&key).not());
        assert!(store.get(&key).is_none());
    }

    #[test]
    fn insert_unsaved_then_get_returns_clone() {
        // Goal: the unsaved insertion path puts the value in the cache so
        // subsequent get/has/len observe it. The diagnoser relies on this
        // for the build-batch-then-save flow.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        let cal = sample_calibration();
        store.insert_unsaved(key.clone(), cal.clone());
        assert!(store.has(&key));
        assert_eq!(store.len(), 1);
        let recovered = store.get(&key).expect("present after insert");
        assert_eq!(recovered, cal);
    }

    #[test]
    fn all_returns_inserted_entries_in_insertion_order() {
        // Goal: the bulk `/calibrations` route depends on `all()` returning
        // every inserted (key, calibration) pair. Verify both that the
        // entries are present and that the values are cloned (mutating
        // the recovered value must not mutate the cache).
        let store = CalibrationStore::empty();
        let key_a: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        let key_b: ChannelKey = ("dev-b".to_string(), "pump".to_string());
        let cal_a = sample_calibration();
        let mut cal_b = sample_calibration();
        cal_b.kick_duration_ms = 1234;
        store.insert_unsaved(key_a.clone(), cal_a.clone());
        store.insert_unsaved(key_b.clone(), cal_b.clone());
        let all = store.all();
        assert_eq!(all.len(), 2);
        // IndexMap preserves insertion order.
        assert_eq!(all[0].0, key_a);
        assert_eq!(all[0].1, cal_a);
        assert_eq!(all[1].0, key_b);
        assert_eq!(all[1].1, cal_b);
        // Mutating the returned value must not leak back into the store.
        let mut leaked = all;
        leaked[0].1.kick_duration_ms = 9999;
        assert_eq!(
            store.get(&key_a).expect("still present").kick_duration_ms,
            cal_a.kick_duration_ms,
        );
    }

    #[test]
    fn insert_unsaved_replaces_existing_for_same_key() {
        // Goal: re-inserting the same channel key must replace the prior
        // entry, not append a duplicate. Re-calibration depends on this.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        let mut first = sample_calibration();
        first.kick_duration_ms = 500;
        let mut second = sample_calibration();
        second.kick_duration_ms = 900;
        store.insert_unsaved(key.clone(), first);
        store.insert_unsaved(key.clone(), second.clone());
        assert_eq!(store.len(), 1);
        assert_eq!(store.get(&key).expect("present").kick_duration_ms, 900);
    }

    #[test]
    fn snapshot_entries_is_sorted_by_key() {
        // Goal: serialization must produce a key-sorted list so diffs of
        // calibrations.json stay reviewable across saves regardless of
        // insertion order.
        let store = CalibrationStore::empty();
        store.insert_unsaved(
            ("dev-z".to_string(), "fan2".to_string()),
            sample_calibration(),
        );
        store.insert_unsaved(
            ("dev-a".to_string(), "fan9".to_string()),
            sample_calibration(),
        );
        store.insert_unsaved(
            ("dev-a".to_string(), "fan1".to_string()),
            sample_calibration(),
        );
        let entries = store.snapshot_entries();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].device_uid, "dev-a");
        assert_eq!(entries[0].channel_name, "fan1");
        assert_eq!(entries[1].device_uid, "dev-a");
        assert_eq!(entries[1].channel_name, "fan9");
        assert_eq!(entries[2].device_uid, "dev-z");
    }

    #[test]
    fn replace_cache_clears_prior_entries() {
        // Goal: load_from_disk uses replace_cache; the parsed file must
        // become the new source of truth and any prior in-memory state
        // must be dropped. Without this, re-reading a shrunk file would
        // leave stale entries in memory.
        let store = CalibrationStore::empty();
        store.insert_unsaved(
            ("dev-old".to_string(), "fan-old".to_string()),
            sample_calibration(),
        );
        assert_eq!(store.len(), 1);
        let parsed = CalibrationConfigFile {
            calibrations: vec![CalibrationEntry {
                device_uid: "dev-new".to_string(),
                channel_name: "fan-new".to_string(),
                calibration: sample_calibration(),
            }],
        };
        store.replace_cache(parsed);
        assert_eq!(store.len(), 1);
        assert!(store
            .has(&("dev-old".to_string(), "fan-old".to_string()))
            .not());
        assert!(store.has(&("dev-new".to_string(), "fan-new".to_string())));
    }

    #[test]
    fn rpm_to_true_duty_on_uncalibrated_channel_returns_none() {
        // Goal: an unknown channel produces None, signalling the
        // ingestion pipeline to leave the device-duty alone.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        assert!(store.rpm_to_true_duty(&key, 1234).is_none());
    }

    #[test]
    fn rpm_to_true_duty_on_smooth_channel_returns_some() {
        // Goal: a calibrated smooth channel returns a mapped value
        // bounded by 0..=100. The exact value comes from the curve
        // math in curve.rs (covered by its own tests); here we just
        // confirm the store-level routing is correct.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        store.insert_unsaved(key.clone(), sample_calibration());
        let mapped = store
            .rpm_to_true_duty(&key, 1000)
            .expect("smooth channel maps");
        assert!(mapped <= 100);
    }

    #[test]
    fn rpm_to_true_duty_on_stepped_channel_returns_none() {
        // Goal: a calibrated channel whose curve was classified as
        // stepped must signal passthrough via None. The ingestion
        // pipeline then leaves the device-duty unchanged.
        let store = CalibrationStore::empty();
        let mut cal = sample_calibration();
        cal.curve_kind = CurveKind::Stepped;
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        store.insert_unsaved(key, cal);
        let key2: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        assert!(store.rpm_to_true_duty(&key2, 1000).is_none());
    }

    #[test]
    fn was_rpm_only_round_trips_through_json() {
        // Goal: the `was_rpm_only` flag must survive a JSON round-trip so
        // the daemon can read it back at startup and decide on later
        // deletes whether to clear `status_history` duty values. The
        // companion case (`#[serde(default)] = false` for older records)
        // is exercised by `calibration_round_trips_through_json`, which
        // builds via the struct literal and gets the default.
        let mut original = sample_calibration();
        original.was_rpm_only = true;
        let json = serde_json::to_string(&original).expect("serializes");
        let recovered: Calibration = serde_json::from_str(&json).expect("deserializes");
        assert!(recovered.was_rpm_only, "flag must round-trip as true");
        assert_eq!(recovered, original);
    }

    #[test]
    fn was_rpm_only_defaults_to_false_when_absent_from_json() {
        // Goal: an older persisted calibration that pre-dates the
        // `was_rpm_only` field must load cleanly, defaulting the flag
        // to false. This is what `#[serde(default)]` buys us; the test
        // pins the contract so a future refactor cannot silently
        // remove the default.
        let mut original = sample_calibration();
        original.was_rpm_only = true;
        let mut value = serde_json::to_value(&original).expect("serializes");
        // Strip the field to simulate a pre-feature JSON blob.
        if let serde_json::Value::Object(ref mut map) = value {
            map.remove("was_rpm_only");
        }
        let recovered: Calibration = serde_json::from_value(value).expect("deserializes");
        assert!(
            recovered.was_rpm_only.not(),
            "missing field must default to false"
        );
    }

    #[test]
    fn empty_config_file_serializes_as_empty_array() {
        // Goal: the default-on-first-run file written by load_from_disk
        // must contain an empty calibrations array, not null or absent.
        // A malformed default would block the daemon at startup.
        let file = CalibrationConfigFile {
            calibrations: Vec::with_capacity(0),
        };
        let json = serde_json::to_string(&file).expect("serializes");
        assert!(json.contains("\"calibrations\":[]"));
    }
}
