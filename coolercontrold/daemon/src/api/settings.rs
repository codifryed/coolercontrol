// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::devices::{DeviceChannelPath, DevicePath};
use crate::api::{handle_error, AppState, CCError};
use crate::device::{ChannelName, UID};
use crate::overrides::OverridesDocument;
use crate::setting::{
    CCChannelSettings, CCDeviceSettings, CoolerControlSettings, DeviceExtensions,
    STARTUP_DELAY_SECONDS_MAX,
};
use axum::extract::{Path, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// Get General `CoolerControl` settings
pub async fn get_cc(
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<Json<CoolerControlSettingsDto>, CCError> {
    setting_handle
        .get_cc()
        .await
        .map(|settings| Json(CoolerControlSettingsDto::from(settings)))
        .map_err(handle_error)
}

/// Apply General `CoolerControl` settings
pub async fn update_cc(
    State(AppState { setting_handle, .. }): State<AppState>,
    Json(cc_settings_request): Json<CoolerControlSettingsDto>,
) -> Result<(), CCError> {
    setting_handle
        .update_cc(cc_settings_request)
        .await
        .map_err(handle_error)
}

/// Get All `CoolerControl` settings that apply to a specific Device
pub async fn get_all_cc_devices(
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<Json<CoolerControlAllDeviceSettingsDto>, CCError> {
    setting_handle
        .get_all_cc_devices()
        .await
        .map(|devices| Json(CoolerControlAllDeviceSettingsDto { devices }))
        .map_err(handle_error)
}

/// Get `CoolerControl` settings that apply to a specific Device
pub async fn get_cc_device(
    Path(path): Path<DevicePath>,
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<Json<CoolerControlDeviceSettingsDto>, CCError> {
    setting_handle
        .get_cc_device(path.device_uid)
        .await
        .map(Json)
        .map_err(handle_error)
}

/// Save `CoolerControl` settings that apply to a specific Device
pub async fn update_cc_device(
    Path(path): Path<DevicePath>,
    State(AppState { setting_handle, .. }): State<AppState>,
    Json(cc_device_settings_request): Json<CCDeviceSettings>,
) -> Result<(), CCError> {
    setting_handle
        .update_cc_device(path.device_uid, cc_device_settings_request)
        .await
        .map_err(handle_error)
}

/// Returns the raw, sparse name-overrides document (`overrides.toml`).
pub async fn get_overrides(
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<Json<OverridesDocument>, CCError> {
    setting_handle
        .get_overrides()
        .await
        .map(Json)
        .map_err(handle_error)
}

/// Sets or removes the user-defined display name for a device.
pub async fn update_device_overrides(
    Path(path): Path<DevicePath>,
    State(AppState { setting_handle, .. }): State<AppState>,
    Json(request): Json<DeviceNameOverrideRequest>,
) -> Result<(), CCError> {
    setting_handle
        .set_device_name_override(path.device_uid, request.name)
        .await
        .map_err(handle_error)
}

/// Sets or removes the user-defined display label for a channel.
/// The channel does not have to be live; only the device must be known.
pub async fn update_channel_overrides(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { setting_handle, .. }): State<AppState>,
    Json(request): Json<ChannelLabelOverrideRequest>,
) -> Result<(), CCError> {
    setting_handle
        .set_channel_label_override(path.device_uid, path.channel_name, request.label)
        .await
        .map_err(handle_error)
}

/// Retrieves the persisted UI Settings, if found.
pub async fn get_ui(
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<String, CCError> {
    setting_handle.get_ui().await.map_err(handle_error)
}

/// Persists the UI Settings, overriding anything previously saved
pub async fn update_ui(
    State(AppState { setting_handle, .. }): State<AppState>,
    ui_settings_request: String,
) -> Result<(), CCError> {
    setting_handle
        .update_ui(ui_settings_request)
        .await
        .map_err(handle_error)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoolerControlSettingsDto {
    apply_on_boot: Option<bool>,
    no_init: Option<bool>,
    startup_delay: Option<u16>,
    thinkpad_full_speed: Option<bool>,
    liquidctl_integration: Option<bool>,
    hide_duplicate_devices: Option<bool>,
    compress: Option<bool>,
    poll_rate: Option<f64>,
    drivetemp_suspend: Option<bool>,
    /// Custom origins to allow in CORS (for reverse proxy setups)
    origins: Option<Vec<String>>,
    /// Allow unencrypted HTTP connections from non-localhost addresses
    allow_unencrypted: Option<bool>,
    /// Header to check for proxy client protocol (e.g., "X-Forwarded-Proto")
    protocol_header: Option<String>,
    /// Whether to auto-detect Super-I/O sensors at startup (`x86_64` only)
    sensors_auto_detect: Option<bool>,
    /// Whether to listen for device add/remove events at startup
    device_listener_enabled: Option<bool>,
    /// Whether to apply labels and ignores from the lm-sensors configuration
    sensors_conf_enabled: Option<bool>,
    tls_strict: Option<bool>,
    /// The SHA-256 fingerprint of the TLS certificate this daemon serves, or `None` when
    /// TLS is off. Report-only: it is derived from the certificate, so anything sent here
    /// is ignored.
    tls_fingerprint: Option<String>,
}

impl CoolerControlSettingsDto {
    pub fn merge(&self, current: CoolerControlSettings) -> CoolerControlSettings {
        CoolerControlSettings {
            apply_on_boot: self.apply_on_boot.unwrap_or(current.apply_on_boot),
            no_init: self.no_init.unwrap_or(current.no_init),
            startup_delay: self
                .startup_delay
                .map_or(current.startup_delay, clamped_startup_delay),
            thinkpad_full_speed: self
                .thinkpad_full_speed
                .unwrap_or(current.thinkpad_full_speed),
            hide_duplicate_devices: self
                .hide_duplicate_devices
                .unwrap_or(current.hide_duplicate_devices),
            liquidctl_integration: self
                .liquidctl_integration
                .unwrap_or(current.liquidctl_integration),
            port: current.port,
            ipv4_address: current.ipv4_address,
            ipv6_address: current.ipv6_address,
            compress: self.compress.unwrap_or(current.compress),
            poll_rate: self.poll_rate.map_or(current.poll_rate, clamped_poll_rate),
            drivetemp_suspend: self.drivetemp_suspend.unwrap_or(current.drivetemp_suspend),
            tls_enabled: current.tls_enabled,
            tls_cert_path: current.tls_cert_path,
            tls_key_path: current.tls_key_path,
            origins: self.origins.clone().unwrap_or(current.origins),
            allow_unencrypted: self.allow_unencrypted.unwrap_or(current.allow_unencrypted),
            protocol_header: self
                .protocol_header
                .as_deref()
                .map_or(current.protocol_header, non_empty),
            sensors_auto_detect: self
                .sensors_auto_detect
                .unwrap_or(current.sensors_auto_detect),
            device_listener_enabled: self
                .device_listener_enabled
                .unwrap_or(current.device_listener_enabled),
            sensors_conf_enabled: self
                .sensors_conf_enabled
                .unwrap_or(current.sensors_conf_enabled),
            tls_strict: self.tls_strict.unwrap_or(current.tls_strict),
        }
    }
}

fn clamped_startup_delay(delay_seconds: u16) -> Duration {
    Duration::from_secs(u64::from(delay_seconds.clamp(0, STARTUP_DELAY_SECONDS_MAX)))
}

/// Clamps and rounds to the nearest half-second.
fn clamped_poll_rate(poll_rate: f64) -> f64 {
    (poll_rate.clamp(0.5, 5.0) * 2.).round() / 2.
}

/// An empty header name clears the setting.
fn non_empty(header: &str) -> Option<String> {
    if header.is_empty() {
        None
    } else {
        Some(header.to_string())
    }
}

impl From<CoolerControlSettings> for CoolerControlSettingsDto {
    #[allow(clippy::cast_possible_truncation)]
    fn from(settings: CoolerControlSettings) -> Self {
        Self {
            apply_on_boot: Some(settings.apply_on_boot),
            no_init: Some(settings.no_init),
            startup_delay: Some(settings.startup_delay.as_secs() as u16),
            thinkpad_full_speed: Some(settings.thinkpad_full_speed),
            hide_duplicate_devices: Some(settings.hide_duplicate_devices),
            liquidctl_integration: Some(settings.liquidctl_integration),
            compress: Some(settings.compress),
            poll_rate: Some(settings.poll_rate),
            drivetemp_suspend: Some(settings.drivetemp_suspend),
            origins: Some(settings.origins),
            allow_unencrypted: Some(settings.allow_unencrypted),
            protocol_header: settings.protocol_header,
            sensors_auto_detect: Some(settings.sensors_auto_detect),
            device_listener_enabled: Some(settings.device_listener_enabled),
            sensors_conf_enabled: Some(settings.sensors_conf_enabled),
            tls_strict: Some(settings.tls_strict),
            tls_fingerprint: crate::api::tls::served_fingerprint().map(str::to_string),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoolerControlDeviceSettingsDto {
    pub uid: UID,
    pub name: String,
    pub disable: bool,
    pub extensions: DeviceExtensions,
    pub channel_settings: HashMap<ChannelName, CCChannelSettings>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CoolerControlAllDeviceSettingsDto {
    devices: Vec<CoolerControlDeviceSettingsDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceNameOverrideRequest {
    /// The device display name. A null or absent value removes the override.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ChannelLabelOverrideRequest {
    /// The channel display label. A null or absent value removes the override.
    pub label: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    /// Builds a DTO with all fields set to None (partial update with no changes).
    fn empty_dto() -> CoolerControlSettingsDto {
        CoolerControlSettingsDto::all_none()
    }

    impl CoolerControlSettingsDto {
        /// Returns a DTO with every field set to None.
        fn all_none() -> Self {
            Self {
                apply_on_boot: None,
                no_init: None,
                startup_delay: None,
                thinkpad_full_speed: None,
                liquidctl_integration: None,
                hide_duplicate_devices: None,
                compress: None,
                poll_rate: None,
                drivetemp_suspend: None,
                origins: None,
                allow_unencrypted: None,
                protocol_header: None,
                sensors_auto_detect: None,
                device_listener_enabled: None,
                sensors_conf_enabled: None,
                tls_strict: None,
                tls_fingerprint: None,
            }
        }
    }

    #[test]
    fn merge_preserves_defaults_when_none() {
        // When no fields are set in the DTO, merge must preserve all current values.
        let current = CoolerControlSettings {
            sensors_auto_detect: true,
            device_listener_enabled: true,
            apply_on_boot: true,
            ..Default::default()
        };
        let dto = empty_dto();
        let merged = dto.merge(current);
        assert!(merged.sensors_auto_detect);
        assert!(merged.device_listener_enabled);
        assert!(merged.apply_on_boot);
    }

    #[test]
    fn merge_overrides_when_some() {
        // When DTO fields are Some, merge must use the DTO values.
        let current = CoolerControlSettings {
            sensors_auto_detect: true,
            device_listener_enabled: true,
            ..Default::default()
        };
        let mut dto = empty_dto();
        dto.sensors_auto_detect = Some(false);
        dto.device_listener_enabled = Some(false);
        let merged = dto.merge(current);
        assert!(merged.sensors_auto_detect.not());
        assert!(merged.device_listener_enabled.not());
    }

    #[test]
    fn merge_clamps_startup_delay_to_max() {
        // The API boundary must accept the full documented range and clamp above it.
        let mut dto = empty_dto();
        dto.startup_delay = Some(STARTUP_DELAY_SECONDS_MAX);
        let merged = dto.merge(CoolerControlSettings::default());
        assert_eq!(
            merged.startup_delay,
            Duration::from_secs(u64::from(STARTUP_DELAY_SECONDS_MAX))
        );

        dto.startup_delay = Some(STARTUP_DELAY_SECONDS_MAX + 1);
        let merged = dto.merge(CoolerControlSettings::default());
        assert_eq!(
            merged.startup_delay,
            Duration::from_secs(u64::from(STARTUP_DELAY_SECONDS_MAX))
        );
    }

    #[test]
    fn from_settings_includes_all_fields() {
        // The From conversion must include both new fields.
        let settings = CoolerControlSettings {
            sensors_auto_detect: true,
            device_listener_enabled: false,
            ..Default::default()
        };
        let dto = CoolerControlSettingsDto::from(settings);
        assert_eq!(dto.sensors_auto_detect, Some(true));
        assert_eq!(dto.device_listener_enabled, Some(false));
    }
}
