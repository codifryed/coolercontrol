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
    name: String,
    /// The DeviceType. This combines with the type_id are treated as unique identifiers for things like settings.
    #[serde(rename(serialize = "type"))]
    d_type: DeviceType,
    /// The index from the type's device list. Most of the time this is stable.
    type_id: u8,
    status_history: RefCell<Vec<Status>>,
    /// A color map of channel_name: hex_color_str
    colors: HashMap<String, String>,
    /// An Enum representation of the various Liquidctl driver classes
    lc_driver_type: Option<BaseDriver>,
    lc_init_firmware_version: Option<String>,
    info: Option<DeviceInfo>,
}

impl Default for Device {
    fn default() -> Self {
        Device {
            name: "Device".to_string(),
            d_type: DeviceType::Hwmon,
            type_id: 0,
            status_history: RefCell::new(Vec::with_capacity(1900)),
            colors: HashMap::new(),
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

    pub fn name_short(&self) -> String {
        match self.name.split_once(" (") {
            Some((short_name, _)) => short_name.to_string(),
            None => self.name.clone()
        }
    }

    pub fn status_current(&self) -> Option<Status> {
        self.status_history.borrow().last().cloned()
    }

    pub fn set_status(&self, status: Status) {
        // only 1 mutable reference per scope is allowed:
        let mut statuses = self.status_history.borrow_mut();
        statuses.push(status);
        if self.status_history.borrow().len() > 1860 { // only store the last 31 min. of recorded data
            statuses.remove(0);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempStatus {
    name: String,
    temp: f64,
    frontend_name: String,
    external_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelStatus {
    name: String,
    rpm: Option<u32>,
    duty: Option<f64>,
    pwm_mode: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// A Model which contains various applicable device statuses
pub struct Status {
    timestamp: DateTime<Local>,
    firmware_version: Option<String>,
    temps: Vec<TempStatus>,
    channels: Vec<ChannelStatus>,
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    channels: HashMap<String, ChannelInfo>,
    lighting_speeds: Vec<String>,
    temp_min: u8,
    temp_max: u8,
    temp_ext_available: bool,
    profile_max_length: u8,
    profile_min_length: u8,
    model: Option<String>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelInfo {
    speed_options: Option<SpeedOptions>,
    lighting_modes: Vec<LightingMode>,
}

impl Default for ChannelInfo {
    fn default() -> Self {
        ChannelInfo {
            speed_options: None,
            lighting_modes: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeedOptions {
    min_duty: u8,
    max_duty: u8,
    profiles_enabled: bool,
    fixed_enabled: bool,
    manual_profiles_enabled: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingMode {
    name: String,
    frontend_name: String,
    min_colors: u8,
    max_colors: u8,
    speed_enabled: bool,
    backward_enabled: bool,
    #[serde(rename(serialize = "type"))]
    _type: LightingModeType,
}
