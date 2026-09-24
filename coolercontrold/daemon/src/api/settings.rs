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
    /// Custom parent sources that may embed the CoolerControl UI
    frame_ancestors: Option<Vec<String>>,
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
    /// Whether to log at DEBUG level. Applies after a daemon restart.
    debug_logging: Option<bool>,
}

impl CoolerControlSettingsDto {
    pub fn merge(&self, current_settings: CoolerControlSettings) -> CoolerControlSettings {
        let apply_on_boot = if let Some(apply) = self.apply_on_boot {
            apply
        } else {
            current_settings.apply_on_boot
        };
        let no_init = if let Some(init) = self.no_init {
            init
        } else {
            current_settings.no_init
        };
        let startup_delay = if let Some(delay) = self.startup_delay {
            Duration::from_secs(u64::from(delay.clamp(0, STARTUP_DELAY_SECONDS_MAX)))
        } else {
            current_settings.startup_delay
        };
        let thinkpad_full_speed = if let Some(full_speed) = self.thinkpad_full_speed {
            full_speed
        } else {
            current_settings.thinkpad_full_speed
        };
        let hide_duplicate_devices = if let Some(hide) = self.hide_duplicate_devices {
            hide
        } else {
            current_settings.hide_duplicate_devices
        };
        let liquidctl_integration = if let Some(integrate) = self.liquidctl_integration {
            integrate
        } else {
            current_settings.liquidctl_integration
        };
        let compress = if let Some(compress) = self.compress {
            compress
        } else {
            current_settings.compress
        };
        let poll_rate = if let Some(poll_rate) = self.poll_rate {
            // clamps and rounds to the nearest half-second.
            (poll_rate.clamp(0.5, 5.0) * 2.).round() / 2.
        } else {
            current_settings.poll_rate
        };
        let drivetemp_suspend = if let Some(d_suspend) = self.drivetemp_suspend {
            d_suspend
        } else {
            current_settings.drivetemp_suspend
        };
        let origins = if let Some(ref origins) = self.origins {
            origins.clone()
        } else {
            current_settings.origins
        };
        let frame_ancestors = if let Some(ref frame_ancestors) = self.frame_ancestors {
            frame_ancestors.clone()
        } else {
            current_settings.frame_ancestors
        };
        let allow_unencrypted = if let Some(allow) = self.allow_unencrypted {
            allow
        } else {
            current_settings.allow_unencrypted
        };
        let protocol_header = if let Some(ref header) = self.protocol_header {
            if header.is_empty() {
                None
            } else {
                Some(header.clone())
            }
        } else {
            current_settings.protocol_header
        };
        let sensors_auto_detect = self
            .sensors_auto_detect
            .unwrap_or(current_settings.sensors_auto_detect);
        let device_listener_enabled = self
            .device_listener_enabled
            .unwrap_or(current_settings.device_listener_enabled);
        let sensors_conf_enabled = self
            .sensors_conf_enabled
            .unwrap_or(current_settings.sensors_conf_enabled);
        let debug_logging = self.debug_logging.unwrap_or(current_settings.debug_logging);
        CoolerControlSettings {
            apply_on_boot,
            no_init,
            startup_delay,
            thinkpad_full_speed,
            hide_duplicate_devices,
            liquidctl_integration,
            port: current_settings.port,
            ipv4_address: current_settings.ipv4_address,
            ipv6_address: current_settings.ipv6_address,
            compress,
            poll_rate,
            drivetemp_suspend,
            tls_enabled: current_settings.tls_enabled,
            tls_cert_path: current_settings.tls_cert_path,
            tls_key_path: current_settings.tls_key_path,
            origins,
            frame_ancestors,
            allow_unencrypted,
            protocol_header,
            sensors_auto_detect,
            device_listener_enabled,
            sensors_conf_enabled,
            debug_logging,
        }
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
            frame_ancestors: Some(settings.frame_ancestors),
            allow_unencrypted: Some(settings.allow_unencrypted),
            protocol_header: settings.protocol_header,
            sensors_auto_detect: Some(settings.sensors_auto_detect),
            device_listener_enabled: Some(settings.device_listener_enabled),
            sensors_conf_enabled: Some(settings.sensors_conf_enabled),
            debug_logging: Some(settings.debug_logging),
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
                frame_ancestors: None,
                allow_unencrypted: None,
                protocol_header: None,
                sensors_auto_detect: None,
                device_listener_enabled: None,
                sensors_conf_enabled: None,
                debug_logging: None,
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

    // Goal: a PATCH that omits debug_logging keeps the saved value in both states, and one
    // that sets it replaces it, so saving an unrelated setting never flips debug logging.
    #[test]
    fn merge_debug_logging_only_when_set() {
        for saved in [true, false] {
            let current = CoolerControlSettings {
                debug_logging: saved,
                ..Default::default()
            };
            assert_eq!(empty_dto().merge(current.clone()).debug_logging, saved);
            let mut dto = empty_dto();
            dto.debug_logging = Some(saved.not());
            assert_eq!(dto.merge(current.clone()).debug_logging, saved.not());
            assert_eq!(
                CoolerControlSettingsDto::from(current).debug_logging,
                Some(saved)
            );
        }
    }
}
