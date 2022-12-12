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


use serde::{Deserialize, Serialize};
use crate::device::{LightingMode, UID};

/// Setting is a passed struct used to apply various settings to a specific device.
/// Usually only one specific lighting or speed setting is applied at a time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Setting {
    pub channel_name: String,

    /// The fixed duty speed to set. eg: 20 (%)
    pub speed_fixed: Option<u8>,

    /// The profile temp/duty speeds to set. eg: [(20, 50), (25, 80)]
    pub speed_profile: Option<Vec<(u8, u8)>>,

    /// The associated temperature source
    pub temp_source: Option<TempSource>,

    /// Settings for lighting
    pub lighting: Option<LightingSettings>,
    pub lighting_mode: Option<LightingMode>,

    /// the current pwm_mode to set for hwmon devices, eg: 1
    pub pwm_mode: Option<u8>,

    /// Used to set hwmon & nvidia channels back to their default 'automatic' values.
    pub reset_to_default: Option<bool>,

    /// (internal use) the last duty speeds that we set manually. This keeps track of applied settings
    /// to not re-apply the same setting over and over again needlessly. eg: [20, 25, 30]
    #[serde(skip_serializing, skip_deserializing)]
    pub last_manual_speeds_set: Vec<u8>,

    /// (internal use) a counter to be able to know how many times the to-be-applied duty was under
    /// the apply-threshold. This helps mitigate issues where the duty is 1% off target for a long time.
    #[serde(skip_serializing, skip_deserializing)]
    pub under_threshold_counter: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingSettings {
    /// The lighting mode name
    pub mode: String,

    /// The speed to set
    pub speed: Option<String>,

    /// run backwards or not
    pub backward: bool,

    /// a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
    pub colors: Vec<(u8, u8, u8)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempSource {
    /// The frontend name for this Temperature Source
    pub frontend_temp_name: String,

    /// The associated device uid containing current temp values
    pub device_uid: UID,
}

