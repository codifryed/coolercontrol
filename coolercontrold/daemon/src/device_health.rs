// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
use crate::device::{DeviceType, DeviceUID, Status, UID};
use crate::hardware_support::{ChannelVerdictRef, SystemFinding};
use crate::overrides::OverridesController;
use crate::setting::{CustomSensor, CustomSensorMetric, Profile, SettingKind, TempSource};
use crate::{AllDevices, Repos};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
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
/// editor; `source` is the referenced {`device_uid`, `temp_name`}.
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

/// A device whose driver has stopped answering, so the daemon can neither read from it nor write
/// to it.
///
/// Distinct from `FailsafeRef`, which means the device is alive with stale readings and safe
/// values substituted. Presenting this as that would tell the user their fans are on a safe curve
/// when no curve can be applied.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UnreachableRef {
    pub device_uid: DeviceUID,
    pub device_name: String,
    /// Consecutive operations that timed out before the device was given up on.
    pub consecutive_timeouts: u8,
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

/// SSE delta broadcast when a device stops or resumes answering.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UnreachableDelta {
    #[serde(flatten)]
    pub reference: UnreachableRef,
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
    Unreachable(Vec<UnreachableDelta>),
}

/// Full current health snapshot returned by `GET /devices/health`.
///
/// Two sections with different meanings, deliberately not interleaved.
/// `failsafe`, `missing`, `stale_source` and `firmware_overrides` are
/// **current state**: conditions that can clear on their own.
/// `channel_capabilities` and `system_findings` are **permanent**: facts about
/// what this hardware can do, which will not change while the machine is
/// running. Presenting a permanent capability as a fault would imply the user
/// should wait for it to resolve; presenting a fault as a capability would
/// imply the opposite.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceHealthDto {
    pub failsafe: Vec<FailsafeRef>,
    /// Current state: devices whose driver has stopped answering entirely.
    pub unreachable: Vec<UnreachableRef>,
    pub missing: Vec<SourceRef>,
    pub stale_source: Vec<SourceRef>,
    /// Current state: firmware reclaimed a channel this daemon controls.
    pub firmware_overrides: Vec<ChannelVerdictRef>,
    /// Permanent: channels that cannot be driven, and why. Controllable
    /// channels are omitted; there is nothing to report about working hardware.
    pub channel_capabilities: Vec<ChannelVerdictRef>,
    /// Permanent: machine-scope facts, such as a detected chip with no driver
    /// bound or a probe blocked by the environment.
    pub system_findings: Vec<SystemFinding>,
}

struct LcdRef {
    device_uid: DeviceUID,
    device_name: String,
    channel_name: String,
    source: TempSource,
}

/// A config reference to watch, with the metric that says where its source lives: among the
/// device's temps, or as that value of a channel. A non-temperature Custom Sensor's
/// `source.temp_name` holds the channel name.
struct Candidate {
    reference: SourceRef,
    metric: CustomSensorMetric,
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
    unreachable: RefCell<Vec<UnreachableRef>>,
    hardware_support: Option<Rc<crate::hardware_support::HardwareSupportController>>,
    /// Source references extracted from config. Re-extracted only when the
    /// config generation moves, so unchanged ticks parse no config at all.
    candidates: RefCell<Vec<Candidate>>,
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
            unreachable: RefCell::new(Vec::new()),
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
            hardware_support: None,
        }
    }

    /// Attaches the hardware-support registry that supplies the permanent
    /// capability section. Absent in tests, which yields empty sections.
    #[must_use]
    pub fn with_hardware_support(
        mut self,
        hardware_support: Rc<crate::hardware_support::HardwareSupportController>,
    ) -> Self {
        self.hardware_support = Some(hardware_support);
        self
    }

    pub fn set_handle(&self, handle: DeviceHealthHandle) {
        self.handle.replace(Some(handle));
    }

    pub fn get_all(&self) -> DeviceHealthDto {
        let (channel_capabilities, firmware_overrides) = self
            .hardware_support
            .as_ref()
            .map_or_else(Default::default, |hs| hs.partitioned_verdicts());
        let system_findings = self
            .hardware_support
            .as_ref()
            .map(|hs| hs.system_findings())
            .unwrap_or_default();
        DeviceHealthDto {
            failsafe: self.failsafe.borrow().clone(),
            unreachable: self.unreachable.borrow().clone(),
            missing: self.missing.borrow().clone(),
            stale_source: self.stale_source.borrow().clone(),
            firmware_overrides,
            channel_capabilities,
            system_findings,
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
        self.diff_and_broadcast_unreachable(self.scan_unreachable());
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
            let reference = &mut candidate.reference;
            reference.source_device_name = self.source_device_name(&reference.source.device_uid);
        }
        self.candidates.replace(candidates);
        self.config_generation_seen.set(Some(generation));
    }

    /// Resolves the source device's display name through the name chain: user override first,
    /// then live devices, then the config `devices` list, which retains devices no longer
    /// detected. `None` when no layer names it, which callers render as unknown.
    fn source_device_name(&self, device_uid: &DeviceUID) -> Option<String> {
        self.overrides.known_device_name(device_uid)
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

    fn scan_unreachable(&self) -> Vec<UnreachableRef> {
        let mut out = Vec::new();
        for repo in self.repos.iter() {
            out.extend(repo.unreachable_devices());
        }
        out
    }

    fn scan_missing(&self) -> Vec<SourceRef> {
        self.candidates
            .borrow()
            .iter()
            .filter(|candidate| self.is_missing(candidate))
            .map(|candidate| candidate.reference.clone())
            .collect()
    }

    /// References whose target temp is currently failsafed. Disjoint from
    /// `missing` by construction: failsafed temps keep reporting (failsafe)
    /// values, so they are present.
    fn scan_stale_sources(&self, failsafe: &[FailsafeRef]) -> Vec<SourceRef> {
        Self::filter_stale_sources(failsafe, &self.candidates.borrow())
    }

    /// A reference is missing when its device is gone or the device's newest status lacks
    /// the source.
    fn is_missing(&self, candidate: &Candidate) -> bool {
        let source = &candidate.reference.source;
        let Some(device_lock) = self.all_devices.get(&source.device_uid) else {
            return true;
        };
        let device = device_lock.borrow();
        Self::is_absent(
            device.status_history.back(),
            candidate.metric,
            &source.temp_name,
        )
    }

    /// Whether `status` lacks the source. Read with the reference's own metric, as the
    /// Custom Sensors repository reads it, so the two cannot disagree. A device with no
    /// status yet has nothing, so its references read as missing.
    fn is_absent(status: Option<&Status>, metric: CustomSensorMetric, name: &str) -> bool {
        status.is_none_or(|status| metric.read(status, name).is_none())
    }

    fn collect_custom_sensor_candidates(&self, candidates: &mut Vec<Candidate>) {
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
            let reference = &mut candidate.reference;
            if let Some(label) = self
                .overrides
                .channel_label_override(&cs_device_uid, &reference.entity_uid)
            {
                reference.entity_name = label;
            }
        }
    }

    async fn collect_profile_candidates(&self, candidates: &mut Vec<Candidate>) {
        let Ok(profiles) = self.config.get_profiles().await else {
            return;
        };
        Self::profile_candidates(&profiles, candidates);
    }

    fn collect_lcd_candidates(&self, candidates: &mut Vec<Candidate>) {
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

    fn custom_sensor_candidates(sensors: &[CustomSensor], candidates: &mut Vec<Candidate>) {
        for sensor in sensors {
            for source_data in sensor.sources() {
                let reference = SourceRef {
                    entity_type: HealthEntityType::CustomSensor,
                    entity_uid: sensor.id.clone(),
                    entity_name: sensor.id.clone(),
                    channel_name: None,
                    source: source_data.temp_source(),
                    source_device_name: None,
                };
                candidates.push(Candidate {
                    reference,
                    metric: sensor.metric,
                });
            }
        }
    }

    fn profile_candidates(profiles: &[Profile], candidates: &mut Vec<Candidate>) {
        for profile in profiles {
            let Some(source) = profile.temp_source() else {
                continue;
            };
            let reference = SourceRef {
                entity_type: HealthEntityType::Profile,
                entity_uid: profile.uid.clone(),
                entity_name: profile.name.clone(),
                channel_name: None,
                source: source.clone(),
                source_device_name: None,
            };
            candidates.push(Candidate {
                reference,
                metric: CustomSensorMetric::Temp,
            });
        }
    }

    fn lcd_candidates(refs: &[LcdRef], candidates: &mut Vec<Candidate>) {
        for lcd in refs {
            let reference = SourceRef {
                entity_type: HealthEntityType::Lcd,
                entity_uid: lcd.device_uid.clone(),
                entity_name: lcd.device_name.clone(),
                channel_name: Some(lcd.channel_name.clone()),
                source: lcd.source.clone(),
                source_device_name: None,
            };
            candidates.push(Candidate {
                reference,
                metric: CustomSensorMetric::Temp,
            });
        }
    }

    /// References whose source is serving failsafe values. Matched by kind as well as name:
    /// a temp and a channel of one device may share a name.
    fn filter_stale_sources(failsafe: &[FailsafeRef], candidates: &[Candidate]) -> Vec<SourceRef> {
        if failsafe.is_empty() {
            return Vec::new();
        }
        let failsafed: HashSet<(bool, &str, &str)> = failsafe
            .iter()
            .map(|reference| {
                (
                    reference.kind == FailsafeKind::Temp,
                    reference.device_uid.as_str(),
                    reference.name.as_str(),
                )
            })
            .collect();
        candidates
            .iter()
            .filter(|candidate| {
                let source = &candidate.reference.source;
                failsafed.contains(&(
                    candidate.metric.is_temp(),
                    source.device_uid.as_str(),
                    source.temp_name.as_str(),
                ))
            })
            .map(|candidate| candidate.reference.clone())
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

    fn diff_and_broadcast_unreachable(&self, current: Vec<UnreachableRef>) {
        let (added, removed) = Self::diff_added_removed(&self.unreachable.borrow(), &current);
        self.unreachable.replace(current);
        let handle_ref = self.handle.borrow();
        let Some(handle) = handle_ref.as_ref() else {
            return;
        };
        let deltas = Self::delta_batch(added, removed, |reference, state| UnreachableDelta {
            reference,
            state,
        });
        if deltas.is_empty() {
            return;
        }
        handle.broadcast(HealthEvent::Unreachable(deltas));
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
    use crate::device::{ChannelStatus, TempStatus};
    use crate::setting::{
        CustomSensorKind, CustomSensorMixFunctionType, FunctionUID, ProfileKind, SensorSource,
    };
    use std::collections::HashMap;

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
            overrides.capture_detected_names(&all_devices, &config);
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

    fn status_with(temps: &[&str]) -> Status {
        Status {
            temps: temps
                .iter()
                .map(|name| TempStatus {
                    name: (*name).to_string(),
                    temp: 40.,
                })
                .collect(),
            channels: vec![ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn candidate(device_uid: &str, temp_name: &str) -> Candidate {
        Candidate {
            reference: source_ref(device_uid, temp_name),
            metric: CustomSensorMetric::Temp,
        }
    }

    fn channel_candidate(device_uid: &str, channel_name: &str) -> Candidate {
        Candidate {
            reference: source_ref(device_uid, channel_name),
            metric: CustomSensorMetric::RPM,
        }
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
        let candidates = vec![candidate("dev1", "temp1"), candidate("dev1", "temp2")];
        let stale = DeviceHealthController::filter_stale_sources(&failsafe, &candidates);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].source, source("dev1", "temp1"));
    }

    #[test]
    fn filter_stale_sources_matches_channel_sources_to_failsafed_channels() {
        // Goal: a channel-metric reference is stale when its channel is failsafed, and
        // not when a temp of the same name is.
        let candidates = vec![channel_candidate("dev1", "fan1")];
        let channel_failsafe = vec![failsafe_ref("dev1", "fan1", FailsafeKind::Channel)];
        let stale = DeviceHealthController::filter_stale_sources(&channel_failsafe, &candidates);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].source, source("dev1", "fan1"));

        let temp_failsafe = vec![failsafe_ref("dev1", "fan1", FailsafeKind::Temp)];
        assert!(
            DeviceHealthController::filter_stale_sources(&temp_failsafe, &candidates).is_empty()
        );
    }

    #[test]
    fn filter_stale_sources_ignores_failsafed_control_channels() {
        // Goal: only temp-kind failsafe entries make a source stale; a control
        // channel sharing the temp's name must not match.
        let failsafe = vec![failsafe_ref("dev1", "temp1", FailsafeKind::Channel)];
        let candidates = vec![candidate("dev1", "temp1")];
        assert!(DeviceHealthController::filter_stale_sources(&failsafe, &candidates).is_empty());
    }

    #[test]
    fn filter_stale_sources_empty_when_no_failsafe() {
        // Goal: the common healthy tick short-circuits to an empty set.
        let candidates = vec![candidate("dev1", "temp1")];
        assert!(DeviceHealthController::filter_stale_sources(&[], &candidates).is_empty());
    }

    #[test]
    fn is_absent_false_when_the_status_reports_the_temp() {
        // Goal: a reference whose device reports the named temp is not missing.
        let status = status_with(&["temp1", "temp2"]);
        let metric = CustomSensorMetric::Temp;
        assert!(DeviceHealthController::is_absent(Some(&status), metric, "temp1").not());
    }

    #[test]
    fn is_absent_true_without_a_status() {
        // Goal: a device with no status yet has nothing, so its references are missing.
        assert!(DeviceHealthController::is_absent(
            None,
            CustomSensorMetric::Temp,
            "temp1"
        ));
    }

    #[test]
    fn is_absent_true_when_the_status_lacks_the_temp() {
        // Goal: a present device that no longer reports the named temp is missing,
        // as is one reporting no temps at all.
        let metric = CustomSensorMetric::Temp;
        let status = status_with(&["temp1"]);
        assert!(DeviceHealthController::is_absent(
            Some(&status),
            metric,
            "tempGone"
        ));
        let no_temps = status_with(&[]);
        assert!(DeviceHealthController::is_absent(
            Some(&no_temps),
            metric,
            "temp1"
        ));
    }

    #[test]
    fn is_absent_reads_a_channel_source_by_its_metric() {
        // Goal: a channel source is present only when the channel carries the metric's
        // value: the fan reports rpm, but no power, and it is no temp.
        let status = status_with(&["temp1"]);
        let is_absent =
            |metric, name| DeviceHealthController::is_absent(Some(&status), metric, name);
        assert!(is_absent(CustomSensorMetric::RPM, "fan1").not());
        assert!(is_absent(CustomSensorMetric::Watts, "fan1"));
        assert!(is_absent(CustomSensorMetric::Temp, "fan1"));
        assert!(is_absent(CustomSensorMetric::RPM, "temp1"));
    }

    #[test]
    fn scan_missing_keeps_only_unresolved() {
        // Goal: scanning returns exactly the candidates whose source is absent from the
        // live devices, temps and channels alike, preserving the others' identity.
        use crate::device::{Device, DeviceInfo};
        use crate::repositories::repository::Repositories;
        use std::cell::RefCell;
        use std::rc::Rc;

        let mut device = Device::new(
            "nct6798".to_string(),
            DeviceType::Hwmon,
            0,
            None,
            DeviceInfo::default(),
            None,
            1.0,
        );
        device.initialize_status_history_with(status_with(&["temp1"]), 1.0);
        let dev1 = device.uid.clone();
        let all_devices = Rc::new(HashMap::from([(
            dev1.clone(),
            Rc::new(RefCell::new(device)),
        )]));
        let controller = DeviceHealthController::new(
            all_devices,
            Rc::new(crate::config::Config::init_default_config().unwrap()),
            Rc::new(Repositories::default()),
            Rc::new(crate::overrides::OverridesController::empty()),
        );
        controller.candidates.replace(vec![
            candidate(&dev1, "temp1"),        // present -> dropped
            candidate(&dev1, "tempGone"),     // missing -> kept
            candidate("devX", "temp1"),       // device gone -> kept
            channel_candidate(&dev1, "fan1"), // present -> dropped
            channel_candidate(&dev1, "fan9"), // missing -> kept
        ]);

        let result = controller.scan_missing();

        assert_eq!(result.len(), 3);
        assert_eq!(result[0].source, source(&dev1, "tempGone"));
        assert_eq!(result[1].source, source("devX", "temp1"));
        assert_eq!(result[2].source, source(&dev1, "fan9"));
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
        assert_eq!(candidates[0].metric, CustomSensorMetric::Temp);
        let reference = &candidates[0].reference;
        assert_eq!(reference.entity_type, HealthEntityType::Profile);
        assert_eq!(reference.entity_uid, "p1");
        assert_eq!(reference.entity_name, "CPU Fan");
        assert_eq!(reference.source, source("dev1", "temp1"));
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
                metric: CustomSensorMetric::Temp,
                kind: CustomSensorKind::Mix {
                    mix_function: CustomSensorMixFunctionType::Avg,
                    sources: vec![
                        SensorSource::from_temp(source("dev1", "temp1"), 1),
                        SensorSource::from_temp(source("dev2", "temp2"), 1),
                    ],
                },
                children: Vec::new(),
                parents: Vec::new(),
            },
            CustomSensor {
                id: "file1".to_string(),
                metric: CustomSensorMetric::Temp,
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
        assert_eq!(candidates[0].metric, CustomSensorMetric::Temp);
        let reference = &candidates[0].reference;
        assert_eq!(reference.entity_type, HealthEntityType::CustomSensor);
        assert_eq!(reference.entity_uid, "sensor1");
        assert_eq!(reference.entity_name, "sensor1");
        assert_eq!(reference.channel_name, None);
        assert_eq!(reference.source, source("dev1", "temp1"));
        assert_eq!(candidates[1].reference.source, source("dev2", "temp2"));
    }

    #[test]
    fn custom_sensor_candidates_carry_the_sensor_metric() {
        // Goal: a non-temperature sensor's source is watched as that metric of a
        // channel. The reference keeps its shape, with the channel name in `temp_name`.
        let sensors = vec![CustomSensor {
            id: "pressure".to_string(),
            metric: CustomSensorMetric::RPM,
            kind: CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Avg,
                sources: vec![SensorSource {
                    device_uid: "dev1".to_string(),
                    name: "fan1".to_string(),
                    weight: 1,
                }],
            },
            children: Vec::new(),
            parents: Vec::new(),
        }];
        let mut candidates = Vec::new();
        DeviceHealthController::custom_sensor_candidates(&sensors, &mut candidates);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].metric, CustomSensorMetric::RPM);
        assert_eq!(candidates[0].reference.source, source("dev1", "fan1"));
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
        assert_eq!(candidates[0].metric, CustomSensorMetric::Temp);
        let reference = &candidates[0].reference;
        assert_eq!(reference.entity_type, HealthEntityType::Lcd);
        assert_eq!(reference.entity_uid, "dev1");
        assert_eq!(reference.entity_name, "Kraken");
        assert_eq!(reference.channel_name, Some("lcd".to_string()));
        assert_eq!(reference.source, source("dev2", "temp1"));
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
        // missing, and stale arrays, the two hardware-support sections, and
        // empty lists serialized as [] not omitted.
        let dto = DeviceHealthDto {
            unreachable: Vec::new(),
            failsafe: vec![FailsafeRef {
                device_uid: "dev1".to_string(),
                name: "temp1".to_string(),
                kind: FailsafeKind::Temp,
                reason: "stale readings".to_string(),
            }],
            missing: Vec::new(),
            stale_source: Vec::new(),
            firmware_overrides: Vec::new(),
            channel_capabilities: Vec::new(),
            system_findings: Vec::new(),
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
                "firmware_overrides": [],
                "channel_capabilities": [],
                "system_findings": [],
            "unreachable": [],
            })
        );
    }

    /// Goal: the permanent and current sections must not be interleaved. A
    /// firmware reclaim can clear on its own and belongs with the faults; a
    /// channel with no pwm never changes and belongs with the capabilities.
    /// Method: record one of each and check which section each lands in.
    #[test]
    fn verdict_sections_split_permanent_from_current() {
        let controller = crate::rt::test_runtime(async {
            crate::hardware_support::HardwareSupportController::init(None, true).await
        });
        let evidence = crate::hardware_support::ChannelEvidence {
            has_pwm: true,
            pwm_writable: true,
            has_rpm: true,
            has_pwm_enable: true,
        };
        controller.record_channel_diagnosis(
            "dev1",
            "fan1",
            crate::hardware_support::diagnose_channel(evidence.clone(), "nct6687", true),
        );
        let no_pwm = crate::hardware_support::ChannelEvidence {
            has_pwm: false,
            pwm_writable: false,
            has_rpm: true,
            has_pwm_enable: false,
        };
        controller.record_channel_diagnosis(
            "dev1",
            "fan2",
            crate::hardware_support::diagnose_channel(no_pwm, "some_driver", false),
        );
        // A working channel is reported in neither section.
        controller.record_channel_diagnosis(
            "dev1",
            "fan3",
            crate::hardware_support::diagnose_channel(evidence, "some_driver", false),
        );
        let (permanent, current) = controller.partitioned_verdicts();
        assert_eq!(current.len(), 1, "expected one current-state verdict");
        assert_eq!(current[0].channel_name, "fan1");
        assert_eq!(
            current[0].verdict,
            crate::hardware_support::ChannelVerdict::FirmwareOverride
        );
        assert_eq!(permanent.len(), 1, "expected one permanent verdict");
        assert_eq!(permanent[0].channel_name, "fan2");
        assert_eq!(
            permanent[0].verdict,
            crate::hardware_support::ChannelVerdict::NoPwm
        );
    }
}
