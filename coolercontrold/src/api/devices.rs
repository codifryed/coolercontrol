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
use crate::device::{ChannelName, DeviceInfo, DeviceType, DeviceUID, LcInfo, UID};
use crate::processing::processors::image;
use crate::setting::{LcdSettings, LightingSettings, Setting};
use crate::Device;
use aide::axum::IntoApiResponse;
use aide::NoApi;
use axum::extract::{Path, State};
use axum::http::header;
use axum::Json;
use axum_typed_multipart::{FieldData, TryFromMultipart, TypedMultipart};
use mime::Mime;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::ops::Not;
use std::str::FromStr;
use tempfile::NamedTempFile;
use tower_sessions::Session;

/// Returns a list of all detected devices and their associated information.
/// Does not return Status, that's for another more-fine-grained endpoint
pub async fn get(
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<Json<DevicesResponse>, CCError> {
    let all_devices = device_handle.devices_get().await?;
    Ok(Json(DevicesResponse {
        devices: all_devices,
    }))
}

/// Returns all the currently applied settings for the given device.
/// It returns the Config Settings model, which includes all possibilities for each channel.
pub async fn device_settings_get(
    Path(path): Path<DevicePath>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<Json<SettingsResponse>, CCError> {
    device_handle
        .device_settings_get(path.device_uid)
        .await
        .map(|settings| Json(SettingsResponse { settings }))
        .map_err(handle_error)
}

pub async fn device_setting_manual_modify(
    Path(path): Path<DeviceChannelPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(manual_request): Json<SettingManualRequest>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .device_setting_manual(
            path.device_uid,
            path.channel_name,
            manual_request.speed_fixed,
        )
        .await
        .map_err(handle_error)
}

pub async fn device_setting_profile_modify(
    Path(path): Path<DeviceChannelPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(profile_uid_json): Json<SettingProfileUID>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .device_setting_profile(
            path.device_uid,
            path.channel_name,
            profile_uid_json.profile_uid,
        )
        .await
        .map_err(handle_error)
}

pub async fn device_setting_lcd_modify(
    Path(path): Path<DeviceChannelPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(lcd_settings): Json<LcdSettings>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .device_setting_lcd(path.device_uid, path.channel_name, lcd_settings)
        .await
        .map_err(handle_error)
}

/// To retrieve the currently applied image
pub async fn get_device_lcd_image(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<impl IntoApiResponse, CCError> {
    let (content_type, image_data) = device_handle
        .device_image_get(path.device_uid, path.channel_name)
        .await?;
    Ok((
        [(header::CONTENT_TYPE, content_type.to_string())],
        image_data,
    ))
}

/// Used to apply LCD settings that contain images.
pub async fn update_device_setting_lcd_image(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
    NoApi(session): NoApi<Session>,
    NoApi(mut form): NoApi<TypedMultipart<LcdImageSettingsForm>>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    let file_data = validate_form_images(&mut form)?;
    device_handle
        .device_image_update(
            path.device_uid,
            path.channel_name,
            form.mode.clone(),
            form.brightness,
            form.orientation,
            file_data,
        )
        .await
        .map_err(handle_error)
}

/// Used to process image files for previewing
pub async fn process_device_lcd_images(
    Path(path): Path<DeviceChannelPath>,
    State(AppState { device_handle, .. }): State<AppState>,
    NoApi(session): NoApi<Session>,
    NoApi(mut form): NoApi<TypedMultipart<LcdImageSettingsForm>>,
) -> Result<impl IntoApiResponse, CCError> {
    verify_admin_permissions(&session).await?;
    let file_data = validate_form_images(&mut form)?;
    device_handle
        .device_image_process(path.device_uid, path.channel_name, file_data)
        .await
        .map(|(content_type, file_data)| {
            (
                [(header::CONTENT_TYPE, content_type.to_string())],
                file_data,
            )
        })
        .map_err(handle_error)
}

fn validate_form_images(
    form: &mut TypedMultipart<LcdImageSettingsForm>,
) -> Result<Vec<(Mime, Vec<u8>)>, CCError> {
    if form.images.is_empty() {
        return Err(CCError::UserError {
            msg: "At least one image is required".to_string(),
        });
    } else if form.images.len() > 1 {
        return Err(CCError::UserError {
            msg: "Only one image is supported at this time".to_string(),
        });
    }
    let mut file_data = Vec::new();
    for file in form.images.as_mut_slice() {
        let mut file_bytes = Vec::new();
        file.contents.read_to_end(&mut file_bytes)?;
        let content_type = file
            .metadata
            .content_type
            .as_ref()
            .and_then(|ct| Mime::from_str(ct.as_str()).ok())
            .unwrap_or(mime::IMAGE_PNG);
        if image::supported_image_types().contains(&content_type).not() {
            return Err(CCError::UserError {
                msg: format!(
                    "Only image types {:?} are supported. Found:{content_type}",
                    image::supported_image_types()
                ),
            });
        }
        file_data.push((content_type, file_bytes));
    }
    Ok(file_data)
}

pub async fn device_setting_lighting_modify(
    Path(path): Path<DeviceChannelPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(lighting_settings): Json<LightingSettings>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .device_setting_lighting(path.device_uid, path.channel_name, lighting_settings)
        .await
        .map_err(handle_error)
}

pub async fn device_setting_pwm_mode_modify(
    Path(path): Path<DeviceChannelPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(pwm_mode_json): Json<SettingPWMMode>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .device_setting_pwm_mode(path.device_uid, path.channel_name, pwm_mode_json.pwm_mode)
        .await
        .map_err(handle_error)
}

pub async fn device_setting_reset(
    Path(path): Path<DeviceChannelPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .device_setting_reset(path.device_uid, path.channel_name)
        .await
        .map_err(handle_error)
}

/// Set `AseTek` Cooler driver type
/// This is needed to set `Legacy690Lc` or `Modern690Lc` device driver type
pub async fn asetek_type_update(
    Path(path): Path<DevicePath>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(asetek690_request): Json<AseTek690Request>,
) -> Result<(), CCError> {
    device_handle
        .device_asetek_type(path.device_uid, asetek690_request.is_legacy690)
        .await
        .map_err(handle_error)
}

pub async fn thinkpad_fan_control_modify(
    NoApi(session): NoApi<Session>,
    State(AppState { device_handle, .. }): State<AppState>,
    Json(fan_control_request): Json<ThinkPadFanControlRequest>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    device_handle
        .thinkpad_fan_control(fan_control_request.enable)
        .await
        .map_err(handle_error)
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceDto {
    pub name: String,
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    pub lc_info: Option<LcInfo>,
    pub info: DeviceInfo,
}

impl From<&Device> for DeviceDto {
    fn from(device: &Device) -> Self {
        Self {
            name: device.name.clone(),
            d_type: device.d_type.clone(),
            type_index: device.type_index,
            uid: device.uid.clone(),
            lc_info: device.lc_info.clone(),
            info: device.info.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevicesResponse {
    devices: Vec<DeviceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingsResponse {
    settings: Vec<Setting>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AseTek690Request {
    is_legacy690: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingManualRequest {
    speed_fixed: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingProfileUID {
    profile_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SettingPWMMode {
    pwm_mode: u8,
}

#[derive(Debug, TryFromMultipart)]
pub struct LcdImageSettingsForm {
    mode: String,
    brightness: Option<u8>,
    orientation: Option<u16>,
    // limited to the request body size limit
    #[form_data(field_name = "images[]", limit = "unlimited")]
    images: Vec<FieldData<NamedTempFile>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThinkPadFanControlRequest {
    enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DevicePath {
    pub device_uid: DeviceUID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceChannelPath {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
}
