// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A device's status history: a fixed-capacity ring of entries, stored column-major in one byte
//! buffer.
//!
//! Every temp and channel name owns lanes, each an array of one value per slot. A lane starts at
//! `start * capacity` bytes, where `start` is the summed width of the lanes added before it. So
//! adding a lane only grows the buffer, and a copy of any capacity keeps the same layout.
//! Presence is explicit: a temp lane pairs with a presence byte per slot and a channel with a
//! flags byte per slot. Absence is never stored as a sentinel value.

use std::borrow::Cow;
use std::fmt::{Debug, Formatter};
use std::sync::Arc;

use chrono::{DateTime, Local, TimeZone, Utc};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Serialize, Serializer};

use super::{ChannelSample, Status, Temp};

/// Every entry's UTC timestamp in nanoseconds.
const TIMESTAMPS: Lane = Lane { start: 0, width: 8 };

/// Temps, and separately channels, one device keeps. Each name holds a lane for every slot, so
/// this bounds the memory a source that keeps inventing names can take: names absent from every
/// held entry are dropped first, and only then are new ones refused. Far above any real device.
pub const MAX_NAMES: usize = 256;

/// Channel flags: the channel is present in the slot, then one bit per present field.
const CHANNEL: u8 = 1;
const RPM: u8 = 1 << 1;
const DUTY: u8 = 1 << 2;
const FREQ: u8 = 1 << 3;
const WATTS: u8 = 1 << 4;
const PWM_MODE: u8 = 1 << 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Lane {
    /// Summed width of the lanes before this one: the lane begins at `start * capacity`.
    start: usize,
    /// Bytes per slot.
    width: usize,
}

#[derive(Debug, Clone)]
struct TempColumn {
    name: Arc<str>,
    present: Lane,
    value: Lane,
}

#[derive(Debug, Clone)]
struct ChannelColumn {
    name: Arc<str>,
    flags: Lane,
    rpm: Option<Lane>,
    duty: Option<Lane>,
    freq: Option<Lane>,
    watts: Option<Lane>,
    pwm_mode: Option<Lane>,
}

#[derive(Debug, Clone, Copy)]
enum Field {
    Rpm,
    Duty,
    Freq,
    Watts,
    PwmMode,
}

impl Field {
    fn width(self) -> usize {
        match self {
            Self::Rpm | Self::Freq => 4,
            Self::Duty | Self::Watts => 8,
            Self::PwmMode => 1,
        }
    }
}

impl ChannelColumn {
    fn lane(&self, field: Field) -> Option<Lane> {
        match field {
            Field::Rpm => self.rpm,
            Field::Duty => self.duty,
            Field::Freq => self.freq,
            Field::Watts => self.watts,
            Field::PwmMode => self.pwm_mode,
        }
    }

    fn lane_mut(&mut self, field: Field) -> &mut Option<Lane> {
        match field {
            Field::Rpm => &mut self.rpm,
            Field::Duty => &mut self.duty,
            Field::Freq => &mut self.freq,
            Field::Watts => &mut self.watts,
            Field::PwmMode => &mut self.pwm_mode,
        }
    }
}

/// The names and lanes of a ring. Shared with its snapshots: offsets do not depend on capacity.
#[derive(Debug, Clone)]
struct Layout {
    /// Bytes per slot over every lane.
    stride: usize,
    /// In first-seen order.
    temps: Vec<TempColumn>,
    /// In first-seen order.
    channels: Vec<ChannelColumn>,
}

impl Layout {
    fn new() -> Self {
        Self {
            stride: TIMESTAMPS.width,
            temps: Vec::new(),
            channels: Vec::new(),
        }
    }

    fn add_lane(&mut self, width: usize) -> Lane {
        let lane = Lane {
            start: self.stride,
            width,
        };
        self.stride += width;
        lane
    }

    /// Every lane, the timestamps first.
    fn lanes(&self) -> impl Iterator<Item = Lane> + '_ {
        let temps = self
            .temps
            .iter()
            .flat_map(|column| [column.present, column.value]);
        let channels = self.channels.iter().flat_map(|column| {
            [
                Some(column.flags),
                column.rpm,
                column.duty,
                column.freq,
                column.watts,
                column.pwm_mode,
            ]
            .into_iter()
            .flatten()
        });
        std::iter::once(TIMESTAMPS).chain(temps).chain(channels)
    }

    fn temp_index(&self, name: &str) -> Option<usize> {
        self.temps.iter().position(|column| &*column.name == name)
    }

    fn channel_index(&self, name: &str) -> Option<usize> {
        self.channels
            .iter()
            .position(|column| &*column.name == name)
    }
}

fn read<const N: usize>(bytes: &[u8], capacity: usize, lane: Lane, slot: usize) -> [u8; N] {
    debug_assert_eq!(lane.width, N);
    debug_assert!(slot < capacity);
    let at = lane.start * capacity + slot * N;
    bytes[at..at + N]
        .try_into()
        .expect("a lane holds whole values")
}

fn write<const N: usize>(
    bytes: &mut [u8],
    capacity: usize,
    lane: Lane,
    slot: usize,
    value: [u8; N],
) {
    debug_assert_eq!(lane.width, N);
    debug_assert!(slot < capacity);
    let at = lane.start * capacity + slot * N;
    bytes[at..at + N].copy_from_slice(&value);
}

fn temp_in(bytes: &[u8], capacity: usize, column: &TempColumn, slot: usize) -> Option<Temp> {
    let [present] = read(bytes, capacity, column.present, slot);
    (present != 0).then(|| f64::from_le_bytes(read(bytes, capacity, column.value, slot)))
}

fn channel_in(
    bytes: &[u8],
    capacity: usize,
    column: &ChannelColumn,
    slot: usize,
) -> Option<ChannelSample> {
    let [flags] = read(bytes, capacity, column.flags, slot);
    if flags & CHANNEL == 0 {
        return None;
    }
    let lane = |bit: u8, lane: Option<Lane>| {
        (flags & bit != 0).then(|| lane.expect("a present field has a lane"))
    };
    Some(ChannelSample {
        rpm: lane(RPM, column.rpm)
            .map(|lane| u32::from_le_bytes(read(bytes, capacity, lane, slot))),
        duty: lane(DUTY, column.duty)
            .map(|lane| f64::from_le_bytes(read(bytes, capacity, lane, slot))),
        freq: lane(FREQ, column.freq)
            .map(|lane| u32::from_le_bytes(read(bytes, capacity, lane, slot))),
        watts: lane(WATTS, column.watts)
            .map(|lane| f64::from_le_bytes(read(bytes, capacity, lane, slot))),
        pwm_mode: lane(PWM_MODE, column.pwm_mode)
            .map(|lane| read::<1>(bytes, capacity, lane, slot)[0]),
    })
}

/// A timestamp as stored: UTC nanoseconds. Instants outside 1677..2262 saturate.
pub fn timestamp_ns(timestamp: DateTime<Local>) -> i64 {
    timestamp
        .timestamp_nanos_opt()
        .unwrap_or(if timestamp.timestamp() < 0 {
            i64::MIN
        } else {
            i64::MAX
        })
}

/// A stored timestamp in local time, with the offset the zone rules give for that instant.
pub fn local_time(timestamp_ns: i64) -> DateTime<Local> {
    Local.from_utc_datetime(&DateTime::<Utc>::from_timestamp_nanos(timestamp_ns).naive_utc())
}

/// The names one entry holds, each channel with its values zeroed.
pub struct ZeroedNames {
    pub temps: Vec<Arc<str>>,
    pub channels: Vec<(Arc<str>, ChannelSample)>,
}

#[derive(Clone)]
pub struct StatusRing {
    capacity: usize,
    /// The slot the next entry is written to.
    head: usize,
    /// Filled slots, at most `capacity`.
    len: usize,
    layout: Arc<Layout>,
    bytes: Vec<u8>,
    /// Entries pushed since the last reset.
    pushes: u64,
    /// Per temp column, the value of `pushes` when it was added or last written. Kept out of
    /// the shared layout so a write never copies it.
    temp_written: Vec<u64>,
    /// Per channel column, as `temp_written`.
    channel_written: Vec<u64>,
}

impl StatusRing {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        let layout = Layout::new();
        let bytes = vec![0; layout.stride * capacity];
        Self {
            capacity,
            head: 0,
            len: 0,
            layout: Arc::new(layout),
            bytes,
            pushes: 0,
            temp_written: Vec::new(),
            channel_written: Vec::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.len
    }

    /// Empties the ring, names included, and sizes it to `capacity` slots. The buffer is only
    /// reallocated when the capacity changes; it is then reserved for the old names, which a
    /// rebuild usually adds again.
    pub fn reset(&mut self, capacity: usize) {
        assert!(capacity > 0);
        if capacity == self.capacity {
            self.bytes.clear();
        } else {
            self.bytes = Vec::with_capacity(self.layout.stride * capacity);
        }
        self.layout = Arc::new(Layout::new());
        self.capacity = capacity;
        self.head = 0;
        self.len = 0;
        self.pushes = 0;
        self.temp_written.clear();
        self.channel_written.clear();
        self.fit_lanes();
    }

    /// Sizes the buffer to the layout after lanes were added. Exact: the ring never grows
    /// again, so amortized slack would be held for good.
    fn fit_lanes(&mut self) {
        let size = self.layout.stride * self.capacity;
        debug_assert!(size >= self.bytes.len());
        self.bytes.reserve_exact(size - self.bytes.len());
        self.bytes.resize(size, 0);
    }

    /// The slot of history `index`, 0 being the oldest entry.
    fn slot(&self, index: usize) -> usize {
        assert!(
            index < self.len,
            "index {index} beyond {} entries",
            self.len
        );
        (self.head + self.capacity - self.len + index) % self.capacity
    }

    /// Starts a new newest entry stamped `timestamp_ns` and returns its slot. Overwrites the
    /// oldest entry when full. Every name starts absent from it.
    pub fn push(&mut self, timestamp_ns: i64) -> usize {
        let slot = self.head;
        self.head = (self.head + 1) % self.capacity;
        self.len = (self.len + 1).min(self.capacity);
        self.pushes += 1;
        let capacity = self.capacity;
        write(
            &mut self.bytes,
            capacity,
            TIMESTAMPS,
            slot,
            timestamp_ns.to_le_bytes(),
        );
        for column in &self.layout.temps {
            write(&mut self.bytes, capacity, column.present, slot, [0]);
        }
        for column in &self.layout.channels {
            write(&mut self.bytes, capacity, column.flags, slot, [0]);
        }
        slot
    }

    /// The column of temp `name`, added absent from every entry on first sight. Past
    /// `MAX_NAMES` temps a column no held entry has a value in is renamed instead; `None` when
    /// there is none.
    pub fn temp_column(&mut self, name: &str) -> Option<usize> {
        if let Some(index) = self.layout.temp_index(name) {
            return Some(index);
        }
        if self.layout.temps.len() >= MAX_NAMES {
            let index = self.unwritten_column(&self.temp_written)?;
            Arc::make_mut(&mut self.layout).temps[index].name = Arc::from(name);
            self.temp_written[index] = self.pushes;
            return Some(index);
        }
        self.temp_written.push(self.pushes);
        let layout = Arc::make_mut(&mut self.layout);
        let present = layout.add_lane(1);
        let value = layout.add_lane(8);
        layout.temps.push(TempColumn {
            name: Arc::from(name),
            present,
            value,
        });
        let index = layout.temps.len() - 1;
        self.fit_lanes();
        Some(index)
    }

    /// The column of channel `name`, added absent from every entry on first sight. Past
    /// `MAX_NAMES` channels a column no held entry has a value in is renamed instead; `None`
    /// when there is none.
    pub fn channel_column(&mut self, name: &str) -> Option<usize> {
        if let Some(index) = self.layout.channel_index(name) {
            return Some(index);
        }
        if self.layout.channels.len() >= MAX_NAMES {
            let index = self.unwritten_column(&self.channel_written)?;
            Arc::make_mut(&mut self.layout).channels[index].name = Arc::from(name);
            self.channel_written[index] = self.pushes;
            return Some(index);
        }
        self.channel_written.push(self.pushes);
        let layout = Arc::make_mut(&mut self.layout);
        let flags = layout.add_lane(1);
        layout.channels.push(ChannelColumn {
            name: Arc::from(name),
            flags,
            rpm: None,
            duty: None,
            freq: None,
            watts: None,
            pwm_mode: None,
        });
        let index = layout.channels.len() - 1;
        self.fit_lanes();
        Some(index)
    }

    /// The column of temp `name`, trying column `hint` first. See `temp_column`.
    pub fn temp_column_from(&mut self, hint: usize, name: &str) -> Option<usize> {
        let at_hint = self.layout.temps.get(hint);
        if at_hint.is_some_and(|column| &*column.name == name) {
            return Some(hint);
        }
        self.temp_column(name)
    }

    /// The column of channel `name`, trying column `hint` first. See `channel_column`.
    pub fn channel_column_from(&mut self, hint: usize, name: &str) -> Option<usize> {
        let at_hint = self.layout.channels.get(hint);
        if at_hint.is_some_and(|column| &*column.name == name) {
            return Some(hint);
        }
        self.channel_column(name)
    }

    /// A column unwritten for `capacity` pushes. Every slot has been pushed since, which
    /// cleared its presence there, so no held entry has a value in it.
    fn unwritten_column(&self, written: &[u64]) -> Option<usize> {
        let capacity = u64::try_from(self.capacity).expect("a capacity fits in u64");
        written
            .iter()
            .position(|written| written + capacity <= self.pushes)
    }

    /// The lane of a channel field, added on the field's first sight.
    fn field_lane(&mut self, column: usize, field: Field) -> Lane {
        if let Some(lane) = self.layout.channels[column].lane(field) {
            return lane;
        }
        let layout = Arc::make_mut(&mut self.layout);
        let lane = layout.add_lane(field.width());
        *layout.channels[column].lane_mut(field) = Some(lane);
        self.fit_lanes();
        lane
    }

    /// Writes `status` as the newest entry. Returns `false` when a name was refused (see
    /// `MAX_NAMES`); every other value is still written.
    pub fn push_status(&mut self, status: &Status) -> bool {
        let slot = self.push(timestamp_ns(status.timestamp));
        let mut kept_all = true;
        for temp in &status.temps {
            match self.temp_column(&temp.name) {
                Some(column) => self.set_temp(slot, column, temp.temp),
                None => kept_all = false,
            }
        }
        for channel in &status.channels {
            match self.channel_column(&channel.name) {
                Some(column) => self.set_channel(slot, column, ChannelSample::from(channel)),
                None => kept_all = false,
            }
        }
        kept_all
    }

    pub fn set_temp(&mut self, slot: usize, column: usize, temp: Temp) {
        self.temp_written[column] = self.pushes;
        let TempColumn { present, value, .. } = self.layout.temps[column];
        let capacity = self.capacity;
        write(&mut self.bytes, capacity, value, slot, temp.to_le_bytes());
        write(&mut self.bytes, capacity, present, slot, [1]);
    }

    pub fn set_channel(&mut self, slot: usize, column: usize, sample: ChannelSample) {
        self.channel_written[column] = self.pushes;
        let mut flags = CHANNEL;
        if let Some(rpm) = sample.rpm {
            self.write_field(slot, column, Field::Rpm, &rpm.to_le_bytes());
            flags |= RPM;
        }
        if let Some(duty) = sample.duty {
            self.write_field(slot, column, Field::Duty, &duty.to_le_bytes());
            flags |= DUTY;
        }
        if let Some(freq) = sample.freq {
            self.write_field(slot, column, Field::Freq, &freq.to_le_bytes());
            flags |= FREQ;
        }
        if let Some(watts) = sample.watts {
            self.write_field(slot, column, Field::Watts, &watts.to_le_bytes());
            flags |= WATTS;
        }
        if let Some(pwm_mode) = sample.pwm_mode {
            self.write_field(slot, column, Field::PwmMode, &[pwm_mode]);
            flags |= PWM_MODE;
        }
        let lane = self.layout.channels[column].flags;
        write(&mut self.bytes, self.capacity, lane, slot, [flags]);
    }

    fn write_field(&mut self, slot: usize, column: usize, field: Field, value: &[u8]) {
        let lane = self.field_lane(column, field);
        assert_eq!(lane.width, value.len());
        let at = lane.start * self.capacity + slot * lane.width;
        self.bytes[at..at + lane.width].copy_from_slice(value);
    }

    pub fn timestamp_ns(&self, index: usize) -> i64 {
        i64::from_le_bytes(read(
            &self.bytes,
            self.capacity,
            TIMESTAMPS,
            self.slot(index),
        ))
    }

    pub fn temp_at(&self, index: usize, name: &str) -> Option<Temp> {
        let slot = self.slot(index);
        let column = &self.layout.temps[self.layout.temp_index(name)?];
        temp_in(&self.bytes, self.capacity, column, slot)
    }

    pub fn channel_at(&self, index: usize, name: &str) -> Option<ChannelSample> {
        let slot = self.slot(index);
        let column = &self.layout.channels[self.layout.channel_index(name)?];
        channel_in(&self.bytes, self.capacity, column, slot)
    }

    /// The temps of history `index`, in first-seen order.
    pub fn temps_at(&self, index: usize) -> impl Iterator<Item = (&str, Temp)> {
        let slot = self.slot(index);
        self.layout.temps.iter().filter_map(move |column| {
            temp_in(&self.bytes, self.capacity, column, slot).map(|temp| (&*column.name, temp))
        })
    }

    /// The channels of history `index`, in first-seen order.
    pub fn channels_at(&self, index: usize) -> impl Iterator<Item = (&str, ChannelSample)> {
        let slot = self.slot(index);
        self.layout.channels.iter().filter_map(move |column| {
            channel_in(&self.bytes, self.capacity, column, slot)
                .map(|sample| (&*column.name, sample))
        })
    }

    /// Fills the duty of channel `name` wherever an entry has its rpm but no duty, with
    /// `duty_for_rpm`. Returns whether any entry was filled.
    pub fn backfill_channel_duty(
        &mut self,
        name: &str,
        duty_for_rpm: impl Fn(u32) -> Option<f64>,
    ) -> bool {
        let Some(column) = self.layout.channel_index(name) else {
            return false;
        };
        let mut filled_any = false;
        for index in 0..self.len {
            let slot = self.slot(index);
            let flags_lane = self.layout.channels[column].flags;
            let [flags] = read(&self.bytes, self.capacity, flags_lane, slot);
            if flags & CHANNEL == 0 || flags & RPM == 0 || flags & DUTY != 0 {
                continue;
            }
            let rpm_lane = self.layout.channels[column]
                .rpm
                .expect("a present field has a lane");
            let rpm = u32::from_le_bytes(read(&self.bytes, self.capacity, rpm_lane, slot));
            let Some(duty) = duty_for_rpm(rpm) else {
                continue;
            };
            self.write_field(slot, column, Field::Duty, &duty.to_le_bytes());
            write(
                &mut self.bytes,
                self.capacity,
                flags_lane,
                slot,
                [flags | DUTY],
            );
            filled_any = true;
        }
        filled_any
    }

    /// Sets temp `name` in every entry, `values[index]` at history `index`. Returns `false`
    /// when the name was refused (see `MAX_NAMES`).
    pub fn set_temp_everywhere(&mut self, name: &str, values: &[Temp]) -> bool {
        assert_eq!(values.len(), self.len);
        let Some(column) = self.temp_column(name) else {
            return false;
        };
        for (index, value) in values.iter().enumerate() {
            let slot = self.slot(index);
            self.set_temp(slot, column, *value);
        }
        true
    }

    /// Sets channel `name` in every entry, `samples[index]` at history `index`. Returns
    /// `false` when the name was refused (see `MAX_NAMES`).
    pub fn set_channel_everywhere(&mut self, name: &str, samples: &[ChannelSample]) -> bool {
        assert_eq!(samples.len(), self.len);
        let Some(column) = self.channel_column(name) else {
            return false;
        };
        for (index, sample) in samples.iter().enumerate() {
            let slot = self.slot(index);
            self.set_channel(slot, column, *sample);
        }
        true
    }

    /// The names present in history `index`, each channel with its values zeroed.
    pub fn zeroed_names_at(&self, index: usize) -> ZeroedNames {
        let slot = self.slot(index);
        let temps = self
            .layout
            .temps
            .iter()
            .filter(|column| temp_in(&self.bytes, self.capacity, column, slot).is_some())
            .map(|column| Arc::clone(&column.name))
            .collect();
        let channels = self
            .layout
            .channels
            .iter()
            .filter_map(|column| {
                channel_in(&self.bytes, self.capacity, column, slot)
                    .map(|sample| (Arc::clone(&column.name), sample.zeroed()))
            })
            .collect();
        ZeroedNames { temps, channels }
    }

    /// Removes the duty of channel `name` from every entry.
    pub fn clear_channel_duty(&mut self, name: &str) {
        let Some(column) = self.layout.channel_index(name) else {
            return;
        };
        let flags_lane = self.layout.channels[column].flags;
        for slot in 0..self.capacity {
            let [flags] = read(&self.bytes, self.capacity, flags_lane, slot);
            write(
                &mut self.bytes,
                self.capacity,
                flags_lane,
                slot,
                [flags & !DUTY],
            );
        }
    }

    /// Drops the temp and the channel called `name`, and their lanes.
    pub fn remove_name(&mut self, name: &str) {
        let temp = self.layout.temp_index(name);
        let channel = self.layout.channel_index(name);
        if temp.is_none() && channel.is_none() {
            return;
        }
        let mut kept = (*self.layout).clone();
        if let Some(index) = temp {
            kept.temps.remove(index);
            self.temp_written.remove(index);
        }
        if let Some(index) = channel {
            kept.channels.remove(index);
            self.channel_written.remove(index);
        }
        self.relayout(&kept);
    }

    /// Moves every lane of `kept` (a subset of the current layout) to a compact layout.
    fn relayout(&mut self, kept: &Layout) {
        let mut layout = Layout::new();
        let mut relocations = vec![(TIMESTAMPS, TIMESTAMPS)];
        let mut relocate = |layout: &mut Layout, lane: Lane| {
            let new = layout.add_lane(lane.width);
            relocations.push((lane, new));
            new
        };
        for column in &kept.temps {
            let present = relocate(&mut layout, column.present);
            let value = relocate(&mut layout, column.value);
            layout.temps.push(TempColumn {
                name: Arc::clone(&column.name),
                present,
                value,
            });
        }
        for column in &kept.channels {
            let flags = relocate(&mut layout, column.flags);
            let mut field = |lane: Option<Lane>| lane.map(|lane| relocate(&mut layout, lane));
            let rpm = field(column.rpm);
            let duty = field(column.duty);
            let freq = field(column.freq);
            let watts = field(column.watts);
            let pwm_mode = field(column.pwm_mode);
            layout.channels.push(ChannelColumn {
                name: Arc::clone(&column.name),
                flags,
                rpm,
                duty,
                freq,
                watts,
                pwm_mode,
            });
        }
        let mut bytes = vec![0; layout.stride * self.capacity];
        for (old, new) in relocations {
            let size = old.width * self.capacity;
            let from = old.start * self.capacity;
            let to = new.start * self.capacity;
            bytes[to..to + size].copy_from_slice(&self.bytes[from..from + size]);
        }
        self.layout = Arc::new(layout);
        self.bytes = bytes;
    }

    /// A copy of every entry, for the API. One allocation.
    pub fn snapshot(&self, filter: SnapshotFilter) -> StatusSnapshot {
        StatusSnapshot {
            layout: Arc::clone(&self.layout),
            capacity: self.capacity,
            head: self.head,
            len: self.len,
            bytes: self.bytes.as_slice().into(),
            filter,
        }
    }

    /// A copy of the newest entry, for the API. At most one allocation.
    pub fn snapshot_latest(&self, filter: SnapshotFilter) -> StatusSnapshot {
        let latest = self.len.checked_sub(1);
        self.snapshot_of(latest.into_iter(), usize::from(latest.is_some()), filter)
    }

    /// A copy of the entries stamped strictly after `after_ns`, judged entry by entry: a
    /// clock step can leave the history out of order. At most one allocation.
    pub fn snapshot_after(&self, after_ns: i64) -> StatusSnapshot {
        let after = |index: &usize| self.timestamp_ns(*index) > after_ns;
        let count = (0..self.len).filter(after).count();
        self.snapshot_of((0..self.len).filter(after), count, SnapshotFilter::All)
    }

    /// A compact copy of the `count` entries at history `indices`, oldest first. At capacity
    /// `count` and head 0, entry `n` sits in slot `n`.
    fn snapshot_of(
        &self,
        indices: impl Iterator<Item = usize>,
        count: usize,
        filter: SnapshotFilter,
    ) -> StatusSnapshot {
        assert!(count <= self.len);
        let mut bytes = vec![0; self.layout.stride * count].into_boxed_slice();
        let mut copied = 0;
        for index in indices {
            let slot = self.slot(index);
            for lane in self.layout.lanes() {
                let from = lane.start * self.capacity + slot * lane.width;
                let to = lane.start * count + copied * lane.width;
                bytes[to..to + lane.width].copy_from_slice(&self.bytes[from..from + lane.width]);
            }
            copied += 1;
        }
        assert_eq!(copied, count);
        StatusSnapshot {
            layout: Arc::clone(&self.layout),
            // A snapshot without entries still needs a nonzero modulus.
            capacity: count.max(1),
            head: 0,
            len: count,
            bytes,
            filter,
        }
    }

    /// The filter keeping only the temp or channel `name`.
    pub fn name_filter(&self, name: &str) -> SnapshotFilter {
        SnapshotFilter::Name {
            temp: self.layout.temp_index(name),
            channel: self.layout.channel_index(name),
        }
    }
}

impl Debug for StatusRing {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("StatusRing");
        debug
            .field("len", &self.len)
            .field("capacity", &self.capacity);
        if let Some(index) = self.len.checked_sub(1) {
            debug
                .field("latest_temps", &self.temps_at(index).collect::<Vec<_>>())
                .field(
                    "latest_channels",
                    &self.channels_at(index).collect::<Vec<_>>(),
                );
        }
        debug.finish_non_exhaustive()
    }
}

/// Which names a snapshot serializes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotFilter {
    All,
    /// Every entry, with only the temp and the channel at these column indices.
    Name {
        temp: Option<usize>,
        channel: Option<usize>,
    },
}

/// An owned copy of a device's history, made on the engine thread and serialized on the API
/// thread. Serializes as the array of `Status` the API has always sent.
#[derive(Clone)]
pub struct StatusSnapshot {
    layout: Arc<Layout>,
    capacity: usize,
    head: usize,
    len: usize,
    bytes: Box<[u8]>,
    filter: SnapshotFilter,
}

impl StatusSnapshot {
    fn slot(&self, index: usize) -> usize {
        assert!(index < self.len);
        (self.head + self.capacity - self.len + index) % self.capacity
    }

    fn timestamp_ns(&self, slot: usize) -> i64 {
        i64::from_le_bytes(read(&self.bytes, self.capacity, TIMESTAMPS, slot))
    }

    fn keeps_temp(&self, column: usize) -> bool {
        match self.filter {
            SnapshotFilter::Name { temp, .. } => temp == Some(column),
            SnapshotFilter::All => true,
        }
    }

    fn keeps_channel(&self, column: usize) -> bool {
        match self.filter {
            SnapshotFilter::Name { channel, .. } => channel == Some(column),
            SnapshotFilter::All => true,
        }
    }

    fn temps_in(&self, slot: usize) -> impl Iterator<Item = (&str, Temp)> {
        self.layout
            .temps
            .iter()
            .enumerate()
            .filter(|(index, _)| self.keeps_temp(*index))
            .filter_map(move |(_, column)| {
                temp_in(&self.bytes, self.capacity, column, slot).map(|temp| (&*column.name, temp))
            })
    }

    fn channels_in(&self, slot: usize) -> impl Iterator<Item = (&str, ChannelSample)> {
        self.layout
            .channels
            .iter()
            .enumerate()
            .filter(|(index, _)| self.keeps_channel(*index))
            .filter_map(move |(_, column)| {
                channel_in(&self.bytes, self.capacity, column, slot)
                    .map(|sample| (&*column.name, sample))
            })
    }

    /// The newest entry's temps, in first-seen order.
    pub fn latest_temps(&self) -> impl Iterator<Item = (&str, Temp)> {
        let slot = self.len.checked_sub(1).map(|index| self.slot(index));
        slot.into_iter().flat_map(|slot| self.temps_in(slot))
    }

    /// The newest entry's channels, in first-seen order.
    pub fn latest_channels(&self) -> impl Iterator<Item = (&str, ChannelSample)> {
        let slot = self.len.checked_sub(1).map(|index| self.slot(index));
        slot.into_iter().flat_map(|slot| self.channels_in(slot))
    }

    /// A snapshot of `statuses`, oldest first, for tests that build API responses by hand.
    #[cfg(test)]
    pub fn from_statuses(statuses: &[Status]) -> Self {
        let mut ring = StatusRing::new(statuses.len().max(1));
        for status in statuses {
            ring.push_status(status);
        }
        ring.snapshot(SnapshotFilter::All)
    }
}

impl Debug for StatusSnapshot {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StatusSnapshot")
            .field("len", &self.len)
            .field("filter", &self.filter)
            .finish_non_exhaustive()
    }
}

impl Serialize for StatusSnapshot {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(self.len))?;
        for index in 0..self.len {
            seq.serialize_element(&Entry {
                snapshot: self,
                slot: self.slot(index),
            })?;
        }
        seq.end()
    }
}

impl JsonSchema for StatusSnapshot {
    fn inline_schema() -> bool {
        <Vec<Status>>::inline_schema()
    }

    fn schema_name() -> Cow<'static, str> {
        <Vec<Status>>::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        <Vec<Status>>::schema_id()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        <Vec<Status>>::json_schema(generator)
    }
}

/// One entry, serialized like a `Status`.
struct Entry<'a> {
    snapshot: &'a StatusSnapshot,
    slot: usize,
}

impl Serialize for Entry<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let has_temps = self.snapshot.temps_in(self.slot).next().is_some();
        let has_channels = self.snapshot.channels_in(self.slot).next().is_some();
        let fields = 1 + usize::from(has_temps) + usize::from(has_channels);
        let mut state = serializer.serialize_struct("Status", fields)?;
        let timestamp = local_time(self.snapshot.timestamp_ns(self.slot));
        state.serialize_field("timestamp", &timestamp)?;
        if has_temps {
            state.serialize_field("temps", &Temps(self))?;
        } else {
            state.skip_field("temps")?;
        }
        if has_channels {
            state.serialize_field("channels", &Channels(self))?;
        } else {
            state.skip_field("channels")?;
        }
        state.end()
    }
}

struct Temps<'a, 'b>(&'b Entry<'a>);

impl Serialize for Temps<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Entry { snapshot, slot } = self.0;
        serializer.collect_seq(
            snapshot
                .temps_in(*slot)
                .map(|(name, temp)| TempOut { name, temp }),
        )
    }
}

struct Channels<'a, 'b>(&'b Entry<'a>);

impl Serialize for Channels<'_, '_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let Entry { snapshot, slot } = self.0;
        serializer.collect_seq(
            snapshot
                .channels_in(*slot)
                .map(|(name, sample)| ChannelOut::of(name, sample)),
        )
    }
}

/// Serializes exactly like `TempStatus`.
#[derive(Serialize)]
struct TempOut<'a> {
    name: &'a str,
    temp: Temp,
}

/// Serializes exactly like `ChannelStatus`.
#[derive(Serialize)]
struct ChannelOut<'a> {
    name: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    rpm: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    duty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    freq: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    watts: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pwm_mode: Option<u8>,
}

impl<'a> ChannelOut<'a> {
    fn of(name: &'a str, sample: ChannelSample) -> Self {
        Self {
            name,
            rpm: sample.rpm,
            duty: sample.duty,
            freq: sample.freq,
            watts: sample.watts,
            pwm_mode: sample.pwm_mode,
        }
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp)] // exact equality on known test constants is intentional
mod tests {
    use super::*;
    use std::ops::Not;

    fn fan(rpm: u32) -> ChannelSample {
        ChannelSample {
            rpm: Some(rpm),
            ..Default::default()
        }
    }

    /// Pushes an entry stamped `at` with `temp1` and, when given, `fan1`.
    fn push(ring: &mut StatusRing, at: i64, temp: f64, rpm: Option<u32>) {
        let slot = ring.push(at);
        let column = ring.temp_column("temp1").unwrap();
        ring.set_temp(slot, column, temp);
        if let Some(rpm) = rpm {
            let column = ring.channel_column("fan1").unwrap();
            ring.set_channel(slot, column, fan(rpm));
        }
    }

    fn timestamps(ring: &StatusRing) -> Vec<i64> {
        (0..ring.len())
            .map(|index| ring.timestamp_ns(index))
            .collect()
    }

    #[test]
    fn a_full_ring_overwrites_its_oldest_entry() {
        // Goal: once full, each push replaces the oldest entry and indices stay oldest
        // first. Method: five pushes into three slots, then read every index back.
        let mut ring = StatusRing::new(3);
        for at in 1..=5 {
            push(&mut ring, at, f64::from(u8::try_from(at).unwrap()), None);
        }
        assert_eq!(ring.len(), 3);
        assert_eq!(timestamps(&ring), vec![3, 4, 5]);
        let temps: Vec<_> = (0..3).map(|index| ring.temp_at(index, "temp1")).collect();
        assert_eq!(temps, vec![Some(3.0), Some(4.0), Some(5.0)]);
    }

    #[test]
    fn an_overwritten_slot_forgets_the_names_it_held() {
        // Goal: a slot reused by a push starts empty, so a name the new entry lacks is
        // absent rather than the value of the entry it replaced.
        let mut ring = StatusRing::new(2);
        push(&mut ring, 1, 40.0, Some(1200));
        push(&mut ring, 2, 41.0, Some(1210));
        push(&mut ring, 3, 42.0, None);
        assert_eq!(ring.channel_at(1, "fan1"), None);
        assert_eq!(ring.channel_at(0, "fan1"), Some(fan(1210)));
    }

    #[test]
    fn a_name_first_seen_late_is_absent_from_older_entries() {
        // Goal: adding a column marks it absent everywhere before, and a channel field seen
        // for the first time is absent from entries that had the channel without it.
        let mut ring = StatusRing::new(4);
        push(&mut ring, 1, 40.0, Some(1200));
        let slot = ring.push(2);
        let fan1 = ring.channel_column("fan1").unwrap();
        ring.set_channel(
            slot,
            fan1,
            ChannelSample {
                rpm: Some(1300),
                duty: Some(55.0),
                ..Default::default()
            },
        );
        let late = ring.temp_column("late").unwrap();
        ring.set_temp(slot, late, 30.0);
        assert_eq!(ring.temp_at(0, "late"), None);
        assert_eq!(ring.temp_at(1, "late"), Some(30.0));
        assert_eq!(ring.channel_at(0, "fan1"), Some(fan(1200)));
        assert_eq!(ring.channel_at(1, "fan1").unwrap().duty, Some(55.0));
        // temp1 is absent from the second entry: it was never set there.
        assert_eq!(ring.temp_at(1, "temp1"), None);
    }

    #[test]
    fn removing_a_name_keeps_every_other_value() {
        // Goal: dropping a column compacts the lanes without disturbing any other name's
        // values, presence or first-seen order.
        let mut ring = StatusRing::new(3);
        for at in 1..=3 {
            let slot = ring.push(at);
            for name in ["a", "b", "c"] {
                let column = ring.temp_column(name).unwrap();
                ring.set_temp(slot, column, f64::from(u8::try_from(at).unwrap()));
            }
            let column = ring.channel_column("b").unwrap();
            ring.set_channel(slot, column, fan(u32::try_from(at).unwrap()));
        }
        let before: Vec<_> = (0..3).map(|index| ring.temp_at(index, "c")).collect();
        ring.remove_name("b");
        assert_eq!(timestamps(&ring), vec![1, 2, 3]);
        assert_eq!(ring.temp_at(2, "b"), None);
        assert_eq!(ring.channel_at(2, "b"), None);
        let after: Vec<_> = (0..3).map(|index| ring.temp_at(index, "c")).collect();
        assert_eq!(after, before);
        let names: Vec<&str> = ring.temps_at(2).map(|(name, _)| name).collect();
        assert_eq!(names, vec!["a", "c"]);
        // Pushing after the relayout still works.
        push(&mut ring, 4, 44.0, Some(1));
        assert_eq!(ring.temp_at(2, "temp1"), Some(44.0));
    }

    #[test]
    fn names_that_rolled_off_make_room_for_new_ones() {
        // Goal: a source that invents a new name every tick cannot grow the ring past
        // `MAX_NAMES`: names absent from every held entry are dropped to make room.
        // Method: four slots, ten times `MAX_NAMES` distinct names, one per entry.
        let mut ring = StatusRing::new(4);
        for at in 0..i64::try_from(10 * MAX_NAMES).unwrap() {
            let slot = ring.push(at);
            let name = format!("invented{at}");
            let column = ring.temp_column(&name).expect("room is made");
            ring.set_temp(slot, column, 1.0);
            let column = ring.channel_column(&name).expect("room is made");
            ring.set_channel(slot, column, fan(1));
        }
        assert!(ring.layout.temps.len() <= MAX_NAMES);
        assert!(ring.layout.channels.len() <= MAX_NAMES);
        assert!(ring.bytes.len() <= (8 + MAX_NAMES * (9 + 5)) * 4);
        // The newest entries keep their own names.
        assert_eq!(
            ring.temp_at(3, &format!("invented{}", 10 * MAX_NAMES - 1)),
            Some(1.0)
        );
    }

    #[test]
    fn a_new_name_is_refused_while_every_kept_name_is_in_use() {
        // Goal: past `MAX_NAMES` names that all have values in the history, a new name gets
        // no column and the names already kept are untouched.
        let mut ring = StatusRing::new(2);
        let slot = ring.push(1);
        for index in 0..MAX_NAMES {
            let column = ring.temp_column(&format!("t{index}")).unwrap();
            ring.set_temp(slot, column, 2.0);
        }
        assert_eq!(ring.temp_column("one too many"), None);
        assert_eq!(ring.temp_at(0, "t0"), Some(2.0));
        assert_eq!(ring.temp_at(0, &format!("t{}", MAX_NAMES - 1)), Some(2.0));
        // Channels have their own allowance.
        assert!(ring.channel_column("fan1").is_some());
    }

    #[test]
    fn duty_backfill_fills_only_rpm_entries_without_duty_and_clear_removes_it() {
        // Goal: the calibration backfill fills duty where the channel has rpm and no duty,
        // reports whether it filled anything, and the clear removes duty everywhere.
        let mut ring = StatusRing::new(4);
        push(&mut ring, 1, 40.0, Some(1000));
        push(&mut ring, 2, 40.0, None);
        let slot = ring.push(3);
        let fan1 = ring.channel_column("fan1").unwrap();
        ring.set_channel(
            slot,
            fan1,
            ChannelSample {
                rpm: Some(2000),
                duty: Some(80.0),
                ..Default::default()
            },
        );
        let filled = ring.backfill_channel_duty("fan1", |rpm| Some(f64::from(rpm) / 100.));
        assert!(filled);
        assert_eq!(ring.channel_at(0, "fan1").unwrap().duty, Some(10.0));
        assert_eq!(ring.channel_at(1, "fan1"), None);
        assert_eq!(ring.channel_at(2, "fan1").unwrap().duty, Some(80.0));
        assert!(ring.backfill_channel_duty("fan1", |_| None).not());
        ring.clear_channel_duty("fan1");
        assert_eq!(ring.channel_at(0, "fan1"), Some(fan(1000)));
        assert_eq!(ring.channel_at(2, "fan1"), Some(fan(2000)));
    }

    #[test]
    fn a_snapshot_is_independent_of_later_writes() {
        // Goal: a snapshot owns its copy. Writes and new names after it was taken do not
        // reach it, and the newest-entry copy holds exactly that entry.
        let mut ring = StatusRing::new(3);
        push(&mut ring, 1, 40.0, Some(1200));
        let all = ring.snapshot(SnapshotFilter::All);
        let latest = ring.snapshot_latest(SnapshotFilter::All);
        push(&mut ring, 2, 50.0, Some(1300));
        let column = ring.temp_column("late").unwrap();
        let slot = ring.push(3);
        ring.set_temp(slot, column, 1.0);
        assert_eq!(all.len, 1);
        assert_eq!(
            all.latest_temps().collect::<Vec<_>>(),
            vec![("temp1", 40.0)]
        );
        assert_eq!(
            latest.latest_channels().collect::<Vec<_>>(),
            vec![("fan1", fan(1200))]
        );
    }

    #[test]
    fn a_since_snapshot_holds_only_the_entries_after_it() {
        // Goal: a `since` snapshot copies only the entries stamped after the bound, judged
        // entry by entry, and serializes exactly as the whole history filtered by hand would.
        // Method: a full ring with a clock step back, then compare against a filtered copy.
        let mut ring = StatusRing::new(4);
        for (at, rpm) in [
            (1, None),
            (2, Some(1200)),
            (5, None),
            (3, Some(1300)),
            (6, None),
        ] {
            push(&mut ring, at, f64::from(u8::try_from(at).unwrap()), rpm);
        }
        let since = ring.snapshot_after(2);
        assert_eq!(since.len, 3);
        assert_eq!(since.bytes.len(), ring.layout.stride * 3);
        let mut expected = StatusRing::new(3);
        for (at, rpm) in [(5, None), (3, Some(1300)), (6, None)] {
            push(&mut expected, at, f64::from(u8::try_from(at).unwrap()), rpm);
        }
        assert_eq!(
            serde_json::to_string(&since).unwrap(),
            serde_json::to_string(&expected.snapshot(SnapshotFilter::All)).unwrap()
        );
        let none = ring.snapshot_after(6);
        assert_eq!(none.bytes.len(), 0);
        assert_eq!(serde_json::to_string(&none).unwrap(), "[]");
    }

    #[test]
    fn an_empty_ring_snapshots_to_no_entries() {
        // Goal: a device without status yet answers with an empty array, as before.
        let ring = StatusRing::new(3);
        let json = |snapshot: &StatusSnapshot| serde_json::to_string(snapshot).unwrap();
        assert_eq!(json(&ring.snapshot(SnapshotFilter::All)), "[]");
        assert_eq!(json(&ring.snapshot_latest(SnapshotFilter::All)), "[]");
        assert_eq!(json(&ring.snapshot_after(i64::MIN)), "[]");
    }

    #[test]
    fn timestamps_round_trip_through_nanoseconds() {
        // Goal: storing an instant as UTC nanoseconds and rendering it again yields the same
        // instant with the same local offset.
        let now = Local::now();
        let rendered = local_time(timestamp_ns(now));
        assert_eq!(rendered, now);
        assert_eq!(rendered.offset(), now.offset());
        assert_eq!(local_time(timestamp_ns(now)).to_rfc3339(), now.to_rfc3339());
    }
}
