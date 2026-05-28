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
///
/// The kind of setting is enforced at the type level via `SettingKind`. The serialized
/// shape is preserved (flat fields under the channel object) for backwards compatibility
/// with both the persisted TOML config and existing REST clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Setting {
    pub channel_name: ChannelName,

    #[serde(flatten)]
    pub kind: SettingKind,
}

/// The variant of a channel `Setting`. Exactly one variant applies to a channel at a time.
///
/// Serialization uses `#[serde(untagged)]` with each variant carrying a single field
/// whose name matches the legacy flat shape. Combined with `#[serde(flatten)]` on
/// `Setting::kind`, the on-the-wire form is identical to the old flat struct.
///
/// The variant declaration order is the deliberate dispatch precedence: when a malformed
/// payload contains keys from more than one variant, the first declared variant wins.
/// This order matches the engine dispatch in `engine::main` so the read path and apply
/// path agree on the same precedence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SettingKind {
    /// Reset the channel to its hardware default. Only `true` is meaningful;
    /// `false` is treated as a no-op by the engine. This variant is never persisted
    /// to the TOML config (it is only used as an in-flight command).
    Reset { reset_to_default: bool },

    /// Apply a fixed duty speed to the channel. eg: 20 (%)
    SpeedFixed { speed_fixed: Duty },

    /// Apply the named profile to the channel.
    Profile { profile_uid: ProfileUID },

    /// Apply lighting settings to the channel.
    Lighting { lighting: LightingSettings },

    /// Apply LCD settings to the channel.
    Lcd { lcd: LcdSettings },
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

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema,
)]
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
    /// The LCD brightness (0-100%).
    pub brightness: Option<u8>,

    /// The LCD orientation (0, 90, 180, 270). Applies across modes, so it stays shared.
    pub orientation: Option<u16>,

    /// The mode and its mode-specific fields. Flattened so `mode` and the payload stay flat
    /// siblings of the shared fields on the wire (the legacy shape).
    #[serde(flatten)]
    pub mode: LcdModeKind,
}

/// LCD mode and its mode-specific fields, internally tagged on the `mode` discriminator
/// (lowercase, as before). Named `LcdModeKind` to avoid colliding with `device::LcdMode`, the
/// device-capability struct. The per-mode fields stay `Option` for now: the enum already makes
/// a mode structurally unable to carry another mode's field; requiredness is still enforced by
/// the existing apply-time validation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum LcdModeKind {
    None,
    Liquid,
    Image {
        /// Processed (preprocessed) image file path location.
        image_file_processed: Option<String>,
    },
    Temp {
        temp_source: Option<TempSource>,
    },
    Carousel {
        carousel: Option<LcdCarouselSettings>,
    },
}

impl LcdSettings {
    /// The mode-name discriminant, for comparisons and the wire `mode` string.
    pub fn mode_name(&self) -> LcdModeName {
        match self.mode {
            LcdModeKind::None => LcdModeName::None,
            LcdModeKind::Liquid => LcdModeName::Liquid,
            LcdModeKind::Image { .. } => LcdModeName::Image,
            LcdModeKind::Temp { .. } => LcdModeName::Temp,
            LcdModeKind::Carousel { .. } => LcdModeName::Carousel,
        }
    }

    /// The processed image path, if this is an `Image` mode with one set.
    pub fn image_file_processed(&self) -> Option<&String> {
        match &self.mode {
            LcdModeKind::Image {
                image_file_processed,
            } => image_file_processed.as_ref(),
            _ => None,
        }
    }

    /// The temp source, if this is a `Temp` mode with one set.
    pub fn temp_source(&self) -> Option<&TempSource> {
        match &self.mode {
            LcdModeKind::Temp { temp_source } => temp_source.as_ref(),
            _ => None,
        }
    }

    /// The carousel settings, if this is a `Carousel` mode with them set.
    pub fn carousel(&self) -> Option<&LcdCarouselSettings> {
        match &self.mode {
            LcdModeKind::Carousel { carousel } => carousel.as_ref(),
            _ => None,
        }
    }
}

impl std::fmt::Display for LcdModeKind {
    /// The lowercase mode name, matching the serialized `mode` tag and the prior `LcdModeName`
    /// display, so existing log/format sites keep their output.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            LcdModeKind::None => "none",
            LcdModeKind::Liquid => "liquid",
            LcdModeKind::Image { .. } => "image",
            LcdModeKind::Temp { .. } => "temp",
            LcdModeKind::Carousel { .. } => "carousel",
        };
        f.write_str(name)
    }
}

impl LcdModeKind {
    /// Builds the variant for `mode`, taking the matching field from the provided options. The
    /// others are dropped, which is behavior-preserving: the old flat struct's mismatched fields
    /// (e.g. a `temp_source` on an Image setting) were already ignored at apply-time.
    pub fn from_name(
        mode: LcdModeName,
        image_file_processed: Option<String>,
        temp_source: Option<TempSource>,
        carousel: Option<LcdCarouselSettings>,
    ) -> Self {
        match mode {
            LcdModeName::None => LcdModeKind::None,
            LcdModeName::Liquid => LcdModeKind::Liquid,
            LcdModeName::Image => LcdModeKind::Image {
                image_file_processed,
            },
            LcdModeName::Temp => LcdModeKind::Temp { temp_source },
            LcdModeName::Carousel => LcdModeKind::Carousel { carousel },
        }
    }
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
    /// Whether to auto-detect Super-I/O sensors and load kernel modules at startup
    pub sensors_auto_detect: bool,
    /// Whether to listen for kernel device add/remove events at startup
    pub device_listener_enabled: bool,
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
            name: "Unmanaged".to_string(),
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

    /// Whether to bypass the minimum step size when the target duty is exactly 0% or 100%.
    /// Useful for ensuring fans fully stop or reach max RPM even when the change is below the
    /// minimum step size. The maximum step size is still respected. Disabled by default.
    #[serde(default)]
    pub bypass_min_at_extremes: bool,
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
            bypass_min_at_extremes: false,
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
    TimeAverage,
    ExponentialMovingAvg,
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

    /// Variant payload, flattened so its fields and the `cs_type` discriminator stay flat
    /// siblings of `id` on the wire (the legacy shape).
    #[serde(flatten)]
    pub kind: SensorKind,

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

/// Variant-specific payload of a `CustomSensor`, internally tagged on `cs_type`. Exactly one
/// variant is valid per sensor. Constraints the type cannot express (single source for
/// `Offset`/`TimeAverage`/`ExponentialMovingAvg`, `offset` in `-100..=100`,
/// `time_window_seconds` in `1..=300`) are enforced at the API boundary in
/// `validate_custom_sensor`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "cs_type")]
pub enum SensorKind {
    Mix {
        mix_function: CustomSensorMixFunctionType,
        sources: Vec<CustomTempSourceData>,
    },
    File {
        file_path: PathBuf,
    },
    Offset {
        offset: Offset,
        sources: Vec<CustomTempSourceData>,
    },
    TimeAverage {
        time_window_seconds: u16,
        sources: Vec<CustomTempSourceData>,
    },
    ExponentialMovingAvg {
        time_window_seconds: u16,
        sources: Vec<CustomTempSourceData>,
    },
}

impl CustomSensor {
    /// The temp sources this sensor reads from. `File` sensors have none.
    pub fn sources(&self) -> &[CustomTempSourceData] {
        match &self.kind {
            SensorKind::Mix { sources, .. }
            | SensorKind::Offset { sources, .. }
            | SensorKind::TimeAverage { sources, .. }
            | SensorKind::ExponentialMovingAvg { sources, .. } => sources,
            SensorKind::File { .. } => &[],
        }
    }

    /// Mutable access to this sensor's temp sources, or `None` for `File` sensors.
    pub fn sources_mut(&mut self) -> Option<&mut Vec<CustomTempSourceData>> {
        match &mut self.kind {
            SensorKind::Mix { sources, .. }
            | SensorKind::Offset { sources, .. }
            | SensorKind::TimeAverage { sources, .. }
            | SensorKind::ExponentialMovingAvg { sources, .. } => Some(sources),
            SensorKind::File { .. } => None,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn fan_channel() -> String {
        "fan1".to_string()
    }

    // Verifies a SpeedFixed setting serializes to the legacy flat shape and deserializes
    // back to an equal value, so existing UI clients reading the REST response continue
    // to work unchanged.
    #[test]
    fn speed_fixed_json_round_trip() {
        let setting = Setting {
            channel_name: fan_channel(),
            kind: SettingKind::SpeedFixed { speed_fixed: 50 },
        };
        let serialized: Value = serde_json::to_value(&setting).unwrap();
        assert_eq!(
            serialized,
            json!({ "channel_name": "fan1", "speed_fixed": 50 })
        );
        let parsed: Setting = serde_json::from_value(serialized).unwrap();
        assert_eq!(parsed, setting);
    }

    // Verifies a Profile setting serializes to the legacy flat shape and round-trips.
    #[test]
    fn profile_json_round_trip() {
        let setting = Setting {
            channel_name: fan_channel(),
            kind: SettingKind::Profile {
                profile_uid: "abc-123".to_string(),
            },
        };
        let serialized: Value = serde_json::to_value(&setting).unwrap();
        assert_eq!(
            serialized,
            json!({ "channel_name": "fan1", "profile_uid": "abc-123" })
        );
        let parsed: Setting = serde_json::from_value(serialized).unwrap();
        assert_eq!(parsed, setting);
    }

    // Verifies a Lighting setting nests under the `lighting` key (legacy shape) and
    // round-trips, including the inner LightingSettings fields.
    #[test]
    fn lighting_json_round_trip() {
        let setting = Setting {
            channel_name: "logo".to_string(),
            kind: SettingKind::Lighting {
                lighting: LightingSettings {
                    mode: "fixed".to_string(),
                    speed: None,
                    backward: None,
                    colors: vec![(0, 255, 255)],
                },
            },
        };
        let serialized: Value = serde_json::to_value(&setting).unwrap();
        assert_eq!(
            serialized,
            json!({
                "channel_name": "logo",
                "lighting": {
                    "mode": "fixed",
                    "speed": null,
                    "backward": null,
                    "colors": [[0, 255, 255]],
                }
            })
        );
        let parsed: Setting = serde_json::from_value(serialized).unwrap();
        assert_eq!(parsed, setting);
    }

    // Verifies an Lcd setting nests under the `lcd` key (legacy shape) and round-trips.
    #[test]
    fn lcd_json_round_trip() {
        let setting = Setting {
            channel_name: "screen".to_string(),
            kind: SettingKind::Lcd {
                lcd: LcdSettings {
                    brightness: Some(80),
                    orientation: None,
                    mode: LcdModeKind::Liquid,
                },
            },
        };
        let serialized: Value = serde_json::to_value(&setting).unwrap();
        let parsed: Setting = serde_json::from_value(serialized).unwrap();
        assert_eq!(parsed, setting);
    }

    fn lcd(mode: LcdModeKind) -> LcdSettings {
        LcdSettings {
            brightness: Some(80),
            orientation: None,
            mode,
        }
    }

    fn lcd_temp_source() -> TempSource {
        TempSource {
            temp_name: "Temp1".to_string(),
            device_uid: "dev-1".to_string(),
        }
    }

    // An Image LcdSettings keeps the lowercase `mode` tag and `image_file_processed` beside the
    // shared fields, and omits the other modes' fields. Round-trips to the Image variant.
    #[test]
    fn lcd_image_mode_round_trip() {
        let lcd_settings = lcd(LcdModeKind::Image {
            image_file_processed: Some("/tmp/img.png".to_string()),
        });
        let v = serde_json::to_value(&lcd_settings).unwrap();
        assert_eq!(v["mode"], json!("image"));
        assert_eq!(v["image_file_processed"], json!("/tmp/img.png"));
        assert!(v.get("temp_source").is_none());
        assert!(v.get("carousel").is_none());
        assert!(v.get("brightness").is_some());

        let parsed: LcdSettings = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.mode, LcdModeKind::Image { .. }));
    }

    // A Temp LcdSettings carries `temp_source` under the `temp` tag and omits other modes' fields.
    #[test]
    fn lcd_temp_mode_round_trip() {
        let lcd_settings = lcd(LcdModeKind::Temp {
            temp_source: Some(lcd_temp_source()),
        });
        let v = serde_json::to_value(&lcd_settings).unwrap();
        assert_eq!(v["mode"], json!("temp"));
        assert!(v.get("temp_source").is_some());
        assert!(v.get("image_file_processed").is_none());
        assert!(v.get("carousel").is_none());

        let parsed: LcdSettings = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.mode, LcdModeKind::Temp { .. }));
    }

    // A Carousel LcdSettings carries `carousel` under the `carousel` tag.
    #[test]
    fn lcd_carousel_mode_round_trip() {
        let lcd_settings = lcd(LcdModeKind::Carousel {
            carousel: Some(LcdCarouselSettings::default()),
        });
        let v = serde_json::to_value(&lcd_settings).unwrap();
        assert_eq!(v["mode"], json!("carousel"));
        assert!(v.get("carousel").is_some());
        assert!(v.get("temp_source").is_none());

        let parsed: LcdSettings = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.mode, LcdModeKind::Carousel { .. }));
    }

    // A unit mode (None) serializes to just the lowercase tag beside the shared fields.
    #[test]
    fn lcd_none_mode_serializes_tag_only() {
        let v = serde_json::to_value(lcd(LcdModeKind::None)).unwrap();
        assert_eq!(v["mode"], json!("none"));
        assert!(v.get("image_file_processed").is_none());
        assert!(v.get("temp_source").is_none());
        assert!(v.get("carousel").is_none());

        let parsed: LcdSettings = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.mode, LcdModeKind::None));
    }

    // A legacy Image payload that still carries other modes' fields (temp_source, carousel) must
    // deserialize and ignore them, so pre-refactor configs/clients keep working.
    #[test]
    fn lcd_reads_legacy_dead_fields() {
        let legacy = json!({
            "mode": "image",
            "brightness": 80,
            "orientation": null,
            "colors": [],
            "image_file_processed": "/tmp/img.png",
            "temp_source": null,
            "carousel": null
        });
        let parsed: LcdSettings = serde_json::from_value(legacy).unwrap();
        assert!(matches!(parsed.mode, LcdModeKind::Image { .. }));
    }

    // Verifies the Reset variant serializes to `{ "reset_to_default": true }`, matching
    // the legacy shape so any external service plugin or older client emitting that
    // payload continues to deserialize correctly.
    #[test]
    fn reset_true_json_round_trip() {
        let setting = Setting {
            channel_name: fan_channel(),
            kind: SettingKind::Reset {
                reset_to_default: true,
            },
        };
        let serialized: Value = serde_json::to_value(&setting).unwrap();
        assert_eq!(
            serialized,
            json!({ "channel_name": "fan1", "reset_to_default": true })
        );
        let parsed: Setting = serde_json::from_value(serialized).unwrap();
        assert_eq!(parsed, setting);
    }

    // Verifies Reset with `false` is also a valid round-trip. The engine treats `false`
    // as a no-op, but the type must still survive serialization without loss so
    // misconfigured input is preserved verbatim rather than silently dropped.
    #[test]
    fn reset_false_json_round_trip() {
        let setting = Setting {
            channel_name: fan_channel(),
            kind: SettingKind::Reset {
                reset_to_default: false,
            },
        };
        let serialized: Value = serde_json::to_value(&setting).unwrap();
        let parsed: Setting = serde_json::from_value(serialized).unwrap();
        assert_eq!(parsed, setting);
    }

    // Locks in the deserialization precedence for malformed multi-key payloads: the
    // first variant declared in `SettingKind` wins. Reset is declared first so that a
    // payload carrying both `reset_to_default` and `speed_fixed` is interpreted as a
    // reset, matching the engine dispatch order in `engine/main.rs`.
    #[test]
    fn multi_key_input_picks_first_declared_variant() {
        let payload = json!({
            "channel_name": "fan1",
            "reset_to_default": true,
            "speed_fixed": 50,
        });
        let setting: Setting = serde_json::from_value(payload).unwrap();
        assert!(matches!(
            setting.kind,
            SettingKind::Reset {
                reset_to_default: true
            }
        ));
    }

    // A Setting payload with no kind-discriminating field is invalid by construction
    // and must be rejected at the deserialization boundary so it cannot reach the
    // engine. The previous flat struct silently produced an "Invalid Setting" error
    // at apply-time; with the enum it fails to parse at all.
    #[test]
    fn empty_kind_rejected() {
        let payload = json!({ "channel_name": "fan1" });
        let result: Result<Setting, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }

    fn sample_source() -> CustomTempSourceData {
        CustomTempSourceData {
            temp_source: TempSource {
                temp_name: "Temp1".to_string(),
                device_uid: "dev-1".to_string(),
            },
            weight: 1,
        }
    }

    // A Mix sensor serializes to the legacy flat shape: the cs_type tag and mix_function sit
    // beside id, sources is present, and no other variant's fields leak in. It deserializes
    // back to the Mix variant.
    #[test]
    fn custom_sensor_mix_round_trip() {
        let sensor = CustomSensor {
            id: "mix1".to_string(),
            kind: SensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Avg,
                sources: vec![sample_source()],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["cs_type"], json!("Mix"));
        assert_eq!(v["mix_function"], json!("Avg"));
        assert!(v.get("sources").is_some());
        assert!(v.get("file_path").is_none());
        assert!(v.get("offset").is_none());
        assert!(v.get("time_window_seconds").is_none());

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert!(matches!(
            parsed.kind,
            SensorKind::Mix { mix_function, .. } if mix_function == CustomSensorMixFunctionType::Avg
        ));
    }

    // A File sensor serializes with only file_path beside the tag. The fields belonging to
    // other variants are absent, not null. mix_function in particular was always present on
    // the old flat struct, so its absence is the notable wire change. Round-trips to File.
    #[test]
    fn custom_sensor_file_round_trip() {
        let sensor = CustomSensor {
            id: "file1".to_string(),
            kind: SensorKind::File {
                file_path: PathBuf::from("/tmp/temp"),
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["cs_type"], json!("File"));
        assert_eq!(v["file_path"], json!("/tmp/temp"));
        assert!(v.get("mix_function").is_none());
        assert!(v.get("sources").is_none());
        assert!(v.get("offset").is_none());
        assert!(v.get("time_window_seconds").is_none());

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.kind, SensorKind::File { .. }));
    }

    // An Offset sensor serializes with offset and sources beside the tag and nothing from
    // the other variants, then deserializes back with the offset value preserved.
    #[test]
    fn custom_sensor_offset_round_trip() {
        let sensor = CustomSensor {
            id: "off1".to_string(),
            kind: SensorKind::Offset {
                offset: -7,
                sources: vec![sample_source()],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["cs_type"], json!("Offset"));
        assert_eq!(v["offset"], json!(-7));
        assert!(v.get("sources").is_some());
        assert!(v.get("mix_function").is_none());
        assert!(v.get("file_path").is_none());
        assert!(v.get("time_window_seconds").is_none());

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.kind, SensorKind::Offset { offset, .. } if offset == -7));
    }

    // A TimeAverage sensor serializes with time_window_seconds and sources beside the tag.
    // mix_function and the other variants' fields are absent. Round-trips to TimeAverage.
    #[test]
    fn custom_sensor_time_average_round_trip() {
        let sensor = CustomSensor {
            id: "ta1".to_string(),
            kind: SensorKind::TimeAverage {
                time_window_seconds: 30,
                sources: vec![sample_source()],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["cs_type"], json!("TimeAverage"));
        assert_eq!(v["time_window_seconds"], json!(30));
        assert!(v.get("sources").is_some());
        assert!(v.get("mix_function").is_none());
        assert!(v.get("file_path").is_none());
        assert!(v.get("offset").is_none());

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert!(matches!(
            parsed.kind,
            SensorKind::TimeAverage { time_window_seconds, .. } if time_window_seconds == 30
        ));
    }

    // An ExponentialMovingAvg sensor serializes like TimeAverage but under its own tag, and
    // round-trips back to the EMA variant.
    #[test]
    fn custom_sensor_ema_round_trip() {
        let sensor = CustomSensor {
            id: "ema1".to_string(),
            kind: SensorKind::ExponentialMovingAvg {
                time_window_seconds: 15,
                sources: vec![sample_source()],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["cs_type"], json!("ExponentialMovingAvg"));
        assert_eq!(v["time_window_seconds"], json!(15));
        assert!(v.get("mix_function").is_none());

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert!(matches!(
            parsed.kind,
            SensorKind::ExponentialMovingAvg { .. }
        ));
    }

    // A legacy persisted File row carries fields that are no longer part of the File variant
    // (mix_function, sources, offset, time_window_seconds). The internally-tagged enum must
    // ignore those dead siblings and deserialize cleanly, so existing configs keep loading.
    #[test]
    fn custom_sensor_reads_legacy_payload_with_dead_fields() {
        let legacy = json!({
            "id": "legacy-file",
            "cs_type": "File",
            "file_path": "/tmp/legacy",
            "mix_function": "Min",
            "sources": [],
            "offset": null,
            "time_window_seconds": null,
            "children": [],
            "parents": []
        });
        let parsed: CustomSensor = serde_json::from_value(legacy).unwrap();
        assert!(matches!(parsed.kind, SensorKind::File { .. }));
    }

    // Without the cs_type discriminator there is no variant to construct, so the payload is
    // rejected at the deserialization boundary rather than defaulting silently.
    #[test]
    fn custom_sensor_missing_cs_type_rejected() {
        let payload = json!({ "id": "x", "mix_function": "Avg", "sources": [] });
        let result: Result<CustomSensor, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }

    // mix_function is required for Mix and no longer Option, so a Mix payload missing it
    // fails to parse instead of reaching a runtime validator branch.
    #[test]
    fn custom_sensor_mix_without_mix_function_rejected() {
        let payload = json!({ "id": "x", "cs_type": "Mix", "sources": [] });
        let result: Result<CustomSensor, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }

    // time_window_seconds is required for TimeAverage and no longer Option, so a payload
    // missing it fails to parse.
    #[test]
    fn custom_sensor_time_average_without_window_rejected() {
        let payload = json!({ "id": "x", "cs_type": "TimeAverage", "sources": [] });
        let result: Result<CustomSensor, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }
}
