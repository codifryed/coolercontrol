// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub mod actor;
mod alerts;
mod auth;
mod auth_breaker;
mod auth_throttle;
mod base;
mod calibration;
mod connection;
mod custom_sensors;
mod detect;
mod device_health;
pub mod devices;
mod dual_protocol;
mod functions;
mod hardware_report;
mod metrics;
pub mod modes;
pub mod peer;
mod plugins;
mod power_profiles;
mod profile_generation;
mod profiles;
mod router;
mod session_store;
mod settings;
mod sse;
pub mod stats;
pub mod status;
mod stress_test;
mod tls;
mod tokens;

use crate::alerts::AlertController;
use crate::api::actor::{
    AlertHandle, AuthHandle, CalibrationHandle, CustomSensorHandle, DetectHandle, DeviceHandle,
    DeviceHealthHandle, FunctionHandle, HardwareReportHandle, HealthHandle, ModeHandle,
    PluginHandle, ProfileHandle, SettingHandle, StatsHandle, StatusHandle, StressTestHandle,
    TokenHandle,
};
use crate::api::dual_protocol::Protocol;
use crate::api::session_store::{FileSessionStore, MemorySessionStore};
use crate::config::Config;
use crate::device_health::DeviceHealthController;
use crate::engine::main::Engine;
use crate::logger::LogBufHandle;
use crate::modes::ModeController;
use crate::overrides::OverridesController;
use crate::paths;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::repositories::service_plugin::plugin_controller::PluginController;
use crate::setting::CoolerControlSettings;
use crate::{admin, cc_fs};
use crate::{
    AllDevices, Repos, ENV_CERT_PATH, ENV_HOST_IP4, ENV_HOST_IP6, ENV_KEY_PATH, ENV_PORT, ENV_TLS,
    VERSION,
};
use aide::openapi::{ApiKeyLocation, Contact, License, OpenApi, SecurityScheme, Tag};
use aide::transform::TransformOpenApi;
use aide::OperationOutput;
use anyhow::{anyhow, Result};
use axum::extract::multipart::MultipartError;
use axum::extract::rejection::JsonRejection;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::header::{HeaderName, HeaderValue};
use axum::http::request::Parts;
use axum::http::StatusCode;
use axum::middleware;
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json, Router, ServiceExt};
use axum_server::accept::DefaultAcceptor;
use axum_server::tls_rustls::RustlsConfig;
use log::{debug, info, warn, Level};
use moro_local::Scope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::env;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::ops::Not;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;
use tower::Layer;
use tower_http::compression::predicate::{And, DefaultPredicate, Predicate};
use tower_http::compression::CompressionLayer;
use tower_http::cors;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::normalize_path::NormalizePathLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::TraceLayer;
use tower_sessions::cookie::SameSite;
use tower_sessions::service::PrivateCookie;
use tower_sessions::session::Id;
use tower_sessions::{
    CachingSessionStore, ExpiredDeletion, Expiry, SessionManagerLayer, SessionStore,
};

const API_SERVER_PORT_DEFAULT: Port = 11987;
const SESSION_COOKIE_NAME: &str = "cc";
const API_TIMEOUT_SECS: u64 = 30;
const API_SHUTDOWN_TIMEOUT_SECS: u64 = 5;
const PASSWD_WATCH_INTERVAL_SECS: u64 = 5;
const SESSION_COOKIE_EXPIRATION: time::Duration = time::Duration::days(365);

type Port = u16;
type SessionStoreType = CachingSessionStore<MemorySessionStore, FileSessionStore>;

#[allow(clippy::too_many_lines)]
pub async fn start_server<'s>(
    all_devices: AllDevices,
    repos: Repos,
    engine: Rc<Engine>,
    config: Rc<Config>,
    custom_sensors_repo: Rc<CustomSensorsRepo>,
    modes_controller: Rc<ModeController>,
    alert_controller: Rc<AlertController>,
    device_health_controller: Rc<DeviceHealthController>,
    hardware_support: Rc<crate::hardware_support::HardwareSupportController>,
    overrides_controller: Rc<OverridesController>,
    plugin_controller: Rc<PluginController>,
    log_buf_handle: LogBufHandle,
    status_handle: StatusHandle,
    notification_handle: crate::notifier::NotificationHandle,
    system_event_handle: crate::system_event::SystemEventHandle,
    power_profiles: crate::power_profile_listener::PowerProfiles,
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
    let (ipv4, ipv6) = resolve_server_addresses(&config, rest_port);

    let settings = config.get_settings()?;
    let compression_layer = settings.compress.then(api_compression_layer);
    let app_state = create_app_state(
        all_devices,
        repos,
        &engine,
        Rc::clone(&config),
        &custom_sensors_repo,
        &modes_controller,
        &alert_controller,
        &device_health_controller,
        hardware_support,
        overrides_controller,
        plugin_controller,
        log_buf_handle,
        status_handle,
        notification_handle,
        system_event_handle,
        power_profiles,
        &cancel_token,
        main_scope,
    )
    .await;
    let tls_config = tls_config(&settings).await;
    // admin uses `sidecar_fs` (always Tokio); this runs during main-thread API init, so dispatch
    // it to the sidecar (no Tokio reactor on the compio main thread).
    let session_key = crate::sidecar::handle()
        .run(admin::load_or_generate_session_key)
        .await??;
    let sessions_dir = paths::sessions_dir().to_path_buf();
    let file_store = FileSessionStore::new(sessions_dir);
    let memory_store = MemorySessionStore::new(10);
    let expired_deletion_store = file_store.clone();
    // Watch the password file for external modification (e.g. CLI
    // `--reset-password` against a running daemon) and clear the
    // in-memory session cache so cached sessions stop working
    // immediately. The on-disk session files are already removed by
    // the CLI's `clear_session_files`; without this, cached sessions
    // would keep authenticating until LRU eviction or expiry.
    main_scope.spawn(watch_passwd_file(
        paths::passwd_file().to_path_buf(),
        memory_store.clone(),
        cancel_token.clone(),
        Duration::from_secs(PASSWD_WATCH_INTERVAL_SECS),
    ));
    let caching_store = CachingSessionStore::new(memory_store, file_store);
    let session_layer = SessionManagerLayer::new(caching_store)
        .with_name(SESSION_COOKIE_NAME)
        .with_private(session_key)
        // unsecure is used for local connections:
        .with_secure(false)
        .with_http_only(true)
        .with_same_site(SameSite::Strict)
        .with_expiry(Expiry::OnInactivity(SESSION_COOKIE_EXPIRATION));

    let trusted_proxies = Arc::new(peer::TrustedProxies::from_config(&settings.trusted_proxies));
    let server_config = ApiServerConfig {
        ipv4,
        ipv6,
        app_state,
        session_layer,
        compression_layer,
        // One limiter for both listeners, so a peer cannot double its share by using both.
        connection_limiter: connection::process_limiter(Arc::clone(&trusted_proxies)),
        trusted_proxies,
        tls_config,
        cancel_token,
        cors_origins: settings.origins.clone(),
        frame_ancestors: settings.frame_ancestors.clone(),
        allow_unencrypted: settings.allow_unencrypted,
        protocol_header: settings.protocol_header.clone(),
    };

    // Run all API servers on the shared sidecar thread. The builder closure captures only `Send`
    // data and is invoked on the sidecar to construct the `!Send` server future.
    crate::sidecar::handle()
        .spawn(move || run_all_api_servers(server_config, expired_deletion_store));
    Ok(())
}

/// What every listener's server is built from. Both listeners share one of each.
#[derive(Clone)]
struct ApiServerConfig {
    ipv4: Option<SocketAddrV4>,
    ipv6: Option<SocketAddrV6>,
    app_state: AppState,
    session_layer: SessionManagerLayer<SessionStoreType, PrivateCookie>,
    compression_layer: Option<ApiCompressionLayer>,
    connection_limiter: Arc<connection::ConnectionLimiter>,
    trusted_proxies: Arc<peer::TrustedProxies>,
    tls_config: Option<RustlsConfig>,
    cancel_token: CancellationToken,
    cors_origins: Vec<String>,
    frame_ancestors: Vec<String>,
    allow_unencrypted: bool,
    protocol_header: Option<String>,
}

async fn run_all_api_servers(config: ApiServerConfig, expired_deletion_store: FileSessionStore) {
    let mut handles = Vec::with_capacity(2);

    // Periodically clean up expired session files
    tokio::task::spawn_local(
        expired_deletion_store.continuously_delete_expired(Duration::from_secs(3600)),
    );

    // REST API servers
    if let Some(addr) = config.ipv4 {
        let server = create_api_server(SocketAddr::from(addr), config.clone());
        handles.push(tokio::task::spawn_local(server));
    }
    if let Some(addr) = config.ipv6 {
        let server = create_api_server(SocketAddr::from(addr), config);
        handles.push(tokio::task::spawn_local(server));
    }

    // Wait for all servers (they run until canceled)
    for handle in handles {
        if let Err(e) = handle.await {
            log::error!("API server task error: {e}");
        }
    }
}

/// Polls the password file's mtime and clears the in-memory session
/// cache on change. The CLI `--reset-password` runs in a separate
/// process that cannot reach the running daemon's `MemorySessionStore`,
/// so this watcher closes the gap between disk-side cleanup
/// (`clear_session_files`) and in-memory cache invalidation.
///
/// Lives on the main thread runtime alongside `main_loop` to keep
/// wakeups coalesced. Shuts down with the rest of the structured
/// concurrency tree via `cancel_token`.
async fn watch_passwd_file(
    passwd_file: std::path::PathBuf,
    memory_store: MemorySessionStore,
    cancel_token: CancellationToken,
    interval: Duration,
) {
    let mut last_mtime = passwd_file_mtime(&passwd_file);
    while cancel_token.is_cancelled().not() {
        tokio::select! {
            biased;
            () = cancel_token.cancelled() => break,
            () = crate::rt::sleep(interval) => {
                let current_mtime = passwd_file_mtime(&passwd_file);
                if current_mtime == last_mtime {
                    continue;
                }
                last_mtime = current_mtime;
                // `MemorySessionStore::delete` clears all entries by
                // design (single-user system); the Id argument is
                // unused. Errors here are unreachable because the
                // implementation only locks a sync mutex and clears
                // the map.
                let _ = SessionStore::delete(&memory_store, &Id::default()).await;
                info!("Password changed externally, cleared session cache.");
            }
        }
    }
}

fn passwd_file_mtime(path: &Path) -> Option<SystemTime> {
    cc_fs::metadata(path).ok().and_then(|m| m.modified().ok())
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
        HeaderValue::from_static("SAMEORIGIN"),
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

fn hostname_is_valid(hostname: &str) -> bool {
    if hostname.is_empty() || hostname.len() > 253 {
        return false;
    }
    hostname.split('.').all(|label| {
        if label.is_empty() || label.len() > 63 {
            return false;
        }
        if label.starts_with('-') {
            return false;
        }
        if label.ends_with('-') {
            return false;
        }
        label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    })
}

struct CspFrameAncestorParts<'a> {
    /// "http://", "https://", etc.
    scheme: &'a str,
    host: &'a str,
    /// port, not including ':'
    port: Option<&'a str>,
    /// path after host (and port), not including leading '/'
    path: Option<&'a str>,
}

fn csp_frame_ancestor_parse(ancestor: &str) -> Option<CspFrameAncestorParts<'_>> {
    let (scheme, after_scheme) = ancestor.split_once("://")?;
    let (origin, path) = after_scheme
        .split_once('/')
        .map_or((after_scheme, None), |(origin, path)| (origin, Some(path)));
    let (host, port) = (if origin.starts_with('[') {
        origin.find(']').and_then(|host_end_index| {
            let host = &origin[0..=host_end_index];
            let after_host = &origin[host_end_index + 1..];
            match after_host {
                s if s.starts_with(':') => Some((host, after_host.strip_prefix(':'))),
                "" => Some((host, None)),
                _ => None,
            }
        })
    } else {
        Some(
            origin
                .split_once(':')
                .map_or((origin, None), |(host, port)| (host, Some(port))),
        )
    })?;
    Some(CspFrameAncestorParts {
        scheme,
        host,
        port,
        path,
    })
}

fn csp_frame_ancestor_is_valid(ancestor: &str) -> bool {
    let Some(parsed) = csp_frame_ancestor_parse(ancestor) else {
        return false;
    };
    // validate scheme
    if ["http", "https"].contains(&parsed.scheme).not() {
        return false;
    }
    // validate host
    let host_valid = parsed
        .host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .map_or_else(
            || parsed.host.parse::<Ipv4Addr>().is_ok() || hostname_is_valid(parsed.host),
            |ipv6addr| ipv6addr.parse::<Ipv6Addr>().is_ok(),
        );
    if host_valid.not() {
        return false;
    }
    // validate port
    if let Some(port) = parsed.port {
        if port.parse::<u16>().is_err() {
            return false;
        }
    }
    // validate path
    if let Some(path) = parsed.path {
        // only allow no path or bare /
        if path.is_empty().not() {
            return false;
        }
    }
    true
}

fn csp_frame_ancestors_value(frame_ancestors: Vec<String>) -> String {
    let exact_frame_ancestors: Vec<String> = frame_ancestors
        .into_iter()
        .filter(|a: &String| csp_frame_ancestor_is_valid(a))
        .collect();
    if exact_frame_ancestors.is_empty() {
        "'none'".to_string()
    } else {
        exact_frame_ancestors.join(" ")
    }
}

async fn csp_frame_ancestors_middleware(
    State(frame_ancestors): State<String>,
    req: Request,
    next: middleware::Next,
) -> Response {
    const CSP_KEY: HeaderName = HeaderName::from_static("content-security-policy");
    let mut response = next.run(req).await;
    let headers = response.headers_mut();

    let Some(csp) = headers.get(CSP_KEY) else {
        return response;
    };
    let Ok(csp) = csp.to_str() else {
        return response;
    };
    let Ok(csp) = HeaderValue::try_from(format!("{csp}; frame-ancestors {frame_ancestors}")) else {
        return response;
    };
    headers.insert(CSP_KEY, csp);
    response
}

async fn create_api_server(addr: SocketAddr, config: ApiServerConfig) -> Result<()> {
    let router = api_router(config.app_state, config.trusted_proxies).await;
    let base_router = with_base_layers(
        router,
        config.compression_layer,
        config.session_layer,
        cors_layer(config.ipv4, config.ipv6, config.cors_origins),
        csp_frame_ancestors_value(config.frame_ancestors),
    );
    let connection_limiter = config.connection_limiter;

    let listener = TcpListener::bind(addr).await?;
    let handle = shutdown_handle(config.cancel_token);

    if let Some(tls) = config.tls_config {
        // Dual-protocol server: accepts both HTTP and HTTPS on the same port
        // HTTP requests from non-localhost are redirected to HTTPS (via middleware)
        // HTTP requests from localhost and to /health are allowed
        info!("Serving HTTP, HTTPS and gRPC API on {addr}");

        // Add HTTPS redirect layer for non-localhost HTTP requests
        let redirect_layer = dual_protocol::HttpsRedirectLayer {
            port: addr.port(),
            allow_unencrypted: config.allow_unencrypted,
            protocol_header: config.protocol_header,
        };
        let router_with_redirect = base_router.layer(redirect_layer);
        let normalized_router =
            NormalizePathLayer::trim_trailing_slash().layer(router_with_redirect);

        let acceptor = dual_protocol::DualProtocolAcceptor::new(tls);
        connection::server(listener.into_std()?, acceptor, connection_limiter)?
            .handle(handle)
            .serve(
                ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(
                    normalized_router,
                ),
            )
            .await?;
    } else {
        // Plain HTTP server (no redirect needed)
        info!("Serving HTTP and gRPC API on {addr}");
        let normalized_router = NormalizePathLayer::trim_trailing_slash().layer(base_router);
        // Connect info matches the TLS path above: the auth throttle keys on the peer
        // address, and without this it would have nothing to key on in the default
        // (TLS-disabled) configuration.
        connection::server(
            listener.into_std()?,
            DefaultAcceptor::new(),
            connection_limiter,
        )?
        .handle(handle)
        .serve(
            ServiceExt::<Request>::into_make_service_with_connect_info::<SocketAddr>(
                normalized_router,
            ),
        )
        .await?;
    }
    Ok(())
}

/// A server handle that shuts down gracefully once `cancel_token` fires.
fn shutdown_handle(cancel_token: CancellationToken) -> axum_server::Handle<SocketAddr> {
    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();
    tokio::task::spawn_local(async move {
        cancel_token.cancelled().await;
        shutdown_handle.graceful_shutdown(Some(Duration::from_secs(API_SHUTDOWN_TIMEOUT_SECS)));
    });
    handle
}

/// The API routes, with the client address resolved before anything reads it.
async fn api_router(app_state: AppState, trusted_proxies: Arc<peer::TrustedProxies>) -> Router {
    aide::generate::on_error(|error| {
        debug!("OpenApi Generation Error: {error}");
    });
    let mut open_api = OpenApi::default();
    let router = router::init(app_state)
        .await
        .finish_api_with(&mut open_api, api_docs)
        .layer(Extension(Arc::new(open_api)));
    with_client_addr(router, trusted_proxies)
}

/// Outside the router, so the client is known before its auth throttles read it.
fn with_client_addr(router: Router, trusted_proxies: Arc<peer::TrustedProxies>) -> Router {
    router.layer(middleware::from_fn_with_state(
        trusted_proxies,
        peer::client_addr_middleware,
    ))
}

/// The layers every listener shares.
///
/// Layers are processed bottom to top: (last is first in the chain)
/// See: <https://docs.rs/axum/latest/axum/middleware/index.html#ordering>
fn with_base_layers(
    router: Router,
    compression_layer: Option<ApiCompressionLayer>,
    session_layer: SessionManagerLayer<SessionStoreType, PrivateCookie>,
    cors_layer: cors::CorsLayer,
    frame_ancestors: String,
) -> Router {
    optional_layers(compression_layer, router)
        // Limits the size of the payload in bytes: (Max 50MB for image files)
        .route_layer(RequestBodyLimitLayer::new(50 * 1024 * 1024))
        // 2MB is the default payload limit:
        .route_layer(DefaultBodyLimit::disable())
        .route_layer(session_layer)
        .layer(cors_layer)
        .layer(middleware::from_fn(security_headers_middleware))
        .layer((
            TraceLayer::new_for_http(),
            TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                Duration::from_secs(API_TIMEOUT_SECS),
            ),
        ))
        .layer(middleware::from_fn_with_state(
            frame_ancestors,
            csp_frame_ancestors_middleware,
        ))
        // Outermost of everything, so the timeout's 408 and the throttle's 429 both reach
        // gRPC clients as statuses they can read. Inside the timeout layer it would never
        // see a 408 at all.
        .layer(middleware::from_fn(router::grpc_error_middleware))
}

/// The `OpenAPI` spec, built straight from the route table. Needs no `AppState`, no hardware and
/// no listening socket, because aide collects its metadata when a route is declared and never
/// touches the state value. This is what `coolercontrold openapi` prints and what the freshness
/// test compares `openapi/openapi.json` against.
pub fn openapi_spec() -> OpenApi {
    let mut open_api = OpenApi::default();
    let _ = router::documented_routes().finish_api_with(&mut open_api, api_docs);
    open_api
}

#[allow(clippy::default_trait_access, clippy::too_many_lines)]
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
        .security_scheme(
            "BearerAuth",
            SecurityScheme::Http {
                scheme: "bearer".to_string(),
                bearer_format: Some("cc_<uuid>".to_string()),
                description: Some(
                    "Bearer token authentication for external services.".to_string(),
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
            name: "stats".to_string(),
            description: Some("Running Channel Stats".to_string()),
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
            name: "calibration".to_string(),
            description: Some("Channel Calibration".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "detect".to_string(),
            description: Some("Hardware Detection".to_string()),
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
        .tag(Tag {
            name: "stress-test".to_string(),
            description: Some("Stress Testing".to_string()),
            ..Tag::default()
        })
        .tag(Tag {
            name: "metrics".to_string(),
            description: Some("Prometheus Metrics".to_string()),
            ..Tag::default()
        })
}

// The `stats_handle` local reads as similar to the `status_handle` param, but
// stats (metrics actor) and status (SSE) are distinct domains; names deliberate.
#[allow(clippy::similar_names)]
async fn create_app_state<'s>(
    all_devices: AllDevices,
    repos: Repos,
    engine: &Rc<Engine>,
    config: Rc<Config>,
    custom_sensors_repo: &Rc<CustomSensorsRepo>,
    modes_controller: &Rc<ModeController>,
    alert_controller: &Rc<AlertController>,
    device_health_controller: &Rc<DeviceHealthController>,
    hardware_support: Rc<crate::hardware_support::HardwareSupportController>,
    overrides_controller: Rc<OverridesController>,
    plugin_controller: Rc<PluginController>,
    log_buf_handle: LogBufHandle,
    status_handle: StatusHandle,
    notification_handle: crate::notifier::NotificationHandle,
    system_event_handle: crate::system_event::SystemEventHandle,
    power_profiles: crate::power_profile_listener::PowerProfiles,
    cancel_token: &CancellationToken,
    main_scope: &'s Scope<'s, 's, Result<()>>,
) -> AppState {
    let health = HealthHandle::new(repos, cancel_token.clone(), main_scope);
    let detect_handle = DetectHandle::new(
        paths::detect_override_file().to_path_buf(),
        cancel_token.clone(),
        main_scope,
    );
    let hardware_report_handle =
        HardwareReportHandle::new(hardware_support, cancel_token.clone(), main_scope);
    let auth_handle = AuthHandle::new(cancel_token.clone());
    let token_handle = TokenHandle::new(cancel_token.clone()).await;
    let device_handle = DeviceHandle::new(
        all_devices.clone(),
        engine.clone(),
        modes_controller.clone(),
        config.clone(),
        overrides_controller.clone(),
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
        overrides_controller.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let mode_handle = ModeHandle::new(modes_controller.clone(), cancel_token.clone(), main_scope);
    let stats_handle = StatsHandle::new(all_devices.clone(), cancel_token.clone(), main_scope);
    let setting_handle = SettingHandle::new(
        all_devices,
        config,
        overrides_controller,
        cancel_token.clone(),
        main_scope,
    );
    let alert_handle = AlertHandle::new(alert_controller.clone(), cancel_token.clone(), main_scope);
    let device_health_handle = DeviceHealthHandle::new(
        device_health_controller.clone(),
        cancel_token.clone(),
        main_scope,
    );
    let calibration_handle =
        CalibrationHandle::new(engine.clone(), cancel_token.clone(), main_scope);
    let plugin_handle = PluginHandle::new(plugin_controller, cancel_token.clone(), main_scope);
    let stress_test_handle = StressTestHandle::new(cancel_token.clone()).await;
    AppState {
        health,
        detect_handle,
        hardware_report_handle,
        auth_handle,
        token_handle,
        device_handle,
        status_handle,
        stats_handle,
        profile_handle,
        function_handle,
        custom_sensor_handle,
        mode_handle,
        setting_handle,
        alert_handle,
        device_health_handle,
        calibration_handle,
        plugin_handle,
        stress_test_handle,
        log_buf_handle,
        notification_handle,
        system_event_handle,
        power_profiles,
    }
}

/// The real app state over no devices and the default config, for tests of the real routes.
/// Its actors run in `main_scope` until `cancel_token` is cancelled.
#[cfg(test)]
async fn empty_app_state<'s>(
    cancel_token: &CancellationToken,
    main_scope: &'s Scope<'s, 's, Result<()>>,
) -> AppState {
    use crate::calibration::{CalibrationStore, FanStateMap};
    crate::sidecar::ensure_test_handle();
    let config = Rc::new(Config::init_default_config().unwrap());
    let all_devices: AllDevices = Rc::new(std::collections::HashMap::new());
    let repos: Repos = Rc::default();
    let overrides = Rc::new(OverridesController::empty());
    let engine = Rc::new(Engine::new(
        Rc::clone(&all_devices),
        &repos,
        Rc::clone(&config),
        Rc::new(CalibrationStore::empty()),
        Rc::new(FanStateMap::new()),
        Rc::clone(&overrides),
    ));
    let power_profiles = crate::power_profile_listener::PowerProfiles::default();
    // Callers hold the `modes_file` lock; a prior modes test may leave the file unparseable.
    if let Err(err) = std::fs::remove_file(paths::mode_config_file()) {
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound, "{err}");
    }
    let modes = ModeController::init(
        Rc::clone(&config),
        Rc::clone(&all_devices),
        Rc::clone(&engine),
        power_profiles.clone(),
    );
    let modes = Rc::new(modes.await.unwrap());
    let alerts = AlertController::init(
        Rc::clone(&all_devices),
        Rc::clone(&overrides),
        engine.diagnosis_registry(),
    );
    let alerts = Rc::new(alerts.await.unwrap());
    let device_health = Rc::new(DeviceHealthController::new(
        Rc::clone(&all_devices),
        Rc::clone(&config),
        Rc::clone(&repos),
        Rc::clone(&overrides),
    ));
    let custom_sensors =
        CustomSensorsRepo::new(Rc::clone(&config), Vec::new(), Rc::clone(&overrides));
    let hardware_support = crate::hardware_support::HardwareSupportController::init(None, false);
    let status_handle =
        StatusHandle::new(Rc::clone(&all_devices), cancel_token.clone(), main_scope);
    create_app_state(
        all_devices,
        repos,
        &engine,
        config,
        &Rc::new(custom_sensors.unwrap()),
        &modes,
        &alerts,
        &device_health,
        Rc::new(hardware_support.await),
        overrides,
        Rc::new(PluginController::new_disabled()),
        LogBufHandle::new(cancel_token.clone()),
        status_handle,
        crate::notifier::NotificationHandle::new(cancel_token.clone()),
        crate::system_event::SystemEventHandle::new(cancel_token.clone()),
        power_profiles,
        cancel_token,
        main_scope,
    )
    .await
}

fn optional_layers(compression_layer: Option<ApiCompressionLayer>, router: Router) -> Router {
    if let Some(layer) = compression_layer {
        router.layer(layer)
    } else {
        router
    }
}

type ApiCompressionLayer = CompressionLayer<And<DefaultPredicate, NotStaticAsset>>;

/// Response compression for everything except the embedded web app. See `base::StaticAsset`.
fn api_compression_layer() -> ApiCompressionLayer {
    CompressionLayer::new().compress_when(DefaultPredicate::new().and(NotStaticAsset))
}

#[derive(Debug, Clone, Copy)]
struct NotStaticAsset;

impl Predicate for NotStaticAsset {
    fn should_compress<B>(&self, response: &Response<B>) -> bool
    where
        B: axum::body::HttpBody,
    {
        response.extensions().get::<base::StaticAsset>().is_none()
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
    // Announce the fingerprint a remote daemon will pin. On a headless server this log
    // line is the only way a user can check a pin against the certificate actually served.
    if let Some(fingerprint) = tls::certificate_fingerprint(&cert_path).await {
        info!("TLS certificate fingerprint (SHA-256): {fingerprint}");
        tls::set_served_fingerprint(fingerprint);
    }
    // `RustlsConfig::from_pem_file` reads the cert/key via `tokio::fs` and parses via
    // `spawn_blocking`, both of which need a Tokio reactor. This runs during main-thread API init
    // (no reactor on the compio main thread), so load it on the sidecar. Harmless on Tokio too.
    match crate::sidecar::handle()
        .run(move || RustlsConfig::from_pem_file(cert_path, key_path))
        .await
    {
        Ok(Ok(tls_config)) => Some(tls_config),
        Ok(Err(err)) => {
            warn!("Failed to load TLS certificates: {err}");
            None
        }
        Err(err) => {
            warn!("Sidecar dispatch for TLS load failed: {err}");
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

/// Validates a user-supplied name. Names are logged, persisted, and
/// displayed by multiple clients, so every character that enables log,
/// terminal, or display injection is rejected outright rather than
/// escaped downstream.
pub fn validate_name_string(name: &str) -> Result<(), CCError> {
    if name.is_empty() {
        return Err(CCError::UserError {
            msg: "name cannot be empty".to_string(),
        });
    }
    if name.len() > 50 {
        return Err(CCError::UserError {
            msg: "name cannot be longer than 50 characters".to_string(),
        });
    }
    if let Some(c) = name.chars().find(|c| is_forbidden_name_char(*c)) {
        return Err(CCError::UserError {
            msg: format!("name cannot contain the control or formatting character {c:?}"),
        });
    }
    Ok(())
}

/// Control characters (C0, DEL, C1) enable log and terminal injection.
/// The Unicode line and paragraph separators break line-oriented and JS
/// string contexts. The bidirectional control characters enable visual
/// spoofing of displayed and logged names.
pub fn is_forbidden_name_char(c: char) -> bool {
    if c.is_control() {
        return true;
    }
    if c == '\u{2028}' || c == '\u{2029}' {
        return true;
    }
    ('\u{202A}'..='\u{202E}').contains(&c) || ('\u{2066}'..='\u{2069}').contains(&c)
}

/// Probes whether `addrs` can be bound. Uses a synchronous std bind so it needs no reactor: it runs
/// on the main thread (which may be compio) during API init, before the server moves to the sidecar.
/// The actual server listener is bound on the sidecar (see `create_api_server`).
fn can_bind_tcp<A: std::net::ToSocketAddrs>(addrs: A) -> bool {
    std::net::TcpListener::bind(addrs).is_ok()
}

fn is_free_tcp_ipv4(address: Option<&str>, port: Port) -> Result<SocketAddrV4> {
    if let Some(addr) = address {
        match addr.parse::<Ipv4Addr>() {
            Ok(ipv4_addr) => {
                let ipv4 = SocketAddrV4::new(ipv4_addr, port);
                if can_bind_tcp(ipv4) {
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
        if can_bind_tcp(ipv4) {
            Ok(ipv4)
        } else {
            Err(anyhow!(
                "Could not bind to standard IPv4 loopback address on port {port}"
            ))
        }
    }
}

fn is_free_tcp_ipv6(address: Option<&str>, port: Port) -> Result<SocketAddrV6> {
    if let Some(addr) = address {
        match addr.parse::<Ipv6Addr>() {
            Ok(ipv6_addr) => {
                let ipv6 = SocketAddrV6::new(ipv6_addr, port, 0, 0);
                if can_bind_tcp(ipv6) {
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
        if can_bind_tcp(ipv6) {
            Ok(ipv6)
        } else {
            Err(anyhow!(
                "Could not bind to standard IPv6 loopback address on port {port}"
            ))
        }
    }
}

/// How an address family was configured. An empty value is a deliberate opt-out, not a failure.
#[derive(Debug, PartialEq, Eq)]
enum AddressSetting<'a> {
    /// Nothing configured. Falls back to the loopback address.
    Unset,
    /// Empty value. This address family is turned off.
    Disabled,
    Set(&'a str),
}

fn address_setting(configured: Option<&str>) -> AddressSetting<'_> {
    match configured {
        None => AddressSetting::Unset,
        Some(address) if address.trim().is_empty() => AddressSetting::Disabled,
        Some(address) => AddressSetting::Set(address),
    }
}

/// What to log about a resolution outcome, if anything. A disabled family is normal
/// operation and only informational: warning about it looks like a malfunction.
fn bind_outcome_log<A>(outcome: &Result<Option<A>>, family: &str) -> Option<(Level, String)> {
    match outcome {
        Ok(Some(_)) => None,
        Ok(None) => Some((Level::Info, format!("{family} address disabled"))),
        Err(err) => Some((Level::Warn, format!("{family} bind error: {err}"))),
    }
}

/// Logs how an address family resolved and reduces it to the address to bind, if any.
fn log_bind_outcome<A>(outcome: Result<Option<A>>, family: &str) -> Option<A> {
    if let Some((level, message)) = bind_outcome_log(&outcome, family) {
        log::log!(level, "{message}");
    }
    outcome.ok().flatten()
}

/// Stable log text: users grep these lines. gRPC shares the REST listener, so there is
/// one socket to bind and one failure to report; neither family failing stops the daemon,
/// whose fan control needs no socket at all.
const API_SERVER_NAME: &str = "REST API";
const API_SERVER_CONSEQUENCE: &str = "No API, UI, or gRPC connection available.";

/// What to log when the server ends up with nothing to listen on. Both families switched
/// off is a deliberate opt-out; anything else means we tried to bind and could not.
fn unavailable_log<A4, A6>(
    ipv4: &Result<Option<A4>>,
    ipv6: &Result<Option<A6>>,
) -> Option<(Level, String)> {
    if matches!(ipv4, Ok(Some(_))) || matches!(ipv6, Ok(Some(_))) {
        return None;
    }
    if matches!(ipv4, Ok(None)) && matches!(ipv6, Ok(None)) {
        return Some((
            Level::Info,
            format!("{API_SERVER_NAME} disabled. {API_SERVER_CONSEQUENCE}"),
        ));
    }
    Some((
        Level::Error,
        format!("Could not bind {API_SERVER_NAME} to any address. {API_SERVER_CONSEQUENCE}"),
    ))
}

/// Resolves and logs both address families for one server. An empty result is not fatal:
/// the caller starts whatever did resolve, so a port conflict on one server cannot take
/// down the other, and the daemon keeps controlling fans either way.
fn resolve_server_addresses(
    config: &Rc<Config>,
    port: Port,
) -> (Option<SocketAddrV4>, Option<SocketAddrV6>) {
    let ipv4_outcome = determine_ipv4_address(config, port);
    let ipv6_outcome = determine_ipv6_address(config, port);
    let unavailable = unavailable_log(&ipv4_outcome, &ipv6_outcome);
    let ipv4 = log_bind_outcome(ipv4_outcome, "IPv4");
    let ipv6 = log_bind_outcome(ipv6_outcome, "IPv6");
    if let Some((level, message)) = unavailable {
        log::log!(level, "{message}");
    }
    (ipv4, ipv6)
}

/// `Ok(None)` means the address family is disabled by configuration.
fn determine_ipv4_address(config: &Rc<Config>, port: Port) -> Result<Option<SocketAddrV4>> {
    let configured = match env::var(ENV_HOST_IP4) {
        Ok(env_ipv4) => Some(env_ipv4),
        Err(_) => config.get_settings()?.ipv4_address,
    };
    match address_setting(configured.as_deref()) {
        AddressSetting::Disabled => Ok(None),
        AddressSetting::Set(address) => is_free_tcp_ipv4(Some(address), port).map(Some),
        AddressSetting::Unset => is_free_tcp_ipv4(None, port).map(Some),
    }
}

/// `Ok(None)` means the address family is disabled by configuration.
fn determine_ipv6_address(config: &Rc<Config>, port: Port) -> Result<Option<SocketAddrV6>> {
    let configured = match env::var(ENV_HOST_IP6) {
        Ok(env_ipv6) => Some(env_ipv6),
        Err(_) => config.get_settings()?.ipv6_address,
    };
    match address_setting(configured.as_deref()) {
        AddressSetting::Disabled => Ok(None),
        AddressSetting::Set(address) => is_free_tcp_ipv6(Some(address), port).map(Some),
        AddressSetting::Unset => is_free_tcp_ipv6(None, port).map(Some),
    }
}

/// How we want errors responses to be serialized
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Serialize, Deserialize, Error, Clone, JsonSchema)]
pub enum CCError {
    #[error("Internal Error: {msg}")]
    InternalError { msg: String },

    #[error("Error with external library: {msg}")]
    ExternalError { msg: String },

    #[error("Resource not found: {msg}")]
    NotFound { msg: String },

    #[error("Conflict: {msg}")]
    Conflict { msg: String },

    #[error("{msg}")]
    UserError { msg: String },

    // #[error("Json serialization error: {}", .0.body_text())]
    // JsonRejection(JsonRejection),
    #[error("{msg}")]
    InvalidCredentials { msg: String },

    #[error("{msg}")]
    InsufficientScope { msg: String },

    #[error("{msg}")]
    TooManyAttempts { msg: String },
}

impl IntoResponse for CCError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            CCError::InternalError { .. } => {
                // We use the Display trait to format our errors
                let err_msg = self.to_string();
                debug!("{err_msg}");
                (StatusCode::INTERNAL_SERVER_ERROR, err_msg)
            }
            CCError::ExternalError { .. } => {
                let err_msg = self.to_string();
                warn!("{err_msg}");
                (StatusCode::BAD_GATEWAY, err_msg)
            }
            CCError::NotFound { .. } => {
                let err_msg = self.to_string();
                debug!("{err_msg}");
                (StatusCode::NOT_FOUND, err_msg)
            }
            CCError::Conflict { .. } => {
                let err_msg = self.to_string();
                debug!("{err_msg}");
                (StatusCode::CONFLICT, err_msg)
            }
            CCError::UserError { .. } => {
                let err_msg = self.to_string();
                debug!("{err_msg}");
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
                debug!("{err_msg}");
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

impl From<MultipartError> for CCError {
    fn from(err: MultipartError) -> Self {
        CCError::UserError {
            msg: format!("Invalid multipart form data: {err}"),
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
    pub detect_handle: DetectHandle,
    pub hardware_report_handle: HardwareReportHandle,
    pub auth_handle: AuthHandle,
    pub token_handle: TokenHandle,
    pub device_handle: DeviceHandle,
    pub status_handle: StatusHandle,
    pub stats_handle: StatsHandle,
    pub profile_handle: ProfileHandle,
    pub function_handle: FunctionHandle,
    pub custom_sensor_handle: CustomSensorHandle,
    pub mode_handle: ModeHandle,
    pub setting_handle: SettingHandle,
    pub alert_handle: AlertHandle,
    pub device_health_handle: DeviceHealthHandle,
    pub calibration_handle: CalibrationHandle,
    pub plugin_handle: PluginHandle,
    pub stress_test_handle: StressTestHandle,
    pub log_buf_handle: LogBufHandle,
    pub notification_handle: crate::notifier::NotificationHandle,
    pub system_event_handle: crate::system_event::SystemEventHandle,
    pub power_profiles: crate::power_profile_listener::PowerProfiles,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower::ServiceExt as _;

    /// Goal: the auth throttles key on the client a trusted proxy forwarded, not on the proxy.
    /// Method: a password route wrapped as `api_router` wraps the real ones, called through a
    /// trusted proxy for two clients. TEST-NET addresses keep the process-wide statics apart,
    /// allotted beside them in `auth_throttle`.
    #[tokio::test]
    async fn auth_throttles_see_the_forwarded_client() {
        use axum::extract::ConnectInfo;
        let proxy = SocketAddr::from(([198, 51, 100, 30], 40000));
        let clients = ["203.0.113.30", "203.0.113.31"];
        let routes = Router::new().route(
            "/login",
            axum::routing::post(|| async { StatusCode::UNAUTHORIZED }).layer(middleware::from_fn(
                auth_throttle::password_throttle_middleware,
            )),
        );
        let trusted = peer::TrustedProxies::from_config(&[proxy.ip().to_string()]);
        let app = with_client_addr(routes, Arc::new(trusted));
        for client in clients {
            let mut request = Request::post("/login")
                .header(axum::http::header::AUTHORIZATION, "Basic Q0NBZG1pbjp4")
                .header("x-forwarded-for", client)
                .body(axum::body::Body::empty())
                .unwrap();
            request.extensions_mut().insert(ConnectInfo(proxy));
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }
        for client in clients {
            let address: std::net::IpAddr = client.parse().unwrap();
            assert_eq!(auth_throttle::password_failures(address), Some(1));
        }
        assert_eq!(auth_throttle::password_failures(proxy.ip()), None);
    }

    /// A JSON body comfortably above the compression predicate's 32 byte floor.
    fn large_json() -> Json<Vec<u32>> {
        Json((0..256).collect())
    }

    fn compressed_app() -> Router {
        let router = Router::new()
            .route("/json", axum::routing::get(|| async { large_json() }))
            .route(
                "/static",
                axum::routing::get(|| async { (Extension(base::StaticAsset), large_json()) }),
            );
        optional_layers(Some(api_compression_layer()), router)
    }

    async fn content_encoding(accept_encoding: Option<&str>, uri: &str) -> Option<String> {
        let mut builder = Request::builder().uri(uri);
        if let Some(value) = accept_encoding {
            builder = builder.header(axum::http::header::ACCEPT_ENCODING, value);
        }
        let response = compressed_app()
            .oneshot(builder.body(axum::body::Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response
            .headers()
            .get(axum::http::header::CONTENT_ENCODING)
            .map(|value| value.to_str().unwrap().to_string())
    }

    /// Goal: a client that accepts gzip receives gzip on the wire. The old stack paired the
    /// compressor with tower-http's client-side decompressor, which undid it on the way out.
    #[tokio::test]
    async fn api_responses_reach_the_client_compressed() {
        assert_eq!(
            content_encoding(Some("gzip"), "/json").await.as_deref(),
            Some("gzip")
        );
    }

    /// Goal: a client that offers no encoding gets identity, so compression stays negotiated
    /// rather than imposed.
    #[tokio::test]
    async fn api_responses_stay_identity_without_accept_encoding() {
        assert_eq!(content_encoding(None, "/json").await, None);
    }

    /// Goal: embedded web app files are never compressed per request, since any client can
    /// fetch them without credentials. Method: a response carrying the `StaticAsset` marker.
    #[tokio::test]
    async fn static_assets_are_not_compressed() {
        assert_eq!(content_encoding(Some("gzip, br"), "/static").await, None);
    }

    /// Repo-root spec, relative to this crate. Absent in vendored/source-tarball builds,
    /// which ship only the crate, so the freshness test skips rather than fails there.
    const SPEC_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../openapi/openapi.json");

    /// Goal: catch a stale checked-in spec, which is a release artifact that is easy to forget
    /// to regenerate. It had already drifted by eight routes before this test existed.
    /// Method: rebuild the spec from the route table and compare it to the file.
    #[test]
    fn checked_in_openapi_spec_is_current() {
        let Ok(checked_in) = std::fs::read_to_string(SPEC_PATH) else {
            return; // Not a full checkout.
        };
        let generated = serde_json::to_string_pretty(&openapi_spec()).expect("the spec serializes");
        if checked_in.trim_end() == generated {
            return;
        }
        let paths = |spec: &str| -> Vec<String> {
            serde_json::from_str::<serde_json::Value>(spec)
                .ok()
                .and_then(|doc| doc.get("paths").cloned())
                .and_then(|paths| paths.as_object().cloned())
                .map(|paths| paths.keys().cloned().collect())
                .unwrap_or_default()
        };
        let (in_file, in_daemon) = (paths(&checked_in), paths(&generated));
        let only_in_daemon: Vec<_> = in_daemon
            .iter()
            .filter(|p| in_file.contains(p).not())
            .collect();
        let only_in_file: Vec<_> = in_file.iter().filter(|p| !in_daemon.contains(p)).collect();
        panic!(
            "openapi/openapi.json is out of date. Regenerate it with `make openapi`.\n\
             routes missing from the file: {only_in_daemon:?}\n\
             routes no longer served: {only_in_file:?}\n\
             (an empty diff here means only schemas or descriptions changed)"
        );
    }

    /// An empty or blank address is the documented way to turn an address family off,
    /// so it must resolve to `Disabled` and never reach the parser.
    #[test]
    fn test_address_setting_empty_is_disabled() {
        assert_eq!(address_setting(Some("")), AddressSetting::Disabled);
        assert_eq!(address_setting(Some("   ")), AddressSetting::Disabled);
        assert_eq!(address_setting(Some("\t\n")), AddressSetting::Disabled);
    }

    /// A missing value falls back to loopback, a present one is used as given.
    #[test]
    fn test_address_setting_unset_and_set() {
        assert_eq!(address_setting(None), AddressSetting::Unset);
        assert_eq!(address_setting(Some("::1")), AddressSetting::Set("::1"));
        assert_eq!(
            address_setting(Some("127.0.0.1")),
            AddressSetting::Set("127.0.0.1")
        );
    }

    /// The regression this guards: disabling IPv6 on purpose used to warn, which reads
    /// as a malfunction in the logs. It must be informational.
    #[test]
    fn test_disabled_family_logs_info() {
        let outcome: Result<Option<SocketAddrV6>> = Ok(None);
        let (level, message) = bind_outcome_log(&outcome, "IPv6").expect("disabled is logged");
        assert_eq!(level, Level::Info);
        assert_eq!(message, "IPv6 address disabled");
    }

    /// A real bind failure must still warn, at both the REST and GRPC call sites.
    #[test]
    fn test_bind_failure_logs_warn() {
        let outcome: Result<Option<SocketAddrV6>> = Err(anyhow!("port in use"));
        let (level, message) = bind_outcome_log(&outcome, "IPv6 GRPC").expect("failure is logged");
        assert_eq!(level, Level::Warn);
        assert_eq!(message, "IPv6 GRPC bind error: port in use");
    }

    /// A successfully resolved address is silent and passes straight through.
    #[test]
    fn test_bound_address_is_silent() {
        let address = SocketAddrV6::new(Ipv6Addr::LOCALHOST, 11987, 0, 0);
        let outcome: Result<Option<SocketAddrV6>> = Ok(Some(address));
        assert!(bind_outcome_log(&outcome, "IPv6").is_none());
        assert_eq!(log_bind_outcome(outcome, "IPv6"), Some(address));
    }

    /// Both log paths reduce to "nothing to bind" so the caller's no-address check fires.
    #[test]
    fn test_disabled_and_failed_both_yield_no_address() {
        let disabled: Result<Option<SocketAddrV4>> = Ok(None);
        assert_eq!(log_bind_outcome(disabled, "IPv4"), None);
        let failed: Result<Option<SocketAddrV4>> = Err(anyhow!("port in use"));
        assert_eq!(log_bind_outcome(failed, "IPv4"), None);
    }

    fn bound_v4() -> Result<Option<SocketAddrV4>> {
        Ok(Some(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 11987)))
    }

    fn bound_v6() -> Result<Option<SocketAddrV6>> {
        Ok(Some(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 11987, 0, 0)))
    }

    /// A server with at least one usable family is up, so there is nothing to report,
    /// even when the other family is off or failed.
    #[test]
    fn test_server_with_one_family_is_not_reported() {
        let disabled: Result<Option<SocketAddrV6>> = Ok(None);
        assert!(unavailable_log(&bound_v4(), &disabled).is_none());
        let failed: Result<Option<SocketAddrV4>> = Err(anyhow!("port in use"));
        assert!(unavailable_log(&failed, &bound_v6()).is_none());
    }

    /// Turning both families off is a deliberate opt-out, so a server that is entirely
    /// off is informational, not an error.
    #[test]
    fn test_server_fully_disabled_logs_info() {
        let v4: Result<Option<SocketAddrV4>> = Ok(None);
        let v6: Result<Option<SocketAddrV6>> = Ok(None);
        let (level, message) = unavailable_log(&v4, &v6).expect("no address is reported");
        assert_eq!(level, Level::Info);
        assert_eq!(
            message,
            "REST API disabled. No API, UI, or gRPC connection available."
        );
    }

    /// Failing to bind every family is a malfunction and must be loud, even when the
    /// other family was switched off on purpose.
    #[test]
    fn test_server_unable_to_bind_logs_error() {
        let failed: Result<Option<SocketAddrV4>> = Err(anyhow!("port in use"));
        let disabled: Result<Option<SocketAddrV6>> = Ok(None);
        let (level, message) = unavailable_log(&failed, &disabled).expect("no address is reported");
        assert_eq!(level, Level::Error);
        assert_eq!(
            message,
            "Could not bind REST API to any address. No API, UI, or gRPC connection available."
        );
    }

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

    #[test]
    fn test_csp_frame_ancestor_is_valid() {
        let valid_schemes = vec!["https://", "http://"];
        let valid_hosts = vec![
            "192.168.0.1",
            "[2001:db8::1]",
            "example.com",
            "sub.example.com",
        ];
        let valid_ports = vec!["", ":9090", ":65535"];
        let valid_paths = vec!["", "/"];

        for scheme in &valid_schemes {
            for host in &valid_hosts {
                for port in &valid_ports {
                    for path in &valid_paths {
                        assert!(csp_frame_ancestor_is_valid(&format!(
                            "{scheme}{host}{port}{path}"
                        )));
                    }
                }
            }
        }

        let invalid_hosts = [
            "",
            "[2001:db8::1",
            "[invalid]",
            "example.com.",
            "-example.com",
            "example-.com",
            "example..com",
        ];
        for host in invalid_hosts {
            assert!(!csp_frame_ancestor_is_valid(&format!(
                "https://{host}:9090/test"
            )));
        }

        let invalid_ports = [":", ":abc", ":65536", ":-1", ":9090x"];
        for port in invalid_ports {
            assert!(!csp_frame_ancestor_is_valid(&format!(
                "https://example.com{port}/test"
            )));
        }

        let invalid_paths = [
            "/test",
            "/a/b/123/test_path/~/./",
            "/ABC",
            "//",
            "/test//",
            "/test//sdf",
            "/@#$%^",
            "/with space",
        ];
        for path in invalid_paths {
            assert!(
                !csp_frame_ancestor_is_valid(&format!("https://example.com:9090{path}")),
                "failed on path {path}"
            );
        }

        assert!(!csp_frame_ancestor_is_valid("https://[::1]junk"));
    }

    #[test]
    fn test_csp_frame_ancestors_value() {
        assert_eq!(csp_frame_ancestors_value(vec![]), "'none'");
        assert_eq!(
            csp_frame_ancestors_value(vec!["https://localhost:9090".to_string()]),
            "https://localhost:9090"
        );
        assert_eq!(
            csp_frame_ancestors_value(vec![
                "https://localhost:9090".to_string(),
                "http://192.168.2.1:1234".to_string(),
            ]),
            "https://localhost:9090 http://192.168.2.1:1234"
        );
        assert_eq!(
            csp_frame_ancestors_value(vec![
                "https://localhost:9090".to_string(),
                "bad.value".to_string(),
                "http://192.168.2.1:1234".to_string(),
            ]),
            "https://localhost:9090 http://192.168.2.1:1234"
        );
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

    #[test]
    fn test_validate_name_rejects_remaining_c0_controls() {
        // Backspace spoofs log lines by erasing previous output; the old
        // blacklist missed it and the other unlisted C0 characters.
        assert!(validate_name_string("name\x08spoof").is_err());
        assert!(validate_name_string("name\x01ctrl").is_err());
    }

    #[test]
    fn test_validate_name_rejects_c1_controls() {
        // C1 controls (U+0080..U+009F) include CSI, which some terminals
        // honor like ESC sequences.
        assert!(validate_name_string("name\u{9B}csi").is_err());
        assert!(validate_name_string("name\u{85}nel").is_err());
    }

    #[test]
    fn test_validate_name_rejects_unicode_line_separators() {
        // U+2028/U+2029 terminate lines in JS string contexts and split
        // log lines without being caught by the \n check.
        assert!(validate_name_string("name\u{2028}break").is_err());
        assert!(validate_name_string("name\u{2029}break").is_err());
    }

    #[test]
    fn test_validate_name_rejects_bidi_controls() {
        // Bidirectional overrides visually reverse displayed text, spoofing
        // names in logs and UIs (the classic RLO trick).
        assert!(validate_name_string("name\u{202E}gnp.exe").is_err());
        assert!(validate_name_string("name\u{2066}iso").is_err());
    }

    #[test]
    fn test_validate_name_allows_plain_unicode_and_html_specials() {
        // Non-ASCII text is legitimate; HTML/markup specials are allowed
        // here because every consumer escapes at its own boundary.
        assert!(validate_name_string("Wasserkühlung Vorne").is_ok());
        assert!(validate_name_string("Fan <Front> & \"Rear\"").is_ok());
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
        assert_eq!(
            response.headers().get("x-frame-options").unwrap(),
            "SAMEORIGIN"
        );
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

    #[tokio::test]
    async fn test_frame_origins_appended_to_csp() {
        use axum::body::Body;
        use axum::http;
        use axum::routing::get;

        let app = Router::new()
            .route(
                "/test",
                get(|| async { ([("content-security-policy", "default-src 'self'")], "ok") }),
            )
            .layer(middleware::from_fn_with_state(
                csp_frame_ancestors_value(vec![
                    "https://cockpit.example.com".to_string(),
                    "http://localhost:9090".to_string(),
                ]),
                csp_frame_ancestors_middleware,
            ));

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
            response.headers().get("content-security-policy").unwrap(),
            "default-src 'self'; frame-ancestors https://cockpit.example.com http://localhost:9090"
        );
    }

    #[tokio::test]
    async fn test_no_frame_origins_appended_to_csp() {
        use axum::body::Body;
        use axum::http;
        use axum::routing::get;

        let app = Router::new()
            .route(
                "/test",
                get(|| async { ([("content-security-policy", "default-src 'self'")], "ok") }),
            )
            .layer(middleware::from_fn_with_state(
                csp_frame_ancestors_value(vec![]),
                csp_frame_ancestors_middleware,
            ));

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
            response.headers().get("content-security-policy").unwrap(),
            "default-src 'self'; frame-ancestors 'none'"
        );
    }

    #[test]
    fn test_validate_name_allows_unicode() {
        assert!(validate_name_string("Ünïcödé Nàmé").is_ok());
        assert!(validate_name_string("日本語").is_ok());
        assert!(validate_name_string("émojis 🎉").is_ok());
    }

    #[test]
    fn test_watch_passwd_file_clears_cache_on_mtime_change() {
        // Goal: verify that the watcher clears the in-memory session
        // cache when the password file's mtime changes (simulating a
        // CLI `--reset-password` against a running daemon). Runs on the
        // active runtime (Tokio or compio) since the watcher uses the rt facade.
        use std::collections::HashMap;
        use time::OffsetDateTime;
        use tower_sessions::session::Record;

        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let passwd_file = dir.path().join("passwd");
            std::fs::write(&passwd_file, b"original-hash").unwrap();
            // Initial mtime needs to be old enough that a rewrite below
            // produces a strictly different mtime even on filesystems with
            // 1s mtime resolution.
            crate::rt::sleep(Duration::from_millis(1100)).await;

            let store = MemorySessionStore::new(10);
            let mut record = Record {
                id: Id::default(),
                data: HashMap::default(),
                expiry_date: OffsetDateTime::now_utc() + time::Duration::hours(1),
            };
            store.create(&mut record).await.unwrap();
            let stored_id = record.id;
            assert!(
                store.load(&stored_id).await.unwrap().is_some(),
                "Session should exist before reset."
            );

            let cancel_token = CancellationToken::new();
            // rt::spawn is detached; the poll loop below plus the cancel token
            // synchronize the test without a join handle.
            crate::rt::spawn(watch_passwd_file(
                passwd_file.clone(),
                store.clone(),
                cancel_token.clone(),
                Duration::from_millis(50),
            ));
            // Let the watcher run its initial mtime read before we mutate
            // the file. Without this, the watcher's first read sees the
            // post-mutation mtime as its baseline and never detects a
            // change.
            crate::rt::sleep(Duration::from_millis(100)).await;

            // Simulate `coolercontrold --reset-password`: rewrite the file.
            std::fs::write(&passwd_file, b"default-hash").unwrap();

            // Poll until the watcher catches the change. Bound the wait so
            // a regression cannot hang the test runner.
            let mut cleared = false;
            for _ in 0..50 {
                crate::rt::sleep(Duration::from_millis(50)).await;
                if store.load(&stored_id).await.unwrap().is_none() {
                    cleared = true;
                    break;
                }
            }
            cancel_token.cancel();
            assert!(
                cleared,
                "Session cache should be cleared after password file mtime changes."
            );
        });
    }

    #[test]
    fn test_watch_passwd_file_no_clear_when_unchanged() {
        // Goal: verify that the watcher leaves the cache alone when
        // the password file is untouched, so we do not invalidate
        // sessions on every poll.
        use std::collections::HashMap;
        use time::OffsetDateTime;
        use tower_sessions::session::Record;

        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let passwd_file = dir.path().join("passwd");
            std::fs::write(&passwd_file, b"hash").unwrap();

            let store = MemorySessionStore::new(10);
            let mut record = Record {
                id: Id::default(),
                data: HashMap::default(),
                expiry_date: OffsetDateTime::now_utc() + time::Duration::hours(1),
            };
            store.create(&mut record).await.unwrap();
            let stored_id = record.id;

            let cancel_token = CancellationToken::new();
            crate::rt::spawn(watch_passwd_file(
                passwd_file,
                store.clone(),
                cancel_token.clone(),
                Duration::from_millis(20),
            ));

            // Let several poll cycles run without modifying the file.
            crate::rt::sleep(Duration::from_millis(150)).await;
            cancel_token.cancel();

            assert!(
                store.load(&stored_id).await.unwrap().is_some(),
                "Session should still be present when password file is unchanged."
            );
        });
    }
}
