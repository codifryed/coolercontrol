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

pub mod actor;
mod auth;
mod base;
mod custom_sensors;
mod devices;
mod functions;
mod modes;
mod profiles;
mod router;
mod settings;
mod sse;
mod status;

use crate::api::actor::{
    AuthHandle, CustomSensorHandle, DeviceHandle, FunctionHandle, ModeHandle, ProfileHandle,
    SettingHandle, StatusHandle,
};
use crate::config::Config;
use crate::logger::LogBufHandle;
use crate::modes::ModeController;
use crate::processing::settings::SettingsController;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::{AllDevices, VERSION};
use aide::openapi::{ApiKeyLocation, Contact, License, OpenApi, SecurityScheme, Tag};
use aide::transform::TransformOpenApi;
use aide::OperationOutput;
use anyhow::{anyhow, Result};
use axum::extract::rejection::JsonRejection;
use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json, Router};
use axum_extra::typed_header::TypedHeaderRejection;
use derive_more::{Display, Error};
use log::{debug, info, warn};
use moro_local::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio_util::sync::CancellationToken;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::GovernorLayer;
use tower_http::compression::CompressionLayer;
use tower_http::cors;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::{Key, SameSite};
use tower_sessions::service::PrivateCookie;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

const API_SERVER_PORT_DEFAULT: Port = 11987;
const SESSION_COOKIE_NAME: &str = "cc";

type Port = u16;

pub async fn start_server<'s>(
    all_devices: AllDevices,
    settings_controller: Rc<SettingsController>,
    config: Rc<Config>,
    custom_sensors_repo: Rc<CustomSensorsRepo>,
    modes_controller: Rc<ModeController>,
    log_buf_handle: LogBufHandle,
    status_handle: StatusHandle,
    cancel_token: CancellationToken,
    main_scope: &'s Scope<'s, 's, Result<()>>,
) -> Result<()> {
    let port = config
        .get_settings()?
        .port
        .unwrap_or(API_SERVER_PORT_DEFAULT);
    let ipv4 = determine_ipv4_address(&config, port)
        .await
        .inspect_err(|err| warn!("IPv4 bind error: {err}"));
    let ipv6 = determine_ipv6_address(&config, port)
        .await
        .inspect_err(|err| warn!("IPv6 bind error: {err}"));
    if ipv4.is_err() && ipv6.is_err() {
        return Err(anyhow!(
            "Could not bind API to any address. No API and UI connection available."
        ));
    }

    let compress_enabled = config.get_settings()?.compress;
    let app_state = create_app_state(
        all_devices,
        &settings_controller,
        config,
        &custom_sensors_repo,
        &modes_controller,
        log_buf_handle,
        status_handle,
        &cancel_token,
        main_scope,
    );
    // Note: to persist user login session across daemon restarts would require a
    //  persisted master key and local db backend for the session data.
    let session_layer = SessionManagerLayer::new(MemoryStore::default())
        .with_name(SESSION_COOKIE_NAME)
        .with_private(Key::generate())
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Strict)
        .with_expiry(Expiry::OnSessionEnd);
    let governor_layer = GovernorLayer {
        config: Arc::new(
            GovernorConfigBuilder::default()
                // startup has quite a few requests (e.g. per device)
                .burst_size(20)
                // 10 req/s
                .per_millisecond(100)
                .finish()
                .unwrap(),
        ),
    };

    if let Ok(ipv4) = ipv4 {
        main_scope.spawn(create_api_server(
            SocketAddr::from(ipv4),
            app_state.clone(),
            compress_enabled,
            session_layer.clone(),
            governor_layer.clone(),
            cancel_token.clone(),
        ));
    }
    if let Ok(ipv6) = ipv6 {
        main_scope.spawn(create_api_server(
            SocketAddr::from(ipv6),
            app_state,
            compress_enabled,
            session_layer,
            governor_layer,
            cancel_token,
        ));
    }
    Ok(())
}

async fn create_api_server(
    addr: SocketAddr,
    app_state: AppState,
    compress_enabled: bool,
    session_layer: SessionManagerLayer<MemoryStore, PrivateCookie>,
    governor_layer: GovernorLayer,
    cancel_token: CancellationToken,
) -> Result<()> {
    aide::gen::on_error(|error| {
        debug!("OpenApi Generation Error: {error}");
    });
    let mut open_api = OpenApi::default();
    let router = router::init(app_state)
        .finish_api_with(&mut open_api, api_docs)
        .layer(Extension(Arc::new(open_api)));
    let listener = TcpListener::bind(addr).await?;
    info!("API bound to address: {}", addr);
    axum::serve(
        listener,
        optional_layers(compress_enabled, router)
            .layer(cors_layer())
            .layer(NormalizePathLayer::trim_trailing_slash())
            .layer(session_layer)
            .layer(governor_layer)
            .layer((
                TraceLayer::new_for_http(),
                TimeoutLayer::new(Duration::from_secs(30)),
            ))
            // 2MB is the default payload limit:
            .layer(DefaultBodyLimit::disable())
            // Limits the size of the payload in bytes: (Max 50MB for image files)
            .layer(RequestBodyLimitLayer::new(50 * 1024 * 1024))
            .into_make_service(),
    )
    .with_graceful_shutdown(async move { cancel_token.cancelled().await })
    .await?;
    Ok(())
}
fn api_docs(api: TransformOpenApi) -> TransformOpenApi {
    let version = VERSION.unwrap_or("unknown");
    api.title("CoolerControl Daemon API")
        .summary("CoolerControl Rest Endpoints")
        .description("Basic OpenAPI documentation for the CoolerControl Daemon API")
        .contact(Contact {
            name: Some("CoolerControl".to_string()),
            url: Some("https://coolercontrol.org".to_string()),
            ..Contact::default()
        })
        .license(License {
            name: "GPL3+".to_string(),
            identifier: Some("GPL3+".to_string()),
            ..License::default()
        })
        .version(version)
        .security_scheme(
            "CookieAuth",
            SecurityScheme::ApiKey {
                location: ApiKeyLocation::Cookie,
                name: SESSION_COOKIE_NAME.to_string(),
                description: Some(
                    "The private session cookie used for authentication.".to_string(),
                ),
                extensions: Default::default(),
            },
        )
        .security_scheme(
            "BasicAuth",
            SecurityScheme::Http {
                scheme: "basic".to_string(),
                bearer_format: Some(String::new()),
                description: Some(
                    "HTTP Basic authentication, mostly used to generate a secure authentication cookie."
                        .to_string(),
                ),
                extensions: Default::default(),
            },
        )
        .tag(Tag {
            name: "base".to_string(),
            description: Some("Foundational endpoints for this API".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "auth".to_string(),
            description: Some("Authentication".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "device".to_string(),
            description: Some("Device Interaction".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "status".to_string(),
            description: Some("Device Status".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "profile".to_string(),
            description: Some("Profiles".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "function".to_string(),
            description: Some("Functions".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "custom-sensor".to_string(),
            description: Some("Custom Sensors".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "mode".to_string(),
            description: Some("Modes".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "setting".to_string(),
            description: Some("Settings".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "sse".to_string(),
            description: Some("Server Side Events".to_string()),
            ..Tag::default()
        })
}

fn create_app_state<'s>(
    all_devices: AllDevices,
    settings_controller: &Rc<SettingsController>,
    config: Rc<Config>,
    custom_sensors_repo: &Rc<CustomSensorsRepo>,
    modes_controller: &Rc<ModeController>,
    log_buf_handle: LogBufHandle,
    status_handle: StatusHandle,
    cancel_token: &CancellationToken,
    main_scope: &'s Scope<'s, 's, Result<()>>,
) -> AppState {
    let auth_handle = AuthHandle::new(cancel_token.clone(), main_scope);
    let device_handle = DeviceHandle::new(
        all_devices.clone(),
        settings_controller.clone(),
        config.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let profile_handle = ProfileHandle::new(
        settings_controller.clone(),
        config.clone(),
        modes_controller.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let function_handle = FunctionHandle::new(
        settings_controller.clone(),
        config.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let custom_sensor_handle = CustomSensorHandle::new(
        custom_sensors_repo.clone(),
        settings_controller.clone(),
        config.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let mode_handle = ModeHandle::new(modes_controller.clone(), cancel_token.clone(), main_scope);
    let setting_handle = SettingHandle::new(all_devices, config, cancel_token.clone(), main_scope);
    AppState {
        auth_handle,
        device_handle,
        status_handle,
        profile_handle,
        function_handle,
        custom_sensor_handle,
        mode_handle,
        setting_handle,
        log_buf_handle,
    }
}

fn optional_layers(compress_enabled: bool, router: Router) -> Router {
    if compress_enabled {
        router.layer(CompressionLayer::new())
    } else {
        router
    }
}

fn cors_layer() -> cors::CorsLayer {
    cors::CorsLayer::new()
        .allow_credentials(true)
        .allow_headers(cors::AllowHeaders::mirror_request())
        .allow_methods(cors::AllowMethods::mirror_request())
        // We don't really care about Origin security, as it runs on each person's server
        // and may also limit access through a proxy. Alternative impl below.
        // Note: We can't use wildcard any with credentials.
        .allow_origin(cors::AllowOrigin::mirror_request())
        .max_age(Duration::from_secs(60) * 5)

    // Alternative:
    //     let mut allowed_addresses = vec![
    //         // always allowed standard addresses:
    //         "//localhost:".to_string(),
    //         "//127.0.0.1:".to_string(),
    //         "//[::1]:".to_string(),
    //     ];
    //     if let Some(ipv4) = ipv4 {
    //         allowed_addresses.push(format!("//{}:", ipv4.ip()));
    //     }
    //     if let Some(ipv6) = ipv6 {
    //         allowed_addresses.push(format!("//[{}]:", ipv6.ip()));
    //     }
    //     Cors::default()
    //         .allow_any_method()
    //         .allow_any_header()
    //         .supports_credentials()
    //         .allowed_origin_fn(move |origin: &HeaderValue, _req_head: &RequestHead| {
    //             if let Ok(str) = origin.to_str() {
    //                 allowed_addresses.iter().any(|addr| str.contains(addr))
    //             } else {
    //                 false
    //             }
    //         })
}

fn handle_error(err: anyhow::Error) -> CCError {
    err.into()
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

async fn determine_ipv4_address(config: &Rc<Config>, port: u16) -> Result<SocketAddrV4> {
    match config.get_settings()?.ipv4_address {
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

async fn determine_ipv6_address(config: &Rc<Config>, port: u16) -> Result<SocketAddrV6> {
    match config.get_settings()?.ipv6_address {
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

/// How we want errors responses to be serialized
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize, Deserialize, Display, Error, Clone, JsonSchema)]
pub enum CCError {
    #[display("Internal Error: {msg}")]
    InternalError { msg: String },

    #[display("Error with external library: {msg}")]
    ExternalError { msg: String },

    #[display("Resource not found: {msg}")]
    NotFound { msg: String },

    #[display("{msg}")]
    UserError { msg: String },

    // #[display("Json serialization error: {}", _0.body_text())]
    // JsonRejection(JsonRejection),
    #[display("{msg}")]
    InvalidCredentials { msg: String },

    #[display("{msg}")]
    InsufficientScope { msg: String },
}

impl IntoResponse for CCError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            CCError::InternalError { .. } => {
                // We use the Display trait to format our errors
                let err_msg = self.to_string();
                warn!("{err_msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, err_msg)
            }
            CCError::ExternalError { .. } => {
                let err_msg = self.to_string();
                warn!("{err_msg}");
                (StatusCode::BAD_GATEWAY, err_msg)
            }
            CCError::NotFound { .. } => {
                let err_msg = self.to_string();
                warn!("{err_msg}");
                (StatusCode::NOT_FOUND, err_msg)
            }
            CCError::UserError { .. } => {
                let err_msg = self.to_string();
                warn!("{err_msg}");
                (StatusCode::BAD_REQUEST, err_msg)
            }
            CCError::InvalidCredentials { .. } => {
                let err_msg = self.to_string();
                // we don't want to confuse the user with these errors, which can happen regularly:
                debug!("{err_msg}");
                (StatusCode::UNAUTHORIZED, err_msg)
            }
            CCError::InsufficientScope { .. } => {
                let err_msg = self.to_string();
                warn!("{err_msg}");
                (StatusCode::FORBIDDEN, err_msg)
            }
        };

        (status, Json(ErrorResponse { error })).into_response()
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

impl From<JsonRejection> for CCError {
    fn from(jr: JsonRejection) -> Self {
        CCError::UserError {
            msg: jr.body_text(),
        }
    }
}

impl From<TypedHeaderRejection> for CCError {
    fn from(thr: TypedHeaderRejection) -> Self {
        CCError::InvalidCredentials {
            msg: format!("Header: {}, Reason: {:?}", thr.name(), thr.reason()),
        }
    }
}

/// Fills in `OpenApi` info for our Error type.
impl OperationOutput for CCError {
    type Inner = ();

    fn operation_response(
        _ctx: &mut aide::gen::GenContext,
        _operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        Some(aide::openapi::Response::default())
    }

    fn inferred_responses(
        ctx: &mut aide::gen::GenContext,
        operation: &mut aide::openapi::Operation,
    ) -> Vec<(Option<u16>, aide::openapi::Response)> {
        if let Some(res) = Self::operation_response(ctx, operation) {
            vec![
                (
                    Some(400),
                    aide::openapi::Response {
                        description: "Bad Request. The request is invalid".to_owned(),
                        ..res.clone()
                    },
                ),
                (
                    Some(401),
                    aide::openapi::Response {
                        description: "Unauthorized. Invalid credentials were provided.".to_owned(),
                        ..res.clone()
                    },
                ),
                (
                    Some(403),
                    aide::openapi::Response {
                        description: "Forbidden. Insufficient permissions.".to_owned(),
                        ..res.clone()
                    },
                ),
                (
                    Some(404),
                    aide::openapi::Response {
                        description: "Whatever you're looking for, it's not here.".to_owned(),
                        ..res.clone()
                    },
                ),
                (
                    Some(500),
                    aide::openapi::Response {
                        description: "An internal error has occurred.".to_owned(),
                        ..res.clone()
                    },
                ),
                (
                    Some(502),
                    aide::openapi::Response {
                        description: "Bad Gateway. An error has occurred with an external library."
                            .to_owned(),
                        ..res
                    },
                ),
            ]
        } else {
            Vec::new()
        }
    }
}

#[derive(Clone)]
pub struct AppState {
    pub auth_handle: AuthHandle,
    pub device_handle: DeviceHandle,
    pub status_handle: StatusHandle,
    pub profile_handle: ProfileHandle,
    pub function_handle: FunctionHandle,
    pub custom_sensor_handle: CustomSensorHandle,
    pub mode_handle: ModeHandle,
    pub setting_handle: SettingHandle,
    pub log_buf_handle: LogBufHandle,
}
