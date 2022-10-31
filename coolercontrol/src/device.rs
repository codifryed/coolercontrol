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

use std::cell::RefCell;
use std::collections::HashMap;

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

use crate::liquidctl::base_driver::BaseDriver;

#[derive(Debug, Serialize, Deserialize)]
pub struct Device {
    pub name: String,
    /// The DeviceType. This combines with the type_id are treated as unique identifiers for things like settings.
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    /// The index from the type's device list. Most of the time this is stable.
    pub type_id: u8,
    /// A Vector of statuses
    pub status_history: Vec<Status>,
    /// An Enum representation of the various Liquidctl driver classes
    pub lc_driver_type: Option<BaseDriver>,
    pub lc_init_firmware_version: Option<String>,
    pub info: Option<DeviceInfo>,
}

impl Default for Device {
    fn default() -> Self {
        Device {
            name: "Device".to_string(),
            d_type: DeviceType::Hwmon,
            type_id: 0,
            // todo: I think we could make this really large (even persist it)
            status_history: Vec::with_capacity(1900),
            lc_driver_type: None,
            lc_init_firmware_version: None,
            info: None,
        }
    }
}

impl PartialEq for Device {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.type_id == other.type_id && self.d_type == self.d_type
    }
}

impl Device {
    pub fn new(name: &String,
               d_type: DeviceType,
               type_id: u8,
               lc_driver_type: Option<BaseDriver>,
               lc_init_firmware_version: Option<String>,
               info: Option<DeviceInfo>,
    ) -> Self {
        Device {
            name: name.clone(),
            d_type,
            type_id,
            lc_driver_type,
            lc_init_firmware_version,
            info,
            ..Default::default()
        }
    }

    pub fn status_current(&self) -> Option<Status> {
        self.status_history.last().cloned()
    }

    pub fn set_status(&mut self, status: Status) {
        self.status_history.push(status);
        if self.status_history.len() > 1860 { // only store the last 31 min. of recorded data
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

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize)]
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
}

impl Default for ChannelInfo {
    fn default() -> Self {
        ChannelInfo {
            speed_options: None,
            lighting_modes: vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeedOptions {
    pub min_duty: u8,
    pub max_duty: u8,
    pub profiles_enabled: bool,
    pub fixed_enabled: bool,
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
    pub _type: LightingModeType,
}
