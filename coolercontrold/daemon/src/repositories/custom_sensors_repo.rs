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

use anyhow::{anyhow, Error, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace, warn};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::string::ToString;
use std::sync::Arc;
use tokio::time::Instant;

use crate::api::CCError;
use crate::config::Config;
use crate::device::{
    Device, DeviceInfo, DeviceType, DriverInfo, DriverType, Status, Temp, TempInfo, TempName,
    TempStatus, UID,
};
use crate::repositories::failsafe::MISSING_TEMP_FAILSAFE;
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{
    CustomSensor, CustomSensorMixFunctionType, CustomTempSourceData, LcdSettings, LightingSettings,
    Offset, SensorKind, TempSource,
};
use crate::{cc_fs, VERSION};

const MAX_CUSTOM_SENSOR_FILE_SIZE_BYTES: usize = 15;

type CustomSensors = RefCell<Vec<CustomSensor>>;
type Relationships = RefCell<HashMap<ChildName, Vec<ParentName>>>;
type ChildName = TempName;
type ParentName = TempName;

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
    /// Set of sensor IDs currently emitting `MISSING_TEMP_FAILSAFE` because their source is
    /// structurally absent (device removed, temp renamed, history empty, child not yet
    /// processed). Membership drives once-per-occurrence entry / recovery logging so a flaky
    /// or misconfigured source does not flood the log every poll.
    failsafing_sensors: RefCell<HashSet<String>>,
}

impl CustomSensorsRepo {
    pub fn new(config: Rc<Config>, all_other_devices: DeviceList) -> Result<Self> {
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
            failsafing_sensors: RefCell::new(HashSet::new()),
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
        Ok(())
    }

    pub async fn set_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        self.verify_sensor_relationships(&custom_sensor)?;
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
        if let SensorKind::File { file_path } = &custom_sensor.kind {
            // Make sure the file exists and temp is properly formatted
            Self::get_custom_sensor_file_temp(file_path).await?;
        }
        // A reconfigured sensor starts fresh: drop any prior failsafing state so the
        // first tick after the update logs a transition cleanly if it failsafes again.
        self.failsafing_sensors
            .borrow_mut()
            .remove(&custom_sensor.id);
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
        if let Some(parents) = self.relationships.borrow().get(custom_sensor_id) {
            for parent_name in parents {
                if let Some(parent_sensor) = self
                    .sensors
                    .borrow_mut()
                    .iter_mut()
                    .find(|s| &s.id == parent_name)
                {
                    if parent_sensor.children.len() < 2 {
                        return Err(CCError::UserError {
                            msg: format!(
                                "Parent sensor {parent_name} for Custom Sensor {custom_sensor_id} \
                                only has this one child. The parent must first be deleted before \
                                deleting this Custom Sensor."
                            ),
                        }
                        .into());
                    }
                    parent_sensor.children.retain(|c| c != custom_sensor_id);
                    if let Some(sources) = parent_sensor.sources_mut() {
                        sources.retain(|s| {
                            s.temp_source.device_uid != self.device_uid
                                && s.temp_source.temp_name != custom_sensor_id
                        });
                    }
                } else {
                    return Err(CCError::InternalError {
                        msg: format!("Parent sensor {parent_name} for Custom Sensor {custom_sensor_id} not found"),
                    }
                    .into());
                }
            }
        }
        self.config.delete_custom_sensor(custom_sensor_id)?;
        Self::remove_status_history_for_sensor(self, custom_sensor_id);
        self.sensors
            .borrow_mut()
            .retain(|cs| cs.id != custom_sensor_id);
        // Drop any failsafing state for the deleted sensor so a future sensor reusing the
        // same id (rare but legal) does not silently inherit "currently failsafing".
        self.failsafing_sensors
            .borrow_mut()
            .remove(custom_sensor_id);
        self.update_device_info_temps();
        self.reconstruct_relationships();
        Ok(())
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
            SensorKind::Mix {
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
            SensorKind::Offset { offset, sources } => {
                for (index, status) in history.iter_mut().enumerate() {
                    let temp_status =
                        self.process_reduced_indexed(&sensor.id, sources, index, |data| {
                            Self::process_offset_temp_data(*offset, data)
                        })?;
                    status.temps.push(temp_status);
                }
            }
            SensorKind::TimeAverage {
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
            SensorKind::ExponentialMovingAvg {
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
            SensorKind::File { file_path } => {
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
                let reason = format!(
                    "source missing: {}:{}",
                    temp_source.device_uid, temp_source.temp_name
                );
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

    /// Processes a `TimeAverage` Custom Sensor for the current tick. Reads the last
    /// `window_seconds / poll_rate` samples from the source's `status_history` and emits
    /// their arithmetic mean. If the source is structurally absent (no samples collectible),
    /// emits `MISSING_TEMP_FAILSAFE` so downstream control reacts to the missing reading
    /// rather than serving a fake-cool value.
    fn process_time_average_current(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        window_seconds: u16,
        custom_temps: &[TempStatus],
    ) -> TempStatus {
        if window_seconds == 0 {
            // Invariant break (validation enforces 1..=300, and window_sample_count
            // debug_asserts >= 1). Failsafe rather than emit a value from a degenerate window.
            return self.emit_failsafe(id, "invalid zero time_window_seconds");
        }
        let sample_count = Self::window_sample_count(window_seconds, self.poll_rate);
        let temps =
            self.collect_recent_source_temps(&source.temp_source, custom_temps, sample_count);
        match Self::compute_time_average(&temps) {
            Some(mean) => self.emit_real_temp(id, mean),
            None => self.emit_failsafe(id, "no source samples available"),
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

    /// How many samples fit in `window_seconds` at the current `poll_rate`. Always at least 1.
    /// Pure helper so it's directly testable without setting up a Repo.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn window_sample_count(window_seconds: u16, poll_rate: f64) -> usize {
        debug_assert!(window_seconds >= 1);
        debug_assert!(poll_rate > 0.0);
        let count = (f64::from(window_seconds) / poll_rate).ceil() as usize;
        count.max(1)
    }

    /// Collects up to `n` most-recent temperature samples for `temp_source`. For external
    /// sources we read straight from the source device's `status_history`. For child sources
    /// (within the Custom Sensors device) the current tick is in `custom_temps` and the prior
    /// `n - 1` ticks are in the Custom Sensors device's history.
    fn collect_recent_source_temps(
        &self,
        temp_source: &TempSource,
        custom_temps: &[TempStatus],
        sample_count: usize,
    ) -> Vec<Temp> {
        let mut temps: Vec<Temp> = Vec::with_capacity(sample_count);
        if temp_source.device_uid == self.device_uid {
            if let Some(current) = custom_temps
                .iter()
                .find(|t| t.name == temp_source.temp_name)
            {
                temps.push(current.temp);
            }
            if let Some(cs_device) = self.custom_sensor_device.as_ref() {
                let remaining = sample_count.saturating_sub(temps.len());
                for status in cs_device
                    .borrow()
                    .status_history
                    .iter()
                    .rev()
                    .take(remaining)
                {
                    if let Some(t) = Self::get_temp_from_status(&temp_source.temp_name, status) {
                        temps.push(t);
                    }
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
                if let Some(t) = Self::get_temp_from_status(&temp_source.temp_name, status) {
                    temps.push(t);
                }
            }
        }
        temps
    }

    /// Returns the arithmetic mean of the slice, or `None` if it's empty.
    /// Pure function — callers compose their own sample collection.
    #[allow(clippy::cast_precision_loss)]
    fn compute_time_average(samples: &[Temp]) -> Option<Temp> {
        if samples.is_empty() {
            return None;
        }
        let sum: Temp = samples.iter().sum();
        Some(sum / samples.len() as Temp)
    }

    /// Computes a single Exponential Moving Average over the samples (oldest first, newest
    /// last). `alpha = 2 / (period + 1)`, the standard EMA smoothing factor for an
    /// equivalent simple-moving-average window of `period` samples. The EMA is initialized
    /// with the first sample and iteratively updated. Returns `None` if `samples` is empty.
    /// Pure function — callers compose their own sample collection and ordering.
    #[allow(clippy::cast_precision_loss)]
    fn compute_ema(samples: &[Temp], period: usize) -> Option<Temp> {
        if samples.is_empty() {
            return None;
        }
        debug_assert!(period >= 1);
        let alpha = 2.0 / (period as f64 + 1.0);
        debug_assert!(alpha > 0.0);
        debug_assert!(alpha <= 1.0);
        let mut ema = samples[0];
        for &value in samples.iter().skip(1) {
            ema = (value - ema).mul_add(alpha, ema);
        }
        Some(ema)
    }

    /// Processes an `ExponentialMovingAvg` Custom Sensor for the current tick. Reads the last
    /// `window_seconds / poll_rate` samples from the source's `status_history` (oldest first)
    /// and emits a single EMA over them. If the source is structurally absent, emits
    /// `MISSING_TEMP_FAILSAFE` so downstream control reacts to the missing reading.
    fn process_ema_current(
        &self,
        id: &TempName,
        source: &CustomTempSourceData,
        window_seconds: u16,
        custom_temps: &[TempStatus],
    ) -> TempStatus {
        if window_seconds == 0 {
            // Invariant break (validation enforces 1..=300, and window_sample_count
            // debug_asserts >= 1). Failsafe rather than emit a value from a degenerate window.
            return self.emit_failsafe(id, "invalid zero time_window_seconds");
        }
        let sample_count = Self::window_sample_count(window_seconds, self.poll_rate);
        let mut temps =
            self.collect_recent_source_temps(&source.temp_source, custom_temps, sample_count);
        // collect_recent_source_temps returns newest-first; EMA is order-dependent and
        // expects oldest-first so the recency weighting works correctly.
        temps.reverse();
        match Self::compute_ema(&temps, sample_count) {
            Some(ema) => self.emit_real_temp(id, ema),
            None => self.emit_failsafe(id, "no source samples available"),
        }
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

    /// Reads the current temp for a File-type Custom Sensor. On unreadable / malformed file,
    /// emits `MISSING_TEMP_FAILSAFE` (and a once-per-occurrence warn log), so a fan curve
    /// driven by the file sensor reacts to the lost source rather than a fake-cool value.
    /// Live-tick path only; backfill reads `get_custom_sensor_file_temp` directly so it
    /// does not interact with the failsafing-state set.
    async fn process_custom_sensor_data_file_current(
        &self,
        id: &TempName,
        file_path: &Path,
    ) -> TempStatus {
        match Self::get_custom_sensor_file_temp(file_path).await {
            Ok(temp) => self.emit_real_temp(id, temp),
            Err(_) => self.emit_failsafe(id, "file unreadable"),
        }
    }

    async fn get_custom_sensor_file_temp(file_path: &Path) -> Result<f64> {
        cc_fs::read_sysfs(file_path)
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

    fn verify_file_size(content: String) -> Result<String> {
        if content.len() > MAX_CUSTOM_SENSOR_FILE_SIZE_BYTES {
            Err(CCError::UserError {
                msg: format!(
                    "File size too large: {:?} bytes. Max allowed: {:?} bytes",
                    content.len(),
                    MAX_CUSTOM_SENSOR_FILE_SIZE_BYTES
                ),
            }
            .into())
        } else {
            Ok(content)
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    fn verify_i32(content: String) -> Result<i32> {
        content.trim().parse::<i32>().map_err(|err| {
            CCError::UserError {
                msg: format!("{err}"),
            }
            .into()
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

    /// Inserts `sensor_id` into the failsafing-sensors set. Returns `true` if this is the
    /// first time this sensor entered failsafe (caller should emit a `warn!` line); `false`
    /// if it was already failsafing on a previous tick.
    fn note_failsafing_sensor(&self, sensor_id: &str) -> bool {
        self.failsafing_sensors
            .borrow_mut()
            .insert(sensor_id.to_string())
    }

    /// Removes `sensor_id` from the failsafing-sensors set. Returns `true` if it was present
    /// (caller should emit a recovery `info!` line); `false` if it was not failsafing.
    fn clear_failsafing_sensor(&self, sensor_id: &str) -> bool {
        self.failsafing_sensors.borrow_mut().remove(sensor_id)
    }

    /// Builds the failsafe `TempStatus` and emits the entry log line on the first occurrence.
    /// `reason` is included in the log so a future operator can tell the cs_type-specific
    /// cause apart (for example "all sources missing" vs "file unreadable").
    fn emit_failsafe(&self, sensor_id: &str, reason: &str) -> TempStatus {
        if self.note_failsafing_sensor(sensor_id) {
            warn!(
                "Custom Sensor {sensor_id} entering failsafe ({MISSING_TEMP_FAILSAFE}°C): {reason}"
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
            info!("Custom Sensor {sensor_id} recovered from failsafe");
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

    async fn update_statuses(&self) -> Result<()> {
        if self.custom_sensor_device.is_none() {
            return Ok(());
        }
        let start_update = Instant::now();
        let mut custom_temps = Vec::new();
        let mut file_sensors: Vec<(TempName, PathBuf)> = Vec::new();
        // process children and standalone sensors first
        self.sensors
            .borrow()
            .iter()
            .filter(|s| s.children.is_empty()) // not parents
            .for_each(|sensor| match &sensor.kind {
                SensorKind::Mix {
                    mix_function,
                    sources,
                } => {
                    let temp_status = self.process_reduced_current(
                        &sensor.id,
                        sources,
                        &custom_temps,
                        |data| Self::process_temp_data(mix_function, data),
                    );
                    custom_temps.push(temp_status);
                }
                SensorKind::Offset { offset, sources } => {
                    let temp_status = self.process_reduced_current(
                        &sensor.id,
                        sources,
                        &custom_temps,
                        |data| Self::process_offset_temp_data(*offset, data),
                    );
                    custom_temps.push(temp_status);
                }
                SensorKind::File { file_path } => {
                    // Clone into owned data to avoid holding the sensors borrow over the await.
                    file_sensors.push((sensor.id.clone(), file_path.clone()));
                }
                SensorKind::TimeAverage {
                    time_window_seconds,
                    sources,
                } => {
                    let temp_status = self.process_time_average_current(
                        &sensor.id,
                        &sources[0],
                        *time_window_seconds,
                        &custom_temps,
                    );
                    custom_temps.push(temp_status);
                }
                SensorKind::ExponentialMovingAvg {
                    time_window_seconds,
                    sources,
                } => {
                    let temp_status = self.process_ema_current(
                        &sensor.id,
                        &sources[0],
                        *time_window_seconds,
                        &custom_temps,
                    );
                    custom_temps.push(temp_status);
                }
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
            .for_each(|sensor| match &sensor.kind {
                SensorKind::Mix {
                    mix_function,
                    sources,
                } => {
                    let temp_status = self.process_reduced_current(
                        &sensor.id,
                        sources,
                        &custom_temps,
                        |data| Self::process_temp_data(mix_function, data),
                    );
                    custom_temps.push(temp_status);
                }
                SensorKind::Offset { offset, sources } => {
                    let temp_status = self.process_reduced_current(
                        &sensor.id,
                        sources,
                        &custom_temps,
                        |data| Self::process_offset_temp_data(*offset, data),
                    );
                    custom_temps.push(temp_status);
                }
                // Parent sensors cannot be File types.
                SensorKind::File { .. } => {}
                SensorKind::TimeAverage {
                    time_window_seconds,
                    sources,
                } => {
                    let temp_status = self.process_time_average_current(
                        &sensor.id,
                        &sources[0],
                        *time_window_seconds,
                        &custom_temps,
                    );
                    custom_temps.push(temp_status);
                }
                SensorKind::ExponentialMovingAvg {
                    time_window_seconds,
                    sources,
                } => {
                    let temp_status = self.process_ema_current(
                        &sensor.id,
                        &sources[0],
                        *time_window_seconds,
                        &custom_temps,
                    );
                    custom_temps.push(temp_status);
                }
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
    use crate::repositories::custom_sensors_repo::{CustomSensorsRepo, TempData};
    use crate::repositories::failsafe::MISSING_TEMP_FAILSAFE;
    use crate::repositories::repository::{DeviceLock, Repository};
    use crate::setting::{
        CustomSensor, CustomSensorMixFunctionType, CustomTempSourceData, SensorKind, TempSource,
    };
    use serial_test::serial;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::ops::Not;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    fn file_sensor(id: &str, file_path: PathBuf) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            kind: SensorKind::File { file_path },
            children: Vec::new(),
            parents: Vec::new(),
        }
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
            let repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();

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
            let repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();

            // when:
            let temp = repo
                .process_custom_sensor_data_file_current(&cs_name, &test_file)
                .await;

            // then:
            assert_eq!(temp.name, cs_name);
            assert!((temp.temp - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains(&cs_name));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
    fn test_verify_relationship_child_multiple_parents() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
        });
    }

    #[test]
    #[serial]
    fn test_delete_cannot_if_only_child() {
        cc_fs::test_runtime(async {
            // given:
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
    // Mirrors HashSet::insert semantics: subsequent inserts of an existing key return
    // false. The bool is what gates the once-per-occurrence warn log so the caller
    // can distinguish "newly entered failsafe" from "already failsafing".
    #[test]
    #[serial]
    fn note_failsafing_returns_true_only_first_time() {
        cc_fs::test_runtime(async {
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
            assert!(repo.note_failsafing_sensor("sensor1"));
            assert!(repo.note_failsafing_sensor("sensor1").not());
            assert!(repo.note_failsafing_sensor("sensor1").not());
            assert!(repo.note_failsafing_sensor("sensor2"));
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
            let repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
            assert!(repo.clear_failsafing_sensor("sensor1").not());
            repo.note_failsafing_sensor("sensor1");
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
            kind: SensorKind::Mix {
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            assert!(repo.failsafing_sensors.borrow().contains("mix1"));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
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
            assert!(repo.failsafing_sensors.borrow().contains("mix1").not());
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "delta1".to_string(),
                kind: SensorKind::Mix {
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
            assert!(repo.failsafing_sensors.borrow().contains("delta1"));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "off1".to_string(),
                kind: SensorKind::Offset {
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
            assert!(repo.failsafing_sensors.borrow().contains("off1"));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ta1".to_string(),
                kind: SensorKind::TimeAverage {
                    time_window_seconds: 5,
                    sources: vec![temp_source(&source_uid, "missing")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!((current_temp_for(&repo, "ta1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains("ta1"));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ema1".to_string(),
                kind: SensorKind::ExponentialMovingAvg {
                    time_window_seconds: 5,
                    sources: vec![temp_source(&source_uid, "missing")],
                },
                children: Vec::new(),
                parents: Vec::new(),
            };

            repo.set_custom_sensor(sensor).await.unwrap();
            repo.update_statuses().await.unwrap();

            assert!((current_temp_for(&repo, "ema1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains("ema1"));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ema_bad".to_string(),
                kind: SensorKind::ExponentialMovingAvg {
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
            assert!(repo.failsafing_sensors.borrow().contains("ema_bad"));
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = CustomSensor {
                id: "ema_ok".to_string(),
                kind: SensorKind::ExponentialMovingAvg {
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
            assert!(repo.failsafing_sensors.borrow().contains("ema_ok").not());
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![source_dev]).unwrap();
            repo.initialize_devices().await.unwrap();
            // Direct injection bypasses validation and backfill; we only exercise the live
            // path here.
            let sensor = CustomSensor {
                id: "ta_bad".to_string(),
                kind: SensorKind::TimeAverage {
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
            assert!(repo.failsafing_sensors.borrow().contains("ta_bad"));
        });
    }

    // File sensor entering failsafe and recovering: write valid file, run a live tick,
    // delete the file, run another live tick (failsafe + set entry), restore the file with
    // a new value, run another live tick (real value + set cleared).
    #[test]
    #[serial]
    fn file_sensor_recovers_from_failsafe() {
        cc_fs::test_runtime(async {
            let test_file = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_file, b"45000".to_vec()).await.unwrap();

            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = file_sensor("file1", test_file.clone());
            repo.set_custom_sensor(sensor).await.unwrap();

            // Tick 1: file readable, real value
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "file1") - 45.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains("file1").not());

            // Remove the file: next tick should failsafe.
            cc_fs::remove_file(&test_file).await.ok();
            repo.update_statuses().await.unwrap();
            assert!(
                (current_temp_for(&repo, "file1") - MISSING_TEMP_FAILSAFE).abs() < f64::EPSILON
            );
            assert!(repo.failsafing_sensors.borrow().contains("file1"));

            // Restore the file with a new value: next tick recovers.
            cc_fs::write(&test_file, b"55000".to_vec()).await.unwrap();
            repo.update_statuses().await.unwrap();
            assert!((current_temp_for(&repo, "file1") - 55.0).abs() < f64::EPSILON);
            assert!(repo.failsafing_sensors.borrow().contains("file1").not());
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = file_sensor("to_delete", test_file);
            repo.set_custom_sensor(sensor).await.unwrap();
            // Plant a failsafing-state entry directly to simulate the sensor having entered
            // failsafe at some prior point.
            repo.note_failsafing_sensor("to_delete");
            assert!(repo.failsafing_sensors.borrow().contains("to_delete"));

            repo.delete_custom_sensor("to_delete").unwrap();

            assert!(repo.failsafing_sensors.borrow().contains("to_delete").not());
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
            repo.initialize_devices().await.unwrap();
            let sensor = file_sensor("to_update", test_file.clone());
            repo.set_custom_sensor(sensor.clone()).await.unwrap();
            repo.note_failsafing_sensor("to_update");
            assert!(repo.failsafing_sensors.borrow().contains("to_update"));

            // Update with the same content; the cleanup hook must drop the failsafing entry.
            repo.update_custom_sensor(sensor).await.unwrap();

            assert!(repo.failsafing_sensors.borrow().contains("to_update").not());
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
            let mut repo = CustomSensorsRepo::new(test_config, vec![]).unwrap();
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
            assert!(repo.failsafing_sensors.borrow().contains("mix_bf").not());
        });
    }
}
