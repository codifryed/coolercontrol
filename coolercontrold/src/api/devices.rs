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
use aide::NoApi;
use axum::extract::{Path, State};
use axum_jsonschema::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::ops::{Deref, Not};
use tower_sessions::Session;

/// Returns a list of all detected devices and their associated information.
/// Does not return Status, that's for another more-fine-grained endpoint
pub async fn devices_get(
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

// todo:
// /// To retrieve the currently applied image
// #[get("/devices/{device_uid}/settings/{channel_name}/lcd/images")]
// async fn get_device_lcd_images(
//     path_params: Path<(String, String)>,
//     settings_controller: Data<Arc<SettingsController>>,
// ) -> Result<impl Responder, CCError> {
//     let (device_uid, channel_name) = path_params.into_inner();
//     let (content_type, image_data) = settings_controller
//         .get_lcd_image(&device_uid, &channel_name)
//         .await?;
//     Ok(HttpResponse::Ok()
//         .content_type(content_type)
//         .body(image_data))
// }
//
// /// Used to apply LCD settings that contain images.
// #[put("/devices/{device_uid}/settings/{channel_name}/lcd/images")]
// async fn apply_device_setting_lcd_images(
//     path_params: Path<(String, String)>,
//     MultipartForm(mut form): MultipartForm<LcdImageSettingsForm>,
//     settings_controller: Data<Arc<SettingsController>>,
//     config: Data<Arc<Config>>,
//     session: Session,
// ) -> Result<impl Responder, CCError> {
//     verify_admin_permissions(&session).await?;
//     let (device_uid, channel_name) = path_params.into_inner();
//     let mut file_data = validate_form_images(&mut form)?;
//     let processed_image_data = settings_controller
//         .process_lcd_images(&device_uid, &channel_name, &mut file_data)
//         .await
//         .map_err(<anyhow::Error as Into<CCError>>::into)?;
//     let image_path = settings_controller
//         .save_lcd_image(&processed_image_data.0, processed_image_data.1)
//         .await?;
//     let lcd_settings = LcdSettings {
//         mode: form.mode.into_inner(),
//         brightness: form.brightness.map(Text::into_inner),
//         orientation: form.orientation.map(Text::into_inner),
//         image_file_processed: Some(image_path),
//         temp_source: None,
//         colors: Vec::with_capacity(0),
//     };
//     settings_controller
//         .set_lcd(&device_uid, channel_name.as_str(), &lcd_settings)
//         .await
//         .map_err(<anyhow::Error as Into<CCError>>::into)?;
//     let config_setting = Setting {
//         channel_name,
//         lcd: Some(lcd_settings),
//         ..Default::default()
//     };
//     config
//         .set_device_setting(&device_uid, &config_setting)
//         .await;
//     handle_simple_result(config.save_config_file().await)
// }
//
// /// Used to process image files for previewing
// #[post("/devices/{device_uid}/settings/{channel_name}/lcd/images")]
// async fn process_device_lcd_images(
//     path_params: Path<(String, String)>,
//     MultipartForm(mut form): MultipartForm<LcdImageSettingsForm>,
//     settings_controller: Data<Arc<SettingsController>>,
//     session: Session,
// ) -> Result<impl Responder, CCError> {
//     verify_admin_permissions(&session).await?;
//     let (device_uid, channel_name) = path_params.into_inner();
//     let mut file_data = validate_form_images(&mut form)?;
//     settings_controller
//         .process_lcd_images(&device_uid, &channel_name, &mut file_data)
//         .await
//         .map(|(content_type, file_data)| {
//             HttpResponse::Ok()
//                 .content_type(content_type)
//                 .body(file_data)
//         })
//         .map_err(handle_error)
// }
//
// fn validate_form_images(form: &mut LcdImageSettingsForm) -> Result<Vec<(&Mime, Vec<u8>)>, CCError> {
//     if form.images.is_empty() {
//         return Err(CCError::UserError {
//             msg: "At least one image is required".to_string(),
//         });
//     } else if form.images.len() > 1 {
//         return Err(CCError::UserError {
//             msg: "Only one image is supported at this time".to_string(),
//         });
//     }
//     let mut file_data = Vec::new();
//     for file in form.images.as_mut_slice() {
//         if file.size > 50_000_000 {
//             return Err(CCError::UserError {
//                 msg: format!(
//                     "No single file can be bigger than 50MB. Found: {}MB",
//                     file.size / 1_000_000
//                 ),
//             });
//         }
//         let mut file_bytes = Vec::new();
//         file.file.read_to_end(&mut file_bytes)?;
//         let content_type = file.content_type.as_ref().unwrap_or(&mime::IMAGE_PNG);
//         if image::supported_image_types().contains(content_type).not() {
//             return Err(CCError::UserError {
//                 msg: format!(
//                     "Only image types {:?} are supported. Found:{content_type}",
//                     image::supported_image_types()
//                 ),
//             });
//         }
//         file_data.push((content_type, file_bytes));
//     }
//     Ok(file_data)
// }

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

// todo:
// #[derive(Debug, MultipartForm)]
// struct LcdImageSettingsForm {
//     mode: Text<String>,
//     brightness: Option<Text<u8>>,
//     orientation: Option<Text<u16>>,
//     #[multipart(rename = "images[]", limit = "50 MiB")]
//     images: Vec<TempFile>,
// }

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
