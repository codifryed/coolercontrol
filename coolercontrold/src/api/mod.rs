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
use crate::settings_processor::SettingsProcessor;

mod devices;
mod status;
mod settings;
mod profiles;
mod functions;

const GUI_SERVER_PORT: u16 = 11987;
const GUI_SERVER_ADDR: &str = "127.0.0.1";

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

#[post("/thinkpad_fan_control")]
async fn thinkpad_fan_control(
    fan_control_request: Json<ThinkPadFanControlRequest>,
    settings_processor: Data<Arc<SettingsProcessor>>,
) -> impl Responder {
    handle_simple_result(
        settings_processor.thinkpad_fan_control(&fan_control_request.enable).await
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThinkPadFanControlRequest {
    enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

fn handle_error(err: anyhow::Error) -> HttpResponse {
    error!("{:?}", err);
    HttpResponse::InternalServerError().json(Json(ErrorResponse { error: err.to_string() }))
}

fn handle_simple_result(result: Result<()>) -> HttpResponse {
    match result {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => handle_error(err)
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
            .service(thinkpad_fan_control)
            .service(devices::get_devices)
            .service(status::get_status)
            .service(devices::get_device_settings)
            .service(devices::apply_device_settings)
            .service(settings::get_cc_settings)
            .service(settings::apply_cc_settings)
            .service(devices::asetek)
            .service(profiles::get_profiles)
            .service(profiles::save_profiles)
            .service(functions::get_functions)
            .service(functions::save_functions)
            .service(settings::save_ui_settings)
            .service(settings::get_ui_settings)
    }).bind((GUI_SERVER_ADDR, GUI_SERVER_PORT))?
        .workers(1)
        .run();
    Ok(server)
}
