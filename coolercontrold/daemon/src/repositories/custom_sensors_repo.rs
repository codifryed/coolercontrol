// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace, warn};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::string::ToString;
use std::sync::Arc;
use std::time::Instant;

use crate::api::CCError;
use crate::config::Config;
use crate::device::{
    Device, DeviceInfo, DeviceType, DriverInfo, DriverType, Status, Temp, TempInfo, TempName,
    TempStatus, UID,
};
use crate::device_health::{FailsafeKind, FailsafeRef};
use crate::overrides::OverridesController;
use crate::repositories::failsafe::{MISSING_STATUS_THRESHOLD, MISSING_TEMP_FAILSAFE};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{
    CustomSensor, CustomSensorKind, CustomSensorMixFunctionType, CustomTempSourceData, LcdSettings,
    LightingSettings, Offset, TempSource,
};
use crate::{cc_fs, VERSION};

const MAX_CUSTOM_SENSOR_FILE_SIZE_BYTES: usize = 15;
/// Upper bound on window slots: the max `time_window_seconds` (300) at the fastest
/// `poll_rate` (0.5 s).
const SAMPLE_WINDOW_MAX_SLOTS: usize = 600;

type CustomSensors = RefCell<Vec<CustomSensor>>;
type Relationships = RefCell<HashMap<ChildName, Vec<ParentName>>>;
type ChildName = TempName;
type ParentName = TempName;

/// Rolling per-tick source-sample window for one `TimeAverage`/`ExponentialMovingAvg`
/// sensor. One slot per tick, oldest first; `None` when the source had no reading that
/// tick, mirroring how the old full-history collection skipped absent entries. The `None`
/// is window bookkeeping, not a data-plane sentinel: the folds skip those slots.
struct SampleWindow {
    /// Oldest first, bounded by `sample_count`.
    samples: VecDeque<Option<Temp>>,
    /// Window size this state was built for. A mismatch (sensor setting change) forces a
    /// reseed from history.
    sample_count: usize,
}

impl SampleWindow {
    /// Appends the current tick's sample, evicting the oldest once the window is full.
    fn push(&mut self, sample: Option<Temp>) {
        debug_assert!(self.sample_count >= 1);
        debug_assert!(self.sample_count <= SAMPLE_WINDOW_MAX_SLOTS);
        if self.samples.len() == self.sample_count {
            self.samples.pop_front();
        }
        self.samples.push_back(sample);
        debug_assert!(self.samples.len() <= self.sample_count);
    }
}

/// A Repository for Custom Sensors defined by the user
pub struct CustomSensorsRepo {
    config: Rc<Config>,
    custom_sensor_device: Option<DeviceLock>,
    device_uid: UID,
    all_devices: HashMap<UID, DeviceLock>,
    sensors: CustomSensors,
    relationships: Relationships,
    /// Captured from the Config at construction. `TimeAverage` derives the number of history
    /// samples from `time_window_seconds / poll_rate`, avoiding any wall-clock comparisons.
    /// `poll_rate` is fixed at runtime, so a plain `f64` is enough.
    poll_rate: f64,
    overrides: Rc<OverridesController>,
    /// Sensor IDs currently emitting `MISSING_TEMP_FAILSAFE` because their source is
    /// structurally absent (device removed, temp renamed, history empty, child not yet
    /// processed), mapped to the reason they entered failsafe. Membership drives
    /// once-per-occurrence entry / recovery logging so a flaky or misconfigured source
    /// does not flood the log every poll; the reason is surfaced via `failsafing()`.
    failsafing_sensors: RefCell<HashMap<String, String>>,
    /// Rolling sample windows for `TimeAverage`/`ExponentialMovingAvg` sensors, keyed by
    /// sensor id. Lets each tick fetch only the current source sample instead of
    /// rescanning up to `SAMPLE_WINDOW_MAX_SLOTS` history entries per sensor.
    sample_windows: RefCell<HashMap<TempName, SampleWindow>>,
    /// Transient read state for `File` sensors. See [`FileReadState`].
    file_read_state: RefCell<HashMap<TempName, FileReadState>>,
}

/// The held value is emitted for up to `MISSING_STATUS_THRESHOLD` consecutive failures, so one
/// unlucky read does not slam a fan curve to `MISSING_TEMP_FAILSAFE`. A sensor that never read
/// successfully has nothing to hold and failsafes at once.
#[derive(Default)]
struct FileReadState {
    consecutive_failures: u16,
    last_good_temp: Option<Temp>,
}

impl CustomSensorsRepo {
    pub fn new(
        config: Rc<Config>,
        all_other_devices: DeviceList,
        overrides: Rc<OverridesController>,
    ) -> Result<Self> {
        let poll_rate = config.get_settings()?.poll_rate;
        let mut all_devices = HashMap::new();
        for device in all_other_devices {
            let uid = device.borrow().uid.clone();
            all_devices.insert(uid, device);
        }
        Ok(Self {
            config,
            custom_sensor_device: None,
            device_uid: String::default(),
            all_devices,
            sensors: RefCell::new(Vec::new()),
            relationships: RefCell::new(HashMap::new()),
            poll_rate,
            overrides,
            failsafing_sensors: RefCell::new(HashMap::new()),
            file_read_state: RefCell::new(HashMap::new()),
            sample_windows: RefCell::new(HashMap::new()),
        })
    }

    pub fn get_device_uid(&self) -> UID {
        self.custom_sensor_device
            .as_ref()
            .expect("Custom Sensor Device should always be present after initialization")
            .borrow()
            .uid
            .clone()
    }

    pub fn get_custom_sensor(&self, custom_sensor_id: &str) -> Result<CustomSensor> {
        self.sensors
            .borrow()
            .iter()
            .find(|cs| cs.id == custom_sensor_id)
            .cloned()
            .ok_or_else(|| {
                CCError::NotFound {
                    msg: "Custom Sensor not found".to_string(),
                }
                .into()
            })
    }

    pub fn get_custom_sensors(&self) -> Vec<CustomSensor> {
        self.sensors.borrow().clone()
    }

    pub fn set_custom_sensors_order(&self, custom_sensors: &[CustomSensor]) -> Result<()> {
        self.config.set_custom_sensor_order(custom_sensors)?;
        self.sensors.borrow_mut().clear();
        self.sensors.borrow_mut().extend(custom_sensors.to_vec());
        // Order payloads carry full sensor bodies without validation; reseed the sample
        // windows rather than trust the bodies unchanged. Reseeding is output-identical.
        self.sample_windows.borrow_mut().clear();
        Ok(())
    }

    pub async fn set_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        self.verify_sensor_relationships(&custom_sensor)?;
        // Before the backfill, which would leave a second entry in every history slot.
        self.verify_sensor_id_is_new(&custom_sensor.id)?;
        self.fill_status_history_for_new_sensor(&custom_sensor)
            .await
            .inspect_err(|err| {
                error!("Failed to fill status history for new Custom Sensor: {err}");
            })?;
        self.config.set_custom_sensor(custom_sensor.clone())?;
        self.sensors.borrow_mut().push(custom_sensor);
        self.update_device_info_temps();
        self.reconstruct_relationships();
        Ok(())
    }

    pub async fn update_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        self.verify_sensor_relationships(&custom_sensor)?;
        if let CustomSensorKind::File { file_path } = &custom_sensor.kind {
            // Make sure the file exists and temp is properly formatted
            Self::get_custom_sensor_file_temp(file_path).await?;
        }
        // A reconfigured sensor starts fresh: drop any prior failsafing state so the
        // first tick after the update logs a transition cleanly if it failsafes again.
        self.failsafing_sensors
            .borrow_mut()
            .remove(&custom_sensor.id);
        // The window or source may have changed; the next tick reseeds from history.
        self.sample_windows.borrow_mut().remove(&custom_sensor.id);
        // A repointed File sensor must not ride out failures on the old file's value.
        self.file_read_state.borrow_mut().remove(&custom_sensor.id);
        self.config.update_custom_sensor(custom_sensor.clone())?;
        {
            let mut sensors = self.sensors.borrow_mut();
            // find check is done in the config update
            let pos = sensors
                .iter()
                .position(|s| s.id == custom_sensor.id)
                .expect("Custom Sensor not found");
            sensors[pos] = custom_sensor;
        }
        self.reconstruct_relationships();
        Ok(())
    }

    pub fn delete_custom_sensor(&self, custom_sensor_id: &str) -> Result<()> {
        // checks are already made to make sure this sensor isn't in use by a Profile.
        let parents = self
            .relationships
            .borrow()
            .get(custom_sensor_id)
            .cloned()
            .unwrap_or_default();
        self.validate_parents_can_drop_child(&parents, custom_sensor_id)?;
        // Persist before mutating: a failed config write must not leave the in-memory
        // parents already stripped, which would report a different value until restart.
        self.config.delete_custom_sensor(custom_sensor_id)?;
        self.drop_child_from_parents(&parents, custom_sensor_id);
        Self::remove_status_history_for_sensor(self, custom_sensor_id);
        self.sensors
            .borrow_mut()
            .retain(|cs| cs.id != custom_sensor_id);
        // Drop any failsafing state for the deleted sensor so a future sensor reusing the
        // same id (rare but legal) does not silently inherit "currently failsafing".
        self.failsafing_sensors
            .borrow_mut()
            .remove(custom_sensor_id);
        self.sample_windows.borrow_mut().remove(custom_sensor_id);
        self.file_read_state.borrow_mut().remove(custom_sensor_id);
        self.update_device_info_temps();
        self.reconstruct_relationships();
        Ok(())
    }

    /// Every parent must be able to give up this child. Checked before any parent is
    /// touched: rejecting partway through the loop used to leave earlier parents already
    /// stripped in memory while the config write never ran.
    fn validate_parents_can_drop_child(
        &self,
        parents: &[ParentName],
        child_id: &str,
    ) -> Result<()> {
        let sensors = self.sensors.borrow();
        for parent_name in parents {
            let Some(parent) = sensors.iter().find(|s| &s.id == parent_name) else {
                return Err(CCError::InternalError {
                    msg: format!(
                        "Parent sensor {parent_name} for Custom Sensor {child_id} not found"
                    ),
                }
                .into());
            };
            if parent.children.len() < 2 {
                return Err(CCError::UserError {
                    msg: format!(
                        "Parent sensor {parent_name} for Custom Sensor {child_id} \
                        only has this one child. The parent must first be deleted before \
                        deleting this Custom Sensor."
                    ),
                }
                .into());
            }
        }
        Ok(())
    }

    /// Strips `child_id` from every parent. Only reached once every parent has been
    /// approved, so it cannot fail partway and leave the set half-updated.
    fn drop_child_from_parents(&self, parents: &[ParentName], child_id: &str) {
        let mut sensors = self.sensors.borrow_mut();
        for parent_name in parents {
            let Some(parent) = sensors.iter_mut().find(|s| &s.id == parent_name) else {
                debug_assert!(false, "parent vanished between validation and mutation");
                continue;
            };
            parent.children.retain(|c| c != child_id);
            let Some(sources) = parent.sources_mut() else {
                continue;
            };
            // Only the deleted child goes: both halves must match for a source to be
            // the one being removed. Every custom-sensor source shares `device_uid`,
            // so requiring both to differ stripped the parent's other children too.
            sources.retain(|s| {
                s.temp_source.device_uid != self.device_uid || s.temp_source.temp_name != child_id
            });
        }
    }

    #[allow(clippy::cast_possible_truncation)]
    fn update_device_info_temps(&self) {
        let temp_infos = self
            .sensors
            .borrow()
            .iter()
            .enumerate()
            .map(|(index, cs)| {
                (
                    cs.id.clone(),
                    TempInfo {
                        label: cs.id.to_title_case(),
                        number: index as u8 + 1,
                    },
                )
            })
            .collect();
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .borrow_mut()
            .info
            .temps = temp_infos;
    }

    /// The function `fill_status_history_for_new_sensor` updates the status history of the
    /// custom sensor device.
    ///
    /// Arguments:
    ///
    /// * `sensor`: The `sensor` parameter is of type `CustomSensor`, which is a struct representing
    ///   a custom sensor.
    ///
    /// Returns: a `Result<()>`.
    async fn fill_status_history_for_new_sensor(&self, sensor: &CustomSensor) -> Result<()> {
        let mut status_history = self
            .custom_sensor_device
            .as_ref()
            .unwrap()
            .borrow()
            .status_history
            .clone();
        // Get mutable access to the VecDeque (will clone if there are other Arc refs)
        let history = Arc::make_mut(&mut status_history);
        match &sensor.kind {
            CustomSensorKind::Mix {
                mix_function,
                sources,
            } => {
                for (index, status) in history.iter_mut().enumerate() {
                    let temp_status =
                        self.process_reduced_indexed(&sensor.id, sources, index, |data| {
                            Self::process_temp_data(mix_function, data)
                        })?;
                    status.temps.push(temp_status);
                }
            }
            CustomSensorKind::Offset { offset, sources } => {
                for (index, status) in history.iter_mut().enumerate() {
                    let temp_status =
                        self.process_reduced_indexed(&sensor.id, sources, index, |data| {
                            Self::process_offset_temp_data(*offset, data)
                        })?;
                    status.temps.push(temp_status);
                }
            }
            CustomSensorKind::TimeAverage {
                time_window_seconds,
                sources,
            } => {
                // The source device's status_history is fully populated, so compute a real
                // time-average for every back-filled tick. Pre-compute sample_count once.
                let sample_count = Self::window_sample_count(*time_window_seconds, self.poll_rate);
                for (index, status) in history.iter_mut().enumerate() {
                    let temp_status = self.process_time_average_indexed(
                        &sensor.id,
                        &sources[0],
                        index,
                        sample_count,
                    );
                    status.temps.push(temp_status);
                }
            }
            CustomSensorKind::ExponentialMovingAvg {
                time_window_seconds,
                sources,
            } => {
                // Same backfill strategy as TimeAverage: compute the smoothed value at every
                // historical position so charts show real values from creation.
                let sample_count = Self::window_sample_count(*time_window_seconds, self.poll_rate);
                for (index, status) in history.iter_mut().enumerate() {
                    let temp_status =
                        self.process_ema_indexed(&sensor.id, &sources[0], index, sample_count);
                    status.temps.push(temp_status);
                }
            }
            CustomSensorKind::File { file_path } => {
                // Single read: verify the file is readable and use the value for the current
                // tick. Older history positions are placeholder 0s by design (File sensors
                // have no real history before creation; not a failsafe substitution).
                let current_temp = Self::get_custom_sensor_file_temp(file_path).await?;
                let current_temp_status = TempStatus {
                    name: sensor.id.clone(),
                    temp: current_temp,
                };
                let status_history_last_index = history.len() - 1;
                for (index, status) in history.iter_mut().enumerate() {
                    if index == status_history_last_index {
                        status.temps.push(current_temp_status.clone());
                    } else {
                        status.temps.push(TempStatus {
                            temp: 0.,
                            ..current_temp_status.clone()
                        });
                    }
                }
            }
        }
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .borrow_mut()
            .status_history = status_history;
        Ok(())
    }

    /// Builds the back-fill `TempStatus` for a Mix or Offset sensor at history `index` by
    /// reading each source at that index and reducing the collected data with `reduce`. If a
    /// source is structurally absent it is skipped; if none resolve, the data is zero-filled
    /// (prior back-fill behavior) so the chart shows a value rather than aborting the fill.
    fn process_reduced_indexed(
        &self,
        id: &TempName,
        sources: &[CustomTempSourceData],
        index: usize,
        reduce: impl Fn(&[TempData]) -> f64,
    ) -> Result<TempStatus> {
        let mut temp_data = Vec::with_capacity(sources.len());
        for custom_temp_source_data in sources {
            let temp_source = &custom_temp_source_data.temp_source;
            let some_temp_source = if temp_source.device_uid == self.device_uid {
                // Only used for NEW sensors, so safe for Parents too: children already have a
                // built status history.
                self.custom_sensor_device.as_ref()
            } else {
                self.all_devices.get(&temp_source.device_uid)
            };
            let Some(temp_source_device) = some_temp_source else {
                continue;
            };
            let some_temp = temp_source_device
                .borrow()
                .status_history
                .get(index)
                .and_then(|status| Self::get_temp_from_status(&temp_source.temp_name, status));
            let Some(temp) = some_temp else {
                let msg = format!(
                    "Temp not found for Custom Sensor: {}:{}",
                    temp_source.device_uid, temp_source.temp_name
                );
                return Err(CCError::InternalError { msg }.into());
            };
            temp_data.push(TempData {
                temp,
                weight: f64::from(custom_temp_source_data.weight),
            });
        }
        if temp_data.is_empty() {
            temp_data.push(TempData {
                temp: 0.,
                weight: 1.,
            });
            debug!("No temp data found for Custom Sensor: {id}. Filling with zeros");
        }
        Ok(TempStatus {
            name: id.clone(),
            temp: reduce(&temp_data),
        })
    }

    /// Builds the current-tick `TempStatus` for a Mix or Offset Custom Sensor by reducing the
    /// collected source data with `reduce`. If any source is structurally absent (device
    /// removed, temp renamed, child not yet processed this tick), or there is no source data,
    /// short-circuits to `MISSING_TEMP_FAILSAFE` so a fan curve driven by this sensor reacts
    /// to the missing reading rather than silently reporting a fake-cool value.
    fn process_reduced_current(
        &self,
        id: &TempName,
        sources: &[CustomTempSourceData],
        custom_temps: &[TempStatus],
        reduce: impl Fn(&[TempData]) -> f64,
    ) -> TempStatus {
        let mut temp_data = Vec::with_capacity(sources.len());
        for custom_temp_source_data in sources {
            let temp_source = &custom_temp_source_data.temp_source;
            let Ok(Some(temp)) = self.get_temp_source_temp(temp_source, custom_temps) else {
                // Device-first with the log convention's pipe separator, matching
                // how the UI composes device and channel names.
                let reason = match self.source_device_name(temp_source) {
                    Some(device_name) => {
                        format!("source missing: {device_name} | {}", temp_source.temp_name)
                    }
                    None => format!("source missing: {}", temp_source.temp_name),
                };
                return self.emit_failsafe(id, &reason);
            };
            temp_data.push(TempData {
                temp,
                weight: f64::from(custom_temp_source_data.weight),
            });
        }
        if temp_data.is_empty() {
            // Validation forbids this for Mix, but defensively failsafe rather than emitting a
            // misleading cool value if a malformed sensor ever slips through.
            return self.emit_failsafe(id, "no sources configured");
        }
        self.emit_real_temp(id, reduce(&temp_data))
    }

    /// Computes one sensor's current-tick temp and pushes it to `custom_temps`. `File` sensors
    /// are deferred into `file_sensors` so their async read happens outside the sensors borrow.
    /// Shared by the children-first and parents passes of `update_statuses`; parents are never
    /// `File`, so that arm is inert for them.
    fn process_live_sensor(
        &self,
        sensor: &CustomSensor,
        custom_temps: &mut Vec<TempStatus>,
        file_sensors: &mut Vec<(TempName, PathBuf)>,
    ) {
        match &sensor.kind {
            CustomSensorKind::Mix {
                mix_function,
                sources,
            } => {
                let temp_status =
                    self.process_reduced_current(&sensor.id, sources, custom_temps, |data| {
                        Self::process_temp_data(mix_function, data)
                    });
                custom_temps.push(temp_status);
            }
            CustomSensorKind::Offset { offset, sources } => {
                let temp_status =
                    self.process_reduced_current(&sensor.id, sources, custom_temps, |data| {
                        Self::process_offset_temp_data(*offset, data)
                    });
                custom_temps.push(temp_status);
            }
            CustomSensorKind::File { file_path } => {
                // Clone into owned data to avoid holding the sensors borrow over the await.
                file_sensors.push((sensor.id.clone(), file_path.clone()));
            }
            CustomSensorKind::TimeAverage {
                time_window_seconds,
                sources,
            } => {
                let temp_status = self.process_time_average_current(
                    &sensor.id,
                    &sources[0],
                    *time_window_seconds,
                    custom_temps,
                );
                custom_temps.push(temp_status);
            }
            CustomSensorKind::ExponentialMovingAvg {
                time_window_seconds,
                sources,
            } => {
                let temp_status = self.process_ema_current(
                    &sensor.id,
                    &sources[0],
                    *time_window_seconds,
                    custom_temps,
                );
                custom_temps.push(temp_status);
            }
        }
    }

    /// Processes a `TimeAverage` Custom Sensor for the current tick: the arithmetic mean
    /// over its rolling sample window. If the source is structurally absent, emits
    /// `MISSING_TEMP_FAILSAFE` so downstream control reacts to the missing reading
    /// rather than serving a fake-cool value.
    fn process_time_average_current(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        window_seconds: u16,
        custom_temps: &[TempStatus],
    ) -> TempStatus {
        self.process_windowed_current(id, source, window_seconds, custom_temps, false)
    }

    /// Shared current-tick processing for the windowed sensors (`TimeAverage` and
    /// `ExponentialMovingAvg`). Maintains a rolling per-sensor sample window so each tick
    /// fetches only the current source sample instead of rescanning up to
    /// `SAMPLE_WINDOW_MAX_SLOTS` history entries. Missing or size-mismatched window state
    /// (first tick, setting change, wake from sleep, restart) reseeds from history, which
    /// visits exactly what the old per-tick walk visited, so a reseed is output-identical.
    fn process_windowed_current(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        window_seconds: u16,
        custom_temps: &[TempStatus],
        is_ema: bool,
    ) -> TempStatus {
        if window_seconds == 0 {
            // Invariant break (validation enforces 1..=300, and window_sample_count
            // debug_asserts >= 1). Failsafe rather than emit a value from a degenerate window.
            return self.emit_failsafe(id, "invalid zero time_window_seconds");
        }
        let sample_count = Self::window_sample_count(window_seconds, self.poll_rate);
        let temp_source = &source.temp_source;
        let is_child_source = temp_source.device_uid == self.device_uid;
        if is_child_source.not() && self.all_devices.contains_key(&temp_source.device_uid).not() {
            // A removed source device failsafes immediately. Dropping the window prevents
            // averaging stale samples if the device ever returns.
            self.sample_windows.borrow_mut().remove(id);
            return self.emit_failsafe(id, "no source samples available");
        }
        let mut windows = self.sample_windows.borrow_mut();
        if windows
            .get(id)
            .is_none_or(|window| window.sample_count != sample_count)
        {
            let seeded = self.seed_sample_window(temp_source, custom_temps, sample_count);
            windows.insert(id.clone(), seeded);
        } else if let Some(window) = windows.get_mut(id) {
            window.push(self.current_source_sample(temp_source, custom_temps));
        }
        let computed = windows.get(id).and_then(|window| {
            if is_ema {
                Self::compute_ema_iter(window.samples.iter().flatten().copied(), sample_count)
            } else {
                // Newest-first fold to bit-match the pre-window summation order.
                Self::compute_time_average_iter(window.samples.iter().rev().flatten().copied())
            }
        });
        drop(windows);
        match computed {
            Some(temp) => self.emit_real_temp(id, temp),
            None => self.emit_failsafe(id, "no source samples available"),
        }
    }

    /// The source's sample for the current tick: child sources from this tick's
    /// `custom_temps`, external sources from the newest history entry (source repos update
    /// before this one). Callers ensure the source device exists.
    fn current_source_sample(
        &self,
        temp_source: &TempSource,
        custom_temps: &[TempStatus],
    ) -> Option<Temp> {
        if temp_source.device_uid == self.device_uid {
            return custom_temps
                .iter()
                .find(|t| t.name == temp_source.temp_name)
                .map(|t| t.temp);
        }
        self.all_devices
            .get(&temp_source.device_uid)
            .and_then(|device| {
                device
                    .borrow()
                    .status_history
                    .back()
                    .and_then(|status| Self::get_temp_from_status(&temp_source.temp_name, status))
            })
    }

    /// Builds a fresh window from the source's history, one slot per visited entry (`None`
    /// when the temp was absent), visiting exactly the entries the old per-tick collection
    /// visited. Child sources take the current tick from `custom_temps` plus
    /// `sample_count - 1` own-history entries; a child missing from `custom_temps` gets a
    /// `None` slot where the old walk read one extra history entry instead (one-sample
    /// divergence in a state validation mostly precludes).
    fn seed_sample_window(
        &self,
        temp_source: &TempSource,
        custom_temps: &[TempStatus],
        sample_count: usize,
    ) -> SampleWindow {
        debug_assert!(sample_count >= 1);
        let mut samples: VecDeque<Option<Temp>> = VecDeque::with_capacity(sample_count);
        // Collect newest-first as the old walk did, then reverse into window order.
        if temp_source.device_uid == self.device_uid {
            samples.push_back(
                custom_temps
                    .iter()
                    .find(|t| t.name == temp_source.temp_name)
                    .map(|t| t.temp),
            );
            if let Some(cs_device) = self.custom_sensor_device.as_ref() {
                for status in cs_device
                    .borrow()
                    .status_history
                    .iter()
                    .rev()
                    .take(sample_count - 1)
                {
                    samples.push_back(Self::get_temp_from_status(&temp_source.temp_name, status));
                }
            }
        } else if let Some(source_device) = self.all_devices.get(&temp_source.device_uid) {
            for status in source_device
                .borrow()
                .status_history
                .iter()
                .rev()
                .take(sample_count)
            {
                samples.push_back(Self::get_temp_from_status(&temp_source.temp_name, status));
            }
        }
        samples.make_contiguous().reverse();
        SampleWindow {
            samples,
            sample_count,
        }
    }

    /// Computes the time-average for a `TimeAverage` sensor at history `index`, used during
    /// back-fill of a newly-created sensor. Averages the last `sample_count` samples of the
    /// source device's `status_history` ending at (and including) `index`.
    fn process_time_average_indexed(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        index: usize,
        sample_count: usize,
    ) -> TempStatus {
        let temps = self.collect_indexed_source_temps(&source.temp_source, index, sample_count);
        let mean = Self::compute_time_average(&temps).unwrap_or(0.);
        TempStatus {
            name: id.clone(),
            temp: mean,
        }
    }

    /// Collects up to `sample_count` source temps from the source device's `status_history`,
    /// ending at history `index` (inclusive). Indices beyond the history's length are skipped.
    fn collect_indexed_source_temps(
        &self,
        temp_source: &TempSource,
        index: usize,
        sample_count: usize,
    ) -> Vec<Temp> {
        let mut temps: Vec<Temp> = Vec::with_capacity(sample_count);
        let some_source_device = if temp_source.device_uid == self.device_uid {
            // Children must exist before parents, so child status_history is already filled
            // by the time fill_status_history_for_new_sensor runs for the parent.
            self.custom_sensor_device.as_ref()
        } else {
            self.all_devices.get(&temp_source.device_uid)
        };
        let Some(source_device) = some_source_device else {
            return temps;
        };
        let device_ref = source_device.borrow();
        let history_len = device_ref.status_history.len();
        let end = index.saturating_add(1).min(history_len);
        let start = end.saturating_sub(sample_count);
        for k in start..end {
            if let Some(status) = device_ref.status_history.get(k) {
                if let Some(t) = Self::get_temp_from_status(&temp_source.temp_name, status) {
                    temps.push(t);
                }
            }
        }
        temps
    }

    /// How many samples fit in `window_seconds` at the current `poll_rate`. Always at least
    /// 1 and never more than `SAMPLE_WINDOW_MAX_SLOTS`. Pure helper so it's directly
    /// testable without setting up a Repo.
    ///
    /// The API caps `window_seconds` at 300 and clamps `poll_rate` to 0.5, which is exactly
    /// the maximum. A hand-edited config.toml reaches this without passing either check, so
    /// the ceiling is enforced here too rather than only by the `debug_assert!` on
    /// `SampleWindow`, which compiles out of a release build.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn window_sample_count(window_seconds: u16, poll_rate: f64) -> usize {
        debug_assert!(window_seconds >= 1);
        debug_assert!(poll_rate > 0.0);
        let count = (f64::from(window_seconds) / poll_rate).ceil() as usize;
        count.clamp(1, SAMPLE_WINDOW_MAX_SLOTS)
    }

    /// Returns the arithmetic mean of the samples in iteration order, or `None` if empty.
    /// Pure function — callers compose their own sample collection.
    #[allow(clippy::cast_precision_loss)]
    fn compute_time_average_iter(samples: impl Iterator<Item = Temp>) -> Option<Temp> {
        let mut sum: Temp = 0.0;
        let mut count: usize = 0;
        for sample in samples {
            sum += sample;
            count += 1;
        }
        if count == 0 {
            return None;
        }
        Some(sum / count as Temp)
    }

    /// Slice form of `compute_time_average_iter`, kept for the back-fill path and tests.
    fn compute_time_average(samples: &[Temp]) -> Option<Temp> {
        Self::compute_time_average_iter(samples.iter().copied())
    }

    /// Computes a single Exponential Moving Average over the samples (oldest first, newest
    /// last). `alpha = 2 / (period + 1)`, the standard EMA smoothing factor for an
    /// equivalent simple-moving-average window of `period` samples. The EMA is initialized
    /// with the first sample and iteratively updated. Returns `None` if `samples` is empty.
    /// Pure function — callers compose their own sample collection and ordering.
    #[allow(clippy::cast_precision_loss)]
    fn compute_ema_iter(samples: impl Iterator<Item = Temp>, period: usize) -> Option<Temp> {
        debug_assert!(period >= 1);
        let alpha = 2.0 / (period as f64 + 1.0);
        debug_assert!(alpha > 0.0);
        debug_assert!(alpha <= 1.0);
        let mut ema: Option<Temp> = None;
        for value in samples {
            ema = Some(match ema {
                Some(previous) => (value - previous).mul_add(alpha, previous),
                None => value,
            });
        }
        ema
    }

    /// Slice form of `compute_ema_iter`, kept for the back-fill path and tests.
    fn compute_ema(samples: &[Temp], period: usize) -> Option<Temp> {
        Self::compute_ema_iter(samples.iter().copied(), period)
    }

    /// Processes an `ExponentialMovingAvg` Custom Sensor for the current tick: a single EMA
    /// over its rolling sample window (oldest first). If the source is structurally absent,
    /// emits `MISSING_TEMP_FAILSAFE` so downstream control reacts to the missing reading.
    fn process_ema_current(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        window_seconds: u16,
        custom_temps: &[TempStatus],
    ) -> TempStatus {
        self.process_windowed_current(id, source, window_seconds, custom_temps, true)
    }

    /// Computes the EMA for an `ExponentialMovingAvg` sensor at history `index`, used during
    /// back-fill of a newly-created sensor. Uses the last `sample_count` samples of the
    /// source device's `status_history` ending at (and including) `index`.
    fn process_ema_indexed(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        index: usize,
        sample_count: usize,
    ) -> TempStatus {
        // collect_indexed_source_temps returns oldest-first, which is what compute_ema wants.
        let temps = self.collect_indexed_source_temps(&source.temp_source, index, sample_count);
        let ema = Self::compute_ema(&temps, sample_count).unwrap_or(0.);
        TempStatus {
            name: id.clone(),
            temp: ema,
        }
    }

    /// Retrieves the temperature value from a specified `TempSource`.
    /// Also handles retrieving the recently created temperature from child custom sensors.
    fn get_temp_source_temp(
        &self,
        temp_source: &TempSource,
        custom_temps: &[TempStatus],
    ) -> Result<Option<Temp>> {
        if temp_source.device_uid == self.device_uid {
            // parents get the child's status from the recent push to custom_temps
            return Ok(custom_temps
                .iter()
                .find(|temp| temp.name == temp_source.temp_name)
                .map(|temp| temp.temp));
        }
        let Some(temp_source_device) = self.all_devices.get(&temp_source.device_uid) else {
            // missing/removed devices are simply skipped
            return Err(anyhow!("Device not found"));
        };
        Ok(temp_source_device
            .borrow()
            .status_history
            .back()
            .and_then(|status| Self::get_temp_from_status(&temp_source.temp_name, status)))
    }

    fn get_temp_from_status(temp_source_name: &str, status: &Status) -> Option<f64> {
        status
            .temps
            .iter()
            .filter(|temp_status| temp_status.name == temp_source_name)
            .map(|temp_status| temp_status.temp)
            .next_back()
    }

    fn process_temp_data(
        mix_function: &CustomSensorMixFunctionType,
        temp_data: &[TempData],
    ) -> f64 {
        match mix_function {
            CustomSensorMixFunctionType::Min => Self::process_mix_min(temp_data),
            CustomSensorMixFunctionType::Max => Self::process_mix_max(temp_data),
            CustomSensorMixFunctionType::Delta => Self::process_mix_delta(temp_data),
            CustomSensorMixFunctionType::Avg => Self::process_mix_avg(temp_data),
            CustomSensorMixFunctionType::WeightedAvg => Self::process_mix_weighted_avg(temp_data),
        }
    }

    fn process_mix_min(temp_data: &[TempData]) -> f64 {
        temp_data.iter().fold(254., |acc, data| data.temp.min(acc))
    }

    fn process_mix_max(temp_data: &[TempData]) -> f64 {
        temp_data.iter().fold(0., |acc, data| data.temp.max(acc))
    }

    fn process_mix_delta(temp_data: &[TempData]) -> f64 {
        if temp_data.is_empty() {
            return 0.;
        }
        let mut min = 105.;
        let mut max = 0.;
        for data in temp_data {
            if data.temp < min {
                min = data.temp;
            }
            if data.temp > max {
                max = data.temp;
            }
        }
        (max - min).abs()
    }

    #[allow(clippy::cast_precision_loss)]
    fn process_mix_avg(temp_data: &[TempData]) -> f64 {
        if temp_data.is_empty() {
            return 0.;
        }
        temp_data.iter().fold(0., |acc, data| acc + data.temp) / temp_data.len() as f64
    }

    fn process_mix_weighted_avg(temp_data: &[TempData]) -> f64 {
        if temp_data.is_empty() {
            return 0.;
        }
        temp_data
            .iter()
            .fold(
                TempData {
                    temp: 0.,
                    weight: 0.,
                },
                |mut acc, data| {
                    let total_weight = acc.weight + data.weight;
                    acc.temp = (acc.temp * acc.weight + data.temp * data.weight) / total_weight;
                    acc.weight = total_weight;
                    acc
                },
            )
            .temp
    }

    /// Returns the first source's temp with the offset applied, or 0 if there is no source
    /// data. Clamps the result to a readable temp between 0 and 150.
    fn process_offset_temp_data(offset: Offset, temp_data: &[TempData]) -> f64 {
        if temp_data.is_empty() {
            return 0.;
        }
        (temp_data[0].temp + Temp::from(offset)).clamp(0.0, 150.0)
    }

    /// Reads the current temp for a File-type Custom Sensor. An unreadable / malformed file
    /// holds the last good value for `MISSING_STATUS_THRESHOLD` consecutive failures, then
    /// emits `MISSING_TEMP_FAILSAFE` (and a once-per-occurrence warn log): a fan curve rides
    /// out a writer caught mid-truncate but still reacts to a genuinely lost source.
    /// Live-tick path only; backfill reads `get_custom_sensor_file_temp` directly so it
    /// does not interact with the failsafing-state set.
    async fn process_custom_sensor_data_file_current(
        &self,
        id: &TempName,
        file_path: &Path,
    ) -> TempStatus {
        match Self::get_custom_sensor_file_temp(file_path).await {
            Ok(temp) => {
                self.record_file_read_success(id, temp);
                self.emit_real_temp(id, temp)
            }
            Err(_) => match self.tolerate_file_read_failure(id) {
                Some(temp) => TempStatus {
                    name: id.clone(),
                    temp,
                },
                None => self.emit_failsafe(id, "file unreadable"),
            },
        }
    }

    /// Clears the failure run and holds `temp` as the value to emit if the next reads fail.
    fn record_file_read_success(&self, id: &TempName, temp: Temp) {
        let mut states = self.file_read_state.borrow_mut();
        let state = states.entry(id.clone()).or_default();
        state.consecutive_failures = 0;
        state.last_good_temp = Some(temp);
    }

    /// Returns the held value while inside the tolerance window, or `None` once the run
    /// of failures passes `MISSING_STATUS_THRESHOLD` and the caller must failsafe.
    fn tolerate_file_read_failure(&self, id: &TempName) -> Option<Temp> {
        let mut states = self.file_read_state.borrow_mut();
        let state = states.entry(id.clone()).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if usize::from(state.consecutive_failures) > MISSING_STATUS_THRESHOLD {
            return None;
        }
        state.last_good_temp
    }

    async fn get_custom_sensor_file_temp(file_path: &Path) -> Result<f64> {
        cc_fs::read_sysfs_value(file_path)
            .await
            .map_err(Self::verify_file_exists)
            .and_then(Self::verify_file_size)
            .and_then(Self::verify_i32)
            .and_then(Self::verify_temp_value)
    }

    fn verify_file_exists(err: Error) -> Error {
        for cause in err.chain() {
            if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                if io_err.kind() == std::io::ErrorKind::NotFound {
                    return CCError::UserError {
                        msg: "File not found".to_string(),
                    }
                    .into();
                }
            }
        }
        err
    }

    fn verify_file_size(value: cc_fs::SysfsValue) -> Result<cc_fs::SysfsValue> {
        // A file beyond the read buffer reports the buffer length here, not its true
        // size. Same error class; the limit is far below the buffer.
        if value.len() > MAX_CUSTOM_SENSOR_FILE_SIZE_BYTES {
            Err(CCError::UserError {
                msg: format!(
                    "File size too large: {:?} bytes. Max allowed: {:?} bytes",
                    value.len(),
                    MAX_CUSTOM_SENSOR_FILE_SIZE_BYTES
                ),
            }
            .into())
        } else {
            Ok(value)
        }
    }

    // Bypasses SysfsValue::parse's full-buffer truncation guard; that is safe only
    // because verify_file_size (15 byte limit, far below the 64 byte read buffer) runs
    // first in the chain. Keep that ordering.
    #[allow(clippy::needless_pass_by_value)]
    fn verify_i32(value: cc_fs::SysfsValue) -> Result<i32> {
        let user_error = |msg: String| CCError::UserError { msg }.into();
        value
            .trimmed_str()
            .map_err(|err| user_error(format!("{err}")))
            .and_then(|content| {
                content
                    .parse::<i32>()
                    .map_err(|err| user_error(format!("{err}")))
            })
    }

    fn verify_temp_value(temp: i32) -> Result<f64> {
        //  temps should be in millidegrees:
        if (0..=120_000).contains(&temp) {
            Ok(f64::from(temp) / 1000.0f64)
        } else {
            Err(CCError::UserError {
                msg: format!("File does not contain a reasonable temperature: {temp}"),
            }
            .into())
        }
    }

    fn remove_status_history_for_sensor(&self, sensor_id: &str) {
        let mut device_lock = self.custom_sensor_device.as_ref().unwrap().borrow_mut();
        let history = Arc::make_mut(&mut device_lock.status_history);
        for status in history {
            status
                .temps
                .retain(|temp_status| temp_status.name != sensor_id);
        }
    }

    /// Verifies that the sensor is not already a child of another sensor
    /// and that any sensor children are not already parents
    /// This makes sure we maintain a 1-level hierarchy and don't end up with cyclic relationships.
    fn verify_sensor_relationships(&self, custom_sensor: &CustomSensor) -> Result<()> {
        // Variant-specific source cardinality (File has none, Offset/TimeAverage/EMA have
        // exactly one) is now enforced by the type, the API validator, and the config reader,
        // so this function only verifies the parent-child hierarchy.
        // The children vector is not necessarily filled at this point, so we check directly.
        for temp_source_data in custom_sensor.sources() {
            if temp_source_data.temp_source.device_uid != self.device_uid {
                continue;
            }
            if temp_source_data.temp_source.temp_name == custom_sensor.id {
                return Err(CCError::UserError {
                    msg: format!(
                        "Custom Sensor {sensor_id} cannot have itself as a child",
                        sensor_id = custom_sensor.id
                    ),
                }
                .into());
            }
            for (child_name, parents) in self.relationships.borrow().iter() {
                if &custom_sensor.id == child_name {
                    return Err(CCError::UserError {
                        msg: format!(
                            "The Custom Sensor {sensor_id} is already a child of {child_name} and \
                            cannot become a parent",
                            sensor_id = custom_sensor.id
                        ),
                    }
                    .into());
                }
                if parents.contains(&temp_source_data.temp_source.temp_name) {
                    return Err(CCError::UserError {
                        msg: format!(
                            "Child Custom Sensor {temp_source_name} is already a parent and \
                            cannot be a child of this Custom Sensor {sensor_id}",
                            temp_source_name = temp_source_data.temp_source.temp_name,
                            sensor_id = custom_sensor.id
                        ),
                    }
                    .into());
                }
            }
        }
        Ok(())
    }

    fn verify_sensor_id_is_new(&self, sensor_id: &str) -> Result<()> {
        let id_is_taken = self
            .sensors
            .borrow()
            .iter()
            .any(|sensor| sensor.id == sensor_id);
        if id_is_taken {
            return Err(CCError::UserError {
                msg: "Custom Sensor already exists. Use the update operation to update it."
                    .to_string(),
            }
            .into());
        }
        Ok(())
    }

    /// This constructs the parent-child relationships between custom sensors.
    fn reconstruct_relationships(&self) {
        // clear relationships on start, as this may be called on any sensor change
        self.relationships.borrow_mut().clear();
        // add children and relationships
        for sensor in self.sensors.borrow_mut().iter_mut() {
            sensor.children.clear();
            sensor.parents.clear();
            // Collect first so the borrow of the sources is released before we mutate
            // sensor.children. Only sources on the Custom Sensors device create relationships.
            let child_names: Vec<TempName> = sensor
                .sources()
                .iter()
                .filter(|data| data.temp_source.device_uid == self.device_uid)
                .map(|data| data.temp_source.temp_name.clone())
                .collect();
            for child_name in child_names {
                sensor.children.push(child_name.clone());
                self.relationships
                    .borrow_mut()
                    .entry(child_name)
                    .or_default()
                    .push(sensor.id.clone());
            }
        }
        // add parent relationships to children
        for (child_name, parents) in self.relationships.borrow().iter() {
            if let Some(child_sensor) = self
                .sensors
                .borrow_mut()
                .iter_mut()
                .find(|s| &s.id == child_name)
            {
                child_sensor.parents.extend(parents.iter().cloned());
            } else {
                error!("Custom Sensor Child: {child_name} not found!");
            }
        }
    }

    /// Records `sensor_id` as failsafing with its entry reason. Returns `true` if this is
    /// the first time this sensor entered failsafe (caller should emit a `warn!` line);
    /// `false` if it was already failsafing on a previous tick. The reason is fixed at
    /// entry so the reported reason always matches the logged transition.
    fn note_failsafing_sensor(&self, sensor_id: &str, reason: &str) -> bool {
        let mut failsafing = self.failsafing_sensors.borrow_mut();
        if failsafing.contains_key(sensor_id) {
            return false;
        }
        failsafing.insert(sensor_id.to_string(), reason.to_string());
        true
    }

    /// Removes `sensor_id` from the failsafing-sensors map. Returns `true` if it was present
    /// (caller should emit a recovery `info!` line); `false` if it was not failsafing.
    fn clear_failsafing_sensor(&self, sensor_id: &str) -> bool {
        self.failsafing_sensors
            .borrow_mut()
            .remove(sensor_id)
            .is_some()
    }

    /// Resolves a source device's display name for failsafe reasons: live devices first,
    /// then the config `devices` list, which retains devices no longer detected.
    fn source_device_name(&self, temp_source: &TempSource) -> Option<String> {
        if let Some(device) = self.all_devices.get(&temp_source.device_uid) {
            return Some(device.borrow().name.clone());
        }
        self.config.device_name(&temp_source.device_uid)
    }

    /// Builds the failsafe `TempStatus` and emits the entry log line on the first occurrence.
    /// `reason` is included in the log so a future operator can tell the cs_type-specific
    /// cause apart (for example "all sources missing" vs "file unreadable").
    fn emit_failsafe(&self, sensor_id: &str, reason: &str) -> TempStatus {
        if self.note_failsafing_sensor(sensor_id, reason) {
            warn!(
                "Custom Sensor {} entering failsafe ({MISSING_TEMP_FAILSAFE}°C): {reason}",
                self.overrides.log_channel_name(&self.device_uid, sensor_id)
            );
        }
        TempStatus {
            name: sensor_id.to_string(),
            temp: MISSING_TEMP_FAILSAFE,
        }
    }

    /// Wraps a real-value `TempStatus` and emits the recovery log line on the first
    /// non-failsafing tick after a failsafe state. Cheap to call on every successful tick:
    /// `HashSet::remove` returns `false` when the id is absent.
    fn emit_real_temp(&self, sensor_id: &str, temp: f64) -> TempStatus {
        if self.clear_failsafing_sensor(sensor_id) {
            info!(
                "Custom Sensor {} recovered from failsafe",
                self.overrides.log_channel_name(&self.device_uid, sensor_id)
            );
        }
        TempStatus {
            name: sensor_id.to_string(),
            temp,
        }
    }
}

#[async_trait(?Send)]
impl Repository for CustomSensorsRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::CustomSensors
    }

    fn failsafing(&self) -> Vec<FailsafeRef> {
        let failsafing = self.failsafing_sensors.borrow();
        if failsafing.is_empty() {
            return Vec::new();
        }
        let device_uid = self.get_device_uid();
        failsafing
            .iter()
            .map(|(sensor_id, reason)| FailsafeRef {
                device_uid: device_uid.clone(),
                name: sensor_id.clone(),
                kind: FailsafeKind::Temp,
                reason: reason.clone(),
            })
            .collect()
    }

    #[allow(clippy::cast_possible_truncation)]
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        let poll_rate = self.poll_rate;
        let custom_sensors = self.config.get_custom_sensors()?;
        let temp_infos = custom_sensors
            .iter()
            .enumerate()
            .map(|(index, cs)| {
                (
                    cs.id.clone(),
                    TempInfo {
                        label: cs.id.to_title_case(),
                        number: index as u8 + 1,
                    },
                )
            })
            .collect();
        let custom_sensor_device = Device::new(
            "Custom Sensors".to_string(),
            DeviceType::CustomSensors,
            1,
            None,
            DeviceInfo {
                temps: temp_infos,
                temp_min: 0,
                temp_max: 150,
                profile_max_length: 21,
                driver_info: DriverInfo {
                    drv_type: DriverType::CoolerControl,
                    name: Some("CustomSensors".to_string()),
                    version: Some(VERSION.to_string()),
                    locations: Vec::new(),
                },
                ..Default::default()
            },
            None,
            poll_rate,
        );
        self.sensors.borrow_mut().extend(custom_sensors);
        self.device_uid.clone_from(&custom_sensor_device.uid);
        self.reconstruct_relationships();
        // not allowed to blacklist this device, otherwise things can get strange
        self.custom_sensor_device = Some(Rc::new(RefCell::new(custom_sensor_device)));
        self.update_statuses().await?;
        let recent_status = self
            .custom_sensor_device
            .as_ref()
            .unwrap()
            .borrow()
            .status_current()
            .unwrap();
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .borrow_mut()
            .initialize_status_history_with(recent_status, poll_rate);
        if log::max_level() == log::LevelFilter::Debug {
            info!(
                "Initialized Custom Sensors Device: {:?}",
                self.custom_sensor_device.as_ref().unwrap().borrow()
            );
        } else {
            info!(
                "Initialized Custom Sensors: {:?}",
                self.sensors
                    .borrow()
                    .iter()
                    .map(|d| d.id.clone())
                    .collect::<Vec<String>>()
            );
        }
        trace!(
            "Time taken to initialize CUSTOM_SENSORS device: {:?}",
            start_initialization.elapsed()
        );
        debug!("CUSTOM_SENSOR Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        if let Some(device) = self.custom_sensor_device.as_ref() {
            vec![device.clone()]
        } else {
            Vec::new()
        }
    }

    /// For composite/sensor repos, there is no need to preload as other device statuses
    /// have already been updated.
    async fn preload_statuses(self: Rc<Self>) {}

    /// Drops all rolling sample windows. `zero_status_history` flattens every history on
    /// wake, so the post-wake reseed reads the zeroed entries exactly as the old per-tick
    /// walk did.
    async fn prepare_for_sleep(&self) {
        self.sample_windows.borrow_mut().clear();
    }

    async fn update_statuses(&self) -> Result<()> {
        if self.custom_sensor_device.is_none() {
            return Ok(());
        }
        let start_update = Instant::now();
        let mut custom_temps = Vec::new();
        let mut file_sensors: Vec<(TempName, PathBuf)> = Vec::new();
        // Children and standalone sensors first, so parents can read child values this tick.
        self.sensors
            .borrow()
            .iter()
            .filter(|s| s.children.is_empty()) // not parents
            .for_each(|sensor| {
                self.process_live_sensor(sensor, &mut custom_temps, &mut file_sensors);
            });
        for (id, file_path) in &file_sensors {
            let temp_status = self
                .process_custom_sensor_data_file_current(id, file_path)
                .await;
            custom_temps.push(temp_status);
        }
        self.sensors
            .borrow()
            .iter()
            .filter(|s| s.children.is_empty().not()) // parents
            .for_each(|sensor| {
                self.process_live_sensor(sensor, &mut custom_temps, &mut file_sensors);
            });
        self.custom_sensor_device
            .as_ref()
            .unwrap()
            .borrow_mut()
            .set_status(Status {
                temps: custom_temps,
                ..Default::default()
            });
        trace!(
            "STATUS SNAPSHOT Time taken for CUSTOM_SENSORS device: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        info!("CUSTOM_SENSORS Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, _device_uid: &UID, _channel_name: &str) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_manual_control(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CUSTOMER_SENSORS devices"
        ))
    }

    async fn apply_setting_speed_fixed(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _speed_fixed: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings Speed Fixed is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings Speed Profile is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings Lighting is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings LCD is not supported for CUSTOMER_SENSORS devices"
        ))
    }
    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings pwm_mode is not supported for CUSTOMER_SENSORS devices"
        ))
    }

    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }
}

struct TempData {
    temp: f64,
    weight: f64,
}

#[cfg(test)]
mod tests {
    use crate::cc_fs;
    use crate::config::Config;
    use crate::device::{
        Device, DeviceInfo, DeviceType, Status, TempInfo, TempName, TempStatus, UID,
    };
    use crate::repositories::custom_sensors_repo::{
        CustomSensorsRepo, SampleWindow, TempData, SAMPLE_WINDOW_MAX_SLOTS,
    };
    use crate::repositories::failsafe::{MISSING_STATUS_THRESHOLD, MISSING_TEMP_FAILSAFE};
    use crate::repositories::repository::{DeviceLock, Repository};
    use crate::setting::{
        CustomSensor, CustomSensorKind, CustomSensorMixFunctionType, CustomTempSourceData,
        TempSource,
    };
    use serial_test::serial;
    use std::cell::RefCell;
    use std::collections::{HashMap, VecDeque};
    use std::ops::Not;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    fn file_sensor(id: &str, file_path: PathBuf) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            kind: CustomSensorKind::File { file_path },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    fn test_overrides() -> Rc<crate::overrides::OverridesController> {
        Rc::new(crate::overrides::OverridesController::empty())
    }

    // Calculates the delta between the minimum and maximum temperature values in the given vector of TempData.
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_calculate_delta() {
        let temp_data = vec![
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
            TempData {
                temp: 5.0,
                weight: 1.0,
            },
            TempData {
                temp: 8.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_delta(&temp_data);
        assert_eq!(result, 5.0);
    }

    // Returns the absolute value of the delta.
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_absolute_value() {
        let temp_data = vec![
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
            TempData {
                temp: 5.0,
                weight: 1.0,
            },
            TempData {
                temp: 8.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_delta(&temp_data);
        assert_eq!(result.abs(), result);
    }

    // Returns 0.0 if the given vector of TempData is empty.
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_empty_vector() {
        let temp_data = vec![];
        let result = CustomSensorsRepo::process_mix_delta(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Returns 0.0 if all temperature values in the given vector of TempData are the same.
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_same_temperatures() {
        let temp_data = vec![
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_delta(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Returns the difference between the only two temperature values in the given vector of TempData if it contains exactly two elements.
    #[test]
    #[allow(clippy::float_cmp)]
    fn test_two_elements() {
        let temp_data = vec![
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
            TempData {
                temp: 5.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_delta(&temp_data);
        assert_eq!(result, 5.0);
    }

    // Returns the minimum temperature from a vector of temperature data.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_minimum_temperature() {
        let temp_data = vec![
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
            TempData {
                temp: 20.0,
                weight: 1.0,
            },
            TempData {
                temp: 30.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_min(&temp_data);
        assert_eq!(result, 20.0);
    }

    // Returns 0 when all temperatures in the vector are 0.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_zero_when_all_temperatures_are_zero() {
        let temp_data = vec![
            TempData {
                temp: 0.0,
                weight: 1.0,
            },
            TempData {
                temp: 0.0,
                weight: 1.0,
            },
            TempData {
                temp: 0.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_min(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Returns the only temperature in the vector when there is only one temperature.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_single_temperature_when_only_one_temperature() {
        let temp_data = vec![TempData {
            temp: 25.0,
            weight: 1.0,
        }];
        let result = CustomSensorsRepo::process_mix_min(&temp_data);
        assert_eq!(result, 25.0);
    }

    // Returns the minimum temperature when there are multiple temperatures in the vector that are the same.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_minimum_temperature_with_multiple_same_temperatures() {
        let temp_data = vec![
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
            TempData {
                temp: 20.0,
                weight: 1.0,
            },
            TempData {
                temp: 20.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_min(&temp_data);
        assert_eq!(result, 20.0);
    }

    // Returns the maximum temperature value from a vector of TempData structs with positive values
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_max_temp_from_positive_values() {
        let temp_data = vec![
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
            TempData {
                temp: 30.0,
                weight: 1.0,
            },
            TempData {
                temp: 28.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_max(&temp_data);
        assert_eq!(result, 30.0);
    }

    // Returns 0 when all temperature values in the vector are 0
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_0_when_all_temps_are_0() {
        let temp_data = vec![
            TempData {
                temp: 0.0,
                weight: 1.0,
            },
            TempData {
                temp: 0.0,
                weight: 1.0,
            },
            TempData {
                temp: 0.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_max(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Returns the maximum temperature value when all temperature values in the vector are the same
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_max_temp_when_all_temps_are_same() {
        let temp_data = vec![
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_max(&temp_data);
        assert_eq!(result, 25.0);
    }

    // Returns 0 when the vector is empty
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_0_when_vector_is_empty() {
        let temp_data: Vec<TempData> = vec![];
        let result = CustomSensorsRepo::process_mix_max(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Returns the maximum temperature value when the vector has only one element
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_max_temp_when_vector_has_one_element() {
        let temp_data = vec![TempData {
            temp: 30.0,
            weight: 1.0,
        }];
        let result = CustomSensorsRepo::process_mix_max(&temp_data);
        assert_eq!(result, 30.0);
    }

    // Returns the maximum temperature value when the vector has two elements with different temperature values
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_max_temp_when_vector_has_two_elements_with_different_temps() {
        let temp_data = vec![
            TempData {
                temp: 25.0,
                weight: 1.0,
            },
            TempData {
                temp: 30.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_max(&temp_data);
        assert_eq!(result, 30.0);
    }

    // Calculates the weighted average of a list of temperature data with weights.
    #[test]
    #[allow(clippy::float_cmp)]
    fn calculates_weighted_average() {
        let temp_data = vec![
            TempData {
                temp: 10.0,
                weight: 2.0,
            },
            TempData {
                temp: 20.0,
                weight: 3.0,
            },
            TempData {
                temp: 30.0,
                weight: 4.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_weighted_avg(&temp_data);
        assert_eq!(result, 22.222_222_222_222_22);
    }

    // Returns the correct weighted average for a list of temperature data with weights.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_correct_weighted_average() {
        let temp_data = vec![
            TempData {
                temp: 5.0,
                weight: 1.0,
            },
            TempData {
                temp: 10.0,
                weight: 2.0,
            },
            TempData {
                temp: 15.0,
                weight: 3.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_weighted_avg(&temp_data);
        assert_eq!(result, 11.666_666_666_666_666);
    }

    // Returns 0 when given an empty list of temperature data.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_zero_for_empty_list() {
        let temp_data = vec![];
        let result = CustomSensorsRepo::process_mix_weighted_avg(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Calculates the average temperature correctly when given a vector of valid temperature data.
    #[test]
    #[allow(clippy::float_cmp)]
    fn calculates_average_temperature_correctly() {
        let temp_data = vec![
            TempData {
                temp: 10.0,
                weight: 1.0,
            },
            TempData {
                temp: 20.0,
                weight: 1.0,
            },
            TempData {
                temp: 30.0,
                weight: 1.0,
            },
        ];
        let result = CustomSensorsRepo::process_mix_avg(&temp_data);
        assert_eq!(result, 20.0);
    }

    // Returns 0 when given an empty vector of temperature data.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_zero_for_empty_vector() {
        let temp_data = vec![];
        let result = CustomSensorsRepo::process_mix_avg(&temp_data);
        assert_eq!(result, 0.0);
    }

    // Returns the only temperature value in the vector when given a vector of length 1.
    #[test]
    #[allow(clippy::float_cmp)]
    fn returns_single_value_for_vector_of_length_one() {
        let temp_data = vec![TempData {
            temp: 15.0,
            weight: 1.0,
        }];
        let result = CustomSensorsRepo::process_mix_avg(&temp_data);
        assert_eq!(result, 15.0);
    }

    #[test]
    #[serial]
    #[allow(clippy::float_cmp)]
    fn test_file_temp_status_valid() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(
                &test_file,
                b"30000".to_vec(), // millidegree temp
            )
            .await
            .unwrap();
            let cs_name = "test_sensor1".to_string();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();

            // when:
            let temp = repo
                .process_custom_sensor_data_file_current(&cs_name, &test_file)
                .await;

            // then:
            assert_eq!(temp.name, cs_name);
            assert_eq!(temp.temp, 30.);
        });
    }

    // An unreadable file emits MISSING_TEMP_FAILSAFE (not 0.) so a fan curve driven by
    // this sensor reacts to the lost source rather than a fake-cool value. The sensor is
    // also recorded in the failsafing-sensors set so the once-per-occurrence warn fires.
    #[test]
    #[serial]
    fn test_file_temp_status_invalid() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = Path::new("/tmp/does_not_exist").to_path_buf();
            let cs_name = "test_sensor1".to_string();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();

            // when:
            let temp = repo
                .process_custom_sensor_data_file_current(&cs_name, &test_file)
                .await;

            // then:
            assert_eq!(temp.name, cs_name);
            assert!((temp.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert_eq!(
                repo.failsafing_sensors.borrow()[cs_name.as_str()],
                "file unreadable"
            );
        });
    }

    #[test]
    #[serial]
    #[allow(clippy::float_cmp)]
    fn test_file_temp_valid() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(
                &test_file,
                b"30000".to_vec(), // millidegree temp
            )
            .await
            .unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_ok());
            let temp = temp_result.unwrap();
            assert_eq!(temp, 30.);
        });
    }

    #[test]
    #[serial]
    #[allow(clippy::float_cmp)]
    fn test_file_temp_valid_with_return() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b" 30000\n\r".to_vec())
                .await
                .unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_ok());
            let temp = temp_result.unwrap();
            assert_eq!(temp, 30.);
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_not_exist() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = Path::new("/tmp/does_not_exist").to_path_buf();

            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("File not found"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_out_of_range_1() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(
                &test_file,
                b"-1000".to_vec(), // millidegree temp
            )
            .await
            .unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err
                    .to_string()
                    .contains("File does not contain a reasonable temperature"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_out_of_range_2() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(
                &test_file,
                b"1000000".to_vec(), // millidegree temp
            )
            .await
            .unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err
                    .to_string()
                    .contains("File does not contain a reasonable temperature"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_format() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"asdf".to_vec()).await.unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("invalid digit"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_too_large_for_i32() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write_string(&test_file, (i64::from(i32::MAX) + 1).to_string())
                .await
                .unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("number too large"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_file_size_too_large() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(
                &test_file,
                b"100000000000000000000000000000000000000000000000000000000000000000000000000"
                    .to_vec(),
            )
            .await
            .unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            // println!("{temp_result:?}");
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("File size too large"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_number_format() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"32.5".to_vec()).await.unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("invalid digit"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_empty() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"".to_vec()).await.unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("empty string"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_file_temp_invalid_blank() {
        cc_fs::test_runtime(async {
            // given:
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b" ".to_vec()).await.unwrap();
            // when:
            let temp_result = CustomSensorsRepo::get_custom_sensor_file_temp(&test_file).await;

            // then:
            assert!(temp_result.is_err());
            assert!(temp_result
                .map_err(|err| err.to_string().contains("empty string"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_verify_relationship_no_parents_of_parents() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let child_sensor = file_sensor("child_sensor", test_file);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "child_sensor".to_string(),
                    },
                }],
            );
            let grandparent_sensor = mix_sensor(
                "grandparent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "parent_sensor".to_string(),
                    },
                }],
            );

            // when:
            repo.set_custom_sensor(child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(parent_sensor)
                .await
                .expect("Failed to set parent sensor");
            let result = repo.set_custom_sensor(grandparent_sensor).await;

            // then:
            assert!(result.is_err());
            assert!(result
                .map_err(|err| err.to_string().contains("already a parent"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_verify_relationship_no_children_of_children() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let mut child_sensor = mix_sensor("child_sensor", vec![]);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "child_sensor".to_string(),
                    },
                }],
            );
            let standalone_sensor = file_sensor("standalone_sensor", test_file);

            // when: A child tries to become a parent/add a child
            repo.set_custom_sensor(child_sensor.clone())
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(parent_sensor)
                .await
                .expect("Failed to set parent sensor");
            repo.set_custom_sensor(standalone_sensor)
                .await
                .expect("Failed to set standalone sensor");
            child_sensor
                .sources_mut()
                .expect("mix sensor has sources")
                .push(CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "standalone_sensor".to_string(),
                    },
                });
            let result = repo.update_custom_sensor(child_sensor).await;

            // then:
            assert!(result.is_err());
            assert!(result
                .map_err(|err| err.to_string().contains("cannot become a parent"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn test_verify_relationship_parent_multiple_children() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let child_sensor = file_sensor("child_sensor", test_file.clone());
            let second_child_sensor = file_sensor("second_child_sensor", test_file);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![
                    CustomTempSourceData {
                        weight: 1,
                        temp_source: TempSource {
                            device_uid: repo.device_uid.clone(),
                            temp_name: "child_sensor".to_string(),
                        },
                    },
                    CustomTempSourceData {
                        weight: 1,
                        temp_source: TempSource {
                            device_uid: repo.device_uid.clone(),
                            temp_name: "second_child_sensor".to_string(),
                        },
                    },
                ],
            );

            // when:
            repo.set_custom_sensor(child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(second_child_sensor)
                .await
                .expect("Failed to set child sensor");
            let result = repo.set_custom_sensor(parent_sensor).await;

            // then:
            assert!(result.is_ok());
        });
    }

    #[test]
    #[serial]
    fn delete_rejected_by_one_parent_leaves_the_others_untouched() {
        // Goal: a delete that a later parent rejects must not have already stripped an
        // earlier one. Method: give the shared child two parents, one that can spare it
        // (two children) and one that cannot (only child). The delete must fail and the
        // two-child parent must still hold both children and both sources, because the
        // config write never ran and the in-memory set must match what is on disk.
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            repo.set_custom_sensor(file_sensor("shared_child", test_file.clone()))
                .await
                .expect("Failed to set shared child");
            repo.set_custom_sensor(file_sensor("other_child", test_file))
                .await
                .expect("Failed to set other child");
            repo.set_custom_sensor(mix_sensor(
                "sparing_parent",
                vec![
                    temp_source(&repo.device_uid.clone(), "shared_child"),
                    temp_source(&repo.device_uid.clone(), "other_child"),
                ],
            ))
            .await
            .expect("Failed to set sparing parent");
            repo.set_custom_sensor(mix_sensor(
                "only_child_parent",
                vec![temp_source(&repo.device_uid.clone(), "shared_child")],
            ))
            .await
            .expect("Failed to set only-child parent");

            let result = repo.delete_custom_sensor("shared_child");

            assert!(
                result.is_err(),
                "the only-child parent must reject the delete"
            );
            let sensors = repo.sensors.borrow();
            let sparing = sensors
                .iter()
                .find(|s| s.id == "sparing_parent")
                .expect("sparing parent still exists");
            assert_eq!(
                sparing.children.len(),
                2,
                "a rejected delete stripped a child from an unrelated parent"
            );
            assert_eq!(
                sparing.sources().len(),
                2,
                "a rejected delete stripped a source from an unrelated parent"
            );
            assert!(sensors.iter().any(|s| s.id == "shared_child"));
        });
    }

    #[test]
    #[serial]
    fn test_verify_relationship_child_multiple_parents() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let child_sensor = file_sensor("child_sensor", test_file);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "child_sensor".to_string(),
                    },
                }],
            );
            let second_parent_sensor = mix_sensor(
                "second_parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "child_sensor".to_string(),
                    },
                }],
            );

            // when:
            repo.set_custom_sensor(child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(parent_sensor)
                .await
                .expect("Failed to set parent sensor");
            let result = repo.set_custom_sensor(second_parent_sensor).await;

            // then:
            assert!(
                result.is_ok(),
                "Failed to set second parent sensor: {}",
                result.unwrap_err()
            );
        });
    }

    #[test]
    #[serial]
    fn test_delete_removes_child_from_parent() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let child_sensor = file_sensor("child_sensor", test_file.clone());
            let second_child_sensor = file_sensor("second_child_sensor", test_file);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![
                    CustomTempSourceData {
                        weight: 1,
                        temp_source: TempSource {
                            device_uid: repo.device_uid.clone(),
                            temp_name: "child_sensor".to_string(),
                        },
                    },
                    CustomTempSourceData {
                        weight: 1,
                        temp_source: TempSource {
                            device_uid: repo.device_uid.clone(),
                            temp_name: "second_child_sensor".to_string(),
                        },
                    },
                ],
            );

            // when:
            repo.set_custom_sensor(child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(second_child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(parent_sensor)
                .await
                .expect("Failed to set parent sensor");
            let result = repo.delete_custom_sensor("child_sensor");

            // then:
            assert!(
                result.is_ok(),
                "Failed to delete child sensor: {}",
                result.unwrap_err()
            );
            assert_eq!(
                repo.sensors.borrow().len(),
                2,
                "Unexpected number of sensors left"
            );
            assert!(
                repo.sensors
                    .borrow()
                    .iter()
                    .any(|sensor| sensor.id == "child_sensor")
                    .not(),
                "Child sensor still exists"
            );
            assert!(
                repo.sensors
                    .borrow()
                    .iter()
                    .any(|sensor| sensor.id == "parent_sensor"
                        && sensor
                            .sources()
                            .iter()
                            .any(|s| s.temp_source.temp_name == "child_sensor")
                            .not()),
                "Parent sensor still has child sensor"
            );
            assert!(
                repo.sensors
                    .borrow()
                    .iter()
                    .any(|sensor| sensor.id == "parent_sensor"
                        && sensor
                            .sources()
                            .iter()
                            .any(|s| s.temp_source.temp_name == "second_child_sensor")),
                "Parent sensor lost its surviving child's source"
            );
        });
    }

    #[test]
    #[serial]
    fn test_delete_cannot_if_only_child() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let child_sensor = file_sensor("child_sensor", test_file);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "child_sensor".to_string(),
                    },
                }],
            );

            // when:
            repo.set_custom_sensor(child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(parent_sensor)
                .await
                .expect("Failed to set parent sensor");
            let result = repo.delete_custom_sensor("child_sensor");

            // then:
            assert!(result.is_err());
            assert!(result
                .map_err(|err| err.to_string().contains("only has this one child"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    #[allow(clippy::float_cmp)]
    fn test_update_children_first() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"80000".to_vec()).await.unwrap();
            let child_sensor = file_sensor("child_sensor", test_file);
            let parent_sensor = mix_sensor(
                "parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "child_sensor".to_string(),
                    },
                }],
            );
            let second_test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&second_test_file, b"90000".to_vec())
                .await
                .unwrap();
            let second_child_sensor = file_sensor("second_child_sensor", second_test_file);
            let second_parent_sensor = mix_sensor(
                "second_parent_sensor",
                vec![CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        temp_name: "second_child_sensor".to_string(),
                    },
                }],
            );

            // when:
            repo.set_custom_sensor(child_sensor)
                .await
                .expect("Failed to set child sensor");
            repo.set_custom_sensor(parent_sensor)
                .await
                .expect("Failed to set parent sensor");
            repo.set_custom_sensor(second_child_sensor)
                .await
                .expect("Failed to set second child sensor");
            repo.set_custom_sensor(second_parent_sensor)
                .await
                .expect("Failed to set second parent sensor");
            repo.sensors.borrow_mut().reverse(); // put the parents first, to be sure.
            let result = repo.update_statuses().await;

            // then:
            assert!(result.is_ok());
            let all_status_temps = repo
                .custom_sensor_device
                .unwrap()
                .borrow()
                .status_current()
                .unwrap()
                .temps;
            assert!(all_status_temps
                .iter()
                .any(|t| &t.name == "child_sensor" && t.temp == 80.0),);
            assert!(all_status_temps
                .iter()
                .any(|t| &t.name == "parent_sensor" && t.temp == 80.0),);
            assert!(all_status_temps
                .iter()
                .any(|t| &t.name == "second_child_sensor" && t.temp == 90.0),);
            assert!(all_status_temps
                .iter()
                .any(|t| &t.name == "second_parent_sensor" && t.temp == 90.0),);
        });
    }

    #[test]
    #[serial]
    #[allow(clippy::float_cmp)]
    fn test_parent_cannot_be_its_own_child() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");

            let mut sensor = mix_sensor("sensor", vec![]);

            // when:
            repo.set_custom_sensor(sensor.clone())
                .await
                .expect("Failed to set child sensor");
            sensor
                .sources_mut()
                .expect("mix sensor has sources")
                .push(CustomTempSourceData {
                    weight: 1,
                    temp_source: TempSource {
                        device_uid: repo.device_uid.clone(),
                        // itself:
                        temp_name: "sensor".to_string(),
                    },
                });
            let result = repo.update_custom_sensor(sensor).await;

            // then:
            assert!(result.is_err());
            assert!(result
                .map_err(|err| err.to_string().contains("cannot have itself as a child"))
                .unwrap_err());
        });
    }

    // Creating a sensor under an id that is already in use is refused, and the refused
    // attempt must not backfill: every history slot keeps exactly one entry for the id.
    #[test]
    #[serial]
    fn test_duplicate_id_is_rejected_before_backfill() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices()
                .await
                .expect("Failed to initialize devices");
            repo.set_custom_sensor(mix_sensor("sensor", vec![]))
                .await
                .expect("Failed to set sensor");

            // when:
            let result = repo.set_custom_sensor(mix_sensor("sensor", vec![])).await;

            // then:
            assert!(result.is_err());
            assert_eq!(repo.sensors.borrow().len(), 1);
            let device = repo.custom_sensor_device.as_ref().unwrap().borrow();
            assert!(device.status_history.is_empty().not());
            for status in device.status_history.iter() {
                let entry_count = status.temps.iter().filter(|t| t.name == "sensor").count();
                assert_eq!(entry_count, 1);
            }
        });
    }

    // ==================== TimeAverage helper tests ====================

    // Empty slice => None. The caller treats this as a fallback case (emit 0, since the source
    // device is gone). When the source device exists, the slice is always non-empty because
    // status_history is guaranteed filled.
    #[test]
    fn time_average_compute_empty_returns_none() {
        let samples: Vec<f64> = Vec::new();
        assert!(CustomSensorsRepo::compute_time_average(&samples).is_none());
    }

    // Basic arithmetic mean across multiple samples.
    #[test]
    #[allow(clippy::float_cmp)]
    fn time_average_compute_basic_mean() {
        let samples = vec![10.0, 20.0, 30.0, 40.0];
        assert_eq!(
            CustomSensorsRepo::compute_time_average(&samples),
            Some(25.0)
        );
    }

    // Single sample => the sample itself (degenerate but must work, e.g. the very first tick
    // where N=1 makes sense conceptually).
    #[test]
    #[allow(clippy::float_cmp)]
    fn time_average_compute_single_sample() {
        let samples = vec![42.0];
        assert_eq!(
            CustomSensorsRepo::compute_time_average(&samples),
            Some(42.0)
        );
    }

    // window_sample_count: 10s window @ 1s poll = 10 samples.
    #[test]
    fn time_average_window_sample_count_basic() {
        assert_eq!(CustomSensorsRepo::window_sample_count(10, 1.0), 10);
    }

    // window_sample_count: ceiling division — 10s @ 0.3s poll = 34 samples (33.33 rounded up).
    #[test]
    fn time_average_window_sample_count_ceiling() {
        assert_eq!(CustomSensorsRepo::window_sample_count(10, 0.3), 34);
    }

    // window_sample_count never returns 0 — guards against division-by-zero in mean computation
    // even at extreme poll rates (e.g. a 1s window with a 10s poll rate would round down to 0).
    #[test]
    fn time_average_window_sample_count_min_one() {
        assert_eq!(CustomSensorsRepo::window_sample_count(1, 10.0), 1);
    }

    // 300s window @ 1s poll = 300 samples — the upper-bound case the validator allows.
    #[test]
    fn time_average_window_sample_count_max_window() {
        assert_eq!(CustomSensorsRepo::window_sample_count(300, 1.0), 300);
    }

    // The API's own limits meet exactly at the declared ceiling: a 300s window at the
    // fastest allowed 0.5s poll rate is 600 slots, no more.
    #[test]
    fn time_average_window_sample_count_api_maximum_is_the_ceiling() {
        assert_eq!(
            CustomSensorsRepo::window_sample_count(300, 0.5),
            SAMPLE_WINDOW_MAX_SLOTS
        );
    }

    // A hand-edited config.toml bypasses the API's 1..=300 check, so the ceiling has to
    // hold here too: the `debug_assert!` on SampleWindow compiles out of a release build,
    // which would otherwise allocate ~131k slots per sensor for a u16 window.
    #[test]
    fn time_average_window_sample_count_clamps_beyond_the_api_range() {
        assert_eq!(
            CustomSensorsRepo::window_sample_count(u16::MAX, 0.5),
            SAMPLE_WINDOW_MAX_SLOTS
        );
        assert_eq!(
            CustomSensorsRepo::window_sample_count(3600, 1.0),
            SAMPLE_WINDOW_MAX_SLOTS
        );
    }

    // ==================== EMA helper tests ====================

    // Empty slice => None. Mirrors compute_time_average's empty contract: callers compose
    // their own collection; an empty result signals "no samples available" and routes to
    // failsafe at the call site.
    #[test]
    fn ema_compute_empty_returns_none() {
        let samples: Vec<f64> = Vec::new();
        assert!(CustomSensorsRepo::compute_ema(&samples, 8).is_none());
    }

    // Single sample => the sample itself (degenerate but must work, e.g. the very first
    // tick where no prior history exists).
    #[test]
    #[allow(clippy::float_cmp)]
    fn ema_compute_single_sample() {
        let samples = vec![42.0];
        assert_eq!(CustomSensorsRepo::compute_ema(&samples, 8), Some(42.0));
    }

    // Constant-value input => the EMA collapses to that constant exactly. Verifies the
    // recurrence is idempotent on a flat signal regardless of period.
    #[test]
    #[allow(clippy::float_cmp)]
    fn ema_compute_constant_input_returns_constant() {
        let samples = vec![50.0; 20];
        assert_eq!(CustomSensorsRepo::compute_ema(&samples, 10), Some(50.0));
    }

    // Step input from 0 to 100 over a long history: the EMA must converge upward toward
    // 100 but stay strictly below the latest sample (the recurrence is asymptotic, not
    // overshooting). Catches sign or operand-order flips in the mul_add update.
    #[test]
    fn ema_compute_step_input_converges_below_target() {
        let mut samples = vec![0.0; 50];
        samples.extend(std::iter::repeat_n(100.0, 50));
        let result = CustomSensorsRepo::compute_ema(&samples, 5).unwrap();
        // After 50 samples of "100" with period 5, EMA should be very close to 100 but not
        // exceed it — single EMA cannot overshoot a bounded input.
        assert!(
            result > 95.0,
            "EMA should converge close to 100, got {result}"
        );
        assert!(
            result < 100.0,
            "single EMA must not overshoot input, got {result}"
        );
    }

    // Period 1 => alpha = 1.0 => the EMA is the most recent sample with no smoothing at
    // all. This is the upper-alpha edge of the recurrence and a useful regression check
    // that the algorithm reduces to identity at period 1.
    #[test]
    #[allow(clippy::float_cmp)]
    fn ema_compute_period_one_is_latest_sample() {
        let samples = vec![10.0, 20.0, 30.0, 40.0];
        assert_eq!(CustomSensorsRepo::compute_ema(&samples, 1), Some(40.0));
    }

    // Order matters: reversing the sample order produces a different EMA because recent
    // samples weigh more. Guards against accidental order-invariance refactors that would
    // make EMA behave like TimeAverage.
    #[test]
    fn ema_compute_is_order_dependent() {
        let ascending = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let descending: Vec<f64> = ascending.iter().rev().copied().collect();
        let ema_ascending = CustomSensorsRepo::compute_ema(&ascending, 5).unwrap();
        let ema_descending = CustomSensorsRepo::compute_ema(&descending, 5).unwrap();
        assert!(
            (ema_ascending - ema_descending).abs() > 1.0,
            "EMA must be order-dependent: {ema_ascending} vs {ema_descending}"
        );
        // Ascending data: most recent sample is the highest, EMA pulls up toward it.
        // Descending data: most recent sample is the lowest, EMA pulls down toward it.
        assert!(ema_ascending > ema_descending);
    }

    // ==================== failsafe state tracking helpers ====================

    // note_failsafing_sensor returns true only on the first insert per sensor id.
    // Subsequent inserts of an existing key return false and must NOT overwrite the
    // stored reason: the reported reason stays the one from the logged transition.
    // The bool is what gates the once-per-occurrence warn log so the caller can
    // distinguish "newly entered failsafe" from "already failsafing".
    #[test]
    #[serial]
    fn note_failsafing_returns_true_only_first_time() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            assert!(repo.note_failsafing_sensor("sensor1", "file unreadable"));
            assert!(repo.note_failsafing_sensor("sensor1", "other reason").not());
            assert!(repo.note_failsafing_sensor("sensor1", "other reason").not());
            assert!(repo.note_failsafing_sensor("sensor2", "file unreadable"));
            let failsafing = repo.failsafing_sensors.borrow();
            assert_eq!(failsafing["sensor1"], "file unreadable");
            assert_eq!(failsafing["sensor2"], "file unreadable");
        });
    }

    // clear_failsafing_sensor returns true only when the sensor was actually in the
    // failsafing set. Calling clear on a sensor that is not failsafing is a no-op
    // and returns false, which is the gate the caller uses to skip the recovery
    // info log on the common (already-fine) path.
    #[test]
    #[serial]
    fn clear_failsafing_returns_true_only_when_was_failsafing() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            assert!(repo.clear_failsafing_sensor("sensor1").not());
            repo.note_failsafing_sensor("sensor1", "file unreadable");
            assert!(repo.clear_failsafing_sensor("sensor1"));
            assert!(repo.clear_failsafing_sensor("sensor1").not());
        });
    }

    // ==================== integration tests: failsafe substitution ====================

    /// Builds a mock source device whose `status_history` is filled with as many ticks as
    /// the repo's poll-rate-derived stack size: older positions are zeroed copies of the
    /// given temps (matching `Device::initialize_status_history_with`'s semantics) and the
    /// most recent tick carries the real values. Returns `(uid, device_lock)` ready to pass
    /// into `CustomSensorsRepo::new`'s `all_other_devices` argument.
    #[allow(clippy::cast_possible_truncation)]
    fn make_mock_source_device(temps: Vec<TempStatus>) -> (UID, DeviceLock) {
        let temp_infos: HashMap<TempName, TempInfo> = temps
            .iter()
            .enumerate()
            .map(|(i, t)| {
                (
                    t.name.clone(),
                    TempInfo {
                        label: t.name.clone(),
                        number: (i + 1) as u8,
                    },
                )
            })
            .collect();
        let mut device = Device::new(
            "MockSource".to_string(),
            DeviceType::Hwmon,
            1,
            None,
            DeviceInfo {
                temps: temp_infos,
                temp_min: 0,
                temp_max: 150,
                ..Default::default()
            },
            None,
            1.0,
        );
        let uid = device.uid.clone();
        // Match the repo's history length so backfill can read every tick.
        device.initialize_status_history_with(
            Status {
                temps,
                ..Default::default()
            },
            1.0,
        );
        (uid, Rc::new(RefCell::new(device)))
    }

    /// Reads the latest temp the Custom Sensors device is reporting for `cs_id`.
    fn current_temp_for(repo: &CustomSensorsRepo, cs_id: &str) -> f64 {
        let device = repo.custom_sensor_device.as_ref().unwrap().borrow();
        let status = device.status_current().unwrap();
        status
            .temps
            .iter()
            .find(|t| t.name == cs_id)
            .unwrap_or_else(|| panic!("temp '{cs_id}' not found in current status"))
            .temp
    }

    fn mix_sensor(id: &str, sources: Vec<CustomTempSourceData>) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            kind: CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Max,
                sources,
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    fn temp_source(uid: &str, name: &str) -> CustomTempSourceData {
        CustomTempSourceData {
            weight: 1,
            temp_source: TempSource {
                device_uid: uid.to_string(),
                temp_name: name.to_string(),
            },
        }
    }

    // Mix sensor pointing at a non-existent source device short-circuits to
    // MISSING_TEMP_FAILSAFE on the live tick (any-miss-fails-the-sensor per Q6c) and is
    // recorded in the failsafing-sensors set so the once-per-occurrence warn fires.
    // A missing source DEVICE is what production-relevant cases look like (device removed
    // mid-session); a present-device-with-missing-temp_name is rejected at backfill time
    // by process_custom_sensor_data_indexed and so cannot reach the live path.
    #[test]
    #[serial]
    fn mix_sensor_with_missing_source_emits_failsafe_on_live_tick() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            // No source device registered; the device_uid here is unknown to the repo.
            let sensor = mix_sensor(
                "mix1",
                vec![temp_source("nonexistent_device_uid", "any_temp")],
            );

            // Backfill skips a missing source device and falls through to the empty
            // temp_data dummy (0); set_custom_sensor therefore succeeds.
            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!((current_temp_for(&repo, "mix1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("mix1"));
        });
    }

    // Mix sensor with all sources present emits the real Max value and does not enter the
    // failsafing set. Regression check that the happy path is unaffected by the refactor.
    #[test]
    #[serial]
    fn mix_sensor_with_all_sources_present_emits_real_value() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![
                TempStatus {
                    name: "cpu".to_string(),
                    temp: 70.0,
                },
                TempStatus {
                    name: "gpu".to_string(),
                    temp: 60.0,
                },
            ]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = mix_sensor(
                "mix1",
                vec![
                    temp_source(&source_uid, "cpu"),
                    temp_source(&source_uid, "gpu"),
                ],
            );

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            // Max(70, 60) = 70
            assert!((current_temp_for(&repo, "mix1") - 70.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("mix1").not());
        });
    }

    // Mix-Delta with one missing source: today's old behavior would silently emit 0 (delta
    // over a one-element [70] = 0). The refactor short-circuits to failsafe so the
    // delta-driven control logic reacts. This is the case Q6c was specifically built for.
    // The second source uses a non-existent device_uid so backfill skips it (rather than
    // erroring on a present device with an unknown temp_name).
    #[test]
    #[serial]
    fn mix_delta_with_one_source_missing_emits_failsafe() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "delta1".to_string(),
                kind: CustomSensorKind::Mix {
                    mix_function: CustomSensorMixFunctionType::Delta,
                    sources: vec![
                        temp_source(&source_uid, "cpu"),
                        temp_source("nonexistent_device_uid", "anything"),
                    ],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!(
                (current_temp_for(&repo, "delta1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON
            );
            assert!(repo.failsafing_sensors.borrow().contains_key("delta1"));
        });
    }

    // Offset sensor with source missing: arithmetic substitution would emit
    // (100 + offset).clamp(0, 150) which is *not* the failsafe value. Short-circuit
    // ensures the actual MISSING_TEMP_FAILSAFE reaches the consuming control logic.
    #[test]
    #[serial]
    fn offset_sensor_with_source_missing_emits_failsafe() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "off1".to_string(),
                kind: CustomSensorKind::Offset {
                    offset: -25,
                    sources: vec![temp_source("nonexistent_device_uid", "any_temp")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            // -25 offset on the failsafe would have been 75 under arithmetic substitution;
            // short-circuit must produce exactly MISSING_TEMP_FAILSAFE.
            assert!((current_temp_for(&repo, "off1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("off1"));
        });
    }

    // TimeAverage with no collectible samples (source's history has no matching temp_name):
    // process_time_average_current's compute_time_average returns None and emits failsafe.
    #[test]
    #[serial]
    fn time_average_with_no_samples_emits_failsafe() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "actual".to_string(),
                temp: 50.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ta1".to_string(),
                kind: CustomSensorKind::TimeAverage {
                    time_window_seconds: 5,
                    sources: vec![temp_source(&source_uid, "missing")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!((current_temp_for(&repo, "ta1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("ta1"));
        });
    }

    // EMA with no collectible samples (source's history has no matching temp_name):
    // process_ema_current's compute_ema returns None and emits failsafe — same contract as
    // TimeAverage so a missing source produces a single warning rather than a misleading
    // value drifting through the EMA recurrence.
    #[test]
    #[serial]
    fn ema_with_no_samples_emits_failsafe() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "actual".to_string(),
                temp: 50.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ema1".to_string(),
                kind: CustomSensorKind::ExponentialMovingAvg {
                    time_window_seconds: 5,
                    sources: vec![temp_source(&source_uid, "missing")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!((current_temp_for(&repo, "ema1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("ema1"));
        });
    }

    // EMA with window_seconds == 0 is an invariant break (validator enforces 1..=300), but
    // the live path defensively emits failsafe and logs error per tick. Direct injection
    // bypasses the set_custom_sensor backfill which would also debug_assert on the zero.
    #[test]
    #[serial]
    fn ema_with_window_zero_emits_failsafe() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "actual".to_string(),
                temp: 50.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ema_bad".to_string(),
                kind: CustomSensorKind::ExponentialMovingAvg {
                    time_window_seconds: 0,
                    sources: vec![temp_source(&source_uid, "actual")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };
            repo.sensors.borrow_mut().push(sensor);

            repo.update_statuses().await.unwrap();

            assert!(
                (current_temp_for(&repo, "ema_bad") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON
            );
            assert!(repo.failsafing_sensors.borrow().contains_key("ema_bad"));
        });
    }

    // EMA with present source emits a bounded real value and does not enter failsafe.
    // make_mock_source_device's history is zeros except the most recent tick (carries the
    // 70.0); the EMA over those last 10 samples is therefore < 70 but > 0. Exact-value
    // correctness is covered by the compute_ema unit tests; this test only verifies the
    // live integration path runs cleanly on a healthy source.
    #[test]
    #[serial]
    fn ema_with_present_source_emits_real_value() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ema_ok".to_string(),
                kind: CustomSensorKind::ExponentialMovingAvg {
                    time_window_seconds: 10,
                    sources: vec![temp_source(&source_uid, "cpu")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            let temp = current_temp_for(&repo, "ema_ok");
            assert!(
                (temp - MISSING_TEMP_FAILSAFE).abs() > f64::EPSILON,
                "EMA must not failsafe on a present source, got {temp}"
            );
            assert!(
                (0.0..=70.0).contains(&temp),
                "EMA must be bounded by input range, got {temp}"
            );
            assert!(repo
                .failsafing_sensors
                .borrow()
                .contains_key("ema_ok")
                .not());
        });
    }

    // TimeAverage with window_seconds == 0 is an invariant break (validator enforces
    // 1..=300), but the live path defensively emits failsafe and logs error per tick.
    // Bypasses set_custom_sensor because that path's debug_assert on window_seconds >= 1
    // would panic on 0; we inject the sensor directly to exercise the live defensive path.
    #[test]
    #[serial]
    fn time_average_with_window_zero_emits_failsafe() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "actual".to_string(),
                temp: 50.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            // Direct injection bypasses validation and backfill; we only exercise the live
            // path here.
            let sensor = CustomSensor {
                id: "ta_bad".to_string(),
                kind: CustomSensorKind::TimeAverage {
                    time_window_seconds: 0,
                    sources: vec![temp_source(&source_uid, "actual")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };
            repo.sensors.borrow_mut().push(sensor);

            repo.update_statuses().await.unwrap();

            assert!(
                (current_temp_for(&repo, "ta_bad") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON
            );
            assert!(repo.failsafing_sensors.borrow().contains_key("ta_bad"));
        });
    }

    // File sensor entering failsafe and recovering: write valid file, run a live tick,
    // delete the file, exhaust the tolerance window (the held value is emitted until the
    // failure run passes MISSING_STATUS_THRESHOLD), then confirm failsafe + set entry, and
    // finally restore the file with a new value and confirm recovery.
    #[test]
    #[serial]
    fn file_sensor_recovers_from_failsafe() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();

            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = file_sensor("file1", test_file.clone());
            repo.set_custom_sensor(sensor).await.unwrap();

            // Tick 1: file readable, real value
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "file1") - 45.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("file1").not());

            // Remove the file. Reads inside the tolerance window hold the last good value
            // rather than commanding a fan curve to 100 C on one unlucky read.
            cc_fs::remove_file(&test_file).await.ok();
            for _ in 0..MISSING_STATUS_THRESHOLD {
                repo.update_statuses().await.unwrap();
                assert!((current_temp_for(&repo, "file1") - 45.0).abs() < f64::EPSILON);
                assert!(repo.failsafing_sensors.borrow().contains_key("file1").not());
            }

            // One more failure passes the threshold: now it failsafes, and the repository
            // must report the ref with the reason from the entry transition.
            repo.update_statuses().await.unwrap();
            assert!(
                (current_temp_for(&repo, "file1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON
            );
            assert!(repo.failsafing_sensors.borrow().contains_key("file1"));
            let refs = repo.failsafing();
            assert_eq!(refs.len(), 1);
            assert_eq!(refs[0].name, "file1");
            assert_eq!(refs[0].reason, "file unreadable");

            // Restore the file with a new value: next tick recovers.
            cc_fs::write(&test_file, b"55000".to_vec()).await.unwrap();
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "file1") - 55.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("file1").not());
        });
    }

    // Goal: the reporter's actual symptom. Their script rewrites the file, so reads land
    // mid-truncate and each one used to command 100 C for a tick. Method: one good tick,
    // one failed read, assert the held value is emitted and no failsafe is entered.
    #[test]
    #[serial]
    fn file_sensor_holds_last_good_value_on_transient_failure() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(file_sensor("file1", test_file.clone()))
                .await
                .unwrap();
            repo.update_statuses().await.unwrap();

            // A truncated file reads as unparseable, exactly like the mid-write race.
            cc_fs::write(&test_file, Vec::new()).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!((current_temp_for(&repo, "file1") - 45.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("file1").not());
        });
    }

    // Goal: negative space for the tolerance window. A sensor that has never read
    // successfully has nothing to hold, so it must failsafe on its first failed tick
    // instead of reporting a stale-but-plausible value it never had.
    #[test]
    #[serial]
    fn file_sensor_failsafes_immediately_when_never_read_successfully() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            // set_custom_sensor validates the file, so it must exist here. Removing it
            // before the first live tick means no tick ever recorded a good value.
            repo.set_custom_sensor(file_sensor("file1", test_file.clone()))
                .await
                .unwrap();
            cc_fs::remove_file(&test_file).await.ok();

            repo.update_statuses().await.unwrap();

            assert!(
                (current_temp_for(&repo, "file1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON
            );
            assert!(repo.failsafing_sensors.borrow().contains_key("file1"));
        });
    }

    // Goal: the window is a run of CONSECUTIVE failures, not a lifetime budget. A flapping
    // writer must not eventually exhaust it. Method: fail almost to the threshold, succeed
    // once, then fail again and assert the held value is still emitted.
    #[test]
    #[serial]
    fn file_sensor_failure_run_resets_after_a_good_read() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(file_sensor("file1", test_file.clone()))
                .await
                .unwrap();
            repo.update_statuses().await.unwrap();

            for _ in 0..MISSING_STATUS_THRESHOLD {
                cc_fs::write(&test_file, Vec::new()).await.unwrap();
                repo.update_statuses().await.unwrap();
            }
            // A good read clears the run.
            cc_fs::write(&test_file, b"55000".to_vec()).await.unwrap();
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "file1") - 55.0).abs() < f64::EPSILON);

            // The very next failure is therefore the first of a fresh run, not the ninth.
            cc_fs::write(&test_file, Vec::new()).await.unwrap();
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "file1") - 55.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains_key("file1").not());
        });
    }

    // delete_custom_sensor must drop the sensor's id from failsafing_sensors so a future
    // sensor reusing the same id starts fresh and its first failsafe entry logs cleanly.
    #[test]
    #[serial]
    fn delete_custom_sensor_clears_failsafing_state() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = file_sensor("to_delete", test_file);
            repo.set_custom_sensor(sensor).await.unwrap();
            // Plant a failsafing-state entry directly to simulate the sensor having entered
            // failsafe at some prior point.
            repo.note_failsafing_sensor("to_delete", "file unreadable");
            assert!(repo.failsafing_sensors.borrow().contains_key("to_delete"));

            repo.delete_custom_sensor("to_delete").unwrap();

            assert!(repo
                .failsafing_sensors
                .borrow()
                .contains_key("to_delete")
                .not());
        });
    }

    // update_custom_sensor must drop the prior failsafing-state entry so a reconfigured
    // sensor starts fresh; if it failsafes on the next tick the warn log fires for the
    // newly configured cause rather than being suppressed by the stale flag.
    #[test]
    #[serial]
    fn update_custom_sensor_clears_failsafing_state() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = file_sensor("to_update", test_file.clone());
            repo.set_custom_sensor(sensor.clone()).await.unwrap();
            repo.note_failsafing_sensor("to_update", "file unreadable");
            assert!(repo.failsafing_sensors.borrow().contains_key("to_update"));

            // Update with the same content; the cleanup hook must drop the failsafing entry.
            repo.update_custom_sensor(sensor).await.unwrap();

            assert!(repo
                .failsafing_sensors
                .borrow()
                .contains_key("to_update")
                .not());
        });
    }

    // Backfill of a brand-new Mix sensor with a missing source must keep emitting 0 in
    // status_history (the historical placeholder), not MISSING_TEMP_FAILSAFE. Backfill is
    // out of scope for the safety contract (Q2a) and substituting failsafe in history would
    // create phantom 100°C spikes at sensor-creation time on charts. Uses a non-existent
    // device_uid so backfill skips the source and falls through to the empty-temp_data
    // dummy push (0); a present-device-with-missing-temp_name would error at backfill.
    #[test]
    #[serial]
    fn backfill_with_missing_source_uses_zero_not_failsafe() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = mix_sensor(
                "mix_bf",
                vec![temp_source("nonexistent_device_uid", "any_temp")],
            );

            // set_custom_sensor only does backfill via process_custom_sensor_data_indexed.
            // We do NOT call update_statuses here, so the live path is never exercised.
            repo.set_custom_sensor(sensor).await.unwrap();

            // status_history entries for mix_bf (all backfilled) must be 0, not failsafe.
            // The sensor must NOT be in the failsafing set: backfill bypasses emit_failsafe.
            let device = repo.custom_sensor_device.as_ref().unwrap().borrow();
            for status in device.status_history.iter() {
                let entry = status
                    .temps
                    .iter()
                    .find(|t| t.name == "mix_bf")
                    .expect("mix_bf temp absent from history");
                assert!(
                    entry.temp.abs() < f64::EPSILON,
                    "backfill must emit 0., got {}",
                    entry.temp
                );
            }
            assert!(repo
                .failsafing_sensors
                .borrow()
                .contains_key("mix_bf")
                .not());
        });
    }

    // ============== integration tests: windowed sensors (TimeAverage / EMA) ==============

    fn time_average_sensor(
        id: &str,
        window_seconds: u16,
        source: CustomTempSourceData,
    ) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            kind: CustomSensorKind::TimeAverage {
                time_window_seconds: window_seconds,
                sources: vec![source],
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    fn ema_sensor(id: &str, window_seconds: u16, source: CustomTempSourceData) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            kind: CustomSensorKind::ExponentialMovingAvg {
                time_window_seconds: window_seconds,
                sources: vec![source],
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    /// Pushes one new tick onto the mock source device: `Some` publishes the temp, `None`
    /// publishes a status without it (a gap tick).
    fn push_source_tick(source_dev: &DeviceLock, temp_name: &str, temp: Option<f64>) {
        let temps = temp.map_or_else(Vec::new, |t| {
            vec![TempStatus {
                name: temp_name.to_string(),
                temp: t,
            }]
        });
        source_dev.borrow_mut().set_status(Status {
            temps,
            ..Default::default()
        });
    }

    /// The pre-window algorithm: a full newest-first rescan of the device's history. Used
    /// as the bit-exact reference the rolling windows must reproduce every tick.
    fn reference_windowed(
        device: &DeviceLock,
        temp_name: &str,
        sample_count: usize,
        is_ema: bool,
    ) -> Option<f64> {
        let device = device.borrow();
        let mut temps: Vec<f64> = device
            .status_history
            .iter()
            .rev()
            .take(sample_count)
            .filter_map(|status| {
                status
                    .temps
                    .iter()
                    .find(|t| t.name == temp_name)
                    .map(|t| t.temp)
            })
            .collect();
        if is_ema {
            temps.reverse();
            CustomSensorsRepo::compute_ema(&temps, sample_count)
        } else {
            CustomSensorsRepo::compute_time_average(&temps)
        }
    }

    // The rolling windows must reproduce the old full-rescan outputs bit-for-bit, for
    // external and child sources alike. Drives ticks with a varied temp stream at window
    // sizes 1, 5, and 300 (poll rate 1.0, so sample_count == window_seconds) and compares
    // every tick's TimeAverage and EMA emissions, via to_bits, against the pre-window
    // algorithm re-run over the same history. Child sources go through a Mix child so the
    // parent windowed sensors read this tick's custom_temps; their post-tick reference is
    // the Custom Sensors device's own history, whose newest entry is the current tick.
    #[test]
    #[serial]
    fn windowed_output_matches_from_scratch_reference() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            let cs_uid = repo.get_device_uid();
            let windows: [(&str, u16); 3] = [("w1", 1), ("w5", 5), ("w300", 300)];
            for (label, window) in windows {
                repo.set_custom_sensor(time_average_sensor(
                    &format!("avg_{label}"),
                    window,
                    temp_source(&source_uid, "cpu"),
                ))
                .await
                .unwrap();
                repo.set_custom_sensor(ema_sensor(
                    &format!("ema_{label}"),
                    window,
                    temp_source(&source_uid, "cpu"),
                ))
                .await
                .unwrap();
            }
            repo.set_custom_sensor(mix_sensor("mix1", vec![temp_source(&source_uid, "cpu")]))
                .await
                .unwrap();
            repo.set_custom_sensor(time_average_sensor(
                "avg_child",
                4,
                temp_source(&cs_uid, "mix1"),
            ))
            .await
            .unwrap();
            repo.set_custom_sensor(ema_sensor("ema_child", 4, temp_source(&cs_uid, "mix1")))
                .await
                .unwrap();
            let stream = [
                70.0, 71.5, 69.2, 75.0, 74.3, 73.1, 72.8, 70.4, 76.9, 74.2, 68.0, 71.1,
            ];
            for temp in stream {
                push_source_tick(&source_dev, "cpu", Some(temp));
                repo.update_statuses().await.unwrap();
                for (label, window) in windows {
                    let sample_count = usize::from(window);
                    let avg_ref =
                        reference_windowed(&source_dev, "cpu", sample_count, false).unwrap();
                    let ema_ref =
                        reference_windowed(&source_dev, "cpu", sample_count, true).unwrap();
                    assert_eq!(
                        current_temp_for(&repo, &format!("avg_{label}")).to_bits(),
                        avg_ref.to_bits(),
                        "TimeAverage {label} diverged from reference"
                    );
                    assert_eq!(
                        current_temp_for(&repo, &format!("ema_{label}")).to_bits(),
                        ema_ref.to_bits(),
                        "EMA {label} diverged from reference"
                    );
                }
                let cs_device = repo.custom_sensor_device.as_ref().unwrap();
                let child_avg_ref = reference_windowed(cs_device, "mix1", 4, false).unwrap();
                let child_ema_ref = reference_windowed(cs_device, "mix1", 4, true).unwrap();
                assert_eq!(
                    current_temp_for(&repo, "avg_child").to_bits(),
                    child_avg_ref.to_bits(),
                    "child-source TimeAverage diverged from reference"
                );
                assert_eq!(
                    current_temp_for(&repo, "ema_child").to_bits(),
                    child_ema_ref.to_bits(),
                    "child-source EMA diverged from reference"
                );
            }
        });
    }

    // A tick where the source publishes no reading must shrink the effective window
    // (matching the old skip-absent walk), never inject a sentinel; a fully absent window
    // must failsafe. Interleaves gap ticks with real ones asserting reference equality,
    // then drains the window with gaps and asserts MISSING_TEMP_FAILSAFE plus the recorded
    // reason.
    #[test]
    #[serial]
    fn gap_ticks_are_holes_not_sentinels() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(time_average_sensor(
                "avg",
                5,
                temp_source(&source_uid, "cpu"),
            ))
            .await
            .unwrap();
            repo.set_custom_sensor(ema_sensor("ema", 5, temp_source(&source_uid, "cpu")))
                .await
                .unwrap();
            let stream = [
                Some(70.0),
                Some(71.5),
                None,
                Some(75.0),
                None,
                None,
                Some(72.0),
            ];
            for temp in stream {
                push_source_tick(&source_dev, "cpu", temp);
                repo.update_statuses().await.unwrap();
                let avg_ref = reference_windowed(&source_dev, "cpu", 5, false).unwrap();
                let ema_ref = reference_windowed(&source_dev, "cpu", 5, true).unwrap();
                assert_eq!(current_temp_for(&repo, "avg").to_bits(), avg_ref.to_bits());
                assert_eq!(current_temp_for(&repo, "ema").to_bits(), ema_ref.to_bits());
            }
            // Drain the whole window with gaps: all-None slots must failsafe.
            for _ in 0..5 {
                push_source_tick(&source_dev, "cpu", None);
                repo.update_statuses().await.unwrap();
            }
            for id in ["avg", "ema"] {
                assert!(
                    (current_temp_for(&repo, id) - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON,
                    "{id} must failsafe on an all-gap window"
                );
                assert_eq!(
                    repo.failsafing_sensors.borrow().get(id).map(String::as_str),
                    Some("no source samples available")
                );
            }
        });
    }

    // Changing a sensor's window must rebuild its state from history so the next tick is
    // output-identical to a from-scratch computation at the new size.
    #[test]
    #[serial]
    fn window_change_reseeds_from_history() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(time_average_sensor(
                "avg",
                5,
                temp_source(&source_uid, "cpu"),
            ))
            .await
            .unwrap();
            for temp in [70.0, 72.0, 74.0, 76.0] {
                push_source_tick(&source_dev, "cpu", Some(temp));
                repo.update_statuses().await.unwrap();
            }
            repo.update_custom_sensor(time_average_sensor(
                "avg",
                3,
                temp_source(&source_uid, "cpu"),
            ))
            .await
            .unwrap();
            assert!(repo.sample_windows.borrow().contains_key("avg").not());
            push_source_tick(&source_dev, "cpu", Some(78.0));
            repo.update_statuses().await.unwrap();
            let avg_ref = reference_windowed(&source_dev, "cpu", 3, false).unwrap();
            assert_eq!(current_temp_for(&repo, "avg").to_bits(), avg_ref.to_bits());
            assert_eq!(
                repo.sample_windows
                    .borrow()
                    .get("avg")
                    .map(|w| w.sample_count),
                Some(3)
            );
        });
    }

    // A removed (unknown) source device must failsafe immediately and drop any window
    // state, so a returning device reseeds instead of averaging stale samples.
    #[test]
    #[serial]
    fn removed_source_device_failsafes_immediately_and_drops_state() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![], test_overrides()).unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(time_average_sensor(
                "avg",
                5,
                temp_source("nonexistent_device_uid", "any_temp"),
            ))
            .await
            .unwrap();
            // Pre-plant stale state to prove the removal branch drops it.
            repo.sample_windows.borrow_mut().insert(
                "avg".to_string(),
                SampleWindow {
                    samples: VecDeque::from([Some(50.0)]),
                    sample_count: 5,
                },
            );
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "avg") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.sample_windows.borrow().contains_key("avg").not());
            assert!(repo.failsafing_sensors.borrow().contains_key("avg"));
        });
    }

    // Invalidation must always be output-safe: clearing all window state mid-stream and
    // continuing must still match the from-scratch reference on the very next tick.
    #[test]
    #[serial]
    fn seed_equals_steady_state() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(ema_sensor("ema", 5, temp_source(&source_uid, "cpu")))
                .await
                .unwrap();
            for temp in [70.0, 71.0, 72.0, 73.0, 74.0, 75.0] {
                push_source_tick(&source_dev, "cpu", Some(temp));
                repo.update_statuses().await.unwrap();
            }
            repo.sample_windows.borrow_mut().clear();
            push_source_tick(&source_dev, "cpu", Some(69.5));
            repo.update_statuses().await.unwrap();
            let ema_ref = reference_windowed(&source_dev, "cpu", 5, true).unwrap();
            assert_eq!(current_temp_for(&repo, "ema").to_bits(), ema_ref.to_bits());
        });
    }

    // The order endpoint replaces sensor bodies without validation, so any cached
    // window may describe a stale source; the replace must drop all window state.
    #[test]
    #[serial]
    fn order_replace_clears_windows() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = time_average_sensor("avg", 5, temp_source(&source_uid, "cpu"));
            repo.set_custom_sensor(sensor.clone()).await.unwrap();
            push_source_tick(&source_dev, "cpu", Some(70.0));
            repo.update_statuses().await.unwrap();
            assert!(repo.sample_windows.borrow().is_empty().not());
            repo.set_custom_sensors_order(&[sensor]).unwrap();
            assert!(repo.sample_windows.borrow().is_empty());
        });
    }

    // Sleep must clear the windows: histories get zeroed on wake, so held-over samples
    // would be wrong. The next tick reseeds from the (zeroed) history.
    #[test]
    #[serial]
    fn prepare_for_sleep_clears_windows() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(time_average_sensor(
                "avg",
                5,
                temp_source(&source_uid, "cpu"),
            ))
            .await
            .unwrap();
            push_source_tick(&source_dev, "cpu", Some(70.0));
            repo.update_statuses().await.unwrap();
            assert!(repo.sample_windows.borrow().is_empty().not());
            repo.prepare_for_sleep().await;
            assert!(repo.sample_windows.borrow().is_empty());
        });
    }

    // The window length is bounded by its sample_count on every tick of a long run
    // (backstop for the push/evict logic; the code also debug_asserts it).
    #[test]
    #[serial]
    fn window_len_never_exceeds_sample_count() {
        cc_fs::test_runtime(async {
            let (source_uid, source_dev) = make_mock_source_device(vec![TempStatus {
                name: "cpu".to_string(),
                temp: 70.0,
            }]);
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo =
                CustomSensorsRepo::new(test_config, vec![source_dev.clone()], test_overrides())
                    .unwrap();
            repo.initialize_devices().await.unwrap();
            repo.set_custom_sensor(time_average_sensor(
                "avg",
                5,
                temp_source(&source_uid, "cpu"),
            ))
            .await
            .unwrap();
            for tick in 0..15 {
                push_source_tick(&source_dev, "cpu", Some(70.0 + f64::from(tick)));
                repo.update_statuses().await.unwrap();
                let windows = repo.sample_windows.borrow();
                let window = windows.get("avg").unwrap();
                assert!(window.samples.len() <= window.sample_count);
            }
        });
    }
}
