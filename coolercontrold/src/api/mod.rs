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

use std::ops::Not;
use std::str::FromStr;
use std::sync::Arc;

use actix_cors::Cors;
use actix_session::config::CookieContentSecurity;
use actix_session::storage::CookieSessionStore;
use actix_session::{Session, SessionMiddleware};
use actix_web::dev::{RequestHead, Server};
use actix_web::http::header::{HeaderValue, AUTHORIZATION};
use actix_web::http::StatusCode;
use actix_web::middleware::{Compat, Condition, Logger};
use actix_web::web::{Data, Json};
use actix_web::{
    cookie, get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use anyhow::Result;
use derive_more::{Display, Error};
use http_auth_basic::Credentials;
use log::{debug, warn, LevelFilter};
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::EnumString;

use crate::config::Config;
use crate::processors::SettingsProcessor;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::{admin, AllDevices};

mod custom_sensors;
mod devices;
mod functions;
mod profiles;
mod settings;
mod status;
mod utils;

const API_SERVER_PORT: u16 = 11987;
const API_SERVER_ADDR_V4: &str = "127.0.0.1";
const API_SERVER_ADDR_V6: &str = "[::1]:11987";
const API_SERVER_WORKERS: usize = 1;
const SESSION_COOKIE_NAME: &str = "cc";
const SESSION_PERMISSIONS: &str = "permissions";
const SESSION_USER_ID: &str = "CCAdmin";

include!(concat!(env!("OUT_DIR"), "/generated.rs"));

/// Returns a simple handshake to verify established connection
#[get("/handshake")]
async fn handshake() -> Result<impl Responder, CCError> {
    Ok(Json(json!({"shake": true})))
}

#[post("/shutdown")]
async fn shutdown(session: Session) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    signal::kill(Pid::this(), Signal::SIGQUIT)
        .map(|_| HttpResponse::Ok().finish())
        .map_err(|err| CCError::InternalError {
            msg: err.to_string(),
        })
}

/// Enables or disables ThinkPad Fan Control
#[put("/thinkpad-fan-control")]
async fn thinkpad_fan_control(
    fan_control_request: Json<ThinkPadFanControlRequest>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    handle_simple_result(
        settings_processor
            .thinkpad_fan_control(&fan_control_request.enable)
            .await,
    )
}

#[post("/login")]
async fn login(req: HttpRequest, session: Session) -> Result<impl Responder, CCError> {
    let auth_header = req.headers().get(AUTHORIZATION);
    if auth_header.is_none() {
        return Err(CCError::InvalidCredentials {
            msg: "Basic Authentication not properly set".to_string(),
        });
    }
    let auth_header_value = auth_header
        .unwrap()
        .to_str()
        .map_err(|err| CCError::InvalidCredentials {
            msg: err.to_string(),
        })?
        .to_string();
    let creds =
        Credentials::from_header(auth_header_value).map_err(|err| CCError::InvalidCredentials {
            msg: err.to_string(),
        })?;
    if creds.user_id == SESSION_USER_ID && admin::passwd_matches(&creds.password).await {
        session
            .insert(SESSION_PERMISSIONS, Permission::Admin.to_string())
            .map_err(|err| CCError::InternalError {
                msg: err.to_string(),
            })?;
        Ok(HttpResponse::Ok().finish())
    } else {
        Err(CCError::InvalidCredentials {
            msg: "Invalid Credentials".to_string(),
        })
    }
}

/// This endpoint is used to verify if the login session is still valid
#[post("/verify-session")]
async fn verify_session(session: Session) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    Ok(HttpResponse::Ok().finish())
}

#[post("/set-passwd")]
async fn set_passwd(req: HttpRequest, session: Session) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let auth_header = req.headers().get(AUTHORIZATION);
    if auth_header.is_none() {
        return Err(CCError::InvalidCredentials {
            msg: "New Authentication not properly set".to_string(),
        });
    }
    let auth_header_value = auth_header
        .unwrap()
        .to_str()
        .map_err(|err| CCError::InvalidCredentials {
            msg: err.to_string(),
        })?
        .to_string();
    let creds =
        Credentials::from_header(auth_header_value).map_err(|err| CCError::InvalidCredentials {
            msg: err.to_string(),
        })?;
    if creds.user_id == SESSION_USER_ID && creds.password.is_empty().not() {
        admin::save_passwd(&creds.password).await?;
        session.renew();
        Ok(HttpResponse::Ok().finish())
    } else {
        Err(CCError::InvalidCredentials {
            msg: "Invalid New Credentials".to_string(),
        })
    }
}

pub async fn verify_admin_permissions(session: &Session) -> Result<(), CCError> {
    let permissions = session
        .get::<String>(SESSION_PERMISSIONS)
        .unwrap_or_else(|_| Some(Permission::Guest.to_string()))
        .unwrap_or_else(|| Permission::Guest.to_string());
    let permission = Permission::from_str(&permissions).unwrap_or_else(|_| Permission::Guest);
    match permission {
        Permission::Admin => Ok(()),
        Permission::Guest => Err(CCError::InvalidCredentials {
            msg: "Invalid Credentials".to_string(),
        }),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ThinkPadFanControlRequest {
    enable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Clone, Display, EnumString)]
pub enum Permission {
    Admin,
    Guest,
}

#[derive(Debug, Clone, Serialize, Deserialize, Display, Error)]
pub enum CCError {
    #[display(fmt = "Internal Error: {}", msg)]
    InternalError { msg: String },

    #[display(fmt = "Error with external library: {}", msg)]
    ExternalError { msg: String },

    #[display(fmt = "Resource not found: {}", msg)]
    NotFound { msg: String },

    #[display(fmt = "{}", msg)]
    UserError { msg: String },

    #[display(fmt = "{}", msg)]
    InvalidCredentials { msg: String },

    #[display(fmt = "{}", msg)]
    InsufficientScope { msg: String },
}

impl actix_web::error::ResponseError for CCError {
    fn status_code(&self) -> StatusCode {
        match *self {
            CCError::InternalError { .. } => StatusCode::INTERNAL_SERVER_ERROR,
            CCError::ExternalError { .. } => StatusCode::BAD_GATEWAY,
            CCError::NotFound { .. } => StatusCode::NOT_FOUND,
            CCError::UserError { .. } => StatusCode::BAD_REQUEST,
            CCError::InvalidCredentials { .. } => StatusCode::UNAUTHORIZED,
            CCError::InsufficientScope { .. } => StatusCode::FORBIDDEN,
        }
    }

    fn error_response(&self) -> HttpResponse {
        match self {
            // we don't want to confuse the user with these errors, which can happen regularly
            CCError::InvalidCredentials { .. } => debug!("{:?}", self.to_string()),
            _ => warn!("{:?}", self.to_string()),
        }
        HttpResponse::build(self.status_code()).json(Json(ErrorResponse {
            error: self.to_string(),
        }))
    }
}

impl From<std::io::Error> for CCError {
    fn from(err: std::io::Error) -> Self {
        CCError::InternalError {
            msg: err.to_string(),
        }
    }
}

impl From<anyhow::Error> for CCError {
    fn from(err: anyhow::Error) -> Self {
        if let Some(underlying_error) = err.downcast_ref::<CCError>() {
            underlying_error.clone()
        } else {
            CCError::InternalError {
                msg: err.to_string(),
            }
        }
    }
}

fn handle_error(err: anyhow::Error) -> CCError {
    err.into()
}

fn handle_simple_result(result: Result<()>) -> Result<impl Responder, CCError> {
    result
        .map(|_| HttpResponse::Ok().finish())
        .map_err(handle_error)
}

pub fn validate_name_string(name: &str) -> Result<(), CCError> {
    let mut invalid_msg: Option<String> = None;
    if name.is_empty() {
        invalid_msg = Some("name cannot be empty".to_string());
    } else if name.len() > 50 {
        invalid_msg = Some("name cannot be longer than 50 characters".to_string());
    } else if name.contains('\t') {
        invalid_msg = Some("name cannot contain tabs".to_string());
    } else if name.contains('\n') {
        invalid_msg = Some("name cannot contain newlines".to_string());
    } else if name.contains('\r') {
        invalid_msg = Some("name cannot contain carriage returns".to_string());
    } else if name.contains('\0') {
        invalid_msg = Some("name cannot contain null characters".to_string());
    } else if name.contains('\x0B') {
        invalid_msg = Some("name cannot contain vertical tabs".to_string());
    } else if name.contains('\x0C') {
        invalid_msg = Some("name cannot contain form feeds".to_string());
    } else if name.contains('\x1B') {
        invalid_msg = Some("name cannot contain escape characters".to_string());
    } else if name.contains('\x7F') {
        invalid_msg = Some("name cannot contain delete characters".to_string());
    }
    if let Some(msg) = invalid_msg {
        Err(CCError::UserError { msg })
    } else {
        Ok(())
    }
}

fn config_server(
    cfg: &mut web::ServiceConfig,
    all_devices: AllDevices,
    settings_processor: Arc<SettingsProcessor>,
    config: Arc<Config>,
    cs_repo: Arc<CustomSensorsRepo>,
) {
    cfg
        // .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
        .app_data(Data::new(all_devices))
        .app_data(Data::new(settings_processor))
        .app_data(Data::new(config))
        .app_data(Data::new(cs_repo))
        .service(handshake)
        .service(login)
        .service(verify_session)
        .service(set_passwd)
        .service(shutdown)
        .service(thinkpad_fan_control)
        .service(devices::get_devices)
        .service(status::get_status)
        .service(devices::get_device_settings)
        .service(devices::apply_device_settings)
        .service(devices::apply_device_setting_manual)
        .service(devices::apply_device_setting_profile)
        .service(devices::apply_device_setting_lcd)
        .service(devices::get_device_lcd_images)
        .service(devices::apply_device_setting_lcd_images)
        .service(devices::process_device_lcd_images)
        .service(devices::apply_device_setting_lighting)
        .service(devices::apply_device_setting_pwm)
        .service(devices::apply_device_setting_reset)
        .service(devices::asetek)
        .service(profiles::get_profiles)
        .service(profiles::save_profiles_order)
        .service(profiles::save_profile)
        .service(profiles::update_profile)
        .service(profiles::delete_profile)
        .service(functions::get_functions)
        .service(functions::save_functions_order)
        .service(functions::save_function)
        .service(functions::update_function)
        .service(functions::delete_function)
        .service(custom_sensors::get_custom_sensors)
        .service(custom_sensors::get_custom_sensor)
        .service(custom_sensors::save_custom_sensors_order)
        .service(custom_sensors::save_custom_sensor)
        .service(custom_sensors::update_custom_sensor)
        .service(custom_sensors::delete_custom_sensor)
        .service(settings::get_cc_settings)
        .service(settings::apply_cc_settings)
        .service(settings::get_cc_settings_for_all_devices)
        .service(settings::get_cc_settings_for_device)
        .service(settings::save_cc_settings_for_device)
        .service(settings::save_ui_settings)
        .service(settings::get_ui_settings)
        .service(actix_web_static_files::ResourceFiles::new("/", generate()));
}

fn config_logger() -> Condition<Compat<Logger>> {
    Condition::new(
        log::max_level() == LevelFilter::Trace,
        Compat::new(Logger::default()),
    )
}

fn config_cors() -> Cors {
    Cors::default()
        .allow_any_method()
        .allow_any_header()
        .supports_credentials()
        .allowed_origin_fn(|origin: &HeaderValue, _req_head: &RequestHead| {
            if let Ok(str) = origin.to_str() {
                str.contains("//localhost:")
                    || str.contains("//127.0.0.1:")
                    || str.contains("//[::1]:")
            } else {
                false
            }
        })
}

pub async fn init_server(
    all_devices: AllDevices,
    settings_processor: Arc<SettingsProcessor>,
    config: Arc<Config>,
    custom_sensors_repo: Arc<CustomSensorsRepo>,
) -> Result<Server> {
    let move_all_devices = all_devices.clone();
    let move_settings_processor = settings_processor.clone();
    let move_config = config.clone();
    let move_cs_repo = custom_sensors_repo.clone();
    let session_key = cookie::Key::generate(); // sessions do not persist across restarts
    let server = HttpServer::new(move || {
        App::new()
            .wrap(config_logger())
            .wrap(
                SessionMiddleware::builder(CookieSessionStore::default(), session_key.clone())
                    .cookie_content_security(CookieContentSecurity::Private)
                    .cookie_secure(false)
                    .cookie_http_only(true)
                    .cookie_same_site(cookie::SameSite::Strict)
                    .cookie_name(SESSION_COOKIE_NAME.to_string())
                    .build(),
            )
            .wrap(config_cors())
            .configure(|cfg| {
                config_server(
                    cfg,
                    move_all_devices.clone(),
                    move_settings_processor.clone(),
                    move_config.clone(),
                    move_cs_repo.clone(),
                )
            })
    })
    .workers(API_SERVER_WORKERS)
    .bind((API_SERVER_ADDR_V4, API_SERVER_PORT))?;
    // we attempt to bind to the standard ipv4 and ipv6 loopback addresses
    // but will fallback to ipv4 only if ipv6 is not enabled
    match server.bind(API_SERVER_ADDR_V6) {
        Ok(ipv6_bound_server) => Ok(ipv6_bound_server.run()),
        Err(err) => {
            warn!("Failed to bind to loopback ipv6 address: {err}");
            Ok(HttpServer::new(move || {
                App::new()
                    .wrap(config_logger())
                    .wrap(config_cors())
                    .configure(|cfg| {
                        config_server(
                            cfg,
                            all_devices.clone(),
                            settings_processor.clone(),
                            config.clone(),
                            custom_sensors_repo.clone(),
                        )
                    })
            })
            .workers(API_SERVER_WORKERS)
            .bind((API_SERVER_ADDR_V4, API_SERVER_PORT))?
            .run())
        }
    }
}
