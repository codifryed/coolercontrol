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

use std::io::Read;
use std::ops::{Deref, Not};
use std::sync::Arc;

use actix_multipart::form::text::Text;
use actix_multipart::form::{tempfile::TempFile, MultipartForm};
use actix_session::Session;
use actix_web::web::{Data, Json, Path};
use actix_web::{get, patch, post, put, HttpResponse, Responder};
use mime::Mime;
use serde::{Deserialize, Serialize};

use crate::api::{handle_error, handle_simple_result, verify_admin_permissions, CCError};
use crate::config::Config;
use crate::device::{DeviceInfo, DeviceType, LcInfo, UID};
use crate::processing::processors::image;
use crate::processing::settings::SettingsController;
use crate::setting::{LcdSettings, LightingSettings, Setting};
use crate::{AllDevices, Device};

/// Returns a list of all detected devices and their associated information.
/// Does not return Status, that's for another more-fine-grained endpoint
#[get("/devices")]
async fn get_devices(all_devices: Data<AllDevices>) -> impl Responder {
    let mut all_devices_list = vec![];
    for device_lock in all_devices.values() {
        all_devices_list.push(device_lock.read().await.deref().into());
    }
    Json(DevicesResponse {
        devices: all_devices_list,
    })
}

/// Returns all the currently applied settings for the given device.
/// It returns the Config Settings model, which includes all possibilities for each channel.
#[get("/devices/{device_uid}/settings")]
async fn get_device_settings(
    device_uid: Path<String>,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    config
        .get_device_settings(device_uid.as_str())
        .await
        .map(|settings| HttpResponse::Ok().json(Json(SettingsResponse { settings })))
        .map_err(handle_error)
}

#[put("/devices/{device_uid}/settings/{channel_name}/manual")]
async fn apply_device_setting_manual(
    path_params: Path<(String, String)>,
    manual_request: Json<SettingManualRequest>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    settings_controller
        .set_fixed_speed(
            &device_uid,
            channel_name.as_str(),
            manual_request.speed_fixed,
        )
        .await
        .map_err(handle_error)?;
    let config_settings = Setting {
        channel_name,
        speed_fixed: Some(manual_request.speed_fixed),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_settings)
        .await;
    handle_simple_result(config.save_config_file().await)
}

#[put("/devices/{device_uid}/settings/{channel_name}/profile")]
async fn apply_device_setting_profile(
    path_params: Path<(String, String)>,
    profile_uid_json: Json<SettingProfileUID>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    settings_controller
        .set_profile(
            &device_uid,
            channel_name.as_str(),
            &profile_uid_json.profile_uid,
        )
        .await
        .map_err(handle_error)?;
    let config_setting = Setting {
        channel_name,
        profile_uid: Some(profile_uid_json.into_inner().profile_uid),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_setting)
        .await;
    handle_simple_result(config.save_config_file().await)
}

#[put("/devices/{device_uid}/settings/{channel_name}/lcd")]
async fn apply_device_setting_lcd(
    path_params: Path<(String, String)>,
    lcd_settings_json: Json<LcdSettings>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    let lcd_settings = lcd_settings_json.into_inner();
    settings_controller
        .set_lcd(&device_uid, channel_name.as_str(), &lcd_settings)
        .await
        .map_err(handle_error)?;
    let config_setting = Setting {
        channel_name,
        lcd: Some(lcd_settings),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_setting)
        .await;
    handle_simple_result(config.save_config_file().await)
}

/// To retrieve the currently applied image
#[get("/devices/{device_uid}/settings/{channel_name}/lcd/images")]
async fn get_device_lcd_images(
    path_params: Path<(String, String)>,
    settings_controller: Data<Arc<SettingsController>>,
) -> Result<impl Responder, CCError> {
    let (device_uid, channel_name) = path_params.into_inner();
    let (content_type, image_data) = settings_controller
        .get_lcd_image(&device_uid, &channel_name)
        .await?;
    Ok(HttpResponse::Ok()
        .content_type(content_type)
        .body(image_data))
}

/// Used to apply LCD settings that contain images.
#[put("/devices/{device_uid}/settings/{channel_name}/lcd/images")]
async fn apply_device_setting_lcd_images(
    path_params: Path<(String, String)>,
    MultipartForm(mut form): MultipartForm<LcdImageSettingsForm>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    let mut file_data = validate_form_images(&mut form)?;
    let processed_image_data = settings_controller
        .process_lcd_images(&device_uid, &channel_name, &mut file_data)
        .await
        .map_err(<anyhow::Error as Into<CCError>>::into)?;
    let image_path = settings_controller
        .save_lcd_image(&processed_image_data.0, processed_image_data.1)
        .await?;
    let lcd_settings = LcdSettings {
        mode: form.mode.into_inner(),
        brightness: form.brightness.map(Text::into_inner),
        orientation: form.orientation.map(Text::into_inner),
        image_file_processed: Some(image_path),
        temp_source: None,
        colors: Vec::with_capacity(0),
    };
    settings_controller
        .set_lcd(&device_uid, channel_name.as_str(), &lcd_settings)
        .await
        .map_err(<anyhow::Error as Into<CCError>>::into)?;
    let config_setting = Setting {
        channel_name,
        lcd: Some(lcd_settings),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_setting)
        .await;
    handle_simple_result(config.save_config_file().await)
}

/// Used to process image files for previewing
#[post("/devices/{device_uid}/settings/{channel_name}/lcd/images")]
async fn process_device_lcd_images(
    path_params: Path<(String, String)>,
    MultipartForm(mut form): MultipartForm<LcdImageSettingsForm>,
    settings_controller: Data<Arc<SettingsController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    let mut file_data = validate_form_images(&mut form)?;
    settings_controller
        .process_lcd_images(&device_uid, &channel_name, &mut file_data)
        .await
        .map(|(content_type, file_data)| {
            HttpResponse::Ok()
                .content_type(content_type)
                .body(file_data)
        })
        .map_err(handle_error)
}

fn validate_form_images(form: &mut LcdImageSettingsForm) -> Result<Vec<(&Mime, Vec<u8>)>, CCError> {
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
        if file.size > 50_000_000 {
            return Err(CCError::UserError {
                msg: format!(
                    "No single file can be bigger than 50MB. Found: {}MB",
                    file.size / 1_000_000
                ),
            });
        }
        let mut file_bytes = Vec::new();
        file.file.read_to_end(&mut file_bytes)?;
        let content_type = file.content_type.as_ref().unwrap_or(&mime::IMAGE_PNG);
        if image::supported_image_types().contains(content_type).not() {
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

#[put("/devices/{device_uid}/settings/{channel_name}/lighting")]
async fn apply_device_setting_lighting(
    path_params: Path<(String, String)>,
    lighting_settings_json: Json<LightingSettings>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    let lighting_settings = lighting_settings_json.into_inner();
    settings_controller
        .set_lighting(&device_uid, channel_name.as_str(), &lighting_settings)
        .await
        .map_err(handle_error)?;
    let config_setting = Setting {
        channel_name,
        lighting: Some(lighting_settings),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_setting)
        .await;
    handle_simple_result(config.save_config_file().await)
}

#[put("/devices/{device_uid}/settings/{channel_name}/pwm")]
async fn apply_device_setting_pwm(
    path_params: Path<(String, String)>,
    pwm_mode_json: Json<SettingPWMMode>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    settings_controller
        .set_pwm_mode(&device_uid, channel_name.as_str(), pwm_mode_json.pwm_mode)
        .await
        .map_err(handle_error)?;
    let config_setting = Setting {
        channel_name,
        pwm_mode: Some(pwm_mode_json.into_inner().pwm_mode),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_setting)
        .await;
    handle_simple_result(config.save_config_file().await)
}

#[put("/devices/{device_uid}/settings/{channel_name}/reset")]
async fn apply_device_setting_reset(
    path_params: Path<(String, String)>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let (device_uid, channel_name) = path_params.into_inner();
    settings_controller
        .set_reset(&device_uid, channel_name.as_str())
        .await
        .map_err(handle_error)?;
    let config_setting = Setting {
        channel_name,
        reset_to_default: Some(true),
        ..Default::default()
    };
    config
        .set_device_setting(&device_uid, &config_setting)
        .await;
    handle_simple_result(config.save_config_file().await)
}

/// Set AseTek Cooler driver type
/// This is needed to set Legacy690Lc or Modern690Lc device driver type
#[patch("/devices/{device_id}/asetek690")]
async fn asetek(
    device_uid: Path<String>,
    asetek690_request: Json<AseTek690Request>,
    config: Data<Arc<Config>>,
    all_devices: Data<AllDevices>,
) -> Result<impl Responder, CCError> {
    config
        .set_legacy690_id(&device_uid.to_string(), &asetek690_request.is_legacy690)
        .await;
    config.save_config_file().await.map_err(handle_error)?;
    // Device is now known. Legacy690Lc devices still require a restart of the daemon.
    if let Some(device) = all_devices.get(&device_uid.to_string()) {
        if device.read().await.lc_info.is_some() {
            device
                .write()
                .await
                .lc_info
                .as_mut()
                .unwrap()
                .unknown_asetek = false;
        }
    }
    Ok(HttpResponse::Ok().finish())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceDto {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicesResponse {
    devices: Vec<DeviceDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingsResponse {
    settings: Vec<Setting>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AseTek690Request {
    is_legacy690: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingManualRequest {
    speed_fixed: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingProfileUID {
    profile_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SettingPWMMode {
    pwm_mode: u8,
}

#[derive(Debug, MultipartForm)]
struct LcdImageSettingsForm {
    mode: Text<String>,
    brightness: Option<Text<u8>>,
    orientation: Option<Text<u16>>,
    #[multipart(rename = "images[]", limit = "50 MiB")]
    images: Vec<TempFile>,
}
