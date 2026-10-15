// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod status_ring;

use std::fmt::Debug;
use std::ops::Not;
use std::rc::Rc;
use std::{collections::HashMap, time::Duration};

use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::repositories::liquidctl::base_driver::BaseDriver;
use log::warn;
pub use status_ring::StatusSnapshot;
use status_ring::{local_time, timestamp_ns, SnapshotFilter, StatusRing, MAX_NAMES};

const STATUS_SIZE_SECONDS: f64 = 3600.; // only store the last 60 min. of recorded data

/// History slots at the slowest and the fastest poll rate the config allows (it clamps the
/// rate to 0.5..=5.0 s).
const HISTORY_CAPACITY_MIN: usize = 3_600_000_usize.div_ceil(5_000);
const HISTORY_CAPACITY_MAX: usize = 3_600_000_usize.div_ceil(500);
const _: () = assert!(HISTORY_CAPACITY_MIN == 720 && HISTORY_CAPACITY_MAX == 7_200);

#[allow(clippy::upper_case_acronyms)]
pub type UID = String;
#[allow(clippy::upper_case_acronyms)]
pub type DeviceUID = UID;
#[allow(clippy::upper_case_acronyms)]
pub type DeviceName = String;
pub type ChannelName = String;
pub type TempName = String;
pub type TempLabel = String;
pub type TypeIndex = u8;
pub type Temp = f64;
pub type Duty = u8;
#[allow(clippy::upper_case_acronyms)]
pub type RPM = u32;
pub type Mhz = u32;
pub type Watts = f64;

/// Hook run on each channel a device stores, before it is stored. Lets a single consumer
/// (today: the calibration true-duty rewrite) adjust the values every reader sees.
pub trait StatusAugmenter {
    /// Adjusts the values of channel `channel_name`.
    fn augment_channel(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        sample: &mut ChannelSample,
    );

    /// Adjusts every channel of `status`.
    fn augment(&self, status: &mut Status, device_uid: &DeviceUID) {
        for channel in &mut status.channels {
            let mut sample = ChannelSample::from(&*channel);
            self.augment_channel(device_uid, &channel.name, &mut sample);
            sample.apply_to(channel);
        }
    }
}

/// Running min/max/avg/count for a single (channel, `data_type`) or temp pair.
/// `count == 0` means no observation yet; min/max/avg are unused in that state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelStats {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub count: u64,
}

impl ChannelStats {
    /// Seed from a first observation. Min/max/avg collapse to the value.
    fn from_first(value: f64) -> Self {
        Self {
            min: value,
            max: value,
            avg: value,
            count: 1,
        }
    }

    /// Fold a new observation into the running stats. Uses cumulative
    /// average `(avg * count + value) / (count + 1)` so the daemon
    /// matches the UI's existing client-side formula when the UI
    /// extends the baseline between `/stats` fetches. Skips NaN.
    fn fold(&mut self, value: f64) {
        if value.is_nan() {
            return;
        }
        if self.count == 0 {
            *self = Self::from_first(value);
            return;
        }
        debug_assert!(self.min <= self.max);
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        let new_count = self.count + 1;
        #[allow(clippy::cast_precision_loss)]
        {
            self.avg = (self.avg * self.count as f64 + value) / new_count as f64;
        }
        self.count = new_count;
    }
}

/// Which numeric field on a `ChannelStatus` a `ChannelStats` entry tracks.
/// Serialized as upper-case (`DUTY`, `RPM`, `FREQ`, `WATTS`) to match the
/// existing UI `DataType` enum on the wire.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelDataType {
    Duty,
    Rpm,
    Freq,
    Watts,
}

/// Most attributes one channel can report: the temperature set is the larger one.
pub const MAX_CHANNEL_ATTRIBUTES: usize = 16;

/// A driver-reported channel attribute that is read on demand rather than polled: limits,
/// targets and sensor details. Serialized upper-case so clients can pick the unit and decide
/// which values to draw as limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[schemars(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ChannelAttributeKind {
    /// Degrees Celsius.
    TempMax,
    /// Degrees Celsius, an absolute temperature per the hwmon ABI, not a delta.
    TempMaxHyst,
    /// Degrees Celsius.
    TempCrit,
    /// Degrees Celsius, absolute.
    TempCritHyst,
    /// Degrees Celsius.
    TempEmergency,
    /// Degrees Celsius, absolute.
    TempEmergencyHyst,
    /// Degrees Celsius.
    TempMin,
    /// Degrees Celsius, absolute.
    TempMinHyst,
    /// Degrees Celsius.
    TempLcrit,
    /// Degrees Celsius, absolute.
    TempLcritHyst,
    /// Degrees Celsius, the lowest reading since the driver loaded or was reset.
    TempLowest,
    /// Degrees Celsius, the highest reading since the driver loaded or was reset.
    TempHighest,
    /// Degrees Celsius, the lowest temperature the sensor is rated for.
    TempRatedMin,
    /// Degrees Celsius, the highest temperature the sensor is rated for.
    TempRatedMax,
    /// Degrees Celsius, added by the chip to the raw reading.
    TempOffset,
    /// Sensor type code 1-6 (CPU diode, transistor, thermal diode, thermistor, AMD AMDSI,
    /// Intel PECI).
    TempType,
    /// Revolutions per minute, or the unit the driver label names, such as `Pressure [ubar]`.
    FanMin,
    /// Revolutions per minute, or the unit the driver label names, such as `Pressure [ubar]`.
    FanMax,
    /// Revolutions per minute, or the unit the driver label names, such as `Pressure [ubar]`.
    FanTarget,
    /// Divisor, a plain number.
    FanDiv,
    /// Tachometer pulses per revolution, a plain number.
    FanPulses,
    /// Watts.
    PowerMax,
    /// Watts.
    PowerCrit,
    /// Watts.
    PowerMin,
    /// Watts.
    PowerLcrit,
    /// Watts, the power limit the device currently enforces.
    PowerCap,
    /// Watts, the margin around the cap, not an absolute power.
    PowerCapHyst,
    /// Watts, the highest cap that can be set.
    PowerCapMax,
    /// Watts, the lowest cap that can be set.
    PowerCapMin,
    /// Watts, the lowest power the sensor is rated for.
    PowerRatedMin,
    /// Watts, the highest power the sensor is rated for.
    PowerRatedMax,
}

/// One driver-reported attribute. `name` is the sysfs file name, so clients can show it as is:
/// the same attribute means different things on different drivers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelAttribute {
    pub name: String,
    pub kind: ChannelAttributeKind,
    /// °C for temperatures, rpm for fan speeds, watts for power, a plain number otherwise.
    /// Fan limits take the unit the driver label names when it is not rpm.
    pub value: f64,
}

/// Per-device running stats since daemon start. Populated lazily as
/// channels/temps are observed. Reset via `Device::reset_stats`.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct DeviceStats {
    pub temps: HashMap<TempName, ChannelStats>,
    pub channels: HashMap<ChannelName, HashMap<ChannelDataType, ChannelStats>>,
}

#[derive(Clone)]
pub struct Device {
    pub name: DeviceName,

    /// The `DeviceType`. This combines with the `type_id` are treated as unique identifiers for things like settings.
    pub d_type: DeviceType,

    /// The index from the type's device list. Most of the time this is stable.
    pub type_index: TypeIndex,

    /// A Unique identifier that is a hash of a combination of values determined by each repository
    pub uid: DeviceUID,

    /// An optional device identifier. This should be pretty unique,
    /// like a serial number or pci device path to be taken into account for the uid.
    #[allow(clippy::struct_field_names)]
    device_id: Option<String>,

    /// The last hour of statuses, in a ring allocated for the poll rate.
    history: StatusRing,

    /// Specific Liquidctl device information
    pub lc_info: Option<LcInfo>,

    /// General Device information
    pub info: DeviceInfo,

    /// Optional hook run on each status `set_status` stores. See [`StatusAugmenter`].
    status_augmenter: Option<Rc<dyn StatusAugmenter>>,

    /// Running per-channel min/max/avg/count since daemon start. Updated
    /// inside `set_status` after the augmenter runs (so true-duty rewrites
    /// feed stats, not the pre-rewrite values).
    stats: DeviceStats,

    /// Whether a name the history could not keep was logged. See [`MAX_NAMES`].
    refused_name_logged: bool,
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.type_index == other.type_index
            && self.d_type == other.d_type
    }
}

impl Debug for Device {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Device: {{ name: {}, device_id: {}, type: {}, type_index: {}, UID: {}, status: {:?}, lc_info: {:?}, info: {:?} }}",
            self.name,
            self.device_id.clone().unwrap_or_default(),
            self.d_type,
            self.type_index,
            self.uid,
            self.history,
            self.lc_info,
            self.info,
        )
    }
}

impl Device {
    /// This should be used every time to create a new device struct
    pub fn new(
        name: DeviceName,
        d_type: DeviceType,
        type_index: u8,
        lc_info: Option<LcInfo>,
        info: DeviceInfo,
        device_id: Option<String>,
        poll_rate: f64,
    ) -> Self {
        let uid = Self::create_uid_from(&name, d_type, type_index, device_id.as_ref());
        Device {
            name,
            d_type,
            type_index,
            uid,
            device_id,
            history: StatusRing::new(Self::history_capacity(poll_rate)),
            lc_info,
            info,
            status_augmenter: None,
            stats: DeviceStats::default(),
            refused_name_logged: false,
        }
    }

    /// Install the hook `set_status` runs on each status before storing it. Calling twice
    /// replaces the prior hook.
    pub fn set_status_augmenter(&mut self, augmenter: Rc<dyn StatusAugmenter>) {
        self.status_augmenter = Some(augmenter);
    }

    /// History slots for `poll_rate`: one hour of entries.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn history_capacity(poll_rate: f64) -> usize {
        let capacity = (STATUS_SIZE_SECONDS / poll_rate).ceil() as usize;
        assert!(
            (HISTORY_CAPACITY_MIN..=HISTORY_CAPACITY_MAX).contains(&capacity),
            "a poll rate of {poll_rate} s is outside the range the config allows"
        );
        capacity
    }

    /// Entries in the status history. Index 0 is the oldest.
    pub fn history_len(&self) -> usize {
        self.history.len()
    }

    pub fn latest_timestamp(&self) -> Option<DateTime<Local>> {
        let index = self.history_len().checked_sub(1)?;
        Some(local_time(self.history.timestamp_ns(index)))
    }

    /// The newest entry's value of temp `name`.
    pub fn latest_temp(&self, name: &str) -> Option<Temp> {
        let index = self.history_len().checked_sub(1)?;
        self.temp_at(index, name)
    }

    /// The newest entry's values of channel `name`.
    pub fn latest_channel(&self, name: &str) -> Option<ChannelSample> {
        let index = self.history_len().checked_sub(1)?;
        self.channel_at(index, name)
    }

    /// The newest entry's temps, in first-seen order.
    pub fn latest_temps(&self) -> impl Iterator<Item = (&str, Temp)> {
        let latest = self.history_len().checked_sub(1);
        latest
            .into_iter()
            .flat_map(|index| self.history.temps_at(index))
    }

    /// The newest entry's channels, in first-seen order.
    pub fn latest_channels(&self) -> impl Iterator<Item = (&str, ChannelSample)> {
        let latest = self.history_len().checked_sub(1);
        latest
            .into_iter()
            .flat_map(|index| self.history.channels_at(index))
    }

    /// Temp `name` at history `index`. `None` past the end of the history.
    pub fn temp_at(&self, index: usize, name: &str) -> Option<Temp> {
        if index >= self.history_len() {
            return None;
        }
        self.history.temp_at(index, name)
    }

    /// Channel `name` at history `index`. `None` past the end of the history.
    pub fn channel_at(&self, index: usize, name: &str) -> Option<ChannelSample> {
        if index >= self.history_len() {
            return None;
        }
        self.history.channel_at(index, name)
    }

    /// Temp `name` over the newest `count` entries, oldest first. Entries without it are
    /// skipped.
    pub fn temp_tail(&self, name: &str, count: usize) -> impl Iterator<Item = Temp> {
        let len = self.history_len();
        (len.saturating_sub(count)..len).filter_map(move |index| self.temp_at(index, name))
    }

    /// Fills the duty of channel `name` wherever an entry has its rpm but no duty, with
    /// `duty_for_rpm`. Returns whether any entry was filled.
    pub fn backfill_channel_duty(
        &mut self,
        name: &str,
        duty_for_rpm: impl Fn(RPM) -> Option<f64>,
    ) -> bool {
        self.history.backfill_channel_duty(name, duty_for_rpm)
    }

    /// Removes the duty of channel `name` from every entry.
    pub fn clear_channel_duty(&mut self, name: &str) {
        self.history.clear_channel_duty(name);
    }

    /// Adds temp `name` to every entry, with `values[index]` at history `index`.
    pub fn add_temp_column(&mut self, name: &str, values: &[Temp]) {
        if self.history.set_temp_everywhere(name, values).not() {
            self.note_refused_name();
        }
    }

    /// Adds channel `name` to every entry, with `samples[index]` at history `index`.
    pub fn add_channel_column(&mut self, name: &str, samples: &[ChannelSample]) {
        if self.history.set_channel_everywhere(name, samples).not() {
            self.note_refused_name();
        }
    }

    /// Removes the temp or channel `name` from every entry.
    pub fn remove_status_name(&mut self, name: &str) {
        self.history.remove_name(name);
    }

    /// The whole history, for the API.
    pub fn snapshot_all(&self) -> StatusSnapshot {
        self.history.snapshot(SnapshotFilter::All)
    }

    /// The entries stamped after `after`, judged entry by entry.
    pub fn snapshot_since(&self, after: DateTime<Local>) -> StatusSnapshot {
        self.history.snapshot_after(timestamp_ns(after))
    }

    /// The newest entry, for the API.
    pub fn snapshot_recent(&self) -> StatusSnapshot {
        self.history.snapshot_latest(SnapshotFilter::All)
    }

    /// Every entry's timestamp with only the temp or channel `name`.
    pub fn snapshot_name(&self, name: &str) -> StatusSnapshot {
        self.history.snapshot(self.history.name_filter(name))
    }

    /// The newest entry's timestamp with only the temp or channel `name`.
    pub fn snapshot_name_recent(&self, name: &str) -> StatusSnapshot {
        self.history.snapshot_latest(self.history.name_filter(name))
    }

    /// History `index` as a `Status`. Allocates.
    fn status_at(&self, index: usize) -> Status {
        Status {
            timestamp: local_time(self.history.timestamp_ns(index)),
            temps: self
                .history
                .temps_at(index)
                .map(|(name, temp)| TempStatus {
                    name: name.to_string(),
                    temp,
                })
                .collect(),
            channels: self
                .history
                .channels_at(index)
                .map(|(name, sample)| sample.to_status(name))
                .collect(),
        }
    }

    /// The newest entry as a `Status`. Allocates; tests only.
    #[cfg(test)]
    pub fn latest_status(&self) -> Option<Status> {
        let index = self.history_len().checked_sub(1)?;
        Some(self.status_at(index))
    }

    /// The whole history as `Status`es, oldest first. Allocates; tests only.
    #[cfg(test)]
    pub fn history_statuses(&self) -> Vec<Status> {
        (0..self.history_len())
            .map(|index| self.status_at(index))
            .collect()
    }

    /// Reset the history and seed it with zeroed entries plus `status`
    /// as the newest entry. Runs the augmenter on the pushed entry but does
    /// NOT record stats: some channels (e.g. CPU watts derived from a
    /// joule-counter delta) have no real reading at init, and folding the
    /// fabricated 0.0 would lock `min` there forever. Stats are seeded by
    /// the first main-loop `set_status` call.
    ///
    /// Arguments:
    ///
    /// * `status`: the first real status, appended as the most recent entry.
    #[allow(clippy::cast_precision_loss)]
    pub fn initialize_status_history_with(&mut self, mut status: Status, poll_rate: f64) {
        let capacity = Self::history_capacity(poll_rate);
        self.history.reset(capacity);
        let (temps, channels) = self.template_columns(
            status.temps.iter().map(|temp| temp.name.as_str()),
            status
                .channels
                .iter()
                .map(|channel| (channel.name.as_str(), ChannelSample::from(channel).zeroed())),
        );
        // Zeroed entries at offsets [-(N-1), ..., -1] * poll_rate, then the real
        // status at offset 0.
        for pos in (1..capacity).rev() {
            let timestamp = status.timestamp - Duration::from_secs_f64(pos as f64 * poll_rate);
            self.push_zeroed(timestamp, &temps, &channels);
        }
        self.push_and_augment(&mut status);
    }

    /// Re-seed the history from its newest entry, as `initialize_status_history_with` would
    /// from a fresh reading. No-op on an empty history.
    pub fn initialize_status_history_from_latest(&mut self, poll_rate: f64) {
        let Some(index) = self.history_len().checked_sub(1) else {
            return;
        };
        let latest = self.status_at(index);
        self.initialize_status_history_with(latest, poll_rate);
    }

    /// Replace the entire status history with zeroed copies of the newest entry, the newest
    /// stamped `timestamp`. Used on wake from sleep when cached values are stale but no real
    /// reading is available yet. Does NOT record stats; the zeroed wake template is not a
    /// real reading and would taint min/avg. No-op on an empty history. Reallocates only
    /// when `poll_rate` changed the slot count.
    #[allow(clippy::cast_precision_loss)]
    pub fn zero_status_history(&mut self, timestamp: DateTime<Local>, poll_rate: f64) {
        let Some(latest) = self.history_len().checked_sub(1) else {
            return;
        };
        let (temp_names, channel_names) = self.history.zeroed_names_at(latest);
        let capacity = Self::history_capacity(poll_rate);
        self.history.reset(capacity);
        let (temps, channels) = self.template_columns(
            temp_names.iter().map(|name| &**name),
            channel_names
                .iter()
                .map(|(name, sample)| (&**name, *sample)),
        );
        for pos in (1..capacity).rev() {
            let at = timestamp - Duration::from_secs_f64(pos as f64 * poll_rate);
            self.push_zeroed(at, &temps, &channels);
        }
        self.push_zeroed(timestamp, &temps, &channels);
    }

    /// The columns of a rebuild's names, in order. A refused name gets none.
    #[allow(clippy::type_complexity)]
    fn template_columns<'a>(
        &mut self,
        temps: impl Iterator<Item = &'a str>,
        channels: impl Iterator<Item = (&'a str, ChannelSample)>,
    ) -> (Vec<usize>, Vec<(usize, ChannelSample)>) {
        let mut refused = false;
        let temps = temps
            .filter_map(|name| {
                let column = self.history.temp_column(name);
                refused |= column.is_none();
                column
            })
            .collect();
        let channels = channels
            .filter_map(|(name, sample)| {
                let column = self.history.channel_column(name);
                refused |= column.is_none();
                Some((column?, sample))
            })
            .collect();
        if refused {
            self.note_refused_name();
        }
        (temps, channels)
    }

    fn push_zeroed(
        &mut self,
        timestamp: DateTime<Local>,
        temps: &[usize],
        channels: &[(usize, ChannelSample)],
    ) {
        let slot = self.history.push(timestamp_ns(timestamp));
        for column in temps {
            self.history.set_temp(slot, *column, 0.0);
        }
        for (column, sample) in channels {
            self.history.set_channel(slot, *column, *sample);
        }
    }

    /// Logs, once per device, that a name went unrecorded: the device reports more names than
    /// its history keeps.
    fn note_refused_name(&mut self) {
        if self.refused_name_logged {
            return;
        }
        self.refused_name_logged = true;
        warn!(
            "{} reports more than {MAX_NAMES} temps or channels. Names beyond that are not \
             recorded in its status history.",
            self.name
        );
    }

    /// Run the augmenter on `status` and store it as the newest entry. Does not touch
    /// stats; callers that need stats folded should use `set_status`.
    fn push_and_augment(&mut self, status: &mut Status) {
        if let Some(augmenter) = self.status_augmenter.as_ref() {
            augmenter.augment(status, &self.uid);
        }
        if self.history.push_status(status).not() {
            self.note_refused_name();
        }
    }

    /// Push a whole `Status` as a tick: run the augmenter and fold the post-augmenter values
    /// into running stats. Tests only; repositories write their ticks with `write_status`.
    #[cfg(test)]
    pub fn set_status(&mut self, mut status: Status) {
        self.push_and_augment(&mut status);
        Self::record_stats(&mut self.stats, &status);
    }

    /// Borrow the per-channel running stats.
    pub fn stats(&self) -> &DeviceStats {
        &self.stats
    }

    /// Fold a `Status` into the running stats. Channels and temps absent
    /// from `status` are not touched (no zero pollution from a tick where
    /// hwmon dropped a channel). Called from `set_status` after the
    /// augmenter runs. Names are cloned into the maps on first sight only.
    #[cfg(test)]
    fn record_stats(device_stats: &mut DeviceStats, status: &Status) {
        for temp in &status.temps {
            Self::record_temp(device_stats, &temp.name, temp.temp);
        }
        for channel in &status.channels {
            Self::record_channel(device_stats, &channel.name, ChannelSample::from(channel));
        }
    }

    fn record_temp(device_stats: &mut DeviceStats, name: &str, temp: Temp) {
        match device_stats.temps.get_mut(name) {
            Some(temp_stats) => temp_stats.fold(temp),
            None => device_stats
                .temps
                .entry(name.to_string())
                .or_default()
                .fold(temp),
        }
    }

    fn record_channel(device_stats: &mut DeviceStats, name: &str, sample: ChannelSample) {
        let by_type = match device_stats.channels.get_mut(name) {
            Some(by_type) => by_type,
            None => device_stats.channels.entry(name.to_string()).or_default(),
        };
        if let Some(duty) = sample.duty {
            by_type.entry(ChannelDataType::Duty).or_default().fold(duty);
        }
        if let Some(rpm) = sample.rpm {
            by_type
                .entry(ChannelDataType::Rpm)
                .or_default()
                .fold(f64::from(rpm));
        }
        if let Some(freq) = sample.freq {
            by_type
                .entry(ChannelDataType::Freq)
                .or_default()
                .fold(f64::from(freq));
        }
        if let Some(watts) = sample.watts {
            by_type
                .entry(ChannelDataType::Watts)
                .or_default()
                .fold(watts);
        }
    }

    /// Starts the newest entry, stamped `timestamp`, overwriting the oldest once the history
    /// is full. Every name starts absent from it: write the ones this tick reports. Each
    /// value passes the augmenter and is folded into the running stats as it is written.
    pub fn write_status(&mut self, timestamp: DateTime<Local>) -> StatusWriter<'_> {
        let slot = self.history.push(timestamp_ns(timestamp));
        StatusWriter {
            device: self,
            slot,
            next_temp: 0,
            next_channel: 0,
        }
    }

    /// Clear all stats and reseed each entry from the most recent status
    /// with `count=1` so the UI never sees a zero-count window after a
    /// reset. Channels absent from the most recent status get no entry
    /// and will reseed naturally on their next observation.
    pub fn reset_stats(&mut self) {
        self.stats.temps.clear();
        self.stats.channels.clear();
        let Some(latest) = self.history_len().checked_sub(1) else {
            return;
        };
        for (name, temp) in self.history.temps_at(latest) {
            self.stats
                .temps
                .insert(name.to_string(), ChannelStats::from_first(temp));
        }
        for (name, channel) in self.history.channels_at(latest) {
            let mut by_type: HashMap<ChannelDataType, ChannelStats> = HashMap::new();
            if let Some(duty) = channel.duty {
                by_type.insert(ChannelDataType::Duty, ChannelStats::from_first(duty));
            }
            if let Some(rpm) = channel.rpm {
                by_type.insert(
                    ChannelDataType::Rpm,
                    ChannelStats::from_first(f64::from(rpm)),
                );
            }
            if let Some(freq) = channel.freq {
                by_type.insert(
                    ChannelDataType::Freq,
                    ChannelStats::from_first(f64::from(freq)),
                );
            }
            if let Some(watts) = channel.watts {
                by_type.insert(ChannelDataType::Watts, ChannelStats::from_first(watts));
            }
            if by_type.is_empty().not() {
                self.stats.channels.insert(name.to_string(), by_type);
            }
        }
    }
}

/// Writes one history entry in place. Made by [`Device::write_status`].
pub struct StatusWriter<'a> {
    device: &'a mut Device,
    slot: usize,
    /// The columns expected next. A source reports its names in the same order every tick,
    /// so a lookup almost always matches on its first comparison.
    next_temp: usize,
    next_channel: usize,
}

impl StatusWriter<'_> {
    pub fn temp(&mut self, name: &str, temp: Temp) {
        let Some(column) = self.device.history.temp_column_from(self.next_temp, name) else {
            self.device.note_refused_name();
            return;
        };
        self.next_temp = column + 1;
        self.device.history.set_temp(self.slot, column, temp);
        Device::record_temp(&mut self.device.stats, name, temp);
    }

    pub fn channel(&mut self, name: &str, mut sample: ChannelSample) {
        if let Some(augmenter) = self.device.status_augmenter.as_ref() {
            augmenter.augment_channel(&self.device.uid, name, &mut sample);
        }
        let Some(column) = self
            .device
            .history
            .channel_column_from(self.next_channel, name)
        else {
            self.device.note_refused_name();
            return;
        };
        self.next_channel = column + 1;
        self.device.history.set_channel(self.slot, column, sample);
        Device::record_channel(&mut self.device.stats, name, sample);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TempStatus {
    pub name: TempName,
    pub temp: Temp,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelStatus {
    pub name: ChannelName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm: Option<RPM>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freq: Option<Mhz>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub watts: Option<Watts>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pwm_mode: Option<u8>,
}

/// One channel's values in one history entry. A field is `None` when the entry lacks it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ChannelSample {
    pub rpm: Option<RPM>,
    pub duty: Option<f64>,
    pub freq: Option<Mhz>,
    pub watts: Option<Watts>,
    pub pwm_mode: Option<u8>,
}

impl From<&ChannelStatus> for ChannelSample {
    fn from(channel: &ChannelStatus) -> Self {
        Self {
            rpm: channel.rpm,
            duty: channel.duty,
            freq: channel.freq,
            watts: channel.watts,
            pwm_mode: channel.pwm_mode,
        }
    }
}

impl ChannelSample {
    /// Writes these values over `channel`'s.
    fn apply_to(self, channel: &mut ChannelStatus) {
        channel.rpm = self.rpm;
        channel.duty = self.duty;
        channel.freq = self.freq;
        channel.watts = self.watts;
        channel.pwm_mode = self.pwm_mode;
    }

    /// Every present value set to zero, keeping which fields are present. `pwm_mode` is a
    /// mode, not a reading, so it stays.
    fn zeroed(self) -> Self {
        Self {
            rpm: self.rpm.map(|_| 0),
            duty: self.duty.map(|_| 0.0),
            freq: self.freq.map(|_| 0),
            watts: self.watts.map(|_| 0.0),
            pwm_mode: self.pwm_mode,
        }
    }

    pub fn to_status(self, name: &str) -> ChannelStatus {
        ChannelStatus {
            name: name.to_string(),
            rpm: self.rpm,
            duty: self.duty,
            freq: self.freq,
            watts: self.watts,
            pwm_mode: self.pwm_mode,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
/// A Model which contains various applicable device statuses
pub struct Status {
    pub timestamp: DateTime<Local>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub temps: Vec<TempStatus>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub channels: Vec<ChannelStatus>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            timestamp: Local::now(),
            temps: vec![],
            channels: vec![],
        }
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
pub enum DeviceType {
    #[allow(clippy::upper_case_acronyms)]
    CPU,
    #[allow(clippy::upper_case_acronyms)]
    GPU,
    Liquidctl,
    Hwmon,
    CustomSensors,
    ServicePlugin,
}

/// Needed Device info per device
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceInfo {
    pub channels: HashMap<String, ChannelInfo>,
    pub temps: HashMap<String, TempInfo>,
    pub lighting_speeds: Vec<String>,
    /// The absolute minimum temp to use for Profiles for this device
    pub temp_min: u8,
    /// The absolute maximum temp to use for Profiles for this device
    pub temp_max: u8,
    pub profile_max_length: u8,
    pub profile_min_length: u8,
    pub model: Option<String>,

    /// When present, then this is a `ThinkPad` device. True or False indicates whether Fan control
    /// is enabled for the kernel module and changing values is possible
    pub thinkpad_fan_control: Option<bool>,

    /// When present, then this is an AMD RDNA3/4 GPU with PMFW fan curve support.
    /// True indicates overdrive is enabled and fan control is available.
    /// False indicates overdrive needs to be enabled via kernel boot parameter.
    pub amd_gpu_overdrive: Option<bool>,
    pub driver_info: DriverInfo,
}

impl DeviceInfo {
    /// The detected label for a channel, when the device reports one.
    pub fn detected_channel_label(&self, channel_name: &str) -> Option<String> {
        if let Some(temp_info) = self.temps.get(channel_name) {
            return Some(temp_info.label.clone());
        }
        self.channels
            .get(channel_name)
            .and_then(|channel| channel.label.clone())
    }
}

impl Default for DeviceInfo {
    fn default() -> Self {
        DeviceInfo {
            channels: HashMap::new(),
            temps: HashMap::new(),
            lighting_speeds: vec![],
            temp_min: 0,
            temp_max: 150,
            profile_max_length: 17, // reasonable default, one control point every 5 degrees for 20-100
            profile_min_length: 2,
            model: None,
            thinkpad_fan_control: None,
            amd_gpu_overdrive: None,
            driver_info: DriverInfo::default(),
        }
    }
}

/// Describes a single device channel. A channel is exactly one of {speed, lighting, lcd} or
/// info-only; the mutual exclusivity is enforced by [`ChannelKind`]. `label` is presentation-only
/// and may accompany any kind.
///
/// NOTE: serde and `JsonSchema` are bridged through the private [`ChannelInfoWire`] (below) solely
/// to keep the REST/OpenAPI wire shape byte-identical to the pre-enum flat struct (all five fields
/// always present). That bridge, the `#[serde(into/from)]` attribute, and the manual `JsonSchema`
/// impl are a backward-compatibility shim and can all be deleted together at the next API major
/// version, after which this type can derive `Serialize`/`Deserialize`/`JsonSchema` directly.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(into = "ChannelInfoWire", from = "ChannelInfoWire")]
pub struct ChannelInfo {
    pub label: Option<String>,
    pub kind: ChannelKind,
}

/// The channel type and its type-specific data. Exactly one variant holds for a given channel.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum ChannelKind {
    Speed(SpeedOptions),
    Lighting(Vec<LightingMode>),
    Lcd {
        modes: Vec<LcdMode>,
        info: Option<LcdInfo>,
    },
    #[default]
    InfoOnly,
}

impl ChannelInfo {
    pub fn speed_options(&self) -> Option<&SpeedOptions> {
        match &self.kind {
            ChannelKind::Speed(speed_options) => Some(speed_options),
            _ => None,
        }
    }

    pub fn lighting_modes(&self) -> &[LightingMode] {
        match &self.kind {
            ChannelKind::Lighting(lighting_modes) => lighting_modes,
            _ => &[],
        }
    }

    pub fn lcd_modes(&self) -> &[LcdMode] {
        match &self.kind {
            ChannelKind::Lcd { modes, .. } => modes,
            _ => &[],
        }
    }

    pub fn lcd_info(&self) -> Option<&LcdInfo> {
        match &self.kind {
            ChannelKind::Lcd { info, .. } => info.as_ref(),
            _ => None,
        }
    }

    pub fn speed_options_mut(&mut self) -> Option<&mut SpeedOptions> {
        match &mut self.kind {
            ChannelKind::Speed(speed_options) => Some(speed_options),
            _ => None,
        }
    }
}

// Backward-compatibility wire shim for `ChannelInfo` (see the NOTE on that type). Preserves the
// legacy flat JSON shape for REST and OpenAPI so existing clients keep working (the UI reads
// `lighting_modes`/`lcd_modes` without null-guards). Delete this struct, the `#[serde(into/from)]`
// attribute on `ChannelInfo`, and the manual `JsonSchema for ChannelInfo` impl together at the next
// API major version.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
struct ChannelInfoWire {
    label: Option<String>,
    speed_options: Option<SpeedOptions>,
    lighting_modes: Vec<LightingMode>,
    lcd_modes: Vec<LcdMode>,
    lcd_info: Option<LcdInfo>,
}

impl From<ChannelInfo> for ChannelInfoWire {
    fn from(value: ChannelInfo) -> Self {
        let ChannelInfo { label, kind } = value;
        let mut wire = ChannelInfoWire {
            label,
            ..Default::default()
        };
        match kind {
            ChannelKind::Speed(speed_options) => wire.speed_options = Some(speed_options),
            ChannelKind::Lighting(lighting_modes) => wire.lighting_modes = lighting_modes,
            ChannelKind::Lcd { modes, info } => {
                wire.lcd_modes = modes;
                wire.lcd_info = info;
            }
            ChannelKind::InfoOnly => {}
        }
        wire
    }
}

impl From<ChannelInfoWire> for ChannelInfo {
    fn from(value: ChannelInfoWire) -> Self {
        let ChannelInfoWire {
            label,
            speed_options,
            lighting_modes,
            lcd_modes,
            lcd_info,
        } = value;
        // Deterministic precedence for a malformed multi-field payload: speed, then lighting, then
        // lcd, else info-only. Daemon-produced payloads only ever set one category.
        let kind = if let Some(speed_options) = speed_options {
            ChannelKind::Speed(speed_options)
        } else if lighting_modes.is_empty().not() {
            ChannelKind::Lighting(lighting_modes)
        } else if lcd_modes.is_empty().not() || lcd_info.is_some() {
            ChannelKind::Lcd {
                modes: lcd_modes,
                info: lcd_info,
            }
        } else {
            ChannelKind::InfoOnly
        };
        Self { label, kind }
    }
}

impl JsonSchema for ChannelInfo {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("ChannelInfo")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        ChannelInfoWire::json_schema(generator)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, JsonSchema)]
pub struct TempInfo {
    pub label: TempLabel,
    pub number: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SpeedOptions {
    /// The minimum fan duty for this speed channel
    pub min_duty: Duty,

    /// The maximum fan duty for this speed channel
    pub max_duty: Duty,

    /// True if manual fan speed control is supported; if false, speeds are read-only (monitoring only).
    pub fixed_enabled: bool,

    /// If present, then this channel has special settings that are applicable.
    pub extension: Option<ChannelExtensionNames>,
}

impl Default for SpeedOptions {
    fn default() -> Self {
        SpeedOptions {
            min_duty: 0,
            max_duty: 100,
            fixed_enabled: true,
            extension: None,
        }
    }
}

/// Channel extension names that signal which `ChannelExtensions` are applicable
/// for a particular device channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, EnumString, JsonSchema)]
pub enum ChannelExtensionNames {
    AutoHWCurve,
    AmdRdnaGpu,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum LightingModeType {
    None,
    Liquidctl,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LightingMode {
    pub name: String,
    pub frontend_name: String,
    pub min_colors: u8,
    pub max_colors: u8,
    pub speed_enabled: bool,
    pub backward_enabled: bool,
    #[serde(rename(serialize = "type"))]
    pub type_: LightingModeType,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum LcdModeType {
    None,
    Liquidctl,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LcdMode {
    pub name: String,
    pub frontend_name: String,
    pub brightness: bool,
    pub orientation: bool,
    pub image: bool,
    pub colors_min: u8,
    pub colors_max: u8,
    #[serde(rename(serialize = "type"))]
    pub type_: LcdModeType,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
/// Specific LCD Screen info
pub struct LcdInfo {
    pub screen_width: u32,
    pub screen_height: u32,
    pub max_image_size_bytes: u32,
    /// Whether this screen can show an animated gif. The Kraken 2023 on firmware 2.x
    /// cannot, and liquidctl refuses outright, so the mode has to stop offering it.
    pub gif_supported: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
/// Specific Liquidctl device information
pub struct LcInfo {
    /// An Enum representation of the various Liquidctl driver classes
    pub driver_type: BaseDriver,
    /// The detected firmware version at initialization
    pub firmware_version: Option<String>,
    /// An indicator for needed user input to determine actual asetek690lc device
    pub unknown_asetek: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
/// Device Driver Information
pub struct DriverInfo {
    pub drv_type: DriverType,

    /// If available the kernel driver name or liquidctl driver class.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    /// If available the driver's version.
    /// For kernel-based drivers this is the current kernel version.
    /// For liquidctl-based drivers this is the liquidctl version.
    /// For Nvidia-based drivers this is the version of the installed nvidia proprietary drivers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,

    /// If available various paths used to access the device.
    /// This can include paths like the kernel device path, hwmon path, HID path, or PCI Bus ID
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub locations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
/// The Driver Type, or source of the driver actively being used for this device.
pub enum DriverType {
    Kernel,
    Liquidctl,
    #[allow(clippy::upper_case_acronyms)]
    NVML,
    NvidiaCLI,
    CoolerControl, // For things like CustomSensors
    External,      // For external device services
}

impl Default for DriverInfo {
    fn default() -> Self {
        Self {
            drv_type: DriverType::Kernel,
            name: None,
            version: None,
            locations: Vec::new(),
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact equality on known test constants is intentional
mod tests {
    use super::*;
    use chrono::Local;
    use std::cell::RefCell;

    const POLL_RATE: f64 = 1.0;

    /// A test `StatusAugmenter` that rewrites every channel's duty to a
    /// fixed sentinel. Lets us verify that `record_stats` reads post-augmenter
    /// values, not the pre-augmenter status passed into `set_status`.
    struct FixedDutyAugmenter {
        target_duty: f64,
        /// Channels augmented.
        invocations: RefCell<u32>,
    }

    impl StatusAugmenter for FixedDutyAugmenter {
        fn augment_channel(
            &self,
            _device_uid: &DeviceUID,
            _channel_name: &str,
            sample: &mut ChannelSample,
        ) {
            *self.invocations.borrow_mut() += 1;
            if sample.duty.is_some() {
                sample.duty = Some(self.target_duty);
            }
        }
    }

    fn make_test_device() -> Device {
        Device::new(
            "test-device".to_string(),
            DeviceType::Hwmon,
            0,
            None,
            DeviceInfo::default(),
            Some("test-id".to_string()),
            POLL_RATE,
        )
    }

    fn status_with(temps: Vec<(&str, f64)>, channels: Vec<ChannelStatus>) -> Status {
        Status {
            timestamp: Local::now(),
            temps: temps
                .into_iter()
                .map(|(name, temp)| TempStatus {
                    name: name.to_string(),
                    temp,
                })
                .collect(),
            channels,
        }
    }

    fn channel(name: &str, duty: Option<f64>, rpm: Option<RPM>) -> ChannelStatus {
        ChannelStatus {
            name: name.to_string(),
            duty,
            rpm,
            freq: None,
            watts: None,
            pwm_mode: None,
        }
    }

    /// Verify the canonical happy path: a status pushed via `set_status`
    /// updates min/max/avg/count for every present temp and channel
    /// data field, lazily creating entries the first time. Init seeds
    /// history only; the first `set_status` seeds stats.
    #[test]
    fn set_status_records_stats_for_all_present_fields() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        device.set_status(status_with(
            vec![("cpu", 40.0)],
            vec![channel("fan1", Some(50.0), Some(1200))],
        ));
        let stats = device.stats();
        assert_eq!(stats.temps.get("cpu").unwrap().count, 1);
        assert_eq!(stats.temps.get("cpu").unwrap().avg, 40.0);
        let fan1 = stats.channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().avg, 50.0);
        assert_eq!(fan1.get(&ChannelDataType::Rpm).unwrap().avg, 1200.0);
    }

    /// Multiple `set_status` calls accumulate. Min tracks the lowest, max
    /// the highest, avg is the cumulative mean, count is total samples.
    /// Init seeds history only and is not counted; the first `set_status`
    /// is observation #1.
    #[test]
    fn record_stats_folds_running_min_max_avg_count() {
        let mut device = make_test_device();
        device.initialize_status_history_with(status_with(vec![("cpu", 30.0)], vec![]), POLL_RATE);
        device.set_status(status_with(vec![("cpu", 30.0)], vec![]));
        device.set_status(status_with(vec![("cpu", 50.0)], vec![]));
        device.set_status(status_with(vec![("cpu", 40.0)], vec![]));
        let cpu = device.stats().temps.get("cpu").unwrap();
        assert_eq!(cpu.count, 3);
        assert_eq!(cpu.min, 30.0);
        assert_eq!(cpu.max, 50.0);
        assert!((cpu.avg - 40.0).abs() < 1e-9);
    }

    /// Channels absent from a tick (e.g. hwmon failsafe omission) must
    /// not pollute their stats. The remaining present channels do update.
    /// Init seeds history only; the first `set_status` seeds stats for all
    /// present fields, and a later `set_status` that omits some leaves
    /// those untouched.
    #[test]
    fn record_stats_skips_absent_channels() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0), ("gpu", 60.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        // First real tick seeds stats for cpu, gpu, fan1.
        device.set_status(status_with(
            vec![("cpu", 40.0), ("gpu", 60.0)],
            vec![channel("fan1", Some(50.0), Some(1200))],
        ));
        // Second push only contains "cpu" and no channels.
        device.set_status(status_with(vec![("cpu", 100.0)], vec![]));
        let stats = device.stats();
        assert_eq!(stats.temps.get("cpu").unwrap().count, 2);
        assert_eq!(stats.temps.get("cpu").unwrap().max, 100.0);
        // gpu and fan1 saw no second observation, still count=1.
        assert_eq!(stats.temps.get("gpu").unwrap().count, 1);
        assert_eq!(
            stats
                .channels
                .get("fan1")
                .unwrap()
                .get(&ChannelDataType::Duty)
                .unwrap()
                .count,
            1
        );
    }

    /// `initialize_status_history_with` deliberately does NOT seed stats:
    /// some channels (CPU watts from a joule-counter delta) have no real
    /// reading at init, and folding a fabricated 0.0 would lock min there
    /// forever. Stats are seeded by the first main-loop `set_status`.
    #[test]
    fn initialize_status_history_does_not_record_stats() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 42.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        let stats = device.stats();
        assert!(stats.temps.is_empty());
        assert!(stats.channels.is_empty());
    }

    /// `zero_status_history` wipes history to zeros for wake-from-sleep
    /// recovery, but must NOT record those zeros into stats (would
    /// otherwise pull min to 0 and skew avg).
    #[test]
    fn zero_status_history_does_not_record_stats() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        // Seed stats with a real first tick before going to sleep.
        device.set_status(status_with(
            vec![("cpu", 40.0)],
            vec![channel("fan1", Some(50.0), Some(1200))],
        ));
        // Wake-from-sleep zeroes the history. Stats must be untouched.
        device.zero_status_history(Local::now(), POLL_RATE);
        let stats = device.stats();
        assert_eq!(stats.temps.get("cpu").unwrap().count, 1);
        assert_eq!(stats.temps.get("cpu").unwrap().min, 40.0);
        let fan1 = stats.channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().min, 50.0);
        assert_eq!(fan1.get(&ChannelDataType::Rpm).unwrap().min, 1200.0);
    }

    /// The augmenter fires on the just-pushed entry for both init and
    /// `set_status`. Stats are only recorded by `set_status` and must see
    /// the post-augmenter value (the true-duty rewrite), not the raw
    /// duty passed in.
    #[test]
    fn record_stats_reads_post_augmenter_value() {
        let mut device = make_test_device();
        let augmenter = Rc::new(FixedDutyAugmenter {
            target_duty: 99.0,
            invocations: RefCell::new(0),
        });
        device.set_status_augmenter(augmenter.clone());
        device.initialize_status_history_with(
            status_with(vec![], vec![channel("fan1", Some(10.0), None)]),
            POLL_RATE,
        );
        // Init runs the augmenter but does not seed stats.
        assert_eq!(*augmenter.invocations.borrow(), 1);
        assert!(device.stats().channels.is_empty());
        // First set_status runs the augmenter again and seeds stats.
        device.set_status(status_with(vec![], vec![channel("fan1", Some(10.0), None)]));
        let duty_stats = device
            .stats()
            .channels
            .get("fan1")
            .unwrap()
            .get(&ChannelDataType::Duty)
            .unwrap();
        assert_eq!(*augmenter.invocations.borrow(), 2);
        assert_eq!(duty_stats.count, 1);
        assert_eq!(duty_stats.avg, 99.0);
        assert_eq!(duty_stats.min, 99.0);
    }

    /// A tick on a warm device must not allocate: `set_status` borrows the
    /// newest entry and finds stats by name instead of cloning the status
    /// and its names. Counts this thread's allocations around one call,
    /// with the status built beforehand. A name seen for the first time
    /// still gets its own entry.
    #[test]
    fn set_status_on_a_warm_device_does_not_allocate() {
        let reading = || {
            status_with(
                vec![("cpu", 40.0), ("gpu", 60.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            )
        };
        let mut device = make_test_device();
        device.initialize_status_history_with(reading(), POLL_RATE);
        // The first tick seeds the stats maps, which allocates their keys once.
        device.set_status(reading());
        let next = reading();

        let ((), allocations) = crate::alloc_count::count_allocations(|| device.set_status(next));

        assert_eq!(allocations, 0);
        assert_eq!(device.stats().temps.get("cpu").unwrap().count, 2);
        let fan1 = device.stats().channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Rpm).unwrap().count, 2);
        device.set_status(status_with(vec![("nvme", 35.0)], vec![]));
        assert_eq!(device.stats().temps.get("nvme").unwrap().count, 1);
        assert_eq!(device.stats().temps.get("cpu").unwrap().count, 2);
    }

    /// A tick written in place on a warm device allocates nothing: no `Status`, no names.
    #[test]
    fn write_status_on_a_warm_device_does_not_allocate() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0), ("gpu", 60.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        let tick = |device: &mut Device, temp: f64| {
            let mut writer = device.write_status(Local::now());
            writer.temp("cpu", temp);
            writer.temp("gpu", 61.0);
            writer.channel(
                "fan1",
                ChannelSample {
                    rpm: Some(1300),
                    duty: Some(55.0),
                    ..Default::default()
                },
            );
        };
        // The first tick seeds the stats maps, which allocates their keys once.
        tick(&mut device, 41.0);

        let ((), allocations) = crate::alloc_count::count_allocations(|| tick(&mut device, 42.0));

        assert_eq!(allocations, 0);
        assert_eq!(device.latest_temp("cpu"), Some(42.0));
        assert_eq!(device.latest_channel("fan1").unwrap().duty, Some(55.0));
        assert_eq!(device.stats().temps.get("cpu").unwrap().count, 2);
    }

    /// The writer stores what `set_status` would: names in any order land in their own
    /// columns, names not written this tick are absent, and every channel passes the
    /// augmenter before it is stored and folded into the stats.
    #[test]
    fn write_status_matches_set_status() {
        let augmenter = Rc::new(FixedDutyAugmenter {
            target_duty: 99.0,
            invocations: RefCell::new(0),
        });
        let mut device = make_test_device();
        device.set_status_augmenter(augmenter.clone());
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0), ("gpu", 60.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        let at = Local::now();
        let mut writer = device.write_status(at);
        writer.temp("gpu", 61.0);
        writer.channel(
            "fan1",
            ChannelSample {
                rpm: Some(1300),
                duty: Some(55.0),
                ..Default::default()
            },
        );
        writer.temp("nvme", 35.0);

        let latest = device.latest_status().unwrap();
        assert_eq!(latest.timestamp, at);
        assert_eq!(
            latest.temps,
            vec![
                TempStatus {
                    name: "gpu".to_string(),
                    temp: 61.0
                },
                TempStatus {
                    name: "nvme".to_string(),
                    temp: 35.0
                },
            ]
        );
        assert_eq!(
            latest.channels,
            vec![channel("fan1", Some(99.0), Some(1300))]
        );
        assert_eq!(*augmenter.invocations.borrow(), 2);
        let fan1 = device.stats().channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().avg, 99.0);
        assert!(device.stats().temps.contains_key("cpu").not());
    }

    /// An API copy of the history is one allocation per device, and a tick taken while that
    /// copy is still being serialized writes in place. Sharing the history behind an `Arc`
    /// made such a tick copy every entry on the engine thread.
    #[test]
    fn snapshots_copy_once_and_never_make_a_tick_copy_the_history() {
        let reading = || {
            status_with(
                vec![("cpu", 40.0), ("gpu", 60.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            )
        };
        let mut device = make_test_device();
        device.initialize_status_history_with(reading(), POLL_RATE);
        device.set_status(reading());

        let (all, all_allocations) =
            crate::alloc_count::count_allocations(|| device.snapshot_all());
        let (recent, recent_allocations) =
            crate::alloc_count::count_allocations(|| device.snapshot_recent());
        let (channel, channel_allocations) =
            crate::alloc_count::count_allocations(|| device.snapshot_name("fan1"));
        let next = reading();
        let ((), tick_allocations) =
            crate::alloc_count::count_allocations(|| device.set_status(next));

        assert_eq!(all_allocations, 1);
        assert_eq!(recent_allocations, 1);
        assert_eq!(channel_allocations, 1);
        assert_eq!(tick_allocations, 0);
        drop((all, recent, channel));
    }

    /// A device reporting more names than its history keeps still initializes, wakes and
    /// ticks: the first `MAX_NAMES` temps are kept in every entry, the rest are left out.
    #[test]
    fn a_device_with_too_many_names_keeps_the_first_ones() {
        let too_many: Vec<(String, f64)> = (0..MAX_NAMES + 44)
            .map(|index| (format!("temp{index}"), 30.0))
            .collect();
        let reading = || Status {
            timestamp: Local::now(),
            temps: too_many
                .iter()
                .map(|(name, temp)| TempStatus {
                    name: name.clone(),
                    temp: *temp,
                })
                .collect(),
            channels: vec![channel("fan1", Some(50.0), Some(1200))],
        };
        let mut device = make_test_device();
        device.initialize_status_history_with(reading(), POLL_RATE);
        device.set_status(reading());
        device.zero_status_history(Local::now(), POLL_RATE);
        device.set_status(reading());

        assert_eq!(device.latest_temps().count(), MAX_NAMES);
        assert_eq!(device.latest_temp("temp0"), Some(30.0));
        assert_eq!(device.latest_temp(&format!("temp{MAX_NAMES}")), None);
        assert_eq!(device.latest_channel("fan1").unwrap().rpm, Some(1200));
        assert!(device.refused_name_logged);
    }

    /// One hour of entries at every poll rate the config allows, and none outside it.
    #[test]
    fn history_capacity_covers_one_hour_at_every_configurable_poll_rate() {
        assert_eq!(Device::history_capacity(0.5), HISTORY_CAPACITY_MAX);
        assert_eq!(Device::history_capacity(1.0), 3_600);
        assert_eq!(Device::history_capacity(1.5), 2_400);
        assert_eq!(Device::history_capacity(5.0), HISTORY_CAPACITY_MIN);
    }

    #[test]
    #[should_panic(expected = "outside the range the config allows")]
    fn a_poll_rate_faster_than_the_config_allows_is_refused() {
        let _ = Device::history_capacity(0.25);
    }

    #[test]
    #[should_panic(expected = "outside the range the config allows")]
    fn a_poll_rate_slower_than_the_config_allows_is_refused() {
        let _ = Device::history_capacity(10.0);
    }

    /// Waking at the same poll rate reuses the ring; a new rate resizes it to the new slot
    /// count. Either way every entry is a zeroed copy of the newest one, `poll_rate` apart.
    #[test]
    fn zero_status_history_reallocates_only_for_a_new_slot_count() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        let woke = Local::now();
        let ((), same_rate) =
            crate::alloc_count::count_allocations(|| device.zero_status_history(woke, POLL_RATE));
        assert_eq!(device.history_len(), 3_600);
        let ((), new_rate) =
            crate::alloc_count::count_allocations(|| device.zero_status_history(woke, 2.0));
        assert_eq!(device.history_len(), 1_800);
        assert_eq!(new_rate, same_rate + 1);

        let history = device.history_statuses();
        assert_eq!(history.last().unwrap().timestamp, woke);
        assert_eq!(
            history[0].timestamp,
            woke - Duration::from_secs_f64(1_799. * 2.0)
        );
        for status in &history {
            assert_eq!(
                status.temps,
                vec![TempStatus {
                    name: "cpu".to_string(),
                    temp: 0.0
                }]
            );
            assert_eq!(status.channels, vec![channel("fan1", Some(0.0), Some(0))]);
        }
    }

    /// `reset_stats` clears everything then reseeds from the most recent
    /// status with count=1. UI never sees a zero-count window.
    #[test]
    fn reset_stats_reseeds_from_latest_status_with_count_one() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0)],
                vec![channel("fan1", Some(50.0), Some(1200))],
            ),
            POLL_RATE,
        );
        device.set_status(status_with(
            vec![("cpu", 80.0)],
            vec![channel("fan1", Some(90.0), Some(2400))],
        ));
        device.reset_stats();
        let stats = device.stats();
        let cpu = stats.temps.get("cpu").unwrap();
        assert_eq!(cpu.count, 1);
        assert_eq!(cpu.min, 80.0);
        assert_eq!(cpu.max, 80.0);
        assert_eq!(cpu.avg, 80.0);
        let fan1 = stats.channels.get("fan1").unwrap();
        let duty = fan1.get(&ChannelDataType::Duty).unwrap();
        assert_eq!(duty.count, 1);
        assert_eq!(duty.min, 90.0);
    }

    /// A channel absent from the most-recent status must NOT carry
    /// forward a stale stat entry across reset. It reseeds on its
    /// next real observation.
    #[test]
    fn reset_stats_drops_channels_absent_from_latest_status() {
        let mut device = make_test_device();
        device.initialize_status_history_with(
            status_with(
                vec![("cpu", 40.0), ("gpu", 60.0)],
                vec![channel("fan1", Some(50.0), None)],
            ),
            POLL_RATE,
        );
        // Most-recent status has no "gpu" temp and no "fan1" channel.
        device.set_status(status_with(vec![("cpu", 50.0)], vec![]));
        device.reset_stats();
        let stats = device.stats();
        assert!(stats.temps.contains_key("cpu"));
        assert!(stats.temps.contains_key("gpu").not());
        assert!(stats.channels.contains_key("fan1").not());
    }

    /// NaN must not poison stats. Folding a NaN is a no-op.
    #[test]
    fn fold_skips_nan() {
        let mut s = ChannelStats::from_first(50.0);
        s.fold(f64::NAN);
        assert_eq!(s.count, 1);
        assert_eq!(s.avg, 50.0);
    }

    /// The first `set_status` after init seeds stats with its own values
    /// only. The init status leaves no trace in stats: count is 1 after
    /// one `set_status` (not 2), and min/max/avg reflect the `set_status`
    /// value, not the init value.
    #[test]
    fn init_then_first_set_status_seeds_stats_from_real_reading() {
        let mut device = make_test_device();
        device.initialize_status_history_with(status_with(vec![("cpu", 40.0)], vec![]), POLL_RATE);
        device.set_status(status_with(vec![("cpu", 70.0)], vec![]));
        let cpu = device.stats().temps.get("cpu").unwrap();
        assert_eq!(cpu.count, 1);
        assert_eq!(cpu.min, 70.0);
        assert_eq!(cpu.max, 70.0);
        assert_eq!(cpu.avg, 70.0);
    }

    /// Even though init does not seed stats, it must still run the
    /// augmenter on the pushed entry so the engine sees the augmented
    /// value on the first tick (e.g. the true-duty rewrite).
    #[test]
    fn init_runs_augmenter_on_pushed_status() {
        let mut device = make_test_device();
        let augmenter = Rc::new(FixedDutyAugmenter {
            target_duty: 77.0,
            invocations: RefCell::new(0),
        });
        device.set_status_augmenter(augmenter.clone());
        device.initialize_status_history_with(
            status_with(vec![], vec![channel("fan1", Some(10.0), None)]),
            POLL_RATE,
        );
        assert_eq!(*augmenter.invocations.borrow(), 1);
        let latest = device.latest_status().unwrap();
        let fan1 = latest.channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.duty, Some(77.0));
    }

    /// Regression: the CPU joule-counter produces 0.0 watts at init
    /// because there is no prior delta. That 0.0 must not become the
    /// stats min. The first real main-loop reading is what seeds stats.
    #[test]
    fn init_does_not_record_zero_watts_into_stats() {
        let mut device = make_test_device();
        let init_watts = ChannelStatus {
            name: "cpu_pkg".to_string(),
            duty: None,
            rpm: None,
            freq: None,
            watts: Some(0.0),
            pwm_mode: None,
        };
        device.initialize_status_history_with(status_with(vec![], vec![init_watts]), POLL_RATE);
        let real_watts = ChannelStatus {
            name: "cpu_pkg".to_string(),
            duty: None,
            rpm: None,
            freq: None,
            watts: Some(120.0),
            pwm_mode: None,
        };
        device.set_status(status_with(vec![], vec![real_watts]));
        let watts = device
            .stats()
            .channels
            .get("cpu_pkg")
            .unwrap()
            .get(&ChannelDataType::Watts)
            .unwrap();
        assert_eq!(watts.count, 1);
        assert_eq!(watts.min, 120.0);
        assert_eq!(watts.max, 120.0);
        assert_eq!(watts.avg, 120.0);
    }

    fn a_lighting_mode() -> LightingMode {
        LightingMode {
            name: "fixed".to_string(),
            frontend_name: "Fixed".to_string(),
            min_colors: 0,
            max_colors: 1,
            speed_enabled: false,
            backward_enabled: false,
            type_: LightingModeType::Liquidctl,
        }
    }

    fn an_lcd_mode() -> LcdMode {
        LcdMode {
            name: "liquid".to_string(),
            frontend_name: "Liquid".to_string(),
            brightness: true,
            orientation: false,
            image: false,
            colors_min: 0,
            colors_max: 0,
            type_: LcdModeType::Liquidctl,
        }
    }

    // The wire shim must always emit all five legacy fields, regardless of variant. This is the
    // contract the UI depends on (it reads `lighting_modes`/`lcd_modes` without null-guards).
    fn assert_all_wire_keys(v: &serde_json::Value) {
        for key in [
            "label",
            "speed_options",
            "lighting_modes",
            "lcd_modes",
            "lcd_info",
        ] {
            assert!(
                v.get(key).is_some(),
                "wire JSON must always include `{key}`"
            );
        }
    }

    // A Speed channel serializes to the legacy flat shape (speed_options set, the rest empty/null)
    // and round-trips. Verifies the byte-identical wire contract for REST clients.
    #[test]
    fn channel_info_speed_wire_is_flat() {
        let ci = ChannelInfo {
            label: Some("Fan 1".to_string()),
            kind: ChannelKind::Speed(SpeedOptions::default()),
        };
        let v = serde_json::to_value(&ci).unwrap();
        assert_all_wire_keys(&v);
        assert!(v.get("speed_options").unwrap().is_object());
        assert_eq!(v["lighting_modes"], serde_json::json!([]));
        assert_eq!(v["lcd_modes"], serde_json::json!([]));
        assert_eq!(v["lcd_info"], serde_json::json!(null));
        let back: ChannelInfo = serde_json::from_value(v).unwrap();
        assert_eq!(back, ci);
    }

    // A Lighting channel: lighting_modes populated, everything else empty/null.
    #[test]
    fn channel_info_lighting_wire_is_flat() {
        let ci = ChannelInfo {
            label: None,
            kind: ChannelKind::Lighting(vec![a_lighting_mode()]),
        };
        let v = serde_json::to_value(&ci).unwrap();
        assert_all_wire_keys(&v);
        assert_eq!(v["speed_options"], serde_json::json!(null));
        assert_eq!(v["lighting_modes"].as_array().unwrap().len(), 1);
        assert_eq!(v["lcd_modes"], serde_json::json!([]));
        assert_eq!(v["lcd_info"], serde_json::json!(null));
        // No deser round-trip here: `LightingMode` uses a serialize-only `rename` on `type_` (a
        // pre-existing asymmetry). ChannelInfo is response-only, so JSON deser is not a real path.
    }

    // An Lcd channel: lcd_modes + lcd_info populated, everything else empty/null.
    #[test]
    fn channel_info_lcd_wire_is_flat() {
        let ci = ChannelInfo {
            label: None,
            kind: ChannelKind::Lcd {
                modes: vec![an_lcd_mode()],
                info: Some(LcdInfo {
                    screen_width: 320,
                    screen_height: 320,
                    max_image_size_bytes: 100_000,
                    gif_supported: true,
                }),
            },
        };
        let v = serde_json::to_value(&ci).unwrap();
        assert_all_wire_keys(&v);
        assert_eq!(v["speed_options"], serde_json::json!(null));
        assert_eq!(v["lighting_modes"], serde_json::json!([]));
        assert_eq!(v["lcd_modes"].as_array().unwrap().len(), 1);
        assert!(v.get("lcd_info").unwrap().is_object());
        // No deser round-trip here: `LcdMode` uses a serialize-only `rename` on `type_` (a
        // pre-existing asymmetry). ChannelInfo is response-only, so JSON deser is not a real path.
    }

    // An info-only channel (label only): every type-specific field is empty/null but present.
    #[test]
    fn channel_info_info_only_wire_is_flat() {
        let ci = ChannelInfo {
            label: Some("CPU Temp".to_string()),
            kind: ChannelKind::InfoOnly,
        };
        let v = serde_json::to_value(&ci).unwrap();
        assert_all_wire_keys(&v);
        assert_eq!(v["speed_options"], serde_json::json!(null));
        assert_eq!(v["lighting_modes"], serde_json::json!([]));
        assert_eq!(v["lcd_modes"], serde_json::json!([]));
        assert_eq!(v["lcd_info"], serde_json::json!(null));
        let back: ChannelInfo = serde_json::from_value(v).unwrap();
        assert_eq!(back, ci);
    }

    // Deserializing a malformed flat payload with multiple categories resolves deterministically by
    // precedence (speed wins), matching the documented `From<ChannelInfoWire>` behavior.
    #[test]
    fn channel_info_deser_precedence_prefers_speed() {
        let v = serde_json::json!({
            "label": null,
            "speed_options": { "min_duty": 0, "max_duty": 100, "fixed_enabled": true, "extension": null },
            "lighting_modes": [],
            "lcd_modes": [],
            "lcd_info": null
        });
        let ci: ChannelInfo = serde_json::from_value(v).unwrap();
        assert!(matches!(ci.kind, ChannelKind::Speed(_)));
    }

    // The OpenAPI/JsonSchema for ChannelInfo must remain the flat five-field object (via the wire
    // shim), so the generated API spec does not change.
    #[test]
    fn channel_info_schema_stays_flat() {
        let schema = schemars::schema_for!(ChannelInfo);
        let value = serde_json::to_value(&schema).unwrap();
        let props = value
            .get("properties")
            .expect("schema should expose object properties");
        for key in [
            "label",
            "speed_options",
            "lighting_modes",
            "lcd_modes",
            "lcd_info",
        ] {
            assert!(
                props.get(key).is_some(),
                "schema must keep the flat `{key}` property"
            );
        }
        assert!(
            props.get("kind").is_none(),
            "schema must not expose the enum `kind`"
        );
    }
}
