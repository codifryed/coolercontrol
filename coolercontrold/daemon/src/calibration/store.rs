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
use crate::device::{ChannelName, DeviceUID};
use crate::paths;
use anyhow::{anyhow, Result};
use indexmap::IndexMap;
use log::info;
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
        let parsed: CalibrationConfigFile = serde_json::from_str(&contents).map_err(|err| {
            anyhow!(
                "Parsing Calibration configuration file {} - {err}",
                path.display()
            )
        })?;
        self.replace_cache(parsed);
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
    use super::super::curve::{CurveKind, SAMPLE_COUNT};
    use super::*;
    use chrono::Local;

    fn sample_calibration() -> Calibration {
        // Build a deterministic Calibration suitable for serde round-trips
        // and CRUD assertions. The values mirror what the diagnoser would
        // produce for a typical 2000-RPM fan calibrated at 5% steps.
        let mut up = [0u32; SAMPLE_COUNT];
        let mut down = [0u32; SAMPLE_COUNT];
        for (i, (u, d)) in up.iter_mut().zip(down.iter_mut()).enumerate() {
            let rpm = 100 * u32::try_from(i).expect("SAMPLE_COUNT fits in u32");
            *u = rpm;
            *d = rpm;
        }
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 750,
            min_start_duty: 5,
            min_sustain_duty: 5,
            max_eff_duty: 95,
            rpm_max: 2000,
            curve_kind: CurveKind::Smooth,
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
