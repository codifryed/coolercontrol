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


use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::device::{LcdMode, LightingMode, UID};

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

    /// Settings for LCD screens
    pub lcd: Option<LcdSettings>,
    pub lcd_mode: Option<LcdMode>,

    /// the current pwm_mode to set for hwmon devices, eg: 1
    pub pwm_mode: Option<u8>,

    /// Used to set hwmon & nvidia channels back to their default 'automatic' values.
    pub reset_to_default: Option<bool>,

}

impl Default for Setting {
    fn default() -> Self {
        Self {
            channel_name: "".to_string(),
            speed_fixed: None,
            speed_profile: None,
            temp_source: None,
            lighting: None,
            lighting_mode: None,
            lcd: None,
            lcd_mode: None,
            pwm_mode: None,
            reset_to_default: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingSettings {
    /// The lighting mode name
    pub mode: String,

    /// The speed to set
    pub speed: Option<String>,

    /// run backwards or not
    pub backward: Option<bool>,

    /// a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
    pub colors: Vec<(u8, u8, u8)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TempSource {
    /// The frontend name for this Temperature Source
    /// The GUI previously also used the external_name here for model simplification,
    /// that is no longer needed.
    pub frontend_temp_name: String,

    /// The associated device uid containing current temp values
    pub device_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LcdSettings {
    /// The Lcd mode name
    pub mode: String,

    /// The LCD brightness (0-100%)
    pub brightness: Option<u8>,

    /// The LCD Image orientation (0,90,180,270)
    pub orientation: Option<u16>,

    /// The LCD Source Image file path location
    pub image_file: Option<String>,

    /// The LCD Image tmp file path location, where the preprocessed image is located
    pub tmp_image_file: Option<String>,

    /// a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
    pub colors: Vec<(u8, u8, u8)>,
}

/// General Settings for CoolerControl
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoolerControlSettings {
    pub apply_on_boot: bool,
    pub no_init: bool,
    pub handle_dynamic_temps: bool,
    pub startup_delay: Duration,
}
