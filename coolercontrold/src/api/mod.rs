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

use actix_cors::Cors;
use actix_web::{App, get, HttpResponse, HttpServer, middleware, post, Responder};
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
use crate::setting::{Function, Profile};
use crate::settings_processor::SettingsProcessor;

mod devices;
mod status;
mod settings;

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
            .service(settings::get_cc_settings)
            .service(settings::apply_cc_settings)
            .service(devices::asetek)
            .service(thinkpad_fan_control)
            .service(get_profiles)
            .service(save_profiles)
            .service(get_functions)
            .service(save_functions)
            .service(settings::save_ui_settings)
            .service(settings::get_ui_settings)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
