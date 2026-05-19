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

use std::fmt::Debug;
use std::ops::Not;
use std::rc::Rc;
use std::sync::Arc;
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strum::{Display, EnumString};

use crate::repositories::liquidctl::base_driver::BaseDriver;

const STATUS_SIZE_SECONDS: f64 = 3600.; // only store the last 60 min. of recorded data

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

/// Post-push hook invoked inside `Device::set_status` while the
/// `status_history` `Arc` is being mutated. Lets a single consumer
/// (today: the calibration true-duty rewrite) piggyback on the
/// existing `Arc::make_mut` instead of taking a second pass that
/// would deep-clone the history whenever any reader holds a clone.
pub trait StatusAugmenter {
    fn augment(&self, status: &mut Status, device_uid: &DeviceUID);
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

/// Per-device running stats since daemon start. Populated lazily as
/// channels/temps are observed. Reset via `Device::reset_stats`.
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct DeviceStats {
    pub temps: HashMap<TempName, ChannelStats>,
    pub channels: HashMap<ChannelName, HashMap<ChannelDataType, ChannelStats>>,
}

/// Shared helper: clone `temps` with all `temp` fields set to 0.0.
fn build_zeroed_temps(temps: &[TempStatus]) -> Vec<TempStatus> {
    temps
        .iter()
        .map(|t| TempStatus {
            temp: 0.0,
            ..t.clone()
        })
        .collect()
}

/// Shared helper: clone `channels` with every present numeric field zeroed.
/// Preserves the Some/None shape so the engine sees the same channel layout.
fn build_zeroed_channels(channels: &[ChannelStatus]) -> Vec<ChannelStatus> {
    channels
        .iter()
        .map(|c| ChannelStatus {
            rpm: if c.rpm.is_some() { Some(0) } else { None },
            duty: if c.duty.is_some() { Some(0.0) } else { None },
            freq: if c.freq.is_some() { Some(0) } else { None },
            watts: if c.watts.is_some() { Some(0.0) } else { None },
            ..c.clone()
        })
        .collect()
}

#[derive(Serialize, Deserialize, Clone, JsonSchema)]
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

    /// A double-sided Vector of statuses wrapped in Arc for efficient sharing
    pub status_history: Arc<VecDeque<Status>>,

    /// Specific Liquidctl device information
    pub lc_info: Option<LcInfo>,

    /// General Device information
    pub info: DeviceInfo,

    /// Optional post-push hook run inside `set_status`. See [`StatusAugmenter`].
    #[serde(skip)]
    #[schemars(skip)]
    status_augmenter: Option<Rc<dyn StatusAugmenter>>,

    /// Running per-channel min/max/avg/count since daemon start. Updated
    /// inside `set_status` after the augmenter runs (so true-duty rewrites
    /// feed stats, not the pre-rewrite values).
    #[serde(skip)]
    #[schemars(skip)]
    stats: DeviceStats,
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
            self.status_current(),
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
        let status_history = Arc::new(VecDeque::with_capacity(Self::calc_history_stack_size(
            poll_rate,
        )));
        Device {
            name,
            d_type,
            type_index,
            uid,
            device_id,
            status_history,
            lc_info,
            info,
            status_augmenter: None,
            stats: DeviceStats::default(),
        }
    }

    /// Install a post-push hook. The hook fires on every `set_status`
    /// inside the same `Arc::make_mut` that grows the history, so it
    /// piggybacks for free instead of forcing a second deep clone
    /// when a reader (REST `GET /status`, gRPC) holds a clone of the
    /// `Arc`. Calling twice replaces the prior hook.
    pub fn set_status_augmenter(&mut self, augmenter: Rc<dyn StatusAugmenter>) {
        self.status_augmenter = Some(augmenter);
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn calc_history_stack_size(poll_rate: f64) -> usize {
        (STATUS_SIZE_SECONDS / poll_rate).ceil() as usize
    }

    /// This returns a sha256 hash string of an attempted unique identifier for a device.
    /// Unique in the sense, that we try to follow the same device even if, for example:
    ///     - another device has been removed and the order has changed.
    ///     - the device has been swapped with another device plugged into the system
    pub fn create_uid_from(
        name: &str,
        d_type: DeviceType,
        type_index: u8,
        device_id: Option<&String>,
    ) -> UID {
        let mut hasher = Sha256::new();
        hasher.update(d_type.clone().to_string());
        if let Some(d_id) = device_id {
            // this should be pretty unique to the device itself, such as a serial number or device path
            hasher.update(d_id);
        } else {
            // non-optimal fallback if needed:
            hasher.update(name);
            hasher.update([type_index]);
        }
        crate::hashutil::to_lower_hex(&hasher.finalize())
    }

    /// Returns the most recent status in the status history, if it exists.
    ///
    /// Returns:
    ///
    /// an `Option<Status>`.
    pub fn status_current(&self) -> Option<Status> {
        self.status_history.back().cloned()
    }

    /// Clears and fills the `status_history` with zeroed-out statuses based on
    /// the given real `status`, which is consumed and appended as the most
    /// recent entry via `set_status`. Used at device startup. Routing the
    /// final push through `set_status` ensures the first real reading is
    /// recorded in `stats` and any installed `StatusAugmenter` runs.
    /// (Wake-from-sleep uses `zero_status_history` instead so its zeroed
    /// template does not pollute stats.)
    ///
    /// Arguments:
    ///
    /// * `status`: the first real status, appended as the most recent entry.
    #[allow(clippy::cast_precision_loss)]
    pub fn initialize_status_history_with(&mut self, status: Status, poll_rate: f64) {
        let zeroed_temps = build_zeroed_temps(&status.temps);
        let zeroed_channels = build_zeroed_channels(&status.channels);
        let status_stack_size = Self::calc_history_stack_size(poll_rate);
        {
            let history = Arc::make_mut(&mut self.status_history);
            history.clear();
            // Pre-fill with `status_stack_size` zeroed entries at offsets
            // [-N, -(N-1), ..., -1] * poll_rate. set_status below pops the
            // oldest (-N) and pushes the real status at offset 0, leaving
            // the canonical [-{N-1}, ..., -1, 0] * poll_rate layout.
            for pos in (1..=status_stack_size).rev() {
                history.push_back(Status {
                    timestamp: status.timestamp - Duration::from_secs_f64(pos as f64 * poll_rate),
                    temps: zeroed_temps.clone(),
                    channels: zeroed_channels.clone(),
                });
            }
        }
        self.set_status(status);
    }

    /// Replace the entire status history with synthetic zeroed entries. Used
    /// on wake from sleep when cached values are stale but no real reading
    /// is available yet. Does NOT record stats; the zeroed wake template
    /// is not a real reading and would taint min/avg.
    #[allow(clippy::cast_precision_loss)]
    pub fn zero_status_history(&mut self, template: &Status, poll_rate: f64) {
        let zeroed_temps = build_zeroed_temps(&template.temps);
        let zeroed_channels = build_zeroed_channels(&template.channels);
        let history = Arc::make_mut(&mut self.status_history);
        history.clear();
        let status_stack_size = Self::calc_history_stack_size(poll_rate);
        for pos in (1..status_stack_size).rev() {
            history.push_back(Status {
                timestamp: template.timestamp - Duration::from_secs_f64(pos as f64 * poll_rate),
                temps: zeroed_temps.clone(),
                channels: zeroed_channels.clone(),
            });
        }
        history.push_back(Status {
            timestamp: template.timestamp,
            temps: zeroed_temps,
            channels: zeroed_channels,
        });
    }

    /// Adds a new status to a history list and removes the oldest status.
    /// This should be used every time a new status is to be added.
    /// Uses `Arc::make_mut` for copy-on-write semantics: only clones if
    /// there are other references to the history. The installed
    /// `StatusAugmenter` (if any) runs on the just-pushed entry inside
    /// the same mutable borrow, so the calibration rewrite incurs no
    /// extra deep clone when a reader holds an `Arc` clone. Running
    /// stats are folded from the just-pushed (post-augmenter) entry so
    /// min/max/avg track the values the engine actually sees.
    ///
    /// Arguments:
    ///
    /// * `status`: The `Status` to be consumed and added to the history stack.
    pub fn set_status(&mut self, status: Status) {
        let history = Arc::make_mut(&mut self.status_history);
        history.pop_front();
        history.push_back(status);
        if let Some(augmenter) = self.status_augmenter.as_ref() {
            if let Some(latest) = history.back_mut() {
                augmenter.augment(latest, &self.uid);
            }
        }
        // Take a snapshot of the latest entry so we release the &mut
        // borrow on status_history before mutating self.stats below.
        let Some(latest) = history.back().cloned() else {
            return;
        };
        self.record_stats(&latest);
    }

    /// Borrow the per-channel running stats.
    pub fn stats(&self) -> &DeviceStats {
        &self.stats
    }

    /// Fold a `Status` into the running stats. Channels and temps absent
    /// from `status` are not touched (no zero pollution from a tick where
    /// hwmon dropped a channel). Called from `set_status` after the
    /// augmenter runs.
    fn record_stats(&mut self, status: &Status) {
        for temp in &status.temps {
            self.stats
                .temps
                .entry(temp.name.clone())
                .or_default()
                .fold(temp.temp);
        }
        for channel in &status.channels {
            let by_type = self.stats.channels.entry(channel.name.clone()).or_default();
            if let Some(duty) = channel.duty {
                by_type.entry(ChannelDataType::Duty).or_default().fold(duty);
            }
            if let Some(rpm) = channel.rpm {
                by_type
                    .entry(ChannelDataType::Rpm)
                    .or_default()
                    .fold(f64::from(rpm));
            }
            if let Some(freq) = channel.freq {
                by_type
                    .entry(ChannelDataType::Freq)
                    .or_default()
                    .fold(f64::from(freq));
            }
            if let Some(watts) = channel.watts {
                by_type
                    .entry(ChannelDataType::Watts)
                    .or_default()
                    .fold(watts);
            }
        }
    }

    /// Clear all stats and reseed each entry from the most recent status
    /// with `count=1` so the UI never sees a zero-count window after a
    /// reset. Channels absent from the most recent status get no entry
    /// and will reseed naturally on their next observation.
    pub fn reset_stats(&mut self) {
        self.stats.temps.clear();
        self.stats.channels.clear();
        let Some(latest) = self.status_history.back().cloned() else {
            return;
        };
        for temp in &latest.temps {
            self.stats
                .temps
                .insert(temp.name.clone(), ChannelStats::from_first(temp.temp));
        }
        for channel in &latest.channels {
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
                self.stats.channels.insert(channel.name.clone(), by_type);
            }
        }
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, JsonSchema)]
pub struct ChannelInfo {
    pub label: Option<String>,
    pub speed_options: Option<SpeedOptions>,
    pub lighting_modes: Vec<LightingMode>,
    pub lcd_modes: Vec<LcdMode>,
    pub lcd_info: Option<LcdInfo>,
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
mod tests {
    use super::*;
    use chrono::Local;
    use std::cell::RefCell;

    const POLL_RATE: f64 = 1.0;

    /// A test StatusAugmenter that rewrites every channel's duty to a
    /// fixed sentinel. Lets us verify that record_stats reads post-augmenter
    /// values, not the pre-augmenter status passed into set_status.
    struct FixedDutyAugmenter {
        target_duty: f64,
        invocations: RefCell<u32>,
    }

    impl StatusAugmenter for FixedDutyAugmenter {
        fn augment(&self, status: &mut Status, _device_uid: &DeviceUID) {
            *self.invocations.borrow_mut() += 1;
            for channel in &mut status.channels {
                if channel.duty.is_some() {
                    channel.duty = Some(self.target_duty);
                }
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

    /// Verify the canonical happy path: a status pushed via set_status
    /// updates min/max/avg/count for every present temp and channel
    /// data field, lazily creating entries the first time.
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
        let stats = device.stats();
        assert_eq!(stats.temps.get("cpu").unwrap().count, 1);
        assert_eq!(stats.temps.get("cpu").unwrap().avg, 40.0);
        let fan1 = stats.channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().avg, 50.0);
        assert_eq!(fan1.get(&ChannelDataType::Rpm).unwrap().avg, 1200.0);
    }

    /// Multiple set_status calls accumulate. Min tracks the lowest, max
    /// the highest, avg is the cumulative mean, count is total samples.
    #[test]
    fn record_stats_folds_running_min_max_avg_count() {
        let mut device = make_test_device();
        device.initialize_status_history_with(status_with(vec![("cpu", 30.0)], vec![]), POLL_RATE);
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

    /// The first real status passed to initialize_status_history_with
    /// must be counted in stats, since it flows through set_status.
    #[test]
    fn initialize_status_history_counts_the_first_real_reading() {
        let mut device = make_test_device();
        device.initialize_status_history_with(status_with(vec![("cpu", 42.0)], vec![]), POLL_RATE);
        let cpu = device.stats().temps.get("cpu").unwrap();
        assert_eq!(cpu.count, 1);
        assert_eq!(cpu.min, 42.0);
        assert_eq!(cpu.max, 42.0);
        assert_eq!(cpu.avg, 42.0);
    }

    /// zero_status_history wipes history to zeros for wake-from-sleep
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
        // Wake-from-sleep: engine passes a zeroed template. Stats must be untouched.
        device.zero_status_history(
            &status_with(
                vec![("cpu", 0.0)],
                vec![channel("fan1", Some(0.0), Some(0))],
            ),
            POLL_RATE,
        );
        let stats = device.stats();
        assert_eq!(stats.temps.get("cpu").unwrap().count, 1);
        assert_eq!(stats.temps.get("cpu").unwrap().min, 40.0);
        let fan1 = stats.channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().min, 50.0);
        assert_eq!(fan1.get(&ChannelDataType::Rpm).unwrap().min, 1200.0);
    }

    /// The augmenter is documented to fire on the just-pushed entry
    /// before stats are recorded. Verify stats see the post-augmenter
    /// value (the true-duty rewrite), not the raw duty passed in.
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
        let duty_stats = device
            .stats()
            .channels
            .get("fan1")
            .unwrap()
            .get(&ChannelDataType::Duty)
            .unwrap();
        assert_eq!(*augmenter.invocations.borrow(), 1);
        assert_eq!(duty_stats.count, 1);
        assert_eq!(duty_stats.avg, 99.0);
        assert_eq!(duty_stats.min, 99.0);
    }

    /// reset_stats clears everything then reseeds from the most recent
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
}
