/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
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
 ******************************************************************************/

use std::sync::Arc;
use std::time::Duration;

use actix_cors::Cors;
use actix_web::{App, get, HttpResponse, HttpServer, middleware, patch, post, Responder};
use actix_web::dev::Server;
use actix_web::middleware::{Compat, Condition};
use actix_web::web::{Data, Json};
use anyhow::Result;
use log::{error, LevelFilter};
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AllDevices;
use crate::config::Config;
use crate::setting::{CoolerControlSettings, Function, Profile};
use crate::settings_processor::SettingsProcessor;

mod devices;
mod status;

const GUI_SERVER_PORT: u16 = 11987;
const GUI_SERVER_ADDR: &str = "127.0.0.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

/// Returns a simple handshake to verify established connection
#[get("/handshake")]
async fn handshake() -> impl Responder {
    Json(json!({"shake": true}))
}

#[post("/shutdown")]
async fn shutdown() -> impl Responder {
    signal::kill(Pid::this(), Signal::SIGQUIT).unwrap();
    Json(json!({"shutdown": true}))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThinkPadFanControlRequest {
    enable: bool,
}

#[post("/thinkpad_fan_control")]
async fn thinkpad_fan_control(
    fan_control_request: Json<ThinkPadFanControlRequest>,
    settings_processor: Data<Arc<SettingsProcessor>>,
) -> impl Responder {
    match settings_processor.thinkpad_fan_control(&fan_control_request.enable).await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoolerControlSettingsDto {
    apply_on_boot: Option<bool>,
    handle_dynamic_temps: Option<bool>,
    startup_delay: Option<u8>,
    smoothing_level: Option<u8>,
    thinkpad_full_speed: Option<bool>,
}

impl CoolerControlSettingsDto {
    fn merge(&self, current_settings: CoolerControlSettings) -> CoolerControlSettings {
        let apply_on_boot = if let Some(apply) = self.apply_on_boot {
            apply
        } else {
            current_settings.apply_on_boot
        };
        let handle_dynamic_temps = if let Some(should_handle) = self.handle_dynamic_temps {
            should_handle
        } else {
            current_settings.handle_dynamic_temps
        };
        let startup_delay = if let Some(delay) = self.startup_delay {
            Duration::from_secs(delay.max(0).min(10) as u64)
        } else {
            current_settings.startup_delay
        };
        let smoothing_level = if let Some(level) = self.smoothing_level {
            level
        } else {
            current_settings.smoothing_level
        };
        let thinkpad_full_speed = if let Some(full_speed) = self.thinkpad_full_speed {
            full_speed
        } else {
            current_settings.thinkpad_full_speed
        };
        CoolerControlSettings {
            apply_on_boot,
            no_init: current_settings.no_init,
            handle_dynamic_temps,
            startup_delay,
            smoothing_level,
            thinkpad_full_speed,
        }
    }
}

impl From<&CoolerControlSettings> for CoolerControlSettingsDto {
    fn from(settings: &CoolerControlSettings) -> Self {
        Self {
            apply_on_boot: Some(settings.apply_on_boot),
            handle_dynamic_temps: Some(settings.handle_dynamic_temps),
            startup_delay: Some(settings.startup_delay.as_secs() as u8),
            smoothing_level: Some(settings.smoothing_level),
            thinkpad_full_speed: Some(settings.thinkpad_full_speed),
        }
    }
}

/// Get CoolerControl settings
#[get("/settings")]
async fn get_cc_settings(
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.get_settings().await {
        Ok(settings) => HttpResponse::Ok()
            .json(Json(CoolerControlSettingsDto::from(&settings))),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Apply CoolerControl settings
#[patch("/settings")]
async fn apply_cc_settings(
    cc_settings_request: Json<CoolerControlSettingsDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let result = match config.get_settings().await {
        Ok(current_settings) => {
            let settings_to_set = cc_settings_request.merge(current_settings);
            config.set_settings(&settings_to_set).await;
            config.save_config_file().await
        }
        Err(err) => Err(err)
    };
    match result {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesDto {
    profiles: Vec<Profile>,
}

/// Retrieves the persisted Profile list
#[get("/profiles")]
async fn get_profiles(
    config: Data<Arc<Config>>
) -> impl Responder {
    match config.get_profiles().await {
        Ok(profiles) => HttpResponse::Ok().json(Json(ProfilesDto { profiles })),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Set the given profiles, overwriting any existing
#[post("/profiles")]
async fn save_profiles(
    profiles_dto: Json<ProfilesDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    config.set_profiles(&profiles_dto.profiles).await;
    match config.save_config_file().await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionsDto {
    functions: Vec<Function>,
}

/// Retrieves the persisted Function list
#[get("/functions")]
async fn get_functions(
    config: Data<Arc<Config>>
) -> impl Responder {
    match config.get_functions().await {
        Ok(functions) => HttpResponse::Ok().json(Json(FunctionsDto { functions })),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Set the given functions, overwriting any existing
#[post("/functions")]
async fn save_functions(
    functions_dto: Json<FunctionsDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    config.set_functions(&functions_dto.functions).await;
    match config.save_config_file().await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Retrieves the persisted UI Settings, if found.
#[get("/settings/ui")]
async fn get_ui_settings(
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.load_ui_config_file().await {
        Ok(settings) => HttpResponse::Ok().body(settings),
        Err(err) => {
            error!("{:?}", err);
            let error = err.root_cause().to_string();
            if error.contains("No such file") {
                HttpResponse::NotFound()
                    .json(Json(ErrorResponse { error }))
            } else {
                HttpResponse::InternalServerError()
                    .json(Json(ErrorResponse { error }))
            }
        }
    }
}

/// Persists the UI Settings.
#[post("/settings/ui")]
async fn save_ui_settings(
    ui_settings_request: String,
    config: Data<Arc<Config>>,
) -> impl Responder {
    match config.save_ui_config_file(&ui_settings_request).await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

pub async fn init_server(all_devices: AllDevices, settings_processor: Arc<SettingsProcessor>, config: Arc<Config>) -> Result<Server> {
    let server = HttpServer::new(move || {
        App::new()
            .wrap(Condition::new(
                log::max_level() == LevelFilter::Debug,
                Compat::new(middleware::Logger::default()),
            ))
            .wrap(Cors::default()
                .allow_any_method()
                .allow_any_header()
                .allowed_origin_fn(|origin, _req_head| {
                    if let Ok(str) = origin.to_str() {
                        str.contains("//localhost:") || str.contains("//127.0.0.1:")
                    } else {
                        false
                    }
                })
            )
            // .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
            .app_data(Data::new(all_devices.clone()))
            .app_data(Data::new(settings_processor.clone()))
            .app_data(Data::new(config.clone()))
            .service(handshake)
            .service(shutdown)
            .service(devices::get_devices)
            .service(status::get_status)
            .service(devices::get_device_settings)
            .service(devices::apply_device_settings)
            .service(get_cc_settings)
            .service(apply_cc_settings)
            .service(devices::asetek)
            .service(thinkpad_fan_control)
            .service(get_profiles)
            .service(save_profiles)
            .service(get_functions)
            .service(save_functions)
            .service(save_ui_settings)
            .service(get_ui_settings)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
