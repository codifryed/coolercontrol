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

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;
use strum::{Display, EnumString};

use crate::device::ChannelName;
use crate::device::DeviceName;
use crate::device::DeviceUID;
use crate::device::Duty;
use crate::device::Temp;
use crate::device::TempName;
use crate::device::UID;

pub type ProfileUID = UID;
pub type FunctionUID = UID;
pub type R = u8;
pub type G = u8;
pub type B = u8;
type Weight = u8;
pub type Offset = i8;

pub const DEFAULT_PROFILE_UID: &str = "0";
pub const DEFAULT_FUNCTION_UID: &str = "0";

/// Setting is used to store applied Settings to a device channel.
/// These are the general core settings that apply to a wide range of device and channel types.
/// Specialized settings are stored in `DeviceExtensions` and `ChannelExtensions`.
/// Only one specific lighting or speed setting is applied to a specific channel at a time.
#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct Setting {
    pub channel_name: ChannelName,

    /// The fixed duty speed to set. eg: 20 (%)
    pub speed_fixed: Option<Duty>,

    /// Settings for lighting
    pub lighting: Option<LightingSettings>,

    /// Settings for LCD screens
    pub lcd: Option<LcdSettings>,

    /// Used to set hwmon & nvidia channels back to their default 'automatic' values.
    pub reset_to_default: Option<bool>,

    /// The Profile UID that applies to this device channel
    pub profile_uid: Option<ProfileUID>,
}

impl PartialEq for Setting {
    fn eq(&self, other: &Self) -> bool {
        self.channel_name == other.channel_name
            && self.speed_fixed == other.speed_fixed
            && self.lighting == other.lighting
            && self.lcd == other.lcd
            && self.profile_uid == other.profile_uid
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LightingSettings {
    /// The lighting mode name
    pub mode: String,

    /// The speed to set
    pub speed: Option<String>,

    /// run backwards or not
    pub backward: Option<bool>,

    /// a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
    pub colors: Vec<(R, G, B)>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TempSource {
    /// The internal name for this Temperature Source. NOT the `TempInfo` Label.
    pub temp_name: TempName,

    /// The associated device uid containing current temp values
    pub device_uid: DeviceUID,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[strum(serialize_all = "lowercase")]
pub enum LcdModeName {
    None,
    Liquid,
    Image,
    Temp,
    Carousel,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LcdSettings {
    /// The Lcd mode name
    pub mode: LcdModeName,

    /// The LCD brightness (0-100%)
    pub brightness: Option<u8>,

    /// The LCD Image orientation (0,90,180,270)
    pub orientation: Option<u16>,

    /// The LCD Image processed file path location, where the preprocessed image is located.
    pub image_file_processed: Option<String>,

    pub carousel: Option<LcdCarouselSettings>,

    /// a list of RGB tuple values, eg [(20,20,120), (0,0,255)]
    pub colors: Vec<(R, G, B)>,

    /// A temp source for displaying a temperature.
    pub temp_source: Option<TempSource>,
}

/// Settings for the LCD Carousel.
///
/// This can be used to have a carousel of images (static or gif), of sensor data,
/// or a combination of both.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LcdCarouselSettings {
    /// The absolute path directory location for images for the carousel. All applicable images
    /// present are processed when the setting is applied.
    pub images_path: Option<String>,

    /// The interval in seconds (2-900) in which to change images in the carousel.
    pub interval: u64,
    // The list of channel sources to display.
    // pub channel_sources: Vec<ChannelSource>,
}

impl Default for LcdCarouselSettings {
    fn default() -> Self {
        Self {
            images_path: None,
            interval: 4,
            // channel_sources: Vec::new(),
        }
    }
}

/// General Settings for `CoolerControl`
#[allow(clippy::struct_excessive_bools)]
#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoolerControlSettings {
    pub apply_on_boot: bool,
    pub no_init: bool,
    pub startup_delay: Duration,
    pub thinkpad_full_speed: bool,
    pub hide_duplicate_devices: bool,
    pub liquidctl_integration: bool,
    pub port: Option<u16>,
    pub ipv4_address: Option<String>,
    pub ipv6_address: Option<String>,
    pub compress: bool,
    pub poll_rate: f64,
    pub drivetemp_suspend: bool,
    pub tls_enabled: bool,
    pub tls_cert_path: Option<String>,
    pub tls_key_path: Option<String>,
    /// Custom origins to allow in CORS (for reverse proxy setups)
    pub origins: Vec<String>,
    /// Allow unencrypted HTTP connections from non-localhost addresses
    pub allow_unencrypted: bool,
    /// Header to check for proxy client protocol (e.g., "X-Forwarded-Proto")
    pub protocol_header: Option<String>,
}

/// Device Specific settings that generally apply to how the application deals with the device.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CCDeviceSettings {
    /// The device name for this setting. Helpful after blacklisting(disabling) devices.
    pub name: DeviceName,

    /// All communication with this device will be avoided if disabled
    pub disable: bool,

    /// Specialized settings (extensions) that apply to a specific device.
    pub extensions: DeviceExtensions,

    /// A list of channels specific settings, including disable and extension settings.
    pub channel_settings: HashMap<ChannelName, CCChannelSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CCChannelSettings {
    pub label: Option<String>,

    pub disabled: bool,

    /// Specialized settings (extensions) that apply to a specific device channel.
    pub extension: Option<ChannelExtensions>,
}

impl CCDeviceSettings {
    pub fn get_disabled_channels(&self) -> Vec<ChannelName> {
        self.channel_settings
            .iter()
            .filter_map(|(channel_name, channel_settings)| {
                channel_settings.disabled.then_some(channel_name)
            })
            .cloned()
            .collect()
    }
}

/// Device specific extension settings
/// This is used to store specialized settings (extensions) that apply to a specific device.
/// More than one of these settings can be applied at a time.
#[derive(Debug, Default, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceExtensions {
    /// Whether to enable Direct Access for the liquidctl driver,
    /// which will cause liquidctl to ignore the `HWMon` kernel driver
    pub direct_access: bool,

    /// The delay in milliseconds to force between applying settings to this device.
    /// This is to help with communication issues with some devices that may not handle
    /// multiple settings applied in quick succession. (The driver does not always handle this)
    pub delay_millis: u16,
}

/// Device Channel specific settings
/// This is used to store specialized settings (extensions) that apply to a specific device channel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ChannelExtensions {
    /// Whether to use the device channel's internal hardware fan curve functionality.
    AutoHWCurve { auto_hw_curve_enabled: bool },

    /// Whether to use the AMDGPU RDNA3/4 features.
    /// It allows the device to run at zero RPM when the temperature is below a certain threshold.
    AmdRdnaGpu {
        /// Whether to use the internal HW Curve feature, instead of setting regular
        /// flat curves. Using this reduces functionality.
        hw_fan_curve_enabled: bool,
    },
}

/// Profile Settings
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Profile {
    /// The Unique Identifier for this Profile
    pub uid: ProfileUID,

    /// The profile type
    pub p_type: ProfileType,

    /// The User given name for this Profile
    pub name: String,

    /// The fixed duty speed to set. eg: 20 (%)
    pub speed_fixed: Option<Duty>,

    #[allow(clippy::struct_field_names)]
    /// The profile temp/duty speeds to set. eg: [(20.0, 50), (25.7, 80)]
    pub speed_profile: Option<Vec<(Temp, Duty)>>,

    /// The associated temperature source
    pub temp_source: Option<TempSource>,

    /// The minimum temp for this profile
    pub temp_min: Option<Temp>,

    /// The maximum temp for this profile
    pub temp_max: Option<Temp>,

    /// The function uid to apply to this profile
    pub function_uid: FunctionUID,

    /// The profiles that make up the mix profile
    pub member_profile_uids: Vec<ProfileUID>,

    /// The function to mix the members with if this is a Mix Profile
    pub mix_function_type: Option<ProfileMixFunctionType>,

    #[allow(clippy::struct_field_names)]
    /// The graph offset to apply to the associated member profile
    /// This can also be used as a static offset when there is only one duty/offset pair.
    pub offset_profile: Option<Vec<(Duty, Offset)>>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            uid: DEFAULT_PROFILE_UID.to_string(),
            p_type: ProfileType::Default,
            name: "Default Profile".to_string(),
            speed_fixed: None,
            speed_profile: None,
            temp_source: None,
            temp_min: None,
            temp_max: None,
            function_uid: DEFAULT_FUNCTION_UID.to_string(),
            member_profile_uids: Vec::new(),
            mix_function_type: None,
            offset_profile: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum ProfileType {
    Default,
    Fixed,
    Graph,
    Mix,
    Overlay,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Function {
    /// The Unique identifier for this function
    pub uid: FunctionUID,

    /// The user given name for this function
    pub name: String,

    /// The type of this function
    pub f_type: FunctionType,

    /// The minimum duty change (step size) to apply
    /// Previously `duty_minimum`.
    #[serde(rename = "duty_minimum")]
    pub step_size_min: Duty,

    /// The maximum duty change (step size) to apply
    /// A duty maximum of `0` indicates a fixed step size. Use `duty_minimum` to set the step size.
    /// Previously `duty_maximum`.
    #[serde(rename = "duty_maximum")]
    pub step_size_max: Duty,

    /// The minimum step size to apply when decreasing.
    /// A value of `0` indicates a symmetric step size. Use `duty_minimum` to set the step size.
    pub step_size_min_decreasing: Duty,

    /// The maximum step size to apply when decreasing.
    /// A value of `0` indicates a fixed step size. Use `step_size_minimum_decreasing` to set the step size.
    pub step_size_max_decreasing: Duty,

    /// The response delay in seconds
    pub response_delay: Option<u8>,

    /// The temperature deviance threshold in degrees
    pub deviance: Option<Temp>,

    /// Whether to apply settings only on the way down
    pub only_downward: Option<bool>,

    /// The sample window this function should use, particularly applicable to moving averages
    pub sample_window: Option<u8>,

    /// Whether to temporarily bypass thresholds when fan speed remains unchanged for 30+ seconds to meet curve target.
    pub threshold_hopping: bool,
}

impl Default for Function {
    fn default() -> Self {
        Self {
            uid: DEFAULT_FUNCTION_UID.to_string(),
            name: "Default Function".to_string(),
            f_type: FunctionType::Identity,
            step_size_min: 2,
            step_size_max: 100,          // 0 = fixed step size
            step_size_min_decreasing: 0, // 0 = disabled/symmetric step size
            step_size_max_decreasing: 0, // 0 = fixed step size
            response_delay: None,
            deviance: None,
            only_downward: None,
            sample_window: None,
            threshold_hopping: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum FunctionType {
    Identity,
    Standard,
    ExponentialMovingAvg,
}

#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Display,
    EnumString,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum ProfileMixFunctionType {
    Min,
    #[default]
    Max,
    Avg,
    Diff,
    Sum,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum CustomSensorType {
    Mix,
    File,
    Offset,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum CustomSensorMixFunctionType {
    Min,
    Max,
    Delta,
    Avg,
    WeightedAvg,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CustomTempSourceData {
    pub temp_source: TempSource,
    pub weight: Weight,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CustomSensor {
    /// ID MUST be unique, as `temp_name` must be unique.
    pub id: TempName,
    pub cs_type: CustomSensorType,
    pub mix_function: CustomSensorMixFunctionType,
    pub sources: Vec<CustomTempSourceData>,
    pub file_path: Option<PathBuf>,
    pub offset: Option<Offset>,

    /// The Custom Sensor's children, if any.
    ///
    /// Each Custom Sensor is either a child, parent, or standalone, not a combination of those.
    /// Custom Sensors are limited to 1 level of hierarchy. This removes the possibility
    /// of circular references.
    ///
    /// The children and parents vectors are managed and filled internally. For GET endpoints,
    /// they provide this information for clients. For POST or PUT endpoints,
    /// any values here are essentially ignored.
    #[serde(default)]
    pub children: Vec<TempName>,

    /// The Custom Sensor's parents, if any. See `children` for more details.
    #[serde(default)]
    pub parents: Vec<TempName>,
}

impl Default for CustomSensor {
    fn default() -> Self {
        Self {
            id: "default".to_string(),
            cs_type: CustomSensorType::File,
            mix_function: CustomSensorMixFunctionType::Min,
            sources: Vec::new(),
            file_path: None,
            offset: None,
            children: Vec::new(),
            parents: Vec::new(),
        }
    }
}

/// A source for displaying sensor data that is related to a particular channel.
/// This is like `TempSource` but not limited to temperature sensors. (Load, Duty, etc.)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelSource {
    /// The associated device uid containing current values
    pub device_uid: DeviceUID,

    /// The internal name for this channel source. NOT the Label.
    pub channel_name: ChannelName,

    pub channel_metric: ChannelMetric,
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum ChannelMetric {
    Temp,
    Duty,
    Load,
    RPM,
    Freq,
}
