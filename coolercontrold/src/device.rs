/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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
use std::{
    collections::{HashMap, VecDeque},
    time::Duration,
};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strum::{Display, EnumString};

use crate::repositories::liquidctl::base_driver::BaseDriver;

pub const STATUS_SIZE: usize = 3600; // only store the last 60 min. of recorded data

pub type UID = String;
pub type DeviceUID = UID;
pub type DeviceName = String;
pub type ChannelName = String;
pub type TempName = String;
pub type TempLabel = String;
pub type TypeIndex = u8;
pub type Temp = f64;
pub type Duty = u8;
pub type RPM = u32;
pub type Mhz = u32;

#[derive(Serialize, Deserialize, Clone)]
pub struct Device {
    pub name: DeviceName,

    /// The DeviceType. This combines with the type_id are treated as unique identifiers for things like settings.
    pub d_type: DeviceType,

    /// The index from the type's device list. Most of the time this is stable.
    pub type_index: TypeIndex,

    /// A Unique identifier that is a hash of a combination of values determined by each repository
    pub uid: DeviceUID,

    /// An optional device identifier. This should be pretty unique,
    /// like a serial number or pci device path to be taken into account for the uid.
    device_id: Option<String>,

    /// A double-sided Vector of statuses
    pub status_history: VecDeque<Status>,

    /// Specific Liquidctl device information
    pub lc_info: Option<LcInfo>,

    /// General Device information
    pub info: DeviceInfo,
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
    ) -> Self {
        let uid = Self::create_uid_from(&name, &d_type, type_index, &device_id);
        Device {
            name,
            d_type,
            type_index,
            uid,
            device_id,
            status_history: VecDeque::with_capacity(STATUS_SIZE),
            lc_info,
            info,
        }
    }

    /// This returns a sha256 hash string of an attempted unique identifier for a device.
    /// Unique in the sense, that we try to follow the same device even if, for example:
    ///     - another device has been removed and the order has changed.
    ///     - the device has been swapped with another device plugged into the system
    fn create_uid_from(
        name: &str,
        d_type: &DeviceType,
        type_index: u8,
        device_id: &Option<String>,
    ) -> UID {
        let mut hasher = Sha256::new();
        hasher.update(d_type.clone().to_string());
        if let Some(d_id) = device_id.clone() {
            // this should be pretty unique to the device itself, such as a serial number or device path
            hasher.update(d_id);
        } else {
            // non-optimal fallback if needed:
            hasher.update(name);
            hasher.update([type_index]);
        }
        format!("{:x}", hasher.finalize())
    }

    /// Returns the most recent status in the status history, if it exists.
    ///
    /// Returns:
    ///
    /// an `Option<Status>`.
    pub fn status_current(&self) -> Option<Status> {
        self.status_history.back().cloned()
    }

    /// Clears and fills the `status_history` with zeroed out statuses based the given `status`,
    /// which is consumed and appended as the most recent Status.
    /// This should be used on startup and when waking from sleep to initialize the status history.
    ///
    /// Arguments:
    ///
    /// * `status`: of type `Status`. It represents the current status of a system or device.
    pub fn initialize_status_history_with(&mut self, status: Status) {
        let zeroed_temps = status
            .temps
            .iter()
            .map(|t| TempStatus {
                temp: 0.0,
                ..t.clone()
            })
            .collect::<Vec<TempStatus>>();
        let zeroed_channels = status
            .channels
            .iter()
            .map(|c| ChannelStatus {
                rpm: if c.rpm.is_some() { Some(0) } else { None },
                duty: if c.duty.is_some() { Some(0.0) } else { None },
                freq: if c.freq.is_some() { Some(0) } else { None },
                ..c.clone()
            })
            .collect::<Vec<ChannelStatus>>();
        self.status_history.clear();
        for pos in (1..STATUS_SIZE).rev() {
            let zeroed_status = Status {
                timestamp: status.timestamp - Duration::from_secs(pos as u64),
                temps: zeroed_temps.clone(),
                channels: zeroed_channels.clone(),
            };
            self.status_history.push_back(zeroed_status);
        }
        self.status_history.push_back(status);
    }

    /// Adds a new status to a history list and removes the oldest status.
    /// This should be used every time a new status is to be added.
    ///
    /// Arguments:
    ///
    /// * `status`: The `Status` to be consumed and added to the history stack.
    pub fn set_status(&mut self, status: Status) {
        self.status_history.pop_front();
        self.status_history.push_back(status);
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempStatus {
    pub name: TempName,
    pub temp: Temp,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub name: ChannelName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rpm: Option<RPM>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freq: Option<Mhz>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pwm_mode: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum DeviceType {
    CPU,
    GPU,
    Liquidctl,
    Hwmon,
    CustomSensors,
}

/// Needed Device info per device
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub channels: HashMap<String, ChannelInfo>,
    pub temps: HashMap<String, TempInfo>,
    pub lighting_speeds: Vec<String>,
    pub temp_min: u8,
    pub temp_max: u8,
    pub profile_max_length: u8,
    pub profile_min_length: u8,
    pub model: Option<String>,

    /// When present, then this is a ThinkPad device. True or False indicates whether Fan control
    /// is enabled for the kernel module and changing values is possible
    pub thinkpad_fan_control: Option<bool>,
}

impl Default for DeviceInfo {
    fn default() -> Self {
        DeviceInfo {
            channels: HashMap::new(),
            temps: HashMap::new(),
            lighting_speeds: vec![],
            temp_min: 20,
            temp_max: 100,
            profile_max_length: 17, // reasonable default, one control point every 5 degrees for 20-100
            profile_min_length: 2,
            model: None,
            thinkpad_fan_control: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ChannelInfo {
    pub label: Option<String>,
    pub speed_options: Option<SpeedOptions>,
    pub lighting_modes: Vec<LightingMode>,
    pub lcd_modes: Vec<LcdMode>,
    pub lcd_info: Option<LcdInfo>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TempInfo {
    pub label: TempLabel,
    pub number: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedOptions {
    pub min_duty: Duty,
    pub max_duty: Duty,
    /// If (temp, duty) profiles are supported by the device natively or not (device-internal temps)
    pub profiles_enabled: bool,
    pub fixed_enabled: bool,
    /// This enables software-profiles for device-internal temperatures
    /// External temperatures must always be software-profiles and are not handled by this property
    pub manual_profiles_enabled: bool,
}

impl Default for SpeedOptions {
    fn default() -> Self {
        SpeedOptions {
            min_duty: 0,
            max_duty: 100,
            profiles_enabled: false,
            fixed_enabled: true,
            manual_profiles_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
pub enum LightingModeType {
    None,
    Liquidctl,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
pub enum LcdModeType {
    None,
    Liquidctl,
    Custom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// Specific LCD Screen info
pub struct LcdInfo {
    pub screen_width: u32,
    pub screen_height: u32,
    pub max_image_size_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Specific Liquidctl device information
pub struct LcInfo {
    /// An Enum representation of the various Liquidctl driver classes
    pub driver_type: BaseDriver,
    /// The detected firmware version at initialization
    pub firmware_version: Option<String>,
    /// An indicator for needed user input to determine actual asetek690lc device
    pub unknown_asetek: bool,
}
