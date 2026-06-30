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
//! - `failsafe`: a present channel/temp currently serving failsafe values. Added
//!   in a later phase; the snapshot already carries an (empty) list so the wire
//!   contract is stable.

use crate::api::actor::DeviceHealthHandle;
use crate::config::Config;
use crate::device::{DeviceUID, TempName, UID};
use crate::setting::{CustomSensor, Profile, SettingKind, TempSource};
use crate::{AllDevices, Repos};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
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
/// editor; `missing` is the unresolved {device_uid, temp_name}.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MissingRef {
    pub entity_type: HealthEntityType,
    /// Profile uid, Custom Sensor id, or the owning device uid for an LCD setting.
    pub entity_uid: UID,
    /// Display name: profile/device name, or the sensor id.
    pub entity_name: String,
    /// The channel the setting is on. Only set for LCD references.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub channel_name: Option<String>,
    /// The referenced temp source that cannot be resolved.
    pub missing: TempSource,
}

/// Whether a failsafing node is a temp or a control channel. One entry per node
/// (a fan is a single `Channel`, not separate rpm/duty entries).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum FailsafeKind {
    Temp,
    Channel,
}

/// A present channel/temp currently serving failsafe values. Produced by a later
/// phase; defined now so the snapshot DTO and SSE event shapes stay stable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FailsafeRef {
    pub device_uid: DeviceUID,
    pub name: String,
    pub kind: FailsafeKind,
}

/// Whether a tracked condition just appeared or just resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum HealthState {
    Detected,
    Resolved,
}

/// SSE delta broadcast when a missing reference appears or resolves.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MissingDelta {
    #[serde(flatten)]
    pub reference: MissingRef,
    pub state: HealthState,
}

/// SSE delta broadcast when a channel/temp enters or leaves failsafe.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FailsafeDelta {
    #[serde(flatten)]
    pub reference: FailsafeRef,
    pub state: HealthState,
}

/// A device-health transition, folded into the existing status SSE stream as a
/// named event so it does not open another connection. Gains a `Failsafe`
/// variant in the failsafe phase.
#[derive(Debug, Clone)]
pub enum HealthEvent {
    Missing(MissingDelta),
    Failsafe(FailsafeDelta),
}

/// Full current health snapshot returned by `GET /devices/health`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceHealthDto {
    pub failsafe: Vec<FailsafeRef>,
    pub missing: Vec<MissingRef>,
}

/// One LCD temp-source reference, extracted from a device's settings.
struct LcdRef {
    device_uid: DeviceUID,
    device_name: String,
    channel_name: String,
    source: TempSource,
}

/// Tracks which device references and channels are currently unhealthy, diffs
/// the set each tick, and broadcasts transitions. Mirrors `AlertController`:
/// runtime-derived state only, never persisted.
pub struct DeviceHealthController {
    all_devices: AllDevices,
    config: Rc<Config>,
    repos: Repos,
    handle: RefCell<Option<DeviceHealthHandle>>,
    missing: RefCell<Vec<MissingRef>>,
    failsafe: RefCell<Vec<FailsafeRef>>,
}

impl DeviceHealthController {
    pub fn new(all_devices: AllDevices, config: Rc<Config>, repos: Repos) -> Self {
        Self {
            all_devices,
            config,
            repos,
            handle: RefCell::new(None),
            missing: RefCell::new(Vec::new()),
            failsafe: RefCell::new(Vec::new()),
        }
    }

    /// Sets the handle used to broadcast transitions over SSE.
    pub fn set_handle(&self, handle: DeviceHealthHandle) {
        self.handle.replace(Some(handle));
    }

    /// Current health snapshot for the REST endpoint.
    pub fn get_all(&self) -> DeviceHealthDto {
        DeviceHealthDto {
            failsafe: self.failsafe.borrow().clone(),
            missing: self.missing.borrow().clone(),
        }
    }

    /// Recomputes the missing-reference and failsafe sets and broadcasts any
    /// transitions. Called once per main-loop tick after snapshots are taken.
    pub async fn process(&self) {
        let current_missing = self.scan_missing().await;
        self.diff_and_broadcast_missing(current_missing);
        let current_failsafe = self.scan_failsafe();
        self.diff_and_broadcast_failsafe(current_failsafe);
    }

    /// Collects the channels/temps every repository is currently failsafing.
    fn scan_failsafe(&self) -> Vec<FailsafeRef> {
        let mut out = Vec::new();
        for repo in self.repos.iter() {
            out.extend(repo.failsafing());
        }
        out
    }

    /// Gathers every temp-source reference and keeps only the unresolved ones.
    async fn scan_missing(&self) -> Vec<MissingRef> {
        let present = self.build_present_temps();
        let mut candidates = Vec::new();
        self.collect_custom_sensor_candidates(&mut candidates);
        self.collect_profile_candidates(&mut candidates).await;
        self.collect_lcd_candidates(&mut candidates);
        Self::filter_missing(&present, candidates)
    }

    /// Builds the set of currently-present temps, keyed by device uid. A device
    /// with no current status contributes nothing, so its references read as
    /// missing.
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

    fn collect_custom_sensor_candidates(&self, candidates: &mut Vec<MissingRef>) {
        let Ok(sensors) = self.config.get_custom_sensors() else {
            return;
        };
        Self::custom_sensor_candidates(&sensors, candidates);
    }

    async fn collect_profile_candidates(&self, candidates: &mut Vec<MissingRef>) {
        let Ok(profiles) = self.config.get_profiles().await else {
            return;
        };
        Self::profile_candidates(&profiles, candidates);
    }

    fn collect_lcd_candidates(&self, candidates: &mut Vec<MissingRef>) {
        let refs = self.lcd_refs();
        Self::lcd_candidates(&refs, candidates);
    }

    /// Extracts the LCD temp-source references from every device's settings.
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
                    device_name: device_lock.borrow().name.clone(),
                    channel_name: setting.channel_name.clone(),
                    source: source.clone(),
                });
            }
        }
        refs
    }

    fn custom_sensor_candidates(sensors: &[CustomSensor], candidates: &mut Vec<MissingRef>) {
        for sensor in sensors {
            for source_data in sensor.sources() {
                candidates.push(MissingRef {
                    entity_type: HealthEntityType::CustomSensor,
                    entity_uid: sensor.id.clone(),
                    entity_name: sensor.id.clone(),
                    channel_name: None,
                    missing: source_data.temp_source.clone(),
                });
            }
        }
    }

    fn profile_candidates(profiles: &[Profile], candidates: &mut Vec<MissingRef>) {
        for profile in profiles {
            let Some(source) = profile.temp_source() else {
                continue;
            };
            candidates.push(MissingRef {
                entity_type: HealthEntityType::Profile,
                entity_uid: profile.uid.clone(),
                entity_name: profile.name.clone(),
                channel_name: None,
                missing: source.clone(),
            });
        }
    }

    fn lcd_candidates(refs: &[LcdRef], candidates: &mut Vec<MissingRef>) {
        for lcd in refs {
            candidates.push(MissingRef {
                entity_type: HealthEntityType::Lcd,
                entity_uid: lcd.device_uid.clone(),
                entity_name: lcd.device_name.clone(),
                channel_name: Some(lcd.channel_name.clone()),
                missing: lcd.source.clone(),
            });
        }
    }

    /// Keeps only candidates whose temp source is not present. Pure leaf.
    fn filter_missing(
        present: &HashMap<DeviceUID, HashSet<TempName>>,
        candidates: Vec<MissingRef>,
    ) -> Vec<MissingRef> {
        candidates
            .into_iter()
            .filter(|candidate| Self::is_missing(present, &candidate.missing))
            .collect()
    }

    /// A source is missing when its device is absent, or present but no longer
    /// reporting that temp. Pure leaf.
    fn is_missing(present: &HashMap<DeviceUID, HashSet<TempName>>, source: &TempSource) -> bool {
        present
            .get(&source.device_uid)
            .is_none_or(|temps| temps.contains(&source.temp_name).not())
    }

    /// Replaces the stored missing set with `current` and broadcasts one delta
    /// per appeared / resolved reference.
    fn diff_and_broadcast_missing(&self, current: Vec<MissingRef>) {
        let previous = self.missing.replace(current.clone());
        let (added, removed) = Self::diff_added_removed(&previous, &current);
        let handle_ref = self.handle.borrow();
        let Some(handle) = handle_ref.as_ref() else {
            return;
        };
        for reference in added {
            handle.broadcast(HealthEvent::Missing(MissingDelta {
                reference,
                state: HealthState::Detected,
            }));
        }
        for reference in removed {
            handle.broadcast(HealthEvent::Missing(MissingDelta {
                reference,
                state: HealthState::Resolved,
            }));
        }
    }

    /// Replaces the stored failsafe set with `current` and broadcasts one delta
    /// per channel/temp that entered or left failsafe.
    fn diff_and_broadcast_failsafe(&self, current: Vec<FailsafeRef>) {
        let previous = self.failsafe.replace(current.clone());
        let (added, removed) = Self::diff_added_removed(&previous, &current);
        let handle_ref = self.handle.borrow();
        let Some(handle) = handle_ref.as_ref() else {
            return;
        };
        for reference in added {
            handle.broadcast(HealthEvent::Failsafe(FailsafeDelta {
                reference,
                state: HealthState::Detected,
            }));
        }
        for reference in removed {
            handle.broadcast(HealthEvent::Failsafe(FailsafeDelta {
                reference,
                state: HealthState::Resolved,
            }));
        }
    }

    /// Returns `(added, removed)`: items in `current` absent from `previous`,
    /// and items in `previous` absent from `current`. Pure leaf.
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
    use crate::setting::{FunctionUID, ProfileKind};

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

    fn missing_ref(device_uid: &str, temp_name: &str) -> MissingRef {
        MissingRef {
            entity_type: HealthEntityType::Profile,
            entity_uid: "p1".to_string(),
            entity_name: "Profile 1".to_string(),
            channel_name: None,
            missing: source(device_uid, temp_name),
        }
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
            missing_ref("dev1", "temp1"),    // present -> dropped
            missing_ref("dev1", "tempGone"), // missing -> kept
            missing_ref("devX", "temp1"),    // device gone -> kept
        ];
        let result = DeviceHealthController::filter_missing(&present, candidates);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].missing, source("dev1", "tempGone"));
        assert_eq!(result[1].missing, source("devX", "temp1"));
    }

    #[test]
    fn filter_missing_empty_when_all_present() {
        // Goal: when every candidate resolves, the result is empty.
        let present = present_with("dev1", &["temp1", "temp2"]);
        let candidates = vec![missing_ref("dev1", "temp1"), missing_ref("dev1", "temp2")];
        assert!(DeviceHealthController::filter_missing(&present, candidates).is_empty());
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
        assert_eq!(candidates[0].missing, source("dev1", "temp1"));
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
}
