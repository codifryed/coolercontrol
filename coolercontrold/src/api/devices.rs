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
use crate::setting::Setting;
use crate::settings_processor::SettingsProcessor;

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

/// Returns the currently applied settings for the given device
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

/// Apply the settings sent in the request body to the associated device
#[patch("/devices/{device_uid}/settings")]
async fn apply_device_settings(
    device_uid: Path<String>,
    settings_request: Json<Setting>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    if let Err(err) = settings_processor.set_setting(
        &device_uid.to_string(),
        settings_request.deref()).await {
        return handle_error(err);
    }
    config.set_device_setting(&device_uid.to_string(), settings_request.deref()).await;
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
