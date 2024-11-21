/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use crate::api::auth::verify_admin_permissions;
use crate::api::{handle_error, AppState, CCError};
use crate::device::UID;
use crate::setting::{CoolerControlDeviceSettings, CoolerControlSettings};
use aide::NoApi;
use axum::extract::{Json, Path, State};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tower_sessions::Session;

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
    NoApi(session): NoApi<Session>,
    State(AppState { setting_handle, .. }): State<AppState>,
    Json(cc_settings_request): Json<CoolerControlSettingsDto>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
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
    Path(device_uid): Path<String>,
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<Json<CoolerControlDeviceSettingsDto>, CCError> {
    setting_handle
        .get_cc_device(device_uid)
        .await
        .map(Json)
        .map_err(handle_error)
}

/// Save `CoolerControl` settings that apply to a specific Device
pub async fn update_cc_device(
    Path(device_uid): Path<String>,
    NoApi(session): NoApi<Session>,
    State(AppState { setting_handle, .. }): State<AppState>,
    Json(cc_device_settings_request): Json<CoolerControlDeviceSettings>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    setting_handle
        .update_cc_device(device_uid, cc_device_settings_request)
        .await
        .map_err(handle_error)
}

/// Retrieves the persisted UI Settings, if found.
pub async fn get_ui(
    State(AppState { setting_handle, .. }): State<AppState>,
) -> Result<String, CCError> {
    setting_handle.get_ui().await.map_err(|err| {
        let error = err.root_cause().to_string();
        if error.contains("No such file") {
            CCError::NotFound { msg: error }
        } else {
            CCError::InternalError { msg: error }
        }
    })
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
    startup_delay: Option<u8>,
    thinkpad_full_speed: Option<bool>,
    liquidctl_integration: Option<bool>,
    hide_duplicate_devices: Option<bool>,
    compress: Option<bool>,
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
            Duration::from_secs(u64::from(delay.clamp(0, 10)))
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
        }
    }
}

impl From<CoolerControlSettings> for CoolerControlSettingsDto {
    fn from(settings: CoolerControlSettings) -> Self {
        Self {
            apply_on_boot: Some(settings.apply_on_boot),
            no_init: Some(settings.no_init),
            startup_delay: Some(settings.startup_delay.as_secs() as u8),
            thinkpad_full_speed: Some(settings.thinkpad_full_speed),
            hide_duplicate_devices: Some(settings.hide_duplicate_devices),
            liquidctl_integration: Some(settings.liquidctl_integration),
            compress: Some(settings.compress),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CoolerControlDeviceSettingsDto {
    pub uid: UID,
    pub name: String,
    pub disable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct CoolerControlAllDeviceSettingsDto {
    devices: Vec<CoolerControlDeviceSettingsDto>,
}
