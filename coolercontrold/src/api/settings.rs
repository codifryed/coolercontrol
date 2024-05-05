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

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use actix_web::web::{Data, Json, Path};
use actix_web::{get, patch, put, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::api::{handle_error, handle_simple_result, CCError};
use crate::config::Config;
use crate::device::UID;
use crate::setting::{CoolerControlDeviceSettings, CoolerControlSettings};
use crate::AllDevices;

/// Get General CoolerControl settings
#[get("/settings")]
async fn get_cc_settings(config: Data<Arc<Config>>) -> Result<impl Responder, CCError> {
    config
        .get_settings()
        .await
        .map(|settings| HttpResponse::Ok().json(Json(CoolerControlSettingsDto::from(&settings))))
        .map_err(handle_error)
}

/// Apply General CoolerControl settings
#[patch("/settings")]
async fn apply_cc_settings(
    cc_settings_request: Json<CoolerControlSettingsDto>,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    handle_simple_result(match config.get_settings().await {
        Ok(current_settings) => {
            let settings_to_set = cc_settings_request.merge(current_settings);
            config.set_settings(&settings_to_set).await;
            config.save_config_file().await
        }
        Err(err) => Err(err),
    })
}

/// Get All CoolerControl settings that apply to a specific Device
#[get("/settings/devices")]
async fn get_cc_settings_for_all_devices(
    config: Data<Arc<Config>>,
    all_devices: Data<AllDevices>,
) -> Result<impl Responder, CCError> {
    let settings_map = config
        .get_all_cc_devices_settings()
        .await
        .map_err(<anyhow::Error as Into<CCError>>::into)?;
    let mut devices_settings = HashMap::new();
    for (device_uid, device_lock) in all_devices.iter() {
        let name = device_lock.read().await.name.clone();
        // first fill with the default
        devices_settings.insert(
            device_uid.clone(),
            CoolerControlDeviceSettingsDto {
                uid: device_uid.to_string(),
                name,
                disable: false,
            },
        );
    }
    for (device_uid, setting_option) in settings_map {
        let setting = setting_option.ok_or_else(|| CCError::InternalError {
            msg: "CC Settings option should always be present in this situation".to_string(),
        })?;
        // override and fill with blacklisted devices:
        devices_settings.insert(
            device_uid.clone(),
            CoolerControlDeviceSettingsDto {
                uid: device_uid,
                name: setting.name,
                disable: setting.disable,
            },
        );
    }
    let cc_devices_settings = devices_settings
        .into_values()
        .collect::<Vec<CoolerControlDeviceSettingsDto>>();
    Ok(
        HttpResponse::Ok().json(Json(CoolerControlAllDeviceSettingsDto {
            devices: cc_devices_settings,
        })),
    )
}

/// Get CoolerControl settings that apply to a specific Device
#[get("/settings/devices/{device_uid}")]
async fn get_cc_settings_for_device(
    device_uid: Path<String>,
    config: Data<Arc<Config>>,
    all_devices: Data<AllDevices>,
) -> Result<impl Responder, CCError> {
    let settings_option = config
        .get_cc_settings_for_device(&device_uid)
        .await
        .map_err(<anyhow::Error as Into<CCError>>::into)?;
    match settings_option {
        Some(settings) => Ok(HttpResponse::Ok().json(Json(settings))),
        None => {
            let device_name = all_devices
                .get(device_uid.as_str())
                .ok_or_else(|| CCError::NotFound {
                    msg: "Device not found".to_string(),
                })?
                .read()
                .await
                .name
                .clone();
            Ok(
                HttpResponse::Ok().json(Json(CoolerControlDeviceSettingsDto {
                    uid: device_uid.clone(),
                    name: device_name,
                    disable: false,
                })),
            )
        }
    }
}

/// Save CoolerControl settings that apply to a specific Device
#[put("/settings/devices/{device_uid}")]
async fn save_cc_settings_for_device(
    device_uid: Path<String>,
    cc_device_settings_request: Json<CoolerControlDeviceSettings>,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    config
        .set_cc_settings_for_device(&device_uid, &cc_device_settings_request.into_inner())
        .await;
    config
        .save_config_file()
        .await
        .map(|()| HttpResponse::Ok().finish())
        .map_err(std::convert::Into::into)
}

/// Retrieves the persisted UI Settings, if found.
#[get("/settings/ui")]
async fn get_ui_settings(config: Data<Arc<Config>>) -> Result<impl Responder, CCError> {
    config
        .load_ui_config_file()
        .await
        .map(|settings| HttpResponse::Ok().body(settings))
        .map_err(|err| {
            let error = err.root_cause().to_string();
            if error.contains("No such file") {
                CCError::NotFound { msg: error }
            } else {
                CCError::InternalError { msg: error }
            }
        })
}

/// Persists the UI Settings, overriding anything previously saved
#[put("/settings/ui")]
async fn save_ui_settings(
    ui_settings_request: String,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    handle_simple_result(config.save_ui_config_file(&ui_settings_request).await)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoolerControlSettingsDto {
    apply_on_boot: Option<bool>,
    no_init: Option<bool>,
    startup_delay: Option<u8>,
    thinkpad_full_speed: Option<bool>,
}

impl CoolerControlSettingsDto {
    fn merge(&self, current_settings: CoolerControlSettings) -> CoolerControlSettings {
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
        CoolerControlSettings {
            apply_on_boot,
            no_init,
            startup_delay,
            thinkpad_full_speed,
            port: current_settings.port,
            ipv4_address: current_settings.ipv4_address,
            ipv6_address: current_settings.ipv6_address,
        }
    }
}

impl From<&CoolerControlSettings> for CoolerControlSettingsDto {
    fn from(settings: &CoolerControlSettings) -> Self {
        Self {
            apply_on_boot: Some(settings.apply_on_boot),
            no_init: Some(settings.no_init),
            startup_delay: Some(settings.startup_delay.as_secs() as u8),
            thinkpad_full_speed: Some(settings.thinkpad_full_speed),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoolerControlDeviceSettingsDto {
    uid: UID,
    name: String,
    disable: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CoolerControlAllDeviceSettingsDto {
    devices: Vec<CoolerControlDeviceSettingsDto>,
}
