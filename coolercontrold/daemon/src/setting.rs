// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::Infallible;
use std::ops::Not;
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

    /// Unused; kept for downgrade compatibility.
    // Written as an empty no-op because 4.3.x hard-requires this field when parsing
    // config.toml and modes.json, so removing it breaks a daemon downgrade.
    // DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): see DEPRECATIONS.md.
    #[serde(default)]
    pub colors: Vec<(R, G, B)>,

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

/// Upper bound for `CoolerControlSettings::startup_delay`. Some systems need a
/// long wait before their hardware answers, so this is generous.
pub const STARTUP_DELAY_SECONDS_MAX: u16 = 120;

/// `CoolerControlSettings::device_listener_enabled` when the config does not set it.
pub const DEVICE_LISTENER_ENABLED_DEFAULT: bool = false;

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
    /// Custom parent sources that may embed the `CoolerControl` UI
    pub frame_ancestors: Vec<String>,
    /// Allow unencrypted HTTP connections from non-localhost addresses
    pub allow_unencrypted: bool,
    /// Header to check for proxy client protocol (e.g., "X-Forwarded-Proto")
    pub protocol_header: Option<String>,
    /// Reverse proxies, as addresses or CIDR ranges, whose `X-Forwarded-For` names the client
    pub trusted_proxies: Vec<String>,
    /// Whether to auto-detect Super-I/O sensors and load kernel modules at startup
    pub sensors_auto_detect: bool,
    /// Whether to listen for kernel device add/remove events at startup
    pub device_listener_enabled: bool,
    /// Whether to apply the `label` and `ignore` statements from the lm-sensors configuration
    pub sensors_conf_enabled: bool,
    /// Whether outbound device-service TLS connections require a fingerprint the user
    /// pinned in advance, instead of trusting the peer on first contact.
    pub tls_strict: bool,
    /// Whether to log at DEBUG level. Read at startup, so a change applies after a restart.
    pub debug_logging: bool,
}

/// Device Specific settings that generally apply to how the application deals with the device.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CCDeviceSettings {
    /// The last-detected device name, stamped by the daemon. Names disabled
    /// (blacklisted) devices and keeps the config file hand-navigable.
    /// Deprecated as a writable field: client-supplied values are ignored.
    /// User-defined names belong in the name overrides (overrides.toml).
    /// On read it carries the resolved display name.
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
    /// The last-detected channel label, stamped by the daemon. Names disabled
    /// channels. Deprecated as a writable field: client-supplied values are
    /// ignored. User-defined labels belong in the name overrides
    /// (overrides.toml). On read it carries the resolved display label.
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
    pub uid: ProfileUID,

    pub name: String,

    /// The function uid to apply to this profile.
    pub function_uid: FunctionUID,

    /// The profile type and its type-specific fields. Flattened to keep the wire shape
    /// (`p_type` plus the active fields at the top level) identical to the pre-enum struct.
    #[serde(flatten)]
    pub kind: ProfileKind,
}

/// The profile type discriminator (`p_type`) and the fields valid for that type. Mutual
/// exclusivity is enforced by the type; per-variant fields stay `Option` so requiredness is left to
/// apply-time validation, keeping the input wire contract backward-compatible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "p_type")]
pub enum ProfileKind {
    Default,
    Fixed {
        /// The fixed duty speed to set. eg: 20 (%)
        speed_fixed: Option<Duty>,
    },
    Graph {
        /// The profile temp/duty speeds to set. eg: [(20.0, 50), (25.7, 80)]
        speed_profile: Option<Vec<(Temp, Duty)>>,
        temp_source: Option<TempSource>,
        temp_min: Option<Temp>,
        temp_max: Option<Temp>,
    },
    Mix {
        member_profile_uids: Vec<ProfileUID>,
        mix_function_type: Option<ProfileMixFunctionType>,
    },
    Overlay {
        member_profile_uids: Vec<ProfileUID>,
        /// The graph offset to apply to the associated member profile. Can also be a static
        /// offset when there is only one duty/offset pair.
        offset_profile: Option<Vec<(Duty, Offset)>>,
    },
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            uid: DEFAULT_PROFILE_UID.to_string(),
            name: "Unmanaged".to_string(),
            function_uid: DEFAULT_FUNCTION_UID.to_string(),
            kind: ProfileKind::Default,
        }
    }
}

impl Profile {
    pub fn p_type(&self) -> ProfileType {
        match self.kind {
            ProfileKind::Default => ProfileType::Default,
            ProfileKind::Fixed { .. } => ProfileType::Fixed,
            ProfileKind::Graph { .. } => ProfileType::Graph,
            ProfileKind::Mix { .. } => ProfileType::Mix,
            ProfileKind::Overlay { .. } => ProfileType::Overlay,
        }
    }

    pub fn speed_fixed(&self) -> Option<Duty> {
        match &self.kind {
            ProfileKind::Fixed { speed_fixed } => *speed_fixed,
            _ => None,
        }
    }

    pub fn speed_profile(&self) -> Option<&Vec<(Temp, Duty)>> {
        match &self.kind {
            ProfileKind::Graph { speed_profile, .. } => speed_profile.as_ref(),
            _ => None,
        }
    }

    pub fn temp_source(&self) -> Option<&TempSource> {
        match &self.kind {
            ProfileKind::Graph { temp_source, .. } => temp_source.as_ref(),
            _ => None,
        }
    }

    pub fn member_profile_uids(&self) -> &[ProfileUID] {
        match &self.kind {
            ProfileKind::Mix {
                member_profile_uids,
                ..
            }
            | ProfileKind::Overlay {
                member_profile_uids,
                ..
            } => member_profile_uids,
            _ => &[],
        }
    }

    pub fn member_profile_uids_mut(&mut self) -> Option<&mut Vec<ProfileUID>> {
        match &mut self.kind {
            ProfileKind::Mix {
                member_profile_uids,
                ..
            }
            | ProfileKind::Overlay {
                member_profile_uids,
                ..
            } => Some(member_profile_uids),
            _ => None,
        }
    }

    pub fn mix_function_type(&self) -> Option<ProfileMixFunctionType> {
        match &self.kind {
            ProfileKind::Mix {
                mix_function_type, ..
            } => *mix_function_type,
            _ => None,
        }
    }

    pub fn offset_profile(&self) -> Option<&Vec<(Duty, Offset)>> {
        match &self.kind {
            ProfileKind::Overlay { offset_profile, .. } => offset_profile.as_ref(),
            _ => None,
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

    /// Whether to temporarily bypass thresholds when fan speed remains unchanged for 30+ seconds to meet curve target.
    pub threshold_hopping: bool,

    /// Whether to bypass the minimum step size when the target duty is exactly 0% or 100%.
    /// Useful for ensuring fans fully stop or reach max RPM even when the change is below the
    /// minimum step size. The maximum step size is still respected. Disabled by default.
    #[serde(default)]
    pub bypass_min_at_extremes: bool,

    /// The function type and its type-specific fields. Flattened to keep the wire shape (`f_type`
    /// plus the active fields at the top level) identical to the pre-enum struct.
    #[serde(flatten)]
    pub kind: FunctionKind,
}

impl Default for Function {
    fn default() -> Self {
        Self {
            uid: DEFAULT_FUNCTION_UID.to_string(),
            name: "Default Function".to_string(),
            step_size_min: 2,
            step_size_max: 100,          // 0 = fixed step size
            step_size_min_decreasing: 0, // 0 = disabled/symmetric step size
            step_size_max_decreasing: 0, // 0 = fixed step size
            threshold_hopping: true,
            bypass_min_at_extremes: false,
            kind: FunctionKind::Identity,
        }
    }
}

impl Function {
    pub fn f_type(&self) -> FunctionType {
        match self.kind {
            FunctionKind::Identity => FunctionType::Identity,
            FunctionKind::Standard { .. } => FunctionType::Standard,
        }
    }

    pub fn deviance(&self) -> Option<Temp> {
        match &self.kind {
            FunctionKind::Standard { deviance, .. } => *deviance,
            FunctionKind::Identity => None,
        }
    }

    pub fn only_downward(&self) -> Option<bool> {
        match &self.kind {
            FunctionKind::Standard { only_downward, .. } => *only_downward,
            FunctionKind::Identity => None,
        }
    }

    pub fn response_delay(&self) -> Option<u8> {
        match &self.kind {
            FunctionKind::Standard { response_delay, .. } => *response_delay,
            FunctionKind::Identity => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Display, EnumString, Serialize, Deserialize, JsonSchema)]
pub enum FunctionType {
    Identity,
    Standard,
}

/// The function type discriminator (`f_type`) and the fields valid for that type. Most `Function`
/// fields (the step-size/hysteresis and safety-latch config) apply to every type and stay on
/// `Function`; only these are type-specific. Per-variant fields stay `Option` so requiredness is left
/// to apply-time validation, keeping the input wire contract backward-compatible.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "f_type")]
pub enum FunctionKind {
    Identity,
    Standard {
        /// The temperature deviance threshold in degrees
        deviance: Option<Temp>,

        /// Whether to apply settings only on the way down
        only_downward: Option<bool>,

        /// The response delay in seconds
        response_delay: Option<u8>,
    },
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
    Sum,
}

/// The channel metric a Custom Sensor reads and reports. All of its sources share it.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Display,
    EnumString,
    Serialize,
    Deserialize,
    JsonSchema,
)]
pub enum CustomSensorMetric {
    #[default]
    Temp,
    Duty,
    RPM,
    Freq,
    Watts,
}

impl CustomSensorMetric {
    pub fn is_temp(self) -> bool {
        self == Self::Temp
    }

    /// The largest offset, in either direction, a Scale & Offset sensor of this metric
    /// takes. Temperatures keep their range; the other metrics span far wider values.
    pub fn offset_limit(self) -> f64 {
        match self {
            Self::Temp => 100.,
            Self::Duty | Self::RPM | Self::Freq | Self::Watts => 1_000_000.,
        }
    }
}

/// The factor of a Scale & Offset sensor: finite, and with a magnitude inside
/// `SCALE_MAGNITUDE_MIN..=SCALE_MAGNITUDE_MAX`, which excludes 0. Negative inverts.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "f64", into = "f64")]
pub struct Scale(f64);

pub const SCALE_MAGNITUDE_MIN: f64 = 0.000_001;
pub const SCALE_MAGNITUDE_MAX: f64 = 1_000_000.;

impl Scale {
    pub fn get(self) -> f64 {
        self.0
    }

    /// Exactly 1: the value passes through as it did before the scale existed.
    #[allow(clippy::float_cmp)]
    pub fn is_identity(self) -> bool {
        self.0 == 1.
    }
}

impl Default for Scale {
    fn default() -> Self {
        Self(1.)
    }
}

impl TryFrom<f64> for Scale {
    type Error = String;

    fn try_from(value: f64) -> Result<Self, Self::Error> {
        if value.is_finite().not() {
            return Err("scale must be a finite number".to_string());
        }
        if (SCALE_MAGNITUDE_MIN..=SCALE_MAGNITUDE_MAX)
            .contains(&value.abs())
            .not()
        {
            return Err(format!(
                "scale must be non-zero with a magnitude between {SCALE_MAGNITUDE_MIN} and \
                {SCALE_MAGNITUDE_MAX}"
            ));
        }
        Ok(Self(value))
    }
}

impl From<Scale> for f64 {
    fn from(scale: Scale) -> Self {
        scale.0
    }
}

/// One input of a Custom Sensor. The sensor's metric says whether `name` is a temp or a
/// channel of the device.
#[derive(Debug, Clone, PartialEq)]
pub struct SensorSource {
    /// The device holding the current values.
    pub device_uid: DeviceUID,

    /// The internal temp or channel name on that device. NOT the label.
    pub name: String,

    pub weight: Weight,
}

/// NOTE: serde and `JsonSchema` are bridged through the private [`CustomSensorWire`], which
/// keeps the source keys that clients and stored configs use. The runtime works on
/// [`SensorSource`] alone.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(try_from = "CustomSensorWire", into = "CustomSensorWire")]
pub struct CustomSensor {
    /// ID MUST be unique, as `temp_name` must be unique.
    pub id: TempName,

    pub metric: CustomSensorMetric,

    pub kind: CustomSensorKind,

    /// Filled internally, see [`CustomSensorWire`].
    pub children: Vec<TempName>,

    pub parents: Vec<TempName>,
}

/// Variant-specific payload of a `CustomSensor`, internally tagged on `cs_type`. Exactly one
/// variant is valid per sensor. Constraints the type cannot express (single source for
/// `Offset`/`TimeAverage`/`ExponentialMovingAvg`, `offset` within the metric's limit,
/// `time_window_seconds` in `1..=300`) are enforced at the API boundary in
/// `validate_custom_sensor`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "cs_type")]
pub enum CustomSensorKind<S = SensorSource> {
    Mix {
        mix_function: CustomSensorMixFunctionType,
        sources: Vec<S>,
    },
    File {
        file_path: PathBuf,
    },
    /// Scale & Offset: `source * scale + offset`.
    Offset {
        /// 1 when absent.
        #[serde(default)]
        #[schemars(with = "f64")]
        scale: Scale,
        offset: f64,
        sources: Vec<S>,
    },
    TimeAverage {
        time_window_seconds: u16,
        sources: Vec<S>,
    },
    ExponentialMovingAvg {
        time_window_seconds: u16,
        sources: Vec<S>,
    },
}

impl<S> CustomSensorKind<S> {
    /// The same kind with every source converted, or the first conversion error.
    fn try_map_sources<T, E>(
        self,
        convert: impl FnMut(S) -> Result<T, E>,
    ) -> Result<CustomSensorKind<T>, E> {
        let kind = match self {
            Self::Mix {
                mix_function,
                sources,
            } => CustomSensorKind::Mix {
                mix_function,
                sources: sources.into_iter().map(convert).collect::<Result<_, E>>()?,
            },
            Self::File { file_path } => CustomSensorKind::File { file_path },
            Self::Offset {
                scale,
                offset,
                sources,
            } => CustomSensorKind::Offset {
                scale,
                offset,
                sources: sources.into_iter().map(convert).collect::<Result<_, E>>()?,
            },
            Self::TimeAverage {
                time_window_seconds,
                sources,
            } => CustomSensorKind::TimeAverage {
                time_window_seconds,
                sources: sources.into_iter().map(convert).collect::<Result<_, E>>()?,
            },
            Self::ExponentialMovingAvg {
                time_window_seconds,
                sources,
            } => CustomSensorKind::ExponentialMovingAvg {
                time_window_seconds,
                sources: sources.into_iter().map(convert).collect::<Result<_, E>>()?,
            },
        };
        Ok(kind)
    }

    /// The same kind with every source converted.
    fn map_sources<T>(self, mut convert: impl FnMut(S) -> T) -> CustomSensorKind<T> {
        match self.try_map_sources(|source| Ok::<T, Infallible>(convert(source))) {
            Ok(kind) => kind,
            Err(never) => match never {},
        }
    }
}

impl CustomSensor {
    /// The temp sources this sensor reads from. `File` sensors have none.
    pub fn sources(&self) -> &[SensorSource] {
        match &self.kind {
            CustomSensorKind::Mix { sources, .. }
            | CustomSensorKind::Offset { sources, .. }
            | CustomSensorKind::TimeAverage { sources, .. }
            | CustomSensorKind::ExponentialMovingAvg { sources, .. } => sources,
            CustomSensorKind::File { .. } => &[],
        }
    }

    /// Mutable access to this sensor's temp sources, or `None` for `File` sensors.
    pub fn sources_mut(&mut self) -> Option<&mut Vec<SensorSource>> {
        match &mut self.kind {
            CustomSensorKind::Mix { sources, .. }
            | CustomSensorKind::Offset { sources, .. }
            | CustomSensorKind::TimeAverage { sources, .. }
            | CustomSensorKind::ExponentialMovingAvg { sources, .. } => Some(sources),
            CustomSensorKind::File { .. } => None,
        }
    }
}

// Wire shape of `CustomSensor` (see the NOTE on that type).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CustomSensorWire {
    /// ID MUST be unique, as `temp_name` must be unique.
    id: TempName,

    /// The channel metric the sensor reads and reports, `Temp` when absent. All sources
    /// share it: a `Temp` sensor's sources are `temp_source`, any other metric's are
    /// `channel_source`. It cannot change once the sensor exists.
    #[serde(default)]
    metric: CustomSensorMetric,

    /// Variant payload, flattened so its fields and the `cs_type` discriminator stay flat
    /// siblings of `id` on the wire (the legacy shape).
    #[serde(flatten)]
    kind: CustomSensorKind<SensorSourceWire>,

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
    children: Vec<TempName>,

    /// The Custom Sensor's parents, if any. See `children` for more details.
    #[serde(default)]
    parents: Vec<TempName>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "CustomSensorSource")]
struct SensorSourceWire {
    #[serde(flatten)]
    node: SourceNodeWire,
    weight: Weight,
}

// The key a source travels under. Temp is declared first, so it wins when a payload
// carries both.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
enum SourceNodeWire {
    Temp {
        temp_source: TempSource,
    },
    Channel {
        channel_source: CustomSensorChannelSource,
    },
}

/// A channel a non-temperature Custom Sensor reads. The sensor's metric says which of the
/// channel's values.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct CustomSensorChannelSource {
    /// The associated device uid containing current values
    device_uid: DeviceUID,

    /// The internal name for this channel. NOT the Label.
    channel_name: ChannelName,
}

impl SensorSource {
    pub fn from_temp(temp_source: TempSource, weight: Weight) -> Self {
        Self {
            device_uid: temp_source.device_uid,
            name: temp_source.temp_name,
            weight,
        }
    }

    pub fn temp_source(&self) -> TempSource {
        TempSource {
            temp_name: self.name.clone(),
            device_uid: self.device_uid.clone(),
        }
    }
}

impl SensorSourceWire {
    fn of(metric: CustomSensorMetric, source: SensorSource) -> Self {
        let weight = source.weight;
        let node = if metric.is_temp() {
            SourceNodeWire::Temp {
                temp_source: TempSource {
                    temp_name: source.name,
                    device_uid: source.device_uid,
                },
            }
        } else {
            SourceNodeWire::Channel {
                channel_source: CustomSensorChannelSource {
                    device_uid: source.device_uid,
                    channel_name: source.name,
                },
            }
        };
        Self { node, weight }
    }

    /// The runtime source, or why the key does not fit the sensor's metric.
    fn into_source(self, metric: CustomSensorMetric) -> Result<SensorSource, String> {
        match self.node {
            SourceNodeWire::Temp { temp_source } => {
                if metric.is_temp().not() {
                    return Err(format!(
                        "A {metric} Custom Sensor takes channel_source sources, not temp_source"
                    ));
                }
                Ok(SensorSource::from_temp(temp_source, self.weight))
            }
            SourceNodeWire::Channel { channel_source } => {
                if metric.is_temp() {
                    return Err(
                        "A Temp Custom Sensor takes temp_source sources, not channel_source"
                            .to_string(),
                    );
                }
                Ok(SensorSource {
                    device_uid: channel_source.device_uid,
                    name: channel_source.channel_name,
                    weight: self.weight,
                })
            }
        }
    }
}

impl TryFrom<CustomSensorWire> for CustomSensor {
    type Error = String;

    fn try_from(wire: CustomSensorWire) -> Result<Self, Self::Error> {
        let metric = wire.metric;
        Ok(Self {
            id: wire.id,
            metric,
            kind: wire
                .kind
                .try_map_sources(|source| source.into_source(metric))?,
            children: wire.children,
            parents: wire.parents,
        })
    }
}

impl From<CustomSensor> for CustomSensorWire {
    fn from(sensor: CustomSensor) -> Self {
        let metric = sensor.metric;
        Self {
            id: sensor.id,
            metric,
            kind: sensor
                .kind
                .map_sources(|source| SensorSourceWire::of(metric, source)),
            children: sensor.children,
            parents: sensor.parents,
        }
    }
}

impl JsonSchema for CustomSensor {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("CustomSensor")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        CustomSensorWire::json_schema(generator)
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
                    colors: Vec::new(),
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
            colors: Vec::new(),
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

    // The no-op colors field always serializes (4.3.x requires it after a downgrade) and its
    // absence still deserializes (files written by the brief colors-less dev builds).
    #[test]
    fn lcd_colors_downgrade_compat() {
        let serialized = serde_json::to_value(lcd(LcdModeKind::Liquid)).unwrap();
        assert_eq!(serialized["colors"], json!([]));

        let stripped = json!({
            "mode": "liquid",
            "brightness": 80,
            "orientation": null
        });
        let parsed: LcdSettings = serde_json::from_value(stripped).unwrap();
        assert!(parsed.colors.is_empty());
    }

    fn profile(kind: ProfileKind) -> Profile {
        Profile {
            uid: "prof-1".to_string(),
            name: "Test".to_string(),
            function_uid: "fn-1".to_string(),
            kind,
        }
    }

    fn graph_temp_source() -> TempSource {
        TempSource {
            temp_name: "Temp1".to_string(),
            device_uid: "dev-1".to_string(),
        }
    }

    // A Fixed Profile keeps `p_type: "Fixed"` and `speed_fixed` beside the shared fields
    // (uid/name/function_uid) and omits every other type's fields. Dropping the previously-null
    // speed_profile/temp_source and the empty member_profile_uids is the intended output reduction.
    #[test]
    fn profile_fixed_round_trip() {
        let p = profile(ProfileKind::Fixed {
            speed_fixed: Some(42),
        });
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["p_type"], json!("Fixed"));
        assert_eq!(v["speed_fixed"], json!(42));
        assert_eq!(v["uid"], json!("prof-1"));
        assert_eq!(v["function_uid"], json!("fn-1"));
        assert!(v.get("speed_profile").is_none());
        assert!(v.get("member_profile_uids").is_none());
        assert!(v.get("offset_profile").is_none());

        let parsed: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.kind, p.kind);
    }

    // A Graph Profile carries speed_profile/temp_source/temp_min/temp_max under `p_type: "Graph"`
    // and omits the other types' fields.
    #[test]
    fn profile_graph_round_trip() {
        let p = profile(ProfileKind::Graph {
            speed_profile: Some(vec![(20.0, 30), (50.0, 80)]),
            temp_source: Some(graph_temp_source()),
            temp_min: Some(20.0),
            temp_max: Some(80.0),
        });
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["p_type"], json!("Graph"));
        assert!(v.get("speed_profile").is_some());
        assert!(v.get("temp_source").is_some());
        assert!(v.get("temp_min").is_some());
        assert!(v.get("temp_max").is_some());
        assert!(v.get("speed_fixed").is_none());
        assert!(v.get("member_profile_uids").is_none());

        let parsed: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.kind, p.kind);
    }

    // A Mix Profile carries member_profile_uids + mix_function_type under `p_type: "Mix"`.
    #[test]
    fn profile_mix_round_trip() {
        let p = profile(ProfileKind::Mix {
            member_profile_uids: vec!["a".to_string(), "b".to_string()],
            mix_function_type: Some(ProfileMixFunctionType::Max),
        });
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["p_type"], json!("Mix"));
        assert_eq!(v["member_profile_uids"], json!(["a", "b"]));
        assert!(v.get("mix_function_type").is_some());
        assert!(v.get("speed_fixed").is_none());
        assert!(v.get("offset_profile").is_none());

        let parsed: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.kind, p.kind);
    }

    // An Overlay Profile carries member_profile_uids + offset_profile under `p_type: "Overlay"`.
    #[test]
    fn profile_overlay_round_trip() {
        let p = profile(ProfileKind::Overlay {
            member_profile_uids: vec!["a".to_string()],
            offset_profile: Some(vec![(50, -5)]),
        });
        let v = serde_json::to_value(&p).unwrap();
        assert_eq!(v["p_type"], json!("Overlay"));
        assert_eq!(v["member_profile_uids"], json!(["a"]));
        assert!(v.get("offset_profile").is_some());
        assert!(v.get("mix_function_type").is_none());

        let parsed: Profile = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.kind, p.kind);
    }

    // A Default Profile serializes to just `p_type: "Default"` beside the shared fields.
    #[test]
    fn profile_default_serializes_tag_only() {
        let v = serde_json::to_value(profile(ProfileKind::Default)).unwrap();
        assert_eq!(v["p_type"], json!("Default"));
        assert!(v.get("speed_fixed").is_none());
        assert!(v.get("speed_profile").is_none());
        assert!(v.get("member_profile_uids").is_none());

        let parsed: Profile = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.kind, ProfileKind::Default));
    }

    // A legacy Fixed payload that still carries the other types' fields (null speed_profile, empty
    // member_profile_uids, null offset_profile) must deserialize and ignore them, so pre-refactor
    // configs and existing REST/TOML clients keep working.
    #[test]
    fn profile_reads_legacy_dead_fields() {
        let legacy = json!({
            "uid": "prof-1",
            "name": "Test",
            "function_uid": "fn-1",
            "p_type": "Fixed",
            "speed_fixed": 42,
            "speed_profile": null,
            "temp_source": null,
            "temp_min": null,
            "temp_max": null,
            "member_profile_uids": [],
            "mix_function_type": null,
            "offset_profile": null
        });
        let parsed: Profile = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            parsed.kind,
            ProfileKind::Fixed {
                speed_fixed: Some(42)
            }
        );
    }

    fn function(kind: FunctionKind) -> Function {
        Function {
            uid: "fn-1".to_string(),
            name: "Test".to_string(),
            step_size_min: 2,
            step_size_max: 100,
            step_size_min_decreasing: 0,
            step_size_max_decreasing: 0,
            threshold_hopping: true,
            bypass_min_at_extremes: false,
            kind,
        }
    }

    // A Standard Function keeps `f_type: "Standard"` plus deviance/only_downward beside the shared
    // step-size fields, and omits the EMA-only sample_window. The shared fields stay present.
    #[test]
    fn function_standard_round_trip() {
        let f = function(FunctionKind::Standard {
            deviance: Some(2.0),
            only_downward: Some(true),
            response_delay: Some(5),
        });
        let v = serde_json::to_value(&f).unwrap();
        assert_eq!(v["f_type"], json!("Standard"));
        assert_eq!(v["deviance"], json!(2.0));
        assert_eq!(v["only_downward"], json!(true));
        assert_eq!(v["response_delay"], json!(5));
        assert_eq!(v["duty_minimum"], json!(2));
        assert!(v.get("sample_window").is_none());

        let parsed: Function = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.kind, f.kind);
    }

    // The removed EMA type is rejected at the deserialization boundary; config-file conversion
    // happens in the TOML parser, not here, so REST clients get a clear error instead.
    #[test]
    fn function_removed_ema_type_rejected() {
        let payload = json!({
            "uid": "fn-1",
            "name": "Test",
            "f_type": "ExponentialMovingAvg",
            "duty_minimum": 2,
            "duty_maximum": 100,
            "sample_window": 8,
        });
        let result: Result<Function, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }

    // An Identity Function serializes to just `f_type: "Identity"` beside the shared fields, with
    // none of the type-specific fields.
    #[test]
    fn function_identity_serializes_tag_only() {
        let v = serde_json::to_value(function(FunctionKind::Identity)).unwrap();
        assert_eq!(v["f_type"], json!("Identity"));
        assert!(v.get("deviance").is_none());
        assert!(v.get("only_downward").is_none());
        assert!(v.get("response_delay").is_none());
        assert!(v.get("sample_window").is_none());

        let parsed: Function = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.kind, FunctionKind::Identity));
    }

    // A legacy Standard payload that still carries the EMA-only sample_window must deserialize and
    // ignore it, so pre-refactor configs and REST clients keep working.
    #[test]
    fn function_reads_legacy_dead_fields() {
        let legacy = json!({
            "uid": "fn-1",
            "name": "Test",
            "f_type": "Standard",
            "duty_minimum": 2,
            "duty_maximum": 100,
            "step_size_min_decreasing": 0,
            "step_size_max_decreasing": 0,
            "response_delay": 5,
            "deviance": 2.0,
            "only_downward": false,
            "sample_window": 8,
            "threshold_hopping": true,
            "bypass_min_at_extremes": false
        });
        let parsed: Function = serde_json::from_value(legacy).unwrap();
        assert_eq!(
            parsed.kind,
            FunctionKind::Standard {
                deviance: Some(2.0),
                only_downward: Some(false),
                response_delay: Some(5),
            }
        );
    }

    // Without the f_type discriminator there is no variant to construct, so the payload is rejected
    // at the deserialization boundary.
    #[test]
    fn function_missing_f_type_rejected() {
        let payload = json!({ "uid": "x", "name": "x", "duty_minimum": 2 });
        let result: Result<Function, _> = serde_json::from_value(payload);
        assert!(result.is_err());
    }

    // The f_type() accessor maps each kind back to its FunctionType discriminant.
    #[test]
    fn function_f_type_accessor() {
        assert_eq!(
            function(FunctionKind::Identity).f_type(),
            FunctionType::Identity
        );
        assert_eq!(
            function(FunctionKind::Standard {
                deviance: None,
                only_downward: None,
                response_delay: None,
            })
            .f_type(),
            FunctionType::Standard
        );
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

    fn sample_source() -> SensorSource {
        SensorSource {
            name: "Temp1".to_string(),
            device_uid: "dev-1".to_string(),
            weight: 1,
        }
    }

    // The flat runtime source travels as the nested `temp_source` key that clients and stored
    // configs use, and reads back to the same source.
    #[test]
    fn custom_sensor_source_keeps_its_wire_shape() {
        let sensor = CustomSensor {
            id: "mix1".to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Avg,
                sources: vec![sample_source()],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(
            v["sources"],
            json!([{
                "temp_source": { "temp_name": "Temp1", "device_uid": "dev-1" },
                "weight": 1
            }])
        );

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.sources(), &[sample_source()]);
    }

    // A payload from a client that predates the metric carries none and is a temperature
    // sensor. Responses always state the metric.
    #[test]
    fn custom_sensor_metric_defaults_to_temp() {
        let payload = json!({
            "id": "mix1",
            "cs_type": "Mix",
            "mix_function": "Avg",
            "sources": [{
                "temp_source": { "temp_name": "Temp1", "device_uid": "dev-1" },
                "weight": 1
            }]
        });
        let parsed: CustomSensor = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.metric, CustomSensorMetric::Temp);
        assert_eq!(parsed.sources(), &[sample_source()]);

        let v = serde_json::to_value(&parsed).unwrap();
        assert_eq!(v["metric"], json!("Temp"));
    }

    // A non-temperature sensor's sources travel under `channel_source` and read back to
    // the same runtime source a temperature sensor would hold.
    #[test]
    fn custom_sensor_channel_source_round_trip() {
        let sensor = CustomSensor {
            id: "mix1".to_string(),
            metric: CustomSensorMetric::RPM,
            kind: CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Avg,
                sources: vec![SensorSource {
                    device_uid: "dev-1".to_string(),
                    name: "fan1".to_string(),
                    weight: 2,
                }],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["metric"], json!("RPM"));
        assert_eq!(
            v["sources"],
            json!([{
                "channel_source": { "device_uid": "dev-1", "channel_name": "fan1" },
                "weight": 2
            }])
        );

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert_eq!(parsed.metric, CustomSensorMetric::RPM);
        assert_eq!(parsed.sources(), sensor.sources());
    }

    // The source key must fit the metric: a temp key on a channel metric, or a channel key
    // on a temperature sensor, is rejected at the deserialization boundary.
    #[test]
    fn custom_sensor_source_key_must_match_the_metric() {
        let temp_source = json!({
            "temp_source": { "temp_name": "Temp1", "device_uid": "dev-1" }, "weight": 1
        });
        let channel_source = json!({
            "channel_source": { "device_uid": "dev-1", "channel_name": "fan1" }, "weight": 1
        });
        let payload = |metric: &str, source: &Value| {
            json!({
                "id": "x", "metric": metric, "cs_type": "Mix", "mix_function": "Avg",
                "sources": [source]
            })
        };

        let valid: Result<CustomSensor, _> =
            serde_json::from_value(payload("Watts", &channel_source));
        assert!(valid.is_ok());
        let rpm_with_temp: Result<CustomSensor, _> =
            serde_json::from_value(payload("RPM", &temp_source));
        assert!(rpm_with_temp.is_err());
        let temp_with_channel: Result<CustomSensor, _> =
            serde_json::from_value(payload("Temp", &channel_source));
        assert!(temp_with_channel.is_err());
        let unknown_metric: Result<CustomSensor, _> =
            serde_json::from_value(payload("Volts", &channel_source));
        assert!(unknown_metric.is_err());
    }

    // A Mix sensor serializes to the legacy flat shape: the cs_type tag and mix_function sit
    // beside id, sources is present, and no other variant's fields leak in. It deserializes
    // back to the Mix variant.
    #[test]
    fn custom_sensor_mix_round_trip() {
        let sensor = CustomSensor {
            id: "mix1".to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::Mix {
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
            CustomSensorKind::Mix { mix_function, .. } if mix_function == CustomSensorMixFunctionType::Avg
        ));
    }

    // A File sensor serializes with only file_path beside the tag. The fields belonging to
    // other variants are absent, not null. mix_function in particular was always present on
    // the old flat struct, so its absence is the notable wire change. Round-trips to File.
    #[test]
    fn custom_sensor_file_round_trip() {
        let sensor = CustomSensor {
            id: "file1".to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::File {
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
        assert!(matches!(parsed.kind, CustomSensorKind::File { .. }));
    }

    // An Offset sensor serializes with offset and sources beside the tag and nothing from
    // the other variants, then deserializes back with the offset value preserved.
    #[test]
    fn custom_sensor_offset_round_trip() {
        let sensor = CustomSensor {
            id: "off1".to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::Offset {
                scale: Scale::default(),
                offset: -7.,
                sources: vec![sample_source()],
            },
            children: Vec::new(),
            parents: Vec::new(),
        };
        let v = serde_json::to_value(&sensor).unwrap();
        assert_eq!(v["cs_type"], json!("Offset"));
        assert_eq!(v["offset"], json!(-7.0));
        assert_eq!(v["scale"], json!(1.0));
        assert!(v.get("sources").is_some());
        assert!(v.get("mix_function").is_none());
        assert!(v.get("file_path").is_none());
        assert!(v.get("time_window_seconds").is_none());

        let parsed: CustomSensor = serde_json::from_value(v).unwrap();
        assert!(matches!(parsed.kind, CustomSensorKind::Offset { offset, .. } if offset == -7.));
    }

    // A scale is any finite factor whose magnitude lies between the two bounds, on either
    // sign. Zero, anything outside the bounds and non-finite values are refused.
    #[test]
    fn scale_takes_only_usable_factors() {
        for valid in [
            1.,
            -1.,
            0.001,
            SCALE_MAGNITUDE_MIN,
            -SCALE_MAGNITUDE_MIN,
            SCALE_MAGNITUDE_MAX,
            -SCALE_MAGNITUDE_MAX,
        ] {
            assert_eq!(Scale::try_from(valid).map(Scale::get), Ok(valid));
        }
        for invalid in [
            0.,
            -0.,
            0.000_000_9,
            1_000_001.,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
        ] {
            assert!(Scale::try_from(invalid).is_err(), "{invalid}");
        }
        assert!(Scale::default().is_identity());
        assert!(Scale::try_from(1.000_001).unwrap().is_identity().not());
    }

    // Temperatures keep the offset range they always had; every other metric gets the wide one.
    #[test]
    fn offset_limit_follows_the_metric() {
        assert_eq!(CustomSensorMetric::Temp.offset_limit(), 100.);
        for metric in [
            CustomSensorMetric::Duty,
            CustomSensorMetric::RPM,
            CustomSensorMetric::Freq,
            CustomSensorMetric::Watts,
        ] {
            assert_eq!(metric.offset_limit(), 1_000_000.);
        }
    }

    // An Offset payload from before the scale carries a whole offset and no scale, and reads
    // as scale 1. A scale and a decimal offset round-trip. An unusable scale is refused.
    #[test]
    fn custom_sensor_offset_scale_defaults_to_one() {
        let payload = |extra: Value| {
            let mut payload = json!({
                "id": "off1",
                "cs_type": "Offset",
                "sources": [{
                    "temp_source": { "temp_name": "Temp1", "device_uid": "dev-1" },
                    "weight": 1
                }]
            });
            for (key, value) in extra.as_object().unwrap() {
                payload[key] = value.clone();
            }
            payload
        };

        let legacy: CustomSensor =
            serde_json::from_value(payload(json!({ "offset": -7 }))).unwrap();
        assert!(matches!(
            legacy.kind,
            CustomSensorKind::Offset { scale, offset, .. } if scale.is_identity() && offset == -7.
        ));

        let scaled: CustomSensor =
            serde_json::from_value(payload(json!({ "scale": 0.001, "offset": 0.5 }))).unwrap();
        let v = serde_json::to_value(&scaled).unwrap();
        assert_eq!(v["scale"], json!(0.001));
        assert_eq!(v["offset"], json!(0.5));

        let zero_scale: Result<CustomSensor, _> =
            serde_json::from_value(payload(json!({ "scale": 0, "offset": 1 })));
        assert!(zero_scale.is_err());
    }

    // A TimeAverage sensor serializes with time_window_seconds and sources beside the tag.
    // mix_function and the other variants' fields are absent. Round-trips to TimeAverage.
    #[test]
    fn custom_sensor_time_average_round_trip() {
        let sensor = CustomSensor {
            id: "ta1".to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::TimeAverage {
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
            CustomSensorKind::TimeAverage { time_window_seconds, .. } if time_window_seconds == 30
        ));
    }

    // An ExponentialMovingAvg sensor serializes like TimeAverage but under its own tag, and
    // round-trips back to the EMA variant.
    #[test]
    fn custom_sensor_ema_round_trip() {
        let sensor = CustomSensor {
            id: "ema1".to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::ExponentialMovingAvg {
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
            CustomSensorKind::ExponentialMovingAvg { .. }
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
        assert!(matches!(parsed.kind, CustomSensorKind::File { .. }));
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
