/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
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
 */

use std::ops::Deref;
use std::sync::Arc;

use actix_web::{get, HttpResponse, patch, Responder};
use actix_web::web::{Data, Json, Path};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{AllDevices, Device};
use crate::api::{handle_error, handle_simple_result};
use crate::config::Config;
use crate::device::{DeviceInfo, DeviceType, LcInfo, UID};
use crate::processors::SettingsProcessor;
use crate::setting::{LcdSettings, LightingSettings, Setting};

/// Returns a list of all detected devices and their associated information.
/// Does not return Status, that's for another more-fine-grained endpoint
#[get("/devices")]
async fn get_devices(all_devices: Data<AllDevices>) -> impl Responder {
    let mut all_devices_list = vec![];
    for device_lock in all_devices.values() {
        all_devices_list.push(device_lock.read().await.deref().into())
    }
    Json(DevicesResponse { devices: all_devices_list })
}

/// Returns all the currently applied settings for the given device.
/// It returns the Config Settings model, which includes all possibilities for each channel.
#[get("/devices/{device_uid}/settings")]
async fn get_device_settings(
    device_uid: Path<String>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.get_device_settings(device_uid.as_str()).await {
        Ok(settings) => HttpResponse::Ok().json(Json(SettingsResponse { settings })),
        Err(err) => handle_error(err)
    }
}

/// Apply the settings sent in the request body to the associated device.
/// Deprecated.
// #[deprecated(
// since = "0.18.0",
// note = "Use the new specific endpoints for applying device channel settings. Will be removed in a future release."
// )]
#[patch("/devices/{device_uid}/settings")]
async fn apply_device_settings(
    device_uid: Path<String>,
    settings_request: Json<Setting>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    if let Err(err) = settings_processor.set_config_setting(
        &device_uid.to_string(),
        settings_request.deref()).await {
        return handle_error(err);
    }
    config.set_device_setting(&device_uid.to_string(), settings_request.deref()).await;
    handle_simple_result(config.save_config_file().await)
}

#[patch("/devices/{device_uid}/settings/{channel_name}/manual")]
async fn apply_device_setting_manual(
    path_params: Path<(String, String)>,
    manual_request: Json<SettingManualRequest>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let (device_uid, channel_name) = path_params.into_inner();
    if let Err(err) = settings_processor.set_fixed_speed(
        &device_uid,
        channel_name.as_str(),
        manual_request.speed_fixed,
    ).await {
        return handle_error(err);
    }
    let config_settings = Setting {
        channel_name,
        speed_fixed: Some(manual_request.speed_fixed),
        ..Default::default()
    };
    config.set_device_setting(&device_uid, &config_settings).await;
    handle_simple_result(config.save_config_file().await)
}

#[patch("/devices/{device_uid}/settings/{channel_name}/profile")]
async fn apply_device_setting_profile(
    path_params: Path<(String, String)>,
    profile_uid_json: Json<SettingProfileUID>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let (device_uid, channel_name) = path_params.into_inner();
    if let Err(err) = settings_processor.set_profile(
        &device_uid,
        channel_name.as_str(),
        &profile_uid_json.profile_uid,
    ).await {
        return handle_error(err);
    }
    let config_setting = Setting {
        channel_name,
        profile_uid: Some(profile_uid_json.into_inner().profile_uid),
        ..Default::default()
    };
    config.set_device_setting(&device_uid, &config_setting).await;
    handle_simple_result(config.save_config_file().await)
}


#[patch("/devices/{device_uid}/settings/{channel_name}/lcd")]
async fn apply_device_setting_lcd(
    path_params: Path<(String, String)>,
    lcd_settings_json: Json<LcdSettings>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let (device_uid, channel_name) = path_params.into_inner();
    let lcd_settings = lcd_settings_json.into_inner();
    if let Err(err) = settings_processor.set_lcd(
        &device_uid,
        channel_name.as_str(),
        &lcd_settings,
    ).await {
        return handle_error(err);
    }
    let config_setting = Setting {
        channel_name,
        lcd: Some(lcd_settings),
        ..Default::default()
    };
    config.set_device_setting(&device_uid, &config_setting).await;
    handle_simple_result(config.save_config_file().await)
}


#[patch("/devices/{device_uid}/settings/{channel_name}/lighting")]
async fn apply_device_setting_lighting(
    path_params: Path<(String, String)>,
    lighting_settings_json: Json<LightingSettings>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let (device_uid, channel_name) = path_params.into_inner();
    let lighting_settings = lighting_settings_json.into_inner();
    if let Err(err) = settings_processor.set_lighting(
        &device_uid,
        channel_name.as_str(),
        &lighting_settings,
    ).await {
        return handle_error(err);
    }
    let config_setting = Setting {
        channel_name,
        lighting: Some(lighting_settings),
        ..Default::default()
    };
    config.set_device_setting(&device_uid, &config_setting).await;
    handle_simple_result(config.save_config_file().await)
}

#[patch("/devices/{device_uid}/settings/{channel_name}/pwm")]
async fn apply_device_setting_pwm(
    path_params: Path<(String, String)>,
    pwm_mode_json: Json<SettingPWMMode>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let (device_uid, channel_name) = path_params.into_inner();
    if let Err(err) = settings_processor.set_pwm_mode(
        &device_uid,
        channel_name.as_str(),
        pwm_mode_json.pwm_mode,
    ).await {
        return handle_error(err);
    }
    let config_setting = Setting {
        channel_name,
        pwm_mode: Some(pwm_mode_json.into_inner().pwm_mode),
        ..Default::default()
    };
    config.set_device_setting(&device_uid, &config_setting).await;
    handle_simple_result(config.save_config_file().await)
}


#[patch("/devices/{device_uid}/settings/{channel_name}/reset")]
async fn apply_device_setting_reset(
    path_params: Path<(String, String)>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let (device_uid, channel_name) = path_params.into_inner();
    if let Err(err) = settings_processor.set_reset(
        &device_uid,
        channel_name.as_str(),
    ).await {
        return handle_error(err);
    }
    let config_setting = Setting {
        channel_name,
        reset_to_default: Some(true),
        ..Default::default()
    };
    config.set_device_setting(&device_uid, &config_setting).await;
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
) -> impl Responder {
    config.set_legacy690_id(&device_uid.to_string(), &asetek690_request.is_legacy690).await;
    match config.save_config_file().await {
        Ok(_) => {
            // Device is now known. Legacy690Lc devices still require a restart of the daemon.
            if let Some(device) = all_devices.get(&device_uid.to_string()) {
                if device.read().await.lc_info.is_some() {
                    device.write().await.lc_info.as_mut().unwrap().unknown_asetek = false
                }
            }
            HttpResponse::Ok().json(json!({"success": true}))
        }
        Err(err) => handle_error(err)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceDto {
    pub name: String,
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    pub lc_info: Option<LcInfo>,
    pub info: Option<DeviceInfo>,
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
