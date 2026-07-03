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

//! Daemon-authoritative tracking of unhealthy device references and channels.
//!
//! Surfaced to clients so they do not recompute health from raw status:
//! - `missing`: a config temp-source reference whose target temp is absent from
//!   the current device set (a Custom Sensor source, Graph Profile temp source,
//!   or LCD temp source pointing at a removed device or deleted sensor).
//! - `stale_source`: a config temp-source reference whose target is present but
//!   currently failsafed, so the consuming Profile/LCD/Custom Sensor acts on
//!   failsafe values rather than real readings.
//! - `failsafe`: a present channel/temp currently serving failsafe values.

use crate::api::actor::DeviceHealthHandle;
use crate::config::Config;
use crate::device::{DeviceType, DeviceUID, TempName, UID};
use crate::overrides::OverridesController;
use crate::setting::{CustomSensor, Profile, SettingKind, TempSource};
use crate::{AllDevices, Repos};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::rc::Rc;

/// The kind of config entity that holds a temp-source reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HealthEntityType {
    CustomSensor,
    Profile,
    Lcd,
}

/// A config reference whose temp-source target is absent from the current device
/// set. The `entity_*` fields let a client badge and deep-link to the owning
/// editor; `source` is the referenced {device_uid, temp_name}.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub entity_type: HealthEntityType,
    /// Profile uid, Custom Sensor id, or the owning device uid for an LCD setting.
    pub entity_uid: UID,
    /// Display name: profile/device name, or the sensor id.
    pub entity_name: String,
    /// The channel the setting is on. Only set for LCD references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    pub source: TempSource,
    /// Name of the device owning the missing temp, resolved from the live
    /// device set or the config `devices` list (gone devices stay listed).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_device_name: Option<String>,
}

/// Whether a failsafing node is a temp or a control channel. One entry per node
/// (a fan is a single `Channel`, not separate rpm/duty entries).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FailsafeKind {
    Temp,
    Channel,
}

/// A present channel/temp currently serving failsafe values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailsafeRef {
    pub device_uid: DeviceUID,
    pub name: String,
    pub kind: FailsafeKind,
    /// Why the node entered failsafe, matching the transition log line (for
    /// example "stale readings" or "file unreadable"). Fixed at entry, so a
    /// latched node never produces reason-only transitions.
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HealthState {
    Detected,
    Resolved,
}

/// SSE delta broadcast when a missing reference appears or resolves.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SourceDelta {
    #[serde(flatten)]
    pub reference: SourceRef,
    pub state: HealthState,
}

/// SSE delta broadcast when a channel/temp enters or leaves failsafe.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FailsafeDelta {
    #[serde(flatten)]
    pub reference: FailsafeRef,
    pub state: HealthState,
}

/// One tick's device-health transitions, folded into the existing status SSE
/// stream as a named event so it does not open another connection. Batched per
/// subject per tick so a burst (a whole device disconnecting) is one broadcast
/// and cannot overflow the broadcast buffer.
#[derive(Debug, Clone)]
pub enum HealthEvent {
    Missing(Vec<SourceDelta>),
    StaleSource(Vec<SourceDelta>),
    Failsafe(Vec<FailsafeDelta>),
}

/// Full current health snapshot returned by `GET /devices/health`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceHealthDto {
    pub failsafe: Vec<FailsafeRef>,
    pub missing: Vec<SourceRef>,
    pub stale_source: Vec<SourceRef>,
}

struct LcdRef {
    device_uid: DeviceUID,
    device_name: String,
    channel_name: String,
    source: TempSource,
}

/// Tracks which device references and channels are currently unhealthy, diffs
/// the set each tick, and broadcasts transitions. Runtime-derived state only,
/// never persisted.
pub struct DeviceHealthController {
    all_devices: AllDevices,
    config: Rc<Config>,
    repos: Repos,
    overrides: Rc<OverridesController>,
    handle: RefCell<Option<DeviceHealthHandle>>,
    missing: RefCell<Vec<SourceRef>>,
    stale_source: RefCell<Vec<SourceRef>>,
    failsafe: RefCell<Vec<FailsafeRef>>,
    /// Temp-source references extracted from config. Re-extracted only when the
    /// config generation moves, so unchanged ticks parse no config at all.
    candidates: RefCell<Vec<SourceRef>>,
    config_generation_seen: Cell<Option<u64>>,
}

impl DeviceHealthController {
    pub fn new(
        all_devices: AllDevices,
        config: Rc<Config>,
        repos: Repos,
        overrides: Rc<OverridesController>,
    ) -> Self {
        Self {
            all_devices,
            config,
            repos,
            overrides,
            handle: RefCell::new(None),
            missing: RefCell::new(Vec::new()),
            stale_source: RefCell::new(Vec::new()),
            failsafe: RefCell::new(Vec::new()),
            candidates: RefCell::new(Vec::new()),
            config_generation_seen: Cell::new(None),
        }
    }

    pub fn set_handle(&self, handle: DeviceHealthHandle) {
        self.handle.replace(Some(handle));
    }

    pub fn get_all(&self) -> DeviceHealthDto {
        DeviceHealthDto {
            failsafe: self.failsafe.borrow().clone(),
            missing: self.missing.borrow().clone(),
            stale_source: self.stale_source.borrow().clone(),
        }
    }

    /// Recomputes the missing-reference and failsafe sets and broadcasts any
    /// transitions. Called once per main-loop tick after snapshots are taken.
    pub async fn process(&self) {
        self.refresh_candidates_on_config_change().await;
        let current_missing = self.scan_missing();
        self.diff_and_broadcast_missing(current_missing);
        let current_failsafe = self.scan_failsafe();
        let current_stale = self.scan_stale_sources(&current_failsafe);
        self.diff_and_broadcast_failsafe(current_failsafe);
        self.diff_and_broadcast_stale_sources(current_stale);
    }

    /// Re-extracts the config temp-source references, but only when the config
    /// generation has moved. The reference set changes exclusively through
    /// config mutations (hardware loss is the failsafe subject's concern), so
    /// unchanged ticks skip all config parsing.
    async fn refresh_candidates_on_config_change(&self) {
        let generation = self.config.generation();
        if self.config_generation_seen.get() == Some(generation) {
            return;
        }
        let mut candidates = Vec::new();
        self.collect_custom_sensor_candidates(&mut candidates);
        self.collect_profile_candidates(&mut candidates).await;
        self.collect_lcd_candidates(&mut candidates);
        for candidate in &mut candidates {
            candidate.source_device_name = self.source_device_name(&candidate.source.device_uid);
        }
        self.candidates.replace(candidates);
        self.config_generation_seen.set(Some(generation));
    }

    /// Resolves the source device's display name: user override first, then
    /// live devices, then the config `devices` list, which retains devices
    /// no longer detected.
    fn source_device_name(&self, device_uid: &DeviceUID) -> Option<String> {
        if let Some(name) = self.overrides.device_name_override(device_uid) {
            return Some(name);
        }
        if let Some(device) = self.all_devices.get(device_uid) {
            return Some(device.borrow().name.clone());
        }
        self.config.device_name(device_uid)
    }

    /// The custom sensors virtual device UID, when registered.
    fn custom_sensors_device_uid(&self) -> Option<DeviceUID> {
        self.all_devices
            .iter()
            .find_map(|(device_uid, device_lock)| {
                (device_lock.borrow().d_type == DeviceType::CustomSensors)
                    .then(|| device_uid.clone())
            })
    }

    fn scan_failsafe(&self) -> Vec<FailsafeRef> {
        let mut out = Vec::new();
        for repo in self.repos.iter() {
            out.extend(repo.failsafing());
        }
        out
    }

    fn scan_missing(&self) -> Vec<SourceRef> {
        let present = self.build_present_temps();
        Self::filter_missing(&present, &self.candidates.borrow())
    }

    /// References whose target temp is currently failsafed. Disjoint from
    /// `missing` by construction: failsafed temps keep reporting (failsafe)
    /// values, so they are present.
    fn scan_stale_sources(&self, failsafe: &[FailsafeRef]) -> Vec<SourceRef> {
        Self::filter_stale_sources(failsafe, &self.candidates.borrow())
    }

    /// A device with no current status contributes nothing, so its references
    /// read as missing.
    fn build_present_temps(&self) -> HashMap<DeviceUID, HashSet<TempName>> {
        let mut present = HashMap::with_capacity(self.all_devices.len());
        for (device_uid, device_lock) in self.all_devices.iter() {
            let Some(status) = device_lock.borrow().status_current() else {
                continue;
            };
            let mut temps = HashSet::with_capacity(status.temps.len());
            for temp in &status.temps {
                temps.insert(temp.name.clone());
            }
            present.insert(device_uid.clone(), temps);
        }
        present
    }

    fn collect_custom_sensor_candidates(&self, candidates: &mut Vec<SourceRef>) {
        let Ok(sensors) = self.config.get_custom_sensors() else {
            return;
        };
        let first_new = candidates.len();
        Self::custom_sensor_candidates(&sensors, candidates);
        // Display-name resolution only: entity_uid stays the raw sensor id,
        // which clients use for matching and routing.
        let Some(cs_device_uid) = self.custom_sensors_device_uid() else {
            return;
        };
        for candidate in &mut candidates[first_new..] {
            if let Some(label) = self
                .overrides
                .channel_label_override(&cs_device_uid, &candidate.entity_uid)
            {
                candidate.entity_name = label;
            }
        }
    }

    async fn collect_profile_candidates(&self, candidates: &mut Vec<SourceRef>) {
        let Ok(profiles) = self.config.get_profiles().await else {
            return;
        };
        Self::profile_candidates(&profiles, candidates);
    }

    fn collect_lcd_candidates(&self, candidates: &mut Vec<SourceRef>) {
        let refs = self.lcd_refs();
        Self::lcd_candidates(&refs, candidates);
    }

    fn lcd_refs(&self) -> Vec<LcdRef> {
        let mut refs = Vec::new();
        for (device_uid, device_lock) in self.all_devices.iter() {
            let Ok(settings) = self.config.get_device_settings(device_uid) else {
                continue;
            };
            for setting in settings {
                let SettingKind::Lcd { lcd } = &setting.kind else {
                    continue;
                };
                let Some(source) = lcd.temp_source() else {
                    continue;
                };
                refs.push(LcdRef {
                    device_uid: device_uid.clone(),
                    device_name: self.overrides.resolve_device_name(
                        device_uid,
                        None,
                        &device_lock.borrow().name,
                    ),
                    channel_name: setting.channel_name.clone(),
                    source: source.clone(),
                });
            }
        }
        refs
    }

    fn custom_sensor_candidates(sensors: &[CustomSensor], candidates: &mut Vec<SourceRef>) {
        for sensor in sensors {
            for source_data in sensor.sources() {
                candidates.push(SourceRef {
                    entity_type: HealthEntityType::CustomSensor,
                    entity_uid: sensor.id.clone(),
                    entity_name: sensor.id.clone(),
                    channel_name: None,
                    source: source_data.temp_source.clone(),
                    source_device_name: None,
                });
            }
        }
    }

    fn profile_candidates(profiles: &[Profile], candidates: &mut Vec<SourceRef>) {
        for profile in profiles {
            let Some(source) = profile.temp_source() else {
                continue;
            };
            candidates.push(SourceRef {
                entity_type: HealthEntityType::Profile,
                entity_uid: profile.uid.clone(),
                entity_name: profile.name.clone(),
                channel_name: None,
                source: source.clone(),
                source_device_name: None,
            });
        }
    }

    fn lcd_candidates(refs: &[LcdRef], candidates: &mut Vec<SourceRef>) {
        for lcd in refs {
            candidates.push(SourceRef {
                entity_type: HealthEntityType::Lcd,
                entity_uid: lcd.device_uid.clone(),
                entity_name: lcd.device_name.clone(),
                channel_name: Some(lcd.channel_name.clone()),
                source: lcd.source.clone(),
                source_device_name: None,
            });
        }
    }

    fn filter_missing(
        present: &HashMap<DeviceUID, HashSet<TempName>>,
        candidates: &[SourceRef],
    ) -> Vec<SourceRef> {
        candidates
            .iter()
            .filter(|candidate| Self::is_missing(present, &candidate.source))
            .cloned()
            .collect()
    }

    fn is_missing(present: &HashMap<DeviceUID, HashSet<TempName>>, source: &TempSource) -> bool {
        present
            .get(&source.device_uid)
            .is_none_or(|temps| temps.contains(&source.temp_name).not())
    }

    fn filter_stale_sources(failsafe: &[FailsafeRef], candidates: &[SourceRef]) -> Vec<SourceRef> {
        if failsafe.is_empty() {
            return Vec::new();
        }
        let failsafed_temps: HashSet<(&str, &str)> = failsafe
            .iter()
            .filter(|reference| reference.kind == FailsafeKind::Temp)
            .map(|reference| (reference.device_uid.as_str(), reference.name.as_str()))
            .collect();
        candidates
            .iter()
            .filter(|candidate| {
                failsafed_temps.contains(&(
                    candidate.source.device_uid.as_str(),
                    candidate.source.temp_name.as_str(),
                ))
            })
            .cloned()
            .collect()
    }

    fn diff_and_broadcast_missing(&self, current: Vec<SourceRef>) {
        let (added, removed) = Self::diff_added_removed(&self.missing.borrow(), &current);
        self.missing.replace(current);
        let handle_ref = self.handle.borrow();
        let Some(handle) = handle_ref.as_ref() else {
            return;
        };
        let deltas = Self::delta_batch(added, removed, |reference, state| SourceDelta {
            reference,
            state,
        });
        if deltas.is_empty() {
            return;
        }
        handle.broadcast(HealthEvent::Missing(deltas));
    }

    fn diff_and_broadcast_stale_sources(&self, current: Vec<SourceRef>) {
        let (added, removed) = Self::diff_added_removed(&self.stale_source.borrow(), &current);
        self.stale_source.replace(current);
        let handle_ref = self.handle.borrow();
        let Some(handle) = handle_ref.as_ref() else {
            return;
        };
        let deltas = Self::delta_batch(added, removed, |reference, state| SourceDelta {
            reference,
            state,
        });
        if deltas.is_empty() {
            return;
        }
        handle.broadcast(HealthEvent::StaleSource(deltas));
    }

    fn diff_and_broadcast_failsafe(&self, current: Vec<FailsafeRef>) {
        let (added, removed) = Self::diff_added_removed(&self.failsafe.borrow(), &current);
        self.failsafe.replace(current);
        let handle_ref = self.handle.borrow();
        let Some(handle) = handle_ref.as_ref() else {
            return;
        };
        let deltas = Self::delta_batch(added, removed, |reference, state| FailsafeDelta {
            reference,
            state,
        });
        if deltas.is_empty() {
            return;
        }
        handle.broadcast(HealthEvent::Failsafe(deltas));
    }

    fn delta_batch<T, D>(
        added: Vec<T>,
        removed: Vec<T>,
        make: impl Fn(T, HealthState) -> D,
    ) -> Vec<D> {
        let mut deltas = Vec::with_capacity(added.len() + removed.len());
        for reference in added {
            deltas.push(make(reference, HealthState::Detected));
        }
        for reference in removed {
            deltas.push(make(reference, HealthState::Resolved));
        }
        deltas
    }

    fn diff_added_removed<T: Clone + PartialEq>(previous: &[T], current: &[T]) -> (Vec<T>, Vec<T>) {
        let added = current
            .iter()
            .filter(|item| previous.contains(item).not())
            .cloned()
            .collect();
        let removed = previous
            .iter()
            .filter(|item| current.contains(item).not())
            .cloned()
            .collect();
        (added, removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::{
        CustomSensorKind, CustomSensorMixFunctionType, CustomTempSourceData, FunctionUID,
        ProfileKind,
    };

    #[test]
    fn source_device_name_prefers_override_then_live() {
        // Goal: the health display name walks the layers override > live
        // device name > config list, and yields None for unknown devices.
        crate::rt::test_runtime(async {
            use crate::device::{Device, DeviceInfo};
            use crate::repositories::repository::Repositories;
            use std::cell::RefCell;
            use std::rc::Rc;

            let device = Rc::new(RefCell::new(Device::new(
                "nct6798".to_string(),
                DeviceType::Hwmon,
                0,
                None,
                DeviceInfo::default(),
                None,
                1.0,
            )));
            let device_uid = device.borrow().uid.clone();
            let mut devices = HashMap::new();
            devices.insert(device_uid.clone(), device);
            let all_devices = Rc::new(devices);
            let config = Rc::new(crate::config::Config::init_default_config().unwrap());
            config.create_device_list(&all_devices);

            let tmp = tempfile::tempdir().unwrap();
            let overrides = Rc::new(
                crate::overrides::OverridesController::init_from(tmp.path().join("overrides.toml"))
                    .await,
            );
            let controller = DeviceHealthController::new(
                Rc::clone(&all_devices),
                config,
                Rc::new(Repositories::default()),
                Rc::clone(&overrides),
            );

            // Live layer without an override.
            assert_eq!(
                controller.source_device_name(&device_uid),
                Some("nct6798".to_string())
            );
            // Override layer wins over live.
            overrides
                .set_device_name(&device_uid, "hint", Some("Motherboard"))
                .await
                .unwrap();
            assert_eq!(
                controller.source_device_name(&device_uid),
                Some("Motherboard".to_string())
            );
            // Unknown everywhere yields None.
            assert_eq!(controller.source_device_name(&"unknown".to_string()), None);
        });
    }

    fn present_with(device_uid: &str, temps: &[&str]) -> HashMap<DeviceUID, HashSet<TempName>> {
        let set = temps.iter().map(|t| (*t).to_string()).collect();
        HashMap::from([(device_uid.to_string(), set)])
    }

    fn source(device_uid: &str, temp_name: &str) -> TempSource {
        TempSource {
            device_uid: device_uid.to_string(),
            temp_name: temp_name.to_string(),
        }
    }

    fn source_ref(device_uid: &str, temp_name: &str) -> SourceRef {
        SourceRef {
            entity_type: HealthEntityType::Profile,
            entity_uid: "p1".to_string(),
            entity_name: "Profile 1".to_string(),
            channel_name: None,
            source: source(device_uid, temp_name),
            source_device_name: None,
        }
    }

    fn failsafe_ref(device_uid: &str, name: &str, kind: FailsafeKind) -> FailsafeRef {
        FailsafeRef {
            device_uid: device_uid.to_string(),
            name: name.to_string(),
            kind,
            reason: "stale readings".to_string(),
        }
    }

    #[test]
    fn filter_stale_sources_keeps_refs_targeting_failsafed_temps() {
        // Goal: a reference whose source temp is in the failsafe set is stale;
        // references to healthy temps are not.
        let failsafe = vec![failsafe_ref("dev1", "temp1", FailsafeKind::Temp)];
        let candidates = vec![source_ref("dev1", "temp1"), source_ref("dev1", "temp2")];
        let stale = DeviceHealthController::filter_stale_sources(&failsafe, &candidates);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].source, source("dev1", "temp1"));
    }

    #[test]
    fn filter_stale_sources_ignores_failsafed_control_channels() {
        // Goal: only temp-kind failsafe entries make a source stale; a control
        // channel sharing the temp's name must not match.
        let failsafe = vec![failsafe_ref("dev1", "temp1", FailsafeKind::Channel)];
        let candidates = vec![source_ref("dev1", "temp1")];
        assert!(DeviceHealthController::filter_stale_sources(&failsafe, &candidates).is_empty());
    }

    #[test]
    fn filter_stale_sources_empty_when_no_failsafe() {
        // Goal: the common healthy tick short-circuits to an empty set.
        let candidates = vec![source_ref("dev1", "temp1")];
        assert!(DeviceHealthController::filter_stale_sources(&[], &candidates).is_empty());
    }

    #[test]
    fn is_missing_false_when_device_and_temp_present() {
        // Goal: a reference whose device reports the named temp is not missing.
        let present = present_with("dev1", &["temp1", "temp2"]);
        assert!(DeviceHealthController::is_missing(&present, &source("dev1", "temp1")).not());
    }

    #[test]
    fn is_missing_true_when_device_absent() {
        // Goal: a reference to a device not in the present set is missing.
        let present = present_with("dev1", &["temp1"]);
        assert!(DeviceHealthController::is_missing(
            &present,
            &source("devX", "temp1")
        ));
    }

    #[test]
    fn is_missing_true_when_temp_absent_on_present_device() {
        // Goal: a present device that no longer reports the named temp is missing.
        let present = present_with("dev1", &["temp1"]);
        assert!(DeviceHealthController::is_missing(
            &present,
            &source("dev1", "tempGone")
        ));
    }

    #[test]
    fn is_missing_true_when_device_reports_no_temps() {
        // Goal: a device present but reporting an empty temp set is missing.
        let present = present_with("dev1", &[]);
        assert!(DeviceHealthController::is_missing(
            &present,
            &source("dev1", "temp1")
        ));
    }

    #[test]
    fn filter_missing_keeps_only_unresolved() {
        // Goal: filtering returns exactly the candidates whose source is absent,
        // preserving the others' identity.
        let present = present_with("dev1", &["temp1"]);
        let candidates = vec![
            source_ref("dev1", "temp1"),    // present -> dropped
            source_ref("dev1", "tempGone"), // missing -> kept
            source_ref("devX", "temp1"),    // device gone -> kept
        ];
        let result = DeviceHealthController::filter_missing(&present, &candidates);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].source, source("dev1", "tempGone"));
        assert_eq!(result[1].source, source("devX", "temp1"));
    }

    #[test]
    fn filter_missing_empty_when_all_present() {
        // Goal: when every candidate resolves, the result is empty.
        let present = present_with("dev1", &["temp1", "temp2"]);
        let candidates = vec![source_ref("dev1", "temp1"), source_ref("dev1", "temp2")];
        assert!(DeviceHealthController::filter_missing(&present, &candidates).is_empty());
    }

    fn graph_profile(uid: &str, name: &str, temp_source: Option<TempSource>) -> Profile {
        Profile {
            uid: uid.to_string(),
            name: name.to_string(),
            function_uid: FunctionUID::from("0"),
            kind: ProfileKind::Graph {
                speed_profile: None,
                temp_source,
                temp_min: None,
                temp_max: None,
            },
        }
    }

    #[test]
    fn profile_candidates_includes_graph_with_source() {
        // Goal: a Graph profile with a temp source yields one candidate carrying
        // that profile's identity and source.
        let profiles = vec![graph_profile(
            "p1",
            "CPU Fan",
            Some(source("dev1", "temp1")),
        )];
        let mut candidates = Vec::new();
        DeviceHealthController::profile_candidates(&profiles, &mut candidates);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].entity_type, HealthEntityType::Profile);
        assert_eq!(candidates[0].entity_uid, "p1");
        assert_eq!(candidates[0].entity_name, "CPU Fan");
        assert_eq!(candidates[0].source, source("dev1", "temp1"));
    }

    #[test]
    fn profile_candidates_skips_graph_without_source_and_other_kinds() {
        // Goal: a Graph profile with no source and a non-Graph profile both
        // contribute no candidates (only Graph profiles carry a temp source).
        let profiles = vec![
            graph_profile("p1", "No Source", None),
            Profile::default(), // Default kind, no temp source
        ];
        let mut candidates = Vec::new();
        DeviceHealthController::profile_candidates(&profiles, &mut candidates);
        assert!(candidates.is_empty());
    }

    #[test]
    fn diff_added_removed_reports_new_and_gone() {
        // Goal: items only in current are "added", items only in previous are
        // "removed", and shared items appear in neither.
        let previous = vec![1, 2, 3];
        let current = vec![2, 3, 4];
        let (added, removed) = DeviceHealthController::diff_added_removed(&previous, &current);
        assert_eq!(added, vec![4]);
        assert_eq!(removed, vec![1]);
    }

    #[test]
    fn diff_added_removed_empty_when_unchanged() {
        // Goal: identical sets produce no transitions.
        let set = vec!["a".to_string(), "b".to_string()];
        let (added, removed) = DeviceHealthController::diff_added_removed(&set, &set);
        assert!(added.is_empty());
        assert!(removed.is_empty());
    }

    #[test]
    fn custom_sensor_candidates_yield_one_per_source() {
        // Goal: every temp source of a Mix sensor becomes one candidate carrying
        // the sensor's id as both uid and display name; File sensors have no
        // sources and contribute nothing.
        let sensors = vec![
            CustomSensor {
                id: "sensor1".to_string(),
                kind: CustomSensorKind::Mix {
                    mix_function: CustomSensorMixFunctionType::Avg,
                    sources: vec![
                        CustomTempSourceData {
                            temp_source: source("dev1", "temp1"),
                            weight: 1,
                        },
                        CustomTempSourceData {
                            temp_source: source("dev2", "temp2"),
                            weight: 1,
                        },
                    ],
                },
                children: Vec::new(),
                parents: Vec::new(),
            },
            CustomSensor {
                id: "file1".to_string(),
                kind: CustomSensorKind::File {
                    file_path: "/tmp/x".into(),
                },
                children: Vec::new(),
                parents: Vec::new(),
            },
        ];
        let mut candidates = Vec::new();
        DeviceHealthController::custom_sensor_candidates(&sensors, &mut candidates);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].entity_type, HealthEntityType::CustomSensor);
        assert_eq!(candidates[0].entity_uid, "sensor1");
        assert_eq!(candidates[0].entity_name, "sensor1");
        assert_eq!(candidates[0].channel_name, None);
        assert_eq!(candidates[0].source, source("dev1", "temp1"));
        assert_eq!(candidates[1].source, source("dev2", "temp2"));
    }

    #[test]
    fn lcd_candidates_carry_device_and_channel_identity() {
        // Goal: an LCD reference maps to a candidate keyed by the owning device
        // with its channel name set, so the UI can deep-link to the LCD editor.
        let refs = vec![LcdRef {
            device_uid: "dev1".to_string(),
            device_name: "Kraken".to_string(),
            channel_name: "lcd".to_string(),
            source: source("dev2", "temp1"),
        }];
        let mut candidates = Vec::new();
        DeviceHealthController::lcd_candidates(&refs, &mut candidates);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].entity_type, HealthEntityType::Lcd);
        assert_eq!(candidates[0].entity_uid, "dev1");
        assert_eq!(candidates[0].entity_name, "Kraken");
        assert_eq!(candidates[0].channel_name, Some("lcd".to_string()));
        assert_eq!(candidates[0].source, source("dev2", "temp1"));
    }

    #[test]
    fn delta_batch_marks_added_detected_and_removed_resolved() {
        // Goal: one batch carries every transition of the tick: added items become
        // Detected deltas, removed items Resolved deltas, in that order.
        let deltas = DeviceHealthController::delta_batch(
            vec![source_ref("dev1", "tempNew")],
            vec![source_ref("dev1", "tempGone")],
            |reference, state| SourceDelta { reference, state },
        );
        assert_eq!(deltas.len(), 2);
        assert_eq!(deltas[0].reference.source, source("dev1", "tempNew"));
        assert_eq!(deltas[0].state, HealthState::Detected);
        assert_eq!(deltas[1].reference.source, source("dev1", "tempGone"));
        assert_eq!(deltas[1].state, HealthState::Resolved);
    }

    #[test]
    fn failsafe_delta_batch_serializes_flat_for_sse() {
        // Goal: guard the SSE wire shape the UI parses: a JSON array of refs with
        // the state flattened alongside the reference fields.
        let deltas = vec![FailsafeDelta {
            reference: FailsafeRef {
                device_uid: "dev1".to_string(),
                name: "fan1".to_string(),
                kind: FailsafeKind::Channel,
                reason: "stale readings".to_string(),
            },
            state: HealthState::Detected,
        }];
        let json = serde_json::to_value(&deltas).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{
                "device_uid": "dev1",
                "name": "fan1",
                "kind": "Channel",
                "reason": "stale readings",
                "state": "Detected",
            }])
        );
    }

    #[test]
    fn missing_deltas_serialize_flat_for_sse() {
        // Goal: guard the SSE wire shape the UI parses: a JSON array with the
        // entity fields flattened, the temp source nested, `channel_name`
        // absent when None, and the state alongside.
        let deltas = vec![SourceDelta {
            reference: source_ref("dev1", "tempGone"),
            state: HealthState::Resolved,
        }];
        let json = serde_json::to_value(&deltas).unwrap();
        assert_eq!(
            json,
            serde_json::json!([{
                "entity_type": "Profile",
                "entity_uid": "p1",
                "entity_name": "Profile 1",
                "source": { "device_uid": "dev1", "temp_name": "tempGone" },
                "state": "Resolved",
            }])
        );
    }

    #[test]
    fn device_health_dto_serializes_snapshot_shape() {
        // Goal: guard the GET /devices/health shape: top-level failsafe,
        // missing, and stale arrays, empty lists serialized as [] not omitted.
        let dto = DeviceHealthDto {
            failsafe: vec![FailsafeRef {
                device_uid: "dev1".to_string(),
                name: "temp1".to_string(),
                kind: FailsafeKind::Temp,
                reason: "stale readings".to_string(),
            }],
            missing: Vec::new(),
            stale_source: Vec::new(),
        };
        let json = serde_json::to_value(&dto).unwrap();
        assert_eq!(
            json,
            serde_json::json!({
                "failsafe": [{
                    "device_uid": "dev1",
                    "name": "temp1",
                    "kind": "Temp",
                    "reason": "stale readings",
                }],
                "missing": [],
                "stale_source": [],
            })
        );
    }
}
