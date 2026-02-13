/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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
mod alerts;
mod auth;
mod base;
mod custom_sensors;
pub mod devices;
mod dual_protocol;
mod functions;
pub mod modes;
mod plugins;
mod profiles;
mod router;
mod settings;
mod sse;
pub mod status;
mod tls;

use crate::alerts::AlertController;
use crate::api::actor::{
    AlertHandle, AuthHandle, CustomSensorHandle, DeviceHandle, FunctionHandle, HealthHandle,
    ModeHandle, PluginHandle, ProfileHandle, SettingHandle, StatusHandle,
};
use crate::api::dual_protocol::Protocol;
use crate::config::Config;
use crate::engine::main::Engine;
use crate::grpc_api::create_grpc_api_server;
use crate::logger::LogBufHandle;
use crate::modes::ModeController;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::repositories::service_plugin::plugin_controller::PluginController;
use crate::setting::CoolerControlSettings;
use crate::{
    AllDevices, Repos, ENV_CERT_PATH, ENV_HOST_IP4, ENV_HOST_IP6, ENV_KEY_PATH, ENV_PORT, ENV_TLS,
    VERSION,
};
use aide::openapi::{ApiKeyLocation, Contact, License, OpenApi, SecurityScheme, Tag};
use aide::transform::TransformOpenApi;
use aide::OperationOutput;
use anyhow::{anyhow, Result};
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Request};
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json, Router, ServiceExt};
use axum_extra::typed_header::TypedHeaderRejection;
use axum_server::tls_rustls::RustlsConfig;
use derive_more::{Display, Error};
use log::{debug, info, warn};
use moro_local::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::env;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::ops::Not;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::task::LocalSet;
use tokio_util::sync::CancellationToken;
use tower::Layer;
use tower_http::compression::CompressionLayer;
use tower_http::cors;
use tower_http::decompression::DecompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::{Key, SameSite};
use tower_sessions::service::PrivateCookie;
use tower_sessions::{Expiry, MemoryStore, SessionManagerLayer};

const API_SERVER_PORT_DEFAULT: Port = 11987;
const GRPC_SERVER_PORT_DEFAULT: Port = 11988; // Standard API Port +1
const SESSION_COOKIE_NAME: &str = "cc";
const API_TIMEOUT_SECS: u64 = 30;
const API_SHUTDOWN_TIMEOUT_SECS: u64 = 5;

type Port = u16;

#[allow(clippy::too_many_lines)]
pub async fn start_server<'s>(
    all_devices: AllDevices,
    repos: Repos,
    engine: Rc<Engine>,
    config: Rc<Config>,
    custom_sensors_repo: Rc<CustomSensorsRepo>,
    modes_controller: Rc<ModeController>,
    alert_controller: Rc<AlertController>,
    plugin_controller: Rc<PluginController>,
    log_buf_handle: LogBufHandle,
    status_handle: StatusHandle,
    cancel_token: CancellationToken,
    main_scope: &'s Scope<'s, 's, Result<()>>,
) -> Result<()> {
    let rest_port = env::var(ENV_PORT)
        .ok()
        .and_then(|p| p.parse::<u16>().ok())
        .unwrap_or_else(|| {
            config
                .get_settings()
                .ok()
                .and_then(|settings| settings.port)
                .unwrap_or(API_SERVER_PORT_DEFAULT)
        });
    let ipv4 = determine_ipv4_address(&config, rest_port)
        .await
        .inspect_err(|err| warn!("IPv4 bind error: {err}"));
    let ipv6 = determine_ipv6_address(&config, rest_port)
        .await
        .inspect_err(|err| warn!("IPv6 bind error: {err}"));
    if ipv4.is_err() && ipv6.is_err() {
        return Err(anyhow!(
            "Could not bind API to any address. No API and UI connection available."
        ));
    }

    let settings = config.get_settings()?;
    let compression_layers = if settings.compress {
        Some((CompressionLayer::new(), DecompressionLayer::new()))
    } else {
        None
    };
    let app_state = create_app_state(
        all_devices,
        repos,
        &engine,
        Rc::clone(&config),
        &custom_sensors_repo,
        &modes_controller,
        &alert_controller,
        plugin_controller,
        log_buf_handle,
        status_handle,
        &cancel_token,
        main_scope,
    );
    let tls_config = tls_config(&settings).await;
    // Note: to persist user login session across daemon restarts would require a
    //  persisted master key and local db backend for the session data.
    let session_layer = SessionManagerLayer::new(MemoryStore::default())
        .with_name(SESSION_COOKIE_NAME)
        .with_private(Key::generate())
        // unsecure is used for local connections:
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Strict)
        .with_expiry(Expiry::OnSessionEnd);

    // GRPC API
    // We use a separate socket because the purpose and scope is quite different comparatively
    let grpc_port = env::var(ENV_PORT)
        .ok()
        .and_then(|p| p.parse::<u16>().map(|p| p + 1).ok())
        .unwrap_or_else(|| {
            config
                .get_settings()
                .ok()
                .and_then(|settings| settings.port.map(|p| p + 1))
                .unwrap_or(GRPC_SERVER_PORT_DEFAULT)
        });
    let grpc_ipv4 = determine_ipv4_address(&config, grpc_port)
        .await
        .inspect_err(|err| warn!("IPv4 GRPC bind error: {err}"));
    let grpc_ipv6 = determine_ipv6_address(&config, grpc_port)
        .await
        .inspect_err(|err| warn!("IPv6 GRPC bind error: {err}"));
    if grpc_ipv4.is_err() && grpc_ipv6.is_err() {
        return Err(anyhow!(
            "Could not bind GRPC API to any address. External Device services are unavailable."
        ));
    }

    // Extract proxy/cors settings for the API servers
    let cors_origins = settings.origins.clone();
    let allow_unencrypted = settings.allow_unencrypted;
    let protocol_header = settings.protocol_header.clone();

    // Spawn all API servers on a dedicated thread
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .max_blocking_threads(1) // not needed for API servers
            .thread_keep_alive(Duration::from_secs(60))
            .thread_name("cc-api")
            .event_interval(200)
            .global_queue_interval(200)
            .build()
            .expect("Failed to create API server runtime");
        rt.block_on(LocalSet::new().run_until(run_all_api_servers(
            ipv4.ok(),
            ipv6.ok(),
            grpc_ipv4.ok(),
            grpc_ipv6.ok(),
            app_state,
            session_layer,
            compression_layers,
            tls_config,
            cancel_token,
            cors_origins,
            allow_unencrypted,
            protocol_header,
        )));
    });
    Ok(())
}

async fn run_all_api_servers(
    ipv4: Option<SocketAddrV4>,
    ipv6: Option<SocketAddrV6>,
    grpc_ipv4: Option<SocketAddrV4>,
    grpc_ipv6: Option<SocketAddrV6>,
    app_state: AppState,
    session_layer: SessionManagerLayer<MemoryStore, PrivateCookie>,
    compression_layers: Option<(CompressionLayer, DecompressionLayer)>,
    tls_config: Option<RustlsConfig>,
    cancel_token: CancellationToken,
    cors_origins: Vec<String>,
    allow_unencrypted: bool,
    protocol_header: Option<String>,
) {
    let mut handles = Vec::new();
    let grpc_device_handle = app_state.device_handle.clone();
    let grpc_status_handle = app_state.status_handle.clone();

    // REST API servers
    if let Some(addr) = ipv4 {
        handles.push(tokio::task::spawn_local(create_api_server(
            SocketAddr::from(addr),
            ipv4,
            ipv6,
            app_state.clone(),
            session_layer.clone(),
            compression_layers.clone(),
            tls_config.clone(),
            cancel_token.clone(),
            cors_origins.clone(),
            allow_unencrypted,
            protocol_header.clone(),
        )));
    }
    if let Some(addr) = ipv6 {
        handles.push(tokio::task::spawn_local(create_api_server(
            SocketAddr::from(addr),
            ipv4,
            ipv6,
            app_state,
            session_layer,
            compression_layers,
            tls_config,
            cancel_token.clone(),
            cors_origins,
            allow_unencrypted,
            protocol_header,
        )));
    }

    // gRPC API servers
    if let Some(ipv4) = grpc_ipv4 {
        handles.push(tokio::task::spawn_local(create_grpc_api_server(
            SocketAddr::from(ipv4),
            grpc_device_handle.clone(),
            grpc_status_handle.clone(),
            cancel_token.clone(),
        )));
    }
    if let Some(ipv6) = grpc_ipv6 {
        handles.push(tokio::task::spawn_local(create_grpc_api_server(
            SocketAddr::from(ipv6),
            grpc_device_handle,
            grpc_status_handle,
            cancel_token,
        )));
    }

    // Wait for all servers (they run until canceled)
    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("API server task error: {e}");
        }
    }
}

async fn security_headers_middleware(req: Request, next: middleware::Next) -> Response {
    let is_https = req
        .extensions()
        .get::<Protocol>()
        .is_some_and(|p| *p == Protocol::Https);
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        HeaderValue::from_static("DENY"),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(
        HeaderName::from_static("permissions-policy"),
        HeaderValue::from_static("camera=(), microphone=(), geolocation=()"),
    );
    if is_https {
        headers.insert(
            axum::http::header::STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000; includeSubDomains"),
        );
    }
    response
}

async fn create_api_server(
    addr: SocketAddr,
    ipv4: Option<SocketAddrV4>,
    ipv6: Option<SocketAddrV6>,
    app_state: AppState,
    session_layer: SessionManagerLayer<MemoryStore, PrivateCookie>,
    compression_layers: Option<(CompressionLayer, DecompressionLayer)>,
    tls_config: Option<RustlsConfig>,
    cancel_token: CancellationToken,
    cors_origins: Vec<String>,
    allow_unencrypted: bool,
    protocol_header: Option<String>,
) -> Result<()> {
    aide::generate::on_error(|error| {
        debug!("OpenApi Generation Error: {error}");
    });
    let mut open_api = OpenApi::default();
    let router = router::init(app_state)
        .finish_api_with(&mut open_api, api_docs)
        .layer(Extension(Arc::new(open_api)));

    // Build the base router with all layers
    // Layers are processed bottom to top: (last is first in the chain)
    // See: https://docs.rs/axum/latest/axum/middleware/index.html#ordering
    let base_router = optional_layers(compression_layers, router)
        // Limits the size of the payload in bytes: (Max 50MB for image files)
        .route_layer(RequestBodyLimitLayer::new(50 * 1024 * 1024))
        // 2MB is the default payload limit:
        .route_layer(DefaultBodyLimit::disable())
        .route_layer(session_layer)
        .layer(cors_layer(ipv4, ipv6, cors_origins))
        .layer(middleware::from_fn(security_headers_middleware))
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(API_TIMEOUT_SECS),
            ),
        ));

    let listener = TcpListener::bind(addr).await?;
    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();
    tokio::task::spawn_local(async move {
        cancel_token.cancelled().await;
        shutdown_handle.graceful_shutdown(Some(Duration::from_secs(API_SHUTDOWN_TIMEOUT_SECS)));
    });

    if let Some(tls) = tls_config {
        // Dual-protocol server: accepts both HTTP and HTTPS on the same port
        // HTTP requests from non-localhost are redirected to HTTPS (via middleware)
        // HTTP requests from localhost and to /health are allowed
        info!("Serving HTTP and HTTPS API on {addr}");

        // Add HTTPS redirect layer for non-localhost HTTP requests
        let redirect_layer = dual_protocol::HttpsRedirectLayer {
            port: addr.port(),
            allow_unencrypted,
            protocol_header,
        };
        let router_with_redirect = base_router.layer(redirect_layer);
        let normalized_router =
            NormalizePathLayer::trim_trailing_slash().layer(router_with_redirect);

        let acceptor = dual_protocol::DualProtocolAcceptor::new(tls);
        axum_server::from_tcp(listener.into_std()?)?
            .acceptor(acceptor)
            .handle(handle)
            .serve(
                ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(
                    normalized_router,
                ),
            )
            .await?;
    } else {
        // Plain HTTP server (no redirect needed)
        info!("Serving HTTP API on: {addr}");
        let normalized_router = NormalizePathLayer::trim_trailing_slash().layer(base_router);
        axum_server::from_tcp(listener.into_std()?)?
            .handle(handle)
            .serve(ServiceExt::<Request>::into_make_service(normalized_router))
            .await?;
    }
    Ok(())
}

#[allow(clippy::default_trait_access)]
fn api_docs(api: TransformOpenApi) -> TransformOpenApi {
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
        .version(VERSION)
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
            name: "alert".to_string(),
            description: Some("Alerts".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "sse".to_string(),
            description: Some("Server Side Events".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "plugins".to_string(),
            description: Some("Plugins".to_string()),
            ..Tag::default()
        })
}

fn create_app_state<'s>(
    all_devices: AllDevices,
    repos: Repos,
    engine: &Rc<Engine>,
    config: Rc<Config>,
    custom_sensors_repo: &Rc<CustomSensorsRepo>,
    modes_controller: &Rc<ModeController>,
    alert_controller: &Rc<AlertController>,
    plugin_controller: Rc<PluginController>,
    log_buf_handle: LogBufHandle,
    status_handle: StatusHandle,
    cancel_token: &CancellationToken,
    main_scope: &'s Scope<'s, 's, Result<()>>,
) -> AppState {
    let health = HealthHandle::new(repos, cancel_token.clone(), main_scope);
    let auth_handle = AuthHandle::new(cancel_token.clone(), main_scope);
    let device_handle = DeviceHandle::new(
        all_devices.clone(),
        engine.clone(),
        modes_controller.clone(),
        config.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let profile_handle = ProfileHandle::new(
        all_devices.clone(),
        engine.clone(),
        config.clone(),
        modes_controller.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let function_handle = FunctionHandle::new(
        engine.clone(),
        config.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let custom_sensor_handle = CustomSensorHandle::new(
        custom_sensors_repo.clone(),
        engine.clone(),
        config.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let mode_handle = ModeHandle::new(modes_controller.clone(), cancel_token.clone(), main_scope);
    let setting_handle = SettingHandle::new(all_devices, config, cancel_token.clone(), main_scope);
    let alert_handle = AlertHandle::new(alert_controller.clone(), cancel_token.clone(), main_scope);
    let plugin_handle = PluginHandle::new(plugin_controller, cancel_token.clone(), main_scope);
    AppState {
        health,
        auth_handle,
        device_handle,
        status_handle,
        profile_handle,
        function_handle,
        custom_sensor_handle,
        mode_handle,
        setting_handle,
        alert_handle,
        plugin_handle,
        log_buf_handle,
    }
}

fn optional_layers(
    compression_layer: Option<(CompressionLayer, DecompressionLayer)>,
    router: Router,
) -> Router {
    if let Some(layer) = compression_layer {
        router.layer(layer.0).layer(layer.1)
    } else {
        router
    }
}

fn cors_layer(
    ipv4: Option<SocketAddrV4>,
    ipv6: Option<SocketAddrV6>,
    custom_origins: Vec<String>,
) -> cors::CorsLayer {
    // Allowed hosts for localhost and bound IPs (matched against parsed origin host)
    let mut allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "[::1]".to_string(),
    ];
    if let Some(addr) = ipv4 {
        allowed_hosts.push(addr.ip().to_string());
    }
    if let Some(addr) = ipv6 {
        allowed_hosts.push(format!("[{}]", addr.ip()));
    }

    // Custom origins from settings - exact match only
    let exact_origins: Vec<String> = custom_origins
        .into_iter()
        .filter(|o| o.starts_with("http://") || o.starts_with("https://"))
        .collect();

    cors::CorsLayer::new()
        .allow_credentials(true)
        .allow_headers(cors::AllowHeaders::mirror_request())
        .allow_methods(cors::AllowMethods::mirror_request())
        .allow_origin(cors::AllowOrigin::predicate(
            move |origin: &HeaderValue, _req: &Parts| {
                origin
                    .to_str()
                    .is_ok_and(|s| is_origin_allowed(s, &allowed_hosts, &exact_origins))
            },
        ))
        .max_age(Duration::from_secs(60) * 5)
}

/// Checks if an origin is allowed based on allowed hosts and exact origins.
fn is_origin_allowed(origin: &str, allowed_hosts: &[String], exact_origins: &[String]) -> bool {
    // Check exact custom origins first (e.g., "https://coolercontrol.example.com")
    if exact_origins.iter().any(|o| origin == o) {
        return true;
    }

    // Parse origin and check host against allowed hosts
    // Origin format: scheme://host[:port]
    let Some(host_start) = origin.find("://") else {
        return false;
    };
    let after_scheme = &origin[host_start + 3..];

    // Extract host, handling IPv6 addresses in brackets
    let host = if after_scheme.starts_with('[') {
        // IPv6: find closing bracket, host includes brackets
        after_scheme
            .find(']')
            .map_or(after_scheme, |i| &after_scheme[..=i])
    } else {
        // IPv4 or hostname: everything before the port
        after_scheme.split(':').next().unwrap_or(after_scheme)
    };

    allowed_hosts.iter().any(|h| h == host)
}

/// TLS Configuration
async fn tls_config(settings: &CoolerControlSettings) -> Option<RustlsConfig> {
    let tls_enabled = env::var(ENV_TLS)
        .ok()
        .and_then(|env_tls| {
            env_tls
                .parse::<u8>()
                .ok()
                .map(|bb| bb != 0)
                .or_else(|| Some(env_tls.trim().to_lowercase() != "off"))
        })
        .unwrap_or(settings.tls_enabled);
    if tls_enabled.not() {
        return None;
    }
    let cert_path = get_tls_path(ENV_CERT_PATH, settings.tls_cert_path.as_ref());
    let key_path = get_tls_path(ENV_KEY_PATH, settings.tls_key_path.as_ref());
    let (cert_path, key_path) = tls::ensure_certificates(cert_path, key_path)
        .await
        .inspect_err(|err| {
            warn!("Failed to ensure TLS certificates: {err}");
        })
        .ok()?;
    match RustlsConfig::from_pem_file(&cert_path, &key_path).await {
        Ok(tls_config) => Some(tls_config),
        Err(err) => {
            warn!("Failed to load TLS certificates: {err}");
            None
        }
    }
}

fn get_tls_path(env_var: &str, settings_opt: Option<&String>) -> Option<String> {
    let mut tls_path = env::var(env_var)
        .ok()
        .map(|c| c.trim().to_string())
        .filter(|c_path| c_path.is_empty().not());
    if tls_path.is_none() {
        // settings path is already sanitized
        tls_path = settings_opt.cloned();
    }
    tls_path
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
    match env::var(ENV_HOST_IP4) {
        Ok(env_ipv4) => {
            if env_ipv4.trim().is_empty() {
                return Err(anyhow!("IPv4 address disabled"));
            }
            is_free_tcp_ipv4(Some(&env_ipv4), port).await
        }
        Err(_) => {
            // get from config
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
    }
}

async fn determine_ipv6_address(config: &Rc<Config>, port: u16) -> Result<SocketAddrV6> {
    match env::var(ENV_HOST_IP6) {
        Ok(env_ipv6) => {
            if env_ipv6.trim().is_empty() {
                return Err(anyhow!("IPv6 address disabled"));
            }
            is_free_tcp_ipv6(Some(&env_ipv6), port).await
        }
        Err(_) => {
            // get from config
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

    #[display("{msg}")]
    TooManyAttempts { msg: String },
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
            CCError::TooManyAttempts { .. } => {
                let err_msg = self.to_string();
                debug!("{err_msg}");
                (StatusCode::TOO_MANY_REQUESTS, err_msg)
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
        _ctx: &mut aide::generate::GenContext,
        _operation: &mut aide::openapi::Operation,
    ) -> Option<aide::openapi::Response> {
        Some(aide::openapi::Response::default())
    }

    fn inferred_responses(
        ctx: &mut aide::generate::GenContext,
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
                    Some(429),
                    aide::openapi::Response {
                        description: "Too Many Requests. Login attempts have been rate limited."
                            .to_owned(),
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
    pub health: HealthHandle,
    pub auth_handle: AuthHandle,
    pub device_handle: DeviceHandle,
    pub status_handle: StatusHandle,
    pub profile_handle: ProfileHandle,
    pub function_handle: FunctionHandle,
    pub custom_sensor_handle: CustomSensorHandle,
    pub mode_handle: ModeHandle,
    pub setting_handle: SettingHandle,
    pub alert_handle: AlertHandle,
    pub plugin_handle: PluginHandle,
    pub log_buf_handle: LogBufHandle,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt as _;

    fn default_allowed_hosts() -> Vec<String> {
        vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "[::1]".to_string(),
        ]
    }

    #[test]
    fn test_origin_localhost_http() {
        let hosts = default_allowed_hosts();
        assert!(is_origin_allowed("http://localhost:11987", &hosts, &[]));
    }

    #[test]
    fn test_origin_localhost_https() {
        let hosts = default_allowed_hosts();
        assert!(is_origin_allowed("https://localhost:11987", &hosts, &[]));
    }

    #[test]
    fn test_origin_localhost_no_port() {
        let hosts = default_allowed_hosts();
        assert!(is_origin_allowed("http://localhost", &hosts, &[]));
    }

    #[test]
    fn test_origin_ipv4_loopback() {
        let hosts = default_allowed_hosts();
        assert!(is_origin_allowed("http://127.0.0.1:11987", &hosts, &[]));
        assert!(is_origin_allowed("https://127.0.0.1:11987", &hosts, &[]));
    }

    #[test]
    fn test_origin_ipv6_loopback() {
        let hosts = default_allowed_hosts();
        assert!(is_origin_allowed("http://[::1]:11987", &hosts, &[]));
        assert!(is_origin_allowed("https://[::1]:11987", &hosts, &[]));
    }

    #[test]
    fn test_origin_bound_ipv4() {
        let mut hosts = default_allowed_hosts();
        hosts.push("192.168.1.100".to_string());
        assert!(is_origin_allowed("http://192.168.1.100:11987", &hosts, &[]));
        assert!(is_origin_allowed(
            "https://192.168.1.100:11987",
            &hosts,
            &[]
        ));
    }

    #[test]
    fn test_origin_bound_ipv6() {
        let mut hosts = default_allowed_hosts();
        hosts.push("[fe80::1]".to_string());
        assert!(is_origin_allowed("http://[fe80::1]:11987", &hosts, &[]));
        assert!(is_origin_allowed("https://[fe80::1]:11987", &hosts, &[]));
    }

    #[test]
    fn test_origin_custom_exact_match() {
        let hosts = default_allowed_hosts();
        let exact = vec!["https://coolercontrol.example.com".to_string()];
        assert!(is_origin_allowed(
            "https://coolercontrol.example.com",
            &hosts,
            &exact
        ));
    }

    #[test]
    fn test_origin_custom_exact_match_with_port() {
        let hosts = default_allowed_hosts();
        let exact = vec!["https://coolercontrol.example.com:8443".to_string()];
        assert!(is_origin_allowed(
            "https://coolercontrol.example.com:8443",
            &hosts,
            &exact
        ));
    }

    #[test]
    fn test_origin_custom_no_partial_match() {
        let hosts = default_allowed_hosts();
        let exact = vec!["https://coolercontrol.example.com".to_string()];
        // Different port should not match exact origin
        assert!(!is_origin_allowed(
            "https://coolercontrol.example.com:8443",
            &hosts,
            &exact
        ));
    }

    #[test]
    fn test_origin_rejects_evil_localhost_subdomain() {
        let hosts = default_allowed_hosts();
        assert!(!is_origin_allowed(
            "https://localhost.evil.com:443",
            &hosts,
            &[]
        ));
    }

    #[test]
    fn test_origin_rejects_evil_with_localhost_path() {
        let hosts = default_allowed_hosts();
        // This was vulnerable with contains() matching
        assert!(!is_origin_allowed(
            "https://evil.com//localhost:fake",
            &hosts,
            &[]
        ));
    }

    #[test]
    fn test_origin_rejects_lookalike_ip() {
        let hosts = default_allowed_hosts();
        assert!(!is_origin_allowed(
            "https://127.0.0.1.evil.com:443",
            &hosts,
            &[]
        ));
    }

    #[test]
    fn test_origin_rejects_arbitrary_domain() {
        let hosts = default_allowed_hosts();
        assert!(!is_origin_allowed("https://evil.com:443", &hosts, &[]));
        assert!(!is_origin_allowed("https://attacker.org", &hosts, &[]));
    }

    #[test]
    fn test_origin_rejects_invalid_format() {
        let hosts = default_allowed_hosts();
        assert!(!is_origin_allowed("not-a-url", &hosts, &[]));
        assert!(!is_origin_allowed("localhost:11987", &hosts, &[]));
        assert!(!is_origin_allowed("//localhost:11987", &hosts, &[]));
    }

    #[test]
    fn test_origin_rejects_empty() {
        let hosts = default_allowed_hosts();
        assert!(!is_origin_allowed("", &hosts, &[]));
    }

    #[test]
    fn test_origin_multiple_custom_origins() {
        let hosts = default_allowed_hosts();
        let exact = vec![
            "https://coolercontrol.home.lan".to_string(),
            "http://192.168.1.1:8080".to_string(),
        ];
        assert!(is_origin_allowed(
            "https://coolercontrol.home.lan",
            &hosts,
            &exact
        ));
        assert!(is_origin_allowed("http://192.168.1.1:8080", &hosts, &exact));
        assert!(!is_origin_allowed("https://other.com", &hosts, &exact));
    }

    // validate_name_string tests
    #[test]
    fn test_validate_name_valid() {
        assert!(validate_name_string("My Profile").is_ok());
        assert!(validate_name_string("profile-1").is_ok());
        assert!(validate_name_string("Test_Name_123").is_ok());
        assert!(validate_name_string("a").is_ok());
        assert!(validate_name_string(&"a".repeat(50)).is_ok());
    }

    #[test]
    fn test_validate_name_empty() {
        let result = validate_name_string("");
        assert!(result.is_err());
        assert!(matches!(result, Err(CCError::UserError { .. })));
    }

    #[test]
    fn test_validate_name_too_long() {
        let result = validate_name_string(&"a".repeat(51));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_name_rejects_tab() {
        assert!(validate_name_string("name\twith\ttabs").is_err());
    }

    #[test]
    fn test_validate_name_rejects_newline() {
        assert!(validate_name_string("name\nwith\nnewlines").is_err());
    }

    #[test]
    fn test_validate_name_rejects_carriage_return() {
        assert!(validate_name_string("name\rwith\rreturns").is_err());
    }

    #[test]
    fn test_validate_name_rejects_null() {
        assert!(validate_name_string("name\0with\0nulls").is_err());
    }

    #[test]
    fn test_validate_name_rejects_vertical_tab() {
        assert!(validate_name_string("name\x0Bwith\x0Bvtabs").is_err());
    }

    #[test]
    fn test_validate_name_rejects_form_feed() {
        assert!(validate_name_string("name\x0Cwith\x0Cfeeds").is_err());
    }

    #[test]
    fn test_validate_name_rejects_escape() {
        assert!(validate_name_string("name\x1Bwith\x1Besc").is_err());
    }

    #[test]
    fn test_validate_name_rejects_delete() {
        assert!(validate_name_string("name\x7Fwith\x7Fdel").is_err());
    }

    #[tokio::test]
    async fn test_security_headers_present() {
        use axum::body::Body;
        use axum::http;
        use axum::routing::get;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn(security_headers_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("x-content-type-options").unwrap(),
            "nosniff"
        );
        assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
        assert_eq!(
            response.headers().get("referrer-policy").unwrap(),
            "strict-origin-when-cross-origin"
        );
        assert_eq!(
            response.headers().get("permissions-policy").unwrap(),
            "camera=(), microphone=(), geolocation=()"
        );
        assert!(response
            .headers()
            .get("strict-transport-security")
            .is_none());
    }

    #[tokio::test]
    async fn test_hsts_header_set_for_https() {
        use axum::body::Body;
        use axum::http;
        use axum::routing::get;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn(security_headers_middleware))
            .layer(Extension(Protocol::Https));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("strict-transport-security").unwrap(),
            "max-age=31536000; includeSubDomains"
        );
    }

    #[tokio::test]
    async fn test_hsts_header_absent_for_http() {
        use axum::body::Body;
        use axum::http;
        use axum::routing::get;

        let app = Router::new()
            .route("/test", get(|| async { "ok" }))
            .layer(middleware::from_fn(security_headers_middleware))
            .layer(Extension(Protocol::Http));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/test")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response
            .headers()
            .get("strict-transport-security")
            .is_none());
    }

    #[test]
    fn test_validate_name_allows_unicode() {
        assert!(validate_name_string("Ünïcödé Nàmé").is_ok());
        assert!(validate_name_string("日本語").is_ok());
        assert!(validate_name_string("émojis 🎉").is_ok());
    }
}
