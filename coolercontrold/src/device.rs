/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 ******************************************************************************/

use std::collections::HashMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use strum::{Display, EnumString};

use crate::repositories::liquidctl::base_driver::BaseDriver;

// todo: I think we could make this really large in the future (even persist it)
pub const STATUS_SIZE: usize = 1900;
const STATUS_CUTOFF: usize = 1860; // only store the last 31 min./versions of recorded data

pub type UID = String;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Device {
    pub name: String,
    /// The DeviceType. This combines with the type_id are treated as unique identifiers for things like settings.
    pub d_type: DeviceType,
    /// The index from the type's device list. Most of the time this is stable.
    pub type_index: u8,
    /// A Unique identifier that is a hash of a combination of values determined by each repository
    pub uid: UID,
    /// An optional device identifier. This should be pretty unique,
    /// like a serial number or pci device path to be taken into account for the uid.
    device_id: Option<String>,
    /// A Vector of statuses
    pub status_history: Vec<Status>,
    /// Specific Liquidctl device information
    pub lc_info: Option<LcInfo>,
    /// General Device information
    pub info: Option<DeviceInfo>,
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.type_index == other.type_index && self.d_type == self.d_type
    }
}

impl Device {
    /// This should be used everytime to create a new device struct
    pub fn new(name: String,
               d_type: DeviceType,
               type_index: u8,
               lc_info: Option<LcInfo>,
               info: Option<DeviceInfo>,
               starting_status: Option<Status>,
               device_id: Option<String>,
    ) -> Self {
        let mut status_history = Vec::with_capacity(STATUS_SIZE);
        if let Some(status) = starting_status {
            status_history.push(status)
        }
        let uid = Self::create_uid_from(&name, &d_type, type_index, &device_id);
        Device {
            name,
            d_type,
            type_index,
            uid,
            device_id,
            status_history,
            lc_info,
            info,
        }
    }

    /// This returns a sha256 hash string of an attempted unique identifier for a device.
    /// Unique in the sense, that we try to follow the same device even if, for example:
    ///     - another device has been removed and the order has changed.
    ///     - the device has been swapped with another device plugged into the system
    fn create_uid_from(name: &str, d_type: &DeviceType, type_index: u8, device_id: &Option<String>) -> UID {
        let mut hasher = Sha256::new();
        hasher.update(d_type.clone().to_string());
        if let Some(d_id) = device_id.clone() {
            // this should be pretty unique to the device itself, such as a serial number or device path
            hasher.update(d_id);
        } else {
            // non-optimal fallback if needed:
            hasher.update(name.clone());
            hasher.update([type_index]);
        }
        format!("{:x}", hasher.finalize())
    }

    pub fn status_current(&self) -> Option<Status> {
        self.status_history.last().cloned()
    }

    pub fn set_status(&mut self, status: Status) {
        self.status_history.push(status);
        if self.status_history.len() > STATUS_CUTOFF {
            self.status_history.remove(0);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TempStatus {
    pub name: String,
    pub temp: f64,
    pub frontend_name: String,
    pub external_name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelStatus {
    pub name: String,
    pub rpm: Option<u32>,
    pub duty: Option<f64>,
    pub pwm_mode: Option<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// A Model which contains various applicable device statuses
pub struct Status {
    pub timestamp: DateTime<Local>,
    pub firmware_version: Option<String>,
    pub temps: Vec<TempStatus>,
    pub channels: Vec<ChannelStatus>,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            timestamp: Local::now(),
            firmware_version: None,
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
    Composite,
}

/// Needed Device info per device
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub channels: HashMap<String, ChannelInfo>,
    pub lighting_speeds: Vec<String>,
    pub temp_min: u8,
    pub temp_max: u8,
    pub temp_ext_available: bool,
    pub profile_max_length: u8,
    pub profile_min_length: u8,
    pub model: Option<String>,
}


impl Default for DeviceInfo {
    fn default() -> Self {
        DeviceInfo {
            channels: HashMap::new(),
            lighting_speeds: vec![],
            temp_min: 20,
            temp_max: 100,
            temp_ext_available: false,
            profile_max_length: 17, // reasonable default, one control point every 5 degrees for 20-100
            profile_min_length: 2,
            model: None,
        }
    }
}

/// General Device Information
impl DeviceInfo {
    pub fn new() -> Self {
        DeviceInfo {
            ..Default::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChannelInfo {
    pub speed_options: Option<SpeedOptions>,
    pub lighting_modes: Vec<LightingMode>,
    pub lcd_modes: Vec<LcdMode>,
}

impl Default for ChannelInfo {
    fn default() -> Self {
        ChannelInfo {
            speed_options: None,
            lighting_modes: vec![],
            lcd_modes: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedOptions {
    pub min_duty: u8,
    pub max_duty: u8,
    pub profiles_enabled: bool,
    pub fixed_enabled: bool,
    /// This enables software-profiles for device-internal temperatures
    /// External temperatures must always be software-profiles
    pub manual_profiles_enabled: bool,
}

impl Default for SpeedOptions {
    fn default() -> Self {
        SpeedOptions {
            min_duty: 0,
            max_duty: 100,
            profiles_enabled: false,
            fixed_enabled: false,
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
    name: String,
    frontend_name: String,
    brightness: bool,
    orientation: bool,
    image: bool,
    colors_min: u8,
    colors_max: u8,
    #[serde(rename(serialize = "type"))]
    type_: LcdModeType,
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
