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

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddrV4, SocketAddrV6};
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
use actix_web::middleware::{Compat, Condition, Logger, NormalizePath};
use actix_web::web::{Data, Json};
use actix_web::{
    cookie, get, post, put, web, App, HttpRequest, HttpResponse, HttpServer, Responder,
};
use anyhow::{anyhow, Result};
use derive_more::{Display, Error};
use http_auth_basic::Credentials;
use log::{debug, info, warn, LevelFilter};
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde::{Deserialize, Serialize};
use serde_json::json;
use strum::EnumString;
use tokio::net::{TcpListener, ToSocketAddrs};

use crate::config::Config;
use crate::modes::ModeController;
use crate::processing::settings::SettingsController;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::{admin, AllDevices};

mod custom_sensors;
mod devices;
mod functions;
mod modes;
mod profiles;
mod settings;
mod status;

const API_SERVER_PORT_DEFAULT: Port = 11987;
const API_SERVER_WORKERS: usize = 1;
const SESSION_COOKIE_NAME: &str = "cc";
const SESSION_PERMISSIONS: &str = "permissions";
const SESSION_USER_ID: &str = "CCAdmin";

type Port = u16;

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
        .map(|()| HttpResponse::Ok().finish())
        .map_err(|err| CCError::InternalError {
            msg: err.to_string(),
        })
}

/// Enables or disables ThinkPad Fan Control
#[put("/thinkpad-fan-control")]
async fn thinkpad_fan_control(
    fan_control_request: Json<ThinkPadFanControlRequest>,
    settings_controller: Data<Arc<SettingsController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    handle_simple_result(
        settings_controller
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
    let permission = Permission::from_str(&permissions).unwrap_or(Permission::Guest);
    match permission {
        Permission::Admin => Ok(()),
        Permission::Guest => Err(CCError::InvalidCredentials {
            msg: "Invalid Credentials".to_string(),
        }),
    }
}

#[post("/logout")]
async fn logout(session: Session) -> Result<impl Responder, CCError> {
    session.purge();
    Ok(HttpResponse::Ok().finish())
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
    #[display(fmt = "Internal Error: {msg}")]
    InternalError { msg: String },

    #[display(fmt = "Error with external library: {msg}")]
    ExternalError { msg: String },

    #[display(fmt = "Resource not found: {msg}")]
    NotFound { msg: String },

    #[display(fmt = "{msg}")]
    UserError { msg: String },

    #[display(fmt = "{msg}")]
    InvalidCredentials { msg: String },

    #[display(fmt = "{msg}")]
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
        .map(|()| HttpResponse::Ok().finish())
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

async fn can_bind_tcp<A: ToSocketAddrs>(addrs: A) -> bool {
    TcpListener::bind(addrs).await.is_ok()
}

async fn is_free_tcp_ipv4(address: Option<&str>, port: Port) -> Result<SocketAddrV4> {
    if let Some(addr) = address {
        match addr.parse::<Ipv4Addr>() {
            Ok(ipv4_addr) => {
                let ipv4 = SocketAddrV4::new(ipv4_addr, port);
                if can_bind_tcp(ipv4).await {
                    Ok(ipv4)
                } else {
                    Err(anyhow!(
                        "Could not bind to IPv4 address {addr} on port {port}"
                    ))
                }
            }
            Err(err) => Err(anyhow!(err.to_string())),
        }
    } else {
        // try standard loopback address
        let ipv4 = SocketAddrV4::new(Ipv4Addr::LOCALHOST, port);
        if can_bind_tcp(ipv4).await {
            Ok(ipv4)
        } else {
            Err(anyhow!(
                "Could not bind to standard IPv4 loopback address on port {port}"
            ))
        }
    }
}

async fn is_free_tcp_ipv6(address: Option<&str>, port: Port) -> Result<SocketAddrV6> {
    if let Some(addr) = address {
        match addr.parse::<Ipv6Addr>() {
            Ok(ipv6_addr) => {
                let ipv6 = SocketAddrV6::new(ipv6_addr, port, 0, 0);
                if can_bind_tcp(ipv6).await {
                    Ok(ipv6)
                } else {
                    Err(anyhow!(
                        "Could not bind to IPv6 address {addr} on port {port}"
                    ))
                }
            }
            Err(err) => Err(anyhow!(err.to_string())),
        }
    } else {
        // try standard loopback address
        let ipv6 = SocketAddrV6::new(Ipv6Addr::LOCALHOST, port, 0, 0);
        if can_bind_tcp(ipv6).await {
            Ok(ipv6)
        } else {
            Err(anyhow!(
                "Could not bind to standard IPv6 loopback address on port {port}"
            ))
        }
    }
}

async fn determine_ipv4_address(config: &Arc<Config>, port: u16) -> Result<SocketAddrV4> {
    match config.get_settings().await?.ipv4_address {
        Some(ipv4_str) => {
            if ipv4_str.is_empty() {
                Err(anyhow!("IPv4 address disabled"))
            } else {
                is_free_tcp_ipv4(Some(&ipv4_str), port).await
            }
        }
        None => is_free_tcp_ipv4(None, port).await, // Defaults to loopback
    }
}

async fn determine_ipv6_address(config: &Arc<Config>, port: u16) -> Result<SocketAddrV6> {
    match config.get_settings().await?.ipv6_address {
        Some(ipv6_str) => {
            if ipv6_str.is_empty() {
                Err(anyhow!("IPv6 address disabled"))
            } else {
                is_free_tcp_ipv6(Some(&ipv6_str), port).await
            }
        }
        None => is_free_tcp_ipv6(None, port).await, // Defaults to loopback
    }
}

fn config_server(
    cfg: &mut web::ServiceConfig,
    all_devices: AllDevices,
    settings_controller: Arc<SettingsController>,
    config: Arc<Config>,
    cs_repo: Arc<CustomSensorsRepo>,
    mode_controller: Arc<ModeController>,
) {
    cfg
        // .app_data(web::JsonConfig::default().limit(5120)) // <- limit size of the payload
        .app_data(Data::new(all_devices))
        .app_data(Data::new(settings_controller))
        .app_data(Data::new(config))
        .app_data(Data::new(cs_repo))
        .app_data(Data::new(mode_controller))
        .service(handshake)
        .service(login)
        .service(verify_session)
        .service(set_passwd)
        .service(logout)
        .service(shutdown)
        .service(thinkpad_fan_control)
        .service(devices::get_devices)
        .service(status::get_status)
        .service(devices::get_device_settings)
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
        .service(modes::get_modes)
        .service(modes::get_mode)
        .service(modes::set_modes_order)
        .service(modes::create_mode)
        .service(modes::update_mode)
        .service(modes::update_mode_settings)
        .service(modes::delete_mode)
        .service(modes::get_active_mode)
        .service(modes::activate_mode)
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

fn config_cors(ipv4: Option<SocketAddrV4>, ipv6: Option<SocketAddrV6>) -> Cors {
    let mut allowed_addresses = vec![
        // always allowed standard addresses:
        "//localhost:".to_string(),
        "//127.0.0.1:".to_string(),
        "//[::1]:".to_string(),
    ];
    if let Some(ipv4) = ipv4 {
        allowed_addresses.push(format!("//{}:", ipv4.ip()));
    }
    if let Some(ipv6) = ipv6 {
        allowed_addresses.push(format!("//[{}]:", ipv6.ip()));
    }
    Cors::default()
        .allow_any_method()
        .allow_any_header()
        .supports_credentials()
        .allowed_origin_fn(move |origin: &HeaderValue, _req_head: &RequestHead| {
            if let Ok(str) = origin.to_str() {
                allowed_addresses.iter().any(|addr| str.contains(addr))
            } else {
                false
            }
        })
}

pub async fn init_server(
    all_devices: AllDevices,
    settings_controller: Arc<SettingsController>,
    config: Arc<Config>,
    custom_sensors_repo: Arc<CustomSensorsRepo>,
    modes_controller: Arc<ModeController>,
) -> Result<Server> {
    let port = config
        .get_settings()
        .await?
        .port
        .unwrap_or(API_SERVER_PORT_DEFAULT);
    let ipv4_result = determine_ipv4_address(&config, port).await.map_err(|err| {
        warn!("IPv4 bind error: {}", err);
        err
    });
    let ipv6_result = determine_ipv6_address(&config, port).await.map_err(|err| {
        warn!("IPv6 bind error: {}", err);
        err
    });
    if ipv4_result.is_err() && ipv6_result.is_err() {
        return Err(anyhow!(
            "Could not bind API to any address. No API and UI connection available."
        ));
    }
    let ipv4 = ipv4_result.ok();
    let ipv6 = ipv6_result.ok();
    let move_all_devices = all_devices.clone();
    let move_settings_controller = settings_controller.clone();
    let move_config = config.clone();
    let move_cs_repo = custom_sensors_repo.clone();
    let move_mode_controller = modes_controller.clone();
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
            .wrap(config_cors(ipv4, ipv6))
            .wrap(NormalizePath::trim()) // removes trailing slashes for more flexibility
            .configure(|cfg| {
                config_server(
                    cfg,
                    move_all_devices.clone(),
                    move_settings_controller.clone(),
                    move_config.clone(),
                    move_cs_repo.clone(),
                    move_mode_controller.clone(),
                );
            })
    })
    .workers(API_SERVER_WORKERS);
    let bound_server = if ipv4.is_some() && ipv6.is_none() {
        let ipv4_addr = ipv4.unwrap();
        info!("API bound to IPv4 address: {ipv4_addr}");
        server.bind(ipv4_addr)?.run()
    } else if ipv6.is_some() && ipv4.is_none() {
        let ipv6_addr = ipv6.unwrap();
        info!("API bound to IPv6 address: {ipv6_addr}");
        server.bind(ipv6_addr)?.run()
    } else {
        let ipv4_addr = ipv4.unwrap();
        let ipv6_addr = ipv6.unwrap();
        info!("API bound to IPv4 and IPv6 addresses: {ipv4_addr}, {ipv6_addr}");
        server.bind(ipv4_addr)?.bind(ipv6_addr)?.run()
    };
    Ok(bound_server)
}
