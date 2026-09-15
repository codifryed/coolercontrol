// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Persistence + in-memory cache for per-channel calibrations.
//! `RefCell<IndexMap>` populated from JSON; CRUD methods save after
//! each change. Wire format is `Vec<CalibrationEntry>` since JSON
//! object keys can't be tuples.

use super::curve::Calibration;
use super::ChannelKey;
use crate::cc_fs;
use crate::device::{ChannelName, DeviceUID, Duty, Temp, RPM};
use crate::overrides::OverridesController;
use crate::paths;
use anyhow::{anyhow, Result};
use indexmap::IndexMap;
use log::{info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::ops::Not;
use std::rc::Rc;

/// On-disk shape of `/etc/coolercontrol/calibrations.json`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationConfigFile {
    pub calibrations: Vec<CalibrationEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct CalibrationEntry {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub calibration: Calibration,
}

/// Validates that `contents` parse as a calibrations config file.
pub fn validate(contents: &str) -> Result<()> {
    serde_json::from_str::<CalibrationConfigFile>(contents)
        .map_err(|err| anyhow!("Parsing calibrations configuration: {err}"))
        .map(|_| ())
}

/// Marks a field the failed entry did not carry at all.
const UNKNOWN_JSON_FIELD: &str = "<unknown>";

/// Upper bound on a raw field copied into a log line. A real UID is 64 hex characters, so
/// nothing legitimate is cut; this only caps a corrupt or hand-edited file.
const JSON_FIELD_LOG_CHARS_MAX: usize = 64;

/// One string field of an entry that failed to parse. Untrusted: the file is hand-editable and
/// can arrive from a restored backup, so the value is bounded before it reaches a log line.
fn json_str_field(entry_json: &serde_json::Value, field: &str) -> String {
    let raw = entry_json
        .get(field)
        .and_then(serde_json::Value::as_str)
        .unwrap_or(UNKNOWN_JSON_FIELD);
    let bounded: String = raw.chars().take(JSON_FIELD_LOG_CHARS_MAX).collect();
    debug_assert!(bounded.chars().count() <= JSON_FIELD_LOG_CHARS_MAX);
    bounded
}

pub struct CalibrationStore {
    calibrations: RefCell<IndexMap<ChannelKey, Calibration>>,
    /// Resolves the device and channel names this store logs. A constructor argument rather
    /// than a builder because the load itself logs, so a later attachment would arrive after
    /// the lines it is meant to name.
    overrides: Rc<OverridesController>,
}

impl CalibrationStore {
    /// Load from disk, creating an empty file on first run.
    pub async fn init(overrides: Rc<OverridesController>) -> Result<Self> {
        let store = Self::with_overrides(overrides);
        store.load_from_disk().await?;
        Ok(store)
    }

    /// In-memory only; no disk I/O. Names resolve to their raw form.
    #[cfg(test)]
    pub fn empty() -> Self {
        Self::with_overrides(Rc::new(OverridesController::empty()))
    }

    /// In-memory only, resolving names against `overrides`.
    pub fn with_overrides(overrides: Rc<OverridesController>) -> Self {
        Self {
            calibrations: RefCell::new(IndexMap::new()),
            overrides,
        }
    }

    /// Log display form of a channel this store holds a calibration for.
    pub fn log_device_channel(&self, device_uid: &DeviceUID, channel_name: &str) -> String {
        self.overrides.log_device_channel(device_uid, channel_name)
    }

    #[allow(dead_code)] // test-only currently; useful production API.
    pub fn len(&self) -> usize {
        self.calibrations.borrow().len()
    }

    pub fn has(&self, key: &ChannelKey) -> bool {
        self.calibrations.borrow().contains_key(key)
    }

    pub fn get(&self, key: &ChannelKey) -> Option<Calibration> {
        self.calibrations.borrow().get(key).cloned()
    }

    pub fn all(&self) -> Vec<(ChannelKey, Calibration)> {
        let map = self.calibrations.borrow();
        let mut out = Vec::with_capacity(map.len());
        for (k, v) in map.iter() {
            out.push((k.clone(), v.clone()));
        }
        out
    }

    /// `None` for uncalibrated, stepped, or non-mappable channels.
    pub fn rpm_to_true_duty(&self, key: &ChannelKey, rpm: RPM) -> Option<Duty> {
        let map = self.calibrations.borrow();
        let calibration = map.get(key)?;
        calibration.rpm_to_true_duty(rpm)
    }

    /// Stable inverse via the down-curve. `None` semantics as
    /// `rpm_to_true_duty`.
    pub fn device_to_true_duty(&self, key: &ChannelKey, device_duty: Duty) -> Option<Duty> {
        let map = self.calibrations.borrow();
        let calibration = map.get(key)?;
        calibration.device_to_true_duty(device_duty)
    }

    /// Maps a Graph profile's true-duty points into the device-duty a
    /// firmware curve expects. `None` when the channel is uncalibrated or
    /// stepped, so the caller writes the user's points unchanged.
    pub fn map_curve_points(
        &self,
        key: &ChannelKey,
        points: &[(Temp, Duty)],
    ) -> Option<Vec<(Temp, Duty)>> {
        let map = self.calibrations.borrow();
        let calibration = map.get(key)?;
        let mut mapped = Vec::with_capacity(points.len());
        for &(temp, true_duty) in points {
            // Stepped channels have no forward map.
            let device_duty = calibration.true_to_device(true_duty)?.sustain;
            mapped.push((temp, device_duty));
        }
        debug_assert_eq!(mapped.len(), points.len());
        Some(mapped)
    }

    /// Whether the forward map of `true_duty` would land on `device_duty`
    /// for this channel's calibration. False when the channel is
    /// uncalibrated or stepped.
    pub fn preimage_contains_true_duty(
        &self,
        key: &ChannelKey,
        device_duty: Duty,
        true_duty: Duty,
    ) -> bool {
        let map = self.calibrations.borrow();
        map.get(key)
            .is_some_and(|cal| cal.preimage_contains_true_duty(device_duty, true_duty))
    }

    /// Insert without persisting. Used by the diagnoser to build a batch
    /// of changes and save once at the end.
    pub fn insert_unsaved(&self, key: ChannelKey, calibration: Calibration) {
        self.calibrations.borrow_mut().insert(key, calibration);
    }

    /// Removes the calibration; persists only when the key existed.
    pub async fn remove(&self, key: &ChannelKey) -> Result<()> {
        let existed = self.calibrations.borrow_mut().shift_remove(key).is_some();
        if existed {
            self.save_to_disk().await?;
        }
        Ok(())
    }

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
                calibrations: Vec::new(),
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
                    warn!(
                        "Dropping incompatible calibration entry for {} - {err}. \
                         Re-calibrate the channel from the UI to restore mapping.",
                        self.dropped_entry_label(&entry_json)
                    );
                }
            }
        }
        self.replace_cache(CalibrationConfigFile { calibrations });
        for ((device_uid, channel_name), calibration) in self.calibrations.borrow().iter() {
            info!(
                "Calibration loaded for {} (curve_kind={:?}, rpm_max={}, warnings={})",
                self.log_device_channel(device_uid, channel_name),
                calibration.curve_kind,
                calibration.rpm_max,
                calibration.warnings.len()
            );
        }
        Ok(())
    }

    /// Names an entry that failed to deserialize. Its fields never passed validation, so they
    /// are bounded here and sanitized on the way to the log by the resolver.
    fn dropped_entry_label(&self, entry_json: &serde_json::Value) -> String {
        let device_uid = json_str_field(entry_json, "device_uid");
        let channel_name = json_str_field(entry_json, "channel_name");
        self.log_device_channel(&device_uid, &channel_name)
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

    #[test]
    fn dropped_entry_label_marks_fields_the_entry_lacks() {
        // Goal: an entry too broken to parse still names something, so the warning says which
        // channel to re-calibrate instead of printing nothing.
        let store = CalibrationStore::empty();
        let label = store.dropped_entry_label(&serde_json::json!({}));
        assert_eq!(label, "unknown device (<unknown>) | <unknown>");

        let label = store.dropped_entry_label(&serde_json::json!({
            "device_uid": "dev-a",
            "channel_name": "fan1",
        }));
        assert_eq!(label, "unknown device (dev-a) | fan1");
    }

    #[test]
    fn dropped_entry_label_bounds_an_untrusted_field() {
        // Goal: the file is hand-editable and restorable from backup, so a field of any size
        // cannot turn one dropped entry into an unbounded log line.
        let store = CalibrationStore::empty();
        let huge = "z".repeat(10_000);
        let label = store.dropped_entry_label(&serde_json::json!({
            "device_uid": "dev-a",
            "channel_name": huge,
        }));
        assert!(label.contains(&huge).not());
        assert!(label.chars().count() <= JSON_FIELD_LOG_CHARS_MAX * 2 + 32);
    }

    #[test]
    fn loaded_entry_label_resolves_through_the_name_chain() {
        // Goal: the startup line names the device and channel the way the rest of the daemon
        // does, rather than the UID hash the calibration is keyed by.
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let device = Rc::new(RefCell::new(crate::device::Device::new(
                "nct6687".to_string(),
                crate::device::DeviceType::Hwmon,
                0,
                None,
                crate::device::DeviceInfo::default(),
                None,
                1.0,
            )));
            let uid = device.borrow().uid.clone();
            let all_devices: crate::AllDevices =
                Rc::new(std::collections::HashMap::from([(uid.clone(), device)]));
            let config = Rc::new(crate::config::Config::init_default_config().unwrap());
            config.create_device_list(&all_devices);
            let overrides =
                Rc::new(OverridesController::init_from(tmp.path().join("overrides.toml")).await);
            overrides.set_device_context(&all_devices, config);
            overrides
                .set_channel_label(
                    &uid,
                    "hint",
                    &"fan3".to_string(),
                    None,
                    Some("Rear Exhaust"),
                )
                .await
                .unwrap();

            let store = CalibrationStore::with_overrides(overrides);
            assert_eq!(
                store.log_device_channel(&uid, "fan3"),
                "nct6687 | Rear Exhaust (fan3)"
            );
            // Negative space: the UID the calibration is keyed by never reaches the line.
            assert!(store.log_device_channel(&uid, "fan3").contains(&uid).not());
        });
    }

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
            kick_boost_override: None,
            kick_duration_override_ms: None,
            walk_after_kick_override: None,
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
    fn calibration_deserializes_without_walk_after_kick_override() {
        // Goal: loading a JSON blob saved before `walk_after_kick_override`
        // existed must succeed and default the field to None. Guards
        // against `#[serde(default)]` getting dropped during a refactor,
        // which would silently break upgrades for every existing user.
        let mut value = serde_json::to_value(sample_calibration()).expect("serializes");
        let map = value.as_object_mut().expect("object");
        assert!(
            map.remove("walk_after_kick_override").is_some(),
            "fixture must include the field before we strip it"
        );
        let json = serde_json::to_string(&value).expect("re-serializes without field");
        let recovered: Calibration = serde_json::from_str(&json).expect("deserializes");
        assert!(recovered.walk_after_kick_override.is_none());
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
    fn map_curve_points_lifts_true_duty_into_device_duty() {
        // Goal: a firmware curve is written in device-duty, so the
        // mapping must lift each true-duty point. Expected values follow
        // from the fixture's linear curve (rpm = duty * 20, floor 100 rpm
        // at duty 5): true 10 -> 14, true 50 -> 52. Endpoints stay exact
        // so an off-point stays off (AMD zero-RPM detection reads it) and
        // full duty stays full. Temps are untouched.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        store.insert_unsaved(key.clone(), sample_calibration());
        let points = vec![(30.0, 0), (50.0, 10), (70.0, 50), (90.0, 100)];

        let mapped = store.map_curve_points(&key, &points).expect("maps");

        assert_eq!(mapped, vec![(30.0, 0), (50.0, 14), (70.0, 52), (90.0, 100)]);
    }

    #[test]
    fn map_curve_points_stays_monotonic() {
        // Goal: the firmware interpolates linearly between the points it
        // is given, so a non-monotonic mapping would make the curve dip
        // as temperature rises.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        store.insert_unsaved(key.clone(), sample_calibration());
        let points: Vec<(Temp, Duty)> = (0..=100)
            .map(|duty| (f64::from(duty), u8::try_from(duty).expect("fits")))
            .collect();

        let mapped = store.map_curve_points(&key, &points).expect("maps");

        for pair in mapped.windows(2) {
            assert!(
                pair[1].1 >= pair[0].1,
                "mapping must not decrease: {pair:?}"
            );
        }
    }

    #[test]
    fn map_curve_points_none_for_uncalibrated_and_stepped() {
        // Goal: both cases must return None so the caller writes the
        // user's own points. Stepped channels have no forward map at all.
        let store = CalibrationStore::empty();
        let key: ChannelKey = ("dev-a".to_string(), "fan1".to_string());
        let points = vec![(30.0, 20), (70.0, 100)];
        assert!(store.map_curve_points(&key, &points).is_none());

        let mut stepped = sample_calibration();
        stepped.curve_kind = CurveKind::Stepped;
        store.insert_unsaved(key.clone(), stepped);
        assert!(store.map_curve_points(&key, &points).is_none());
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
        // Goal: the `was_rpm_only` flag must survive a JSON round-trip
        // so the daemon can read it back at startup and decide on later
        // deletes whether to clear `status_history` duty values.
        let mut original = sample_calibration();
        original.was_rpm_only = true;
        let json = serde_json::to_string(&original).expect("serializes");
        let recovered: Calibration = serde_json::from_str(&json).expect("deserializes");
        assert!(recovered.was_rpm_only, "flag must round-trip as true");
        assert_eq!(recovered, original);
    }

    #[test]
    fn empty_config_file_serializes_as_empty_array() {
        // Goal: the default-on-first-run file written by load_from_disk
        // must contain an empty calibrations array, not null or absent.
        // A malformed default would block the daemon at startup.
        let file = CalibrationConfigFile {
            calibrations: Vec::new(),
        };
        let json = serde_json::to_string(&file).expect("serializes");
        assert!(json.contains("\"calibrations\":[]"));
    }
}
