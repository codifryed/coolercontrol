// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::{handle_error, AppState, CCError};
use crate::repositories::service_plugin::service_management::manager::ServiceStatus;
use aide::axum::IntoApiResponse;
use axum::body::Body;
use axum::extract::{Path, Request, State};
use axum::response::Response;
use axum::Json;
use http_body_util::{BodyExt, Full};
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tokio::net::TcpStream;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use tower_serve_static::include_file;

/// Content-Security-Policy for plugin UI responses.
/// `connect-src 'none'` forces plugins to use the pluginFetch relay for all network access.
/// `sandbox allow-scripts` keeps a plugin page out of the daemon's origin however it is
/// loaded, not only inside the UI's sandboxed frame.
/// `FramePolicy` widens `frame-ancestors` for configured origins.
pub const PLUGIN_CONTENT_SECURITY_POLICY: &str = "default-src 'none'; \
    script-src 'self' 'unsafe-inline'; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' data: blob:; \
    connect-src 'none'; \
    font-src 'self' data:; \
    frame-ancestors 'self'; \
    object-src 'none'; \
    base-uri 'none'; \
    form-action 'none'; \
    sandbox allow-scripts";

pub async fn get_plugins(
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<Json<PluginsDto>, CCError> {
    plugin_handle
        .get_all()
        .await
        .map(Json)
        .map_err(handle_error)
}

pub async fn get_cc_plugin_lib(request: Request) -> Result<impl IntoApiResponse, CCError> {
    tower_serve_static::ServeFile::new(include_file!("/resources/lib/cc-plugin-lib.js"))
        .oneshot(request)
        .await
        .map_err(|_infallible| CCError::InternalError {
            msg: "Failed to serve file".to_string(),
        })
}

pub async fn get_config(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<String, CCError> {
    plugin_handle
        .get_config(path.plugin_id)
        .await
        .map_err(handle_error)
}

pub async fn update_config(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
    config_request_body: String,
) -> Result<(), CCError> {
    plugin_handle
        .update_config(path.plugin_id, config_request_body)
        .await
        .map_err(handle_error)
}

pub async fn has_ui(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Json<HasUiDto> {
    plugin_handle.get_ui_dir(path.plugin_id).await.map_or_else(
        |_| Json(HasUiDto::default()),
        |plugin_ui_dir| {
            Json(HasUiDto {
                has_ui: plugin_ui_dir.join("index.html").exists(),
            })
        },
    )
}

/// Keeps the whole cause chain. `handle_error` reports the outermost context alone, which
/// for a lifecycle call names the action that failed and nothing about why.
fn handle_lifecycle_error(err: anyhow::Error) -> CCError {
    match err.downcast::<CCError>() {
        Ok(cc_error) => cc_error,
        Err(err) => CCError::InternalError {
            msg: format!("{err:#}"),
        },
    }
}

pub async fn start_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .start_plugin(path.plugin_id)
        .await
        .map_err(handle_lifecycle_error)
}

pub async fn stop_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .stop_plugin(path.plugin_id)
        .await
        .map_err(handle_lifecycle_error)
}

pub async fn restart_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .restart_plugin(path.plugin_id)
        .await
        .map_err(handle_lifecycle_error)
}

pub async fn reload_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .reload_plugin(path.plugin_id)
        .await
        .map_err(handle_lifecycle_error)
}

pub async fn get_plugin_status(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<Json<PluginStatusDto>, CCError> {
    plugin_handle
        .get_plugin_status(path.plugin_id)
        .await
        .map(Json)
        .map_err(handle_error)
}

pub async fn disable_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .disable_plugin(path.plugin_id)
        .await
        .map_err(handle_error)
}

pub async fn enable_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .enable_plugin(path.plugin_id)
        .await
        .map_err(handle_lifecycle_error)
}

pub async fn get_ui_files(
    Path(path): Path<PluginUiPath>,
    State(AppState {
        plugin_handle,
        frame_policy,
        ..
    }): State<AppState>,
    request: Request,
) -> Result<impl IntoApiResponse, CCError> {
    let safe_path = sanitize_file_path(&path.file_path)?;
    let is_html = safe_path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html"));
    let plugin_ui_dir = plugin_handle.get_ui_dir(path.plugin_id).await?;
    let mut response = ServeFile::new(plugin_ui_dir.join(safe_path))
        .oneshot(request)
        .await
        .map_err(|_infallible| CCError::InternalError {
            msg: "Failed to serve file".to_string(),
        })?;
    let headers = response.headers_mut();
    // Every file, not only HTML: a browser also renders SVG and XML as documents, and one
    // served without the policy would run its scripts in the daemon's origin.
    headers.insert(
        axum::http::HeaderName::from_static("content-security-policy"),
        frame_policy.plugin_csp,
    );
    if is_html {
        headers.insert(
            axum::http::header::CACHE_CONTROL,
            axum::http::HeaderValue::from_static("no-cache, max-age=60"),
        );
    }
    Ok(response)
}

/// Sanitize a relative file path for safe use in serving plugin UI files.
/// Rejects absolute paths, directory traversal, null bytes, and paths without extensions.
fn sanitize_file_path(file_path: &str) -> Result<PathBuf, CCError> {
    if file_path.contains('\0') {
        return Err(invalid_file_path());
    }
    let path = PathBuf::from(file_path);
    if path.is_absolute() {
        return Err(invalid_file_path());
    }
    // Rebuild the path, rejecting any component that is not a normal file/directory name.
    let mut safe_path = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(segment) => {
                safe_path.push(segment);
            }
            // Reject .., ., prefix (e.g. C:), and root components.
            _ => return Err(invalid_file_path()),
        }
    }
    if safe_path.as_os_str().is_empty() {
        return Err(invalid_file_path());
    }
    // The final component must have a file extension.
    if safe_path.extension().is_none() {
        return Err(invalid_file_path());
    }
    Ok(safe_path)
}

fn invalid_file_path() -> CCError {
    CCError::UserError {
        msg: "Invalid file path".to_string(),
    }
}

/// Max proxy response body: 10 MB.
const PROXY_MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
/// Max proxy request body: 1 MB.
const PROXY_MAX_REQUEST_BYTES: usize = 1024 * 1024;
/// Proxy operation timeout in seconds.
const PROXY_TIMEOUT_SECS: u64 = 30;

// Compile-time validation of proxy limit invariants.
const _: () = assert!(PROXY_MAX_REQUEST_BYTES > 0);
const _: () = assert!(PROXY_MAX_RESPONSE_BYTES > 0);
const _: () = assert!(PROXY_MAX_REQUEST_BYTES <= PROXY_MAX_RESPONSE_BYTES);
const _: () = assert!(PROXY_TIMEOUT_SECS > 0);

/// Safe response headers to forward from plugin proxy upstream responses.
const PROXY_ALLOWED_RESPONSE_HEADERS: &[&str] = &[
    "content-type",
    "content-length",
    "content-encoding",
    "cache-control",
    "etag",
    "last-modified",
];

/// Reverse-proxy a request to a plugin's local HTTP server.
/// Maps `/plugins/{plugin_id}/data/{*data_path}` to `http://127.0.0.1:{port}/{data_path}`.
/// The plugin must declare `[proxy] enabled = true` and `port = N` in its manifest.
pub async fn proxy_plugin_data(
    Path(path): Path<PluginDataPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
    request: Request,
) -> Result<Response, CCError> {
    if plugin_handle
        .is_plugin_disabled(path.plugin_id.clone())
        .await
        .map_err(handle_error)?
    {
        return Err(CCError::UserError {
            msg: format!("Plugin '{}' is disabled", path.plugin_id),
        });
    }
    let port = plugin_handle
        .get_proxy_port(path.plugin_id.clone())
        .await
        .map_err(handle_error)?
        .ok_or_else(|| CCError::NotFound {
            msg: format!("Plugin '{}' has no proxy configured", path.plugin_id),
        })?;

    // Build the upstream path, preserving query string from the original request.
    let upstream_path = {
        let query = request
            .uri()
            .query()
            .map(|q| format!("?{q}"))
            .unwrap_or_default();
        format!("/{}{}", path.data_path, query)
    };

    let method = request.method().clone();
    let auth_header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .cloned();
    let body_bytes = request
        .into_body()
        .collect()
        .await
        .map_err(|e| CCError::InternalError { msg: e.to_string() })?
        .to_bytes();
    check_proxy_body_size(body_bytes.len(), PROXY_MAX_REQUEST_BYTES, "request")?;

    // The upstream connection and response are wrapped in a timeout.
    tokio::time::timeout(
        Duration::from_secs(PROXY_TIMEOUT_SECS),
        proxy_upstream(port, &upstream_path, method, auth_header, body_bytes),
    )
    .await
    .map_err(|_| CCError::InternalError {
        msg: "Plugin proxy request timed out".to_string(),
    })?
}

/// Execute the upstream proxy connection, send the request, and build the response.
async fn proxy_upstream(
    port: u16,
    upstream_path: &str,
    method: axum::http::Method,
    auth_header: Option<axum::http::HeaderValue>,
    body_bytes: hyper::body::Bytes,
) -> Result<Response, CCError> {
    let stream = TcpStream::connect(format!("127.0.0.1:{port}"))
        .await
        .map_err(|e| CCError::InternalError {
            msg: format!("Cannot connect to plugin proxy on port {port}: {e}"),
        })?;
    let io = TokioIo::new(stream);
    let (mut sender, conn) = http1::handshake(io)
        .await
        .map_err(|e| CCError::InternalError { msg: e.to_string() })?;
    // Drive the connection in the background.
    // Uses tokio::spawn (not spawn_local) because axum handlers run outside a LocalSet.
    tokio::spawn(async move {
        let _ = conn.await;
    });

    let mut upstream_req = hyper::Request::builder()
        .method(method)
        .uri(upstream_path)
        .header("host", format!("127.0.0.1:{port}"))
        .body(Full::new(body_bytes))
        .map_err(|e: axum::http::Error| CCError::InternalError { msg: e.to_string() })?;
    if let Some(auth_val) = auth_header {
        upstream_req
            .headers_mut()
            .insert(axum::http::header::AUTHORIZATION, auth_val);
    }

    let upstream_resp =
        sender
            .send_request(upstream_req)
            .await
            .map_err(|e| CCError::InternalError {
                msg: format!("Plugin proxy request failed: {e}"),
            })?;

    let status = upstream_resp.status();
    let headers = upstream_resp.headers().clone();
    let resp_bytes = upstream_resp
        .into_body()
        .collect()
        .await
        .map_err(|e| CCError::InternalError { msg: e.to_string() })?
        .to_bytes();
    check_proxy_body_size(resp_bytes.len(), PROXY_MAX_RESPONSE_BYTES, "response")?;

    let mut response = Response::builder()
        .status(status)
        .body(Body::from(resp_bytes))
        .map_err(|e| CCError::InternalError { msg: e.to_string() })?;
    for name in PROXY_ALLOWED_RESPONSE_HEADERS {
        let header_name = axum::http::HeaderName::from_static(name);
        if let Some(value) = headers.get(&header_name) {
            response.headers_mut().insert(header_name, value.clone());
        }
    }
    Ok(response)
}

fn check_proxy_body_size(len: usize, max: usize, label: &str) -> Result<(), CCError> {
    if len > max {
        return Err(CCError::InternalError {
            msg: format!("Plugin proxy {label} too large: {len} bytes (max {max})"),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginPath {
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginUiPath {
    pub plugin_id: String,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginDataPath {
    pub plugin_id: String,
    pub data_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginsDto {
    pub plugins: Vec<PluginDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginDto {
    pub id: String,
    pub service_type: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub url: Option<String>,
    pub address: String,
    pub privileged: bool,
    pub path: String,
    pub disabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct HasUiDto {
    pub has_ui: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginStatusDto {
    pub status: String,
    pub reason: Option<String>,
}

impl From<ServiceStatus> for PluginStatusDto {
    fn from(status: ServiceStatus) -> Self {
        match status {
            ServiceStatus::Running => Self {
                status: "Running".to_string(),
                reason: None,
            },
            ServiceStatus::Stopped(reason) => Self {
                status: "Stopped".to_string(),
                reason,
            },
            ServiceStatus::Unmanaged => Self {
                status: "Unmanaged".to_string(),
                reason: None,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::actor::PluginHandle;
    use crate::api::frame_policy::FramePolicy;
    use crate::repositories::service_plugin::plugin_controller::PluginController;
    use crate::repositories::service_plugin::service_manifest::{
        ConnectionType, ServiceManifest, ServiceType,
    };
    use std::rc::Rc;
    use tokio_util::sync::CancellationToken;

    /// Goal: a failed start has to say why, since the message is all the UI can show.
    /// Method: an error built the way the controller builds one, and one that carries an
    /// API error, which must keep its own status rather than become an internal error.
    #[test]
    fn lifecycle_errors_keep_their_cause() {
        use anyhow::Context;
        let failed: anyhow::Result<()> = Err(anyhow::anyhow!("stopped right after starting"));
        let not_found: anyhow::Result<()> = Err(CCError::NotFound {
            msg: "Plugin not found".to_string(),
        }
        .into());

        let failed = handle_lifecycle_error(failed.context("Starting plugin service").unwrap_err());
        let not_found =
            handle_lifecycle_error(not_found.context("Starting plugin service").unwrap_err());

        let CCError::InternalError { msg } = failed else {
            panic!("expected an internal error, got {failed:?}");
        };
        assert_eq!(msg, "Starting plugin service: stopped right after starting");
        assert!(
            matches!(not_found, CCError::NotFound { .. }),
            "{not_found:?}"
        );
    }

    #[test]
    fn test_sanitize_file_path_valid_simple() {
        // A simple file at the root of the ui directory.
        let result = sanitize_file_path("index.html");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("index.html"));
    }

    #[test]
    fn test_sanitize_file_path_valid_with_multiple_dots() {
        // Files with multiple dots in the name are valid.
        let result = sanitize_file_path("app.bundle.js");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("app.bundle.js"));
    }

    #[test]
    fn test_sanitize_file_path_valid_nested() {
        // Nested paths inside the ui directory are now supported.
        let result = sanitize_file_path("assets/app.js");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("assets/app.js"));
    }

    #[test]
    fn test_sanitize_file_path_valid_deeply_nested() {
        // Multiple levels of nesting are valid.
        let result = sanitize_file_path("assets/css/style.css");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from("assets/css/style.css"));
    }

    #[test]
    fn test_sanitize_file_path_rejects_absolute() {
        // Absolute paths must be rejected to prevent serving arbitrary files.
        assert!(sanitize_file_path("/etc/passwd").is_err());
        assert!(sanitize_file_path("/home/user/file.txt").is_err());
    }

    #[test]
    fn test_sanitize_file_path_rejects_traversal() {
        // Directory traversal must be rejected entirely (not stripped).
        assert!(sanitize_file_path("../../../etc/passwd.txt").is_err());
        assert!(sanitize_file_path("assets/../../../etc/shadow.txt").is_err());
    }

    #[test]
    fn test_sanitize_file_path_rejects_dot_segment() {
        // Current directory segments are rejected to keep paths canonical.
        assert!(sanitize_file_path("./index.html").is_err());
    }

    #[test]
    fn test_sanitize_file_path_rejects_no_extension() {
        // Files without extensions are rejected for safety.
        assert!(sanitize_file_path("noextension").is_err());
        assert!(sanitize_file_path("Makefile").is_err());
    }

    #[test]
    fn test_sanitize_file_path_rejects_empty() {
        // An empty path is invalid.
        assert!(sanitize_file_path("").is_err());
    }

    #[test]
    fn test_sanitize_file_path_rejects_null_bytes() {
        // Null bytes in paths could cause truncation in C-based file operations.
        assert!(sanitize_file_path("index\0.html").is_err());
    }

    #[test]
    fn test_sanitize_file_path_hidden_file_with_extension() {
        // Hidden files with extensions are valid (some build tools produce these).
        let result = sanitize_file_path(".hidden.txt");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), PathBuf::from(".hidden.txt"));
    }

    #[test]
    fn test_sanitize_file_path_rejects_hidden_file_no_extension() {
        // ".gitignore" has no extension (the dot is part of the stem).
        assert!(sanitize_file_path(".gitignore").is_err());
    }

    #[test]
    fn test_proxy_body_size_under_limit() {
        // Body within limit should succeed.
        assert!(check_proxy_body_size(1024, PROXY_MAX_RESPONSE_BYTES, "response").is_ok());
    }

    #[test]
    fn test_proxy_body_size_at_limit() {
        // Body exactly at limit should succeed.
        assert!(check_proxy_body_size(
            PROXY_MAX_RESPONSE_BYTES,
            PROXY_MAX_RESPONSE_BYTES,
            "response"
        )
        .is_ok());
    }

    #[test]
    fn test_proxy_body_size_over_limit() {
        // Body exceeding limit should fail.
        let result = check_proxy_body_size(
            PROXY_MAX_RESPONSE_BYTES + 1,
            PROXY_MAX_RESPONSE_BYTES,
            "response",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_proxy_request_body_size_limit() {
        // Request body exceeding the tighter request limit should fail.
        assert!(check_proxy_body_size(
            PROXY_MAX_REQUEST_BYTES + 1,
            PROXY_MAX_REQUEST_BYTES,
            "request"
        )
        .is_err());
        assert!(
            check_proxy_body_size(PROXY_MAX_REQUEST_BYTES, PROXY_MAX_REQUEST_BYTES, "request")
                .is_ok()
        );
    }

    #[test]
    fn test_proxy_response_header_filtering() {
        // Verify that only allowlisted headers pass through the proxy filter.
        use axum::http::header::HeaderName;
        use axum::http::HeaderMap;

        let mut upstream_headers = HeaderMap::new();
        upstream_headers.insert("content-type", "application/json".parse().unwrap());
        upstream_headers.insert("content-length", "42".parse().unwrap());
        upstream_headers.insert("etag", "\"abc123\"".parse().unwrap());
        upstream_headers.insert("set-cookie", "session=evil".parse().unwrap());
        upstream_headers.insert("location", "https://evil.com".parse().unwrap());
        upstream_headers.insert("access-control-allow-origin", "*".parse().unwrap());

        let mut filtered = HeaderMap::new();
        for name in PROXY_ALLOWED_RESPONSE_HEADERS {
            let header_name = HeaderName::from_static(name);
            if let Some(value) = upstream_headers.get(&header_name) {
                filtered.insert(header_name, value.clone());
            }
        }

        assert!(filtered.get("content-type").is_some());
        assert!(filtered.get("content-length").is_some());
        assert!(filtered.get("etag").is_some());
        assert!(filtered.get("set-cookie").is_none());
        assert!(filtered.get("location").is_none());
        assert!(filtered.get("access-control-allow-origin").is_none());
    }

    #[test]
    fn test_plugin_csp_contains_required_directives() {
        // Verify all security-critical CSP directives are present.
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("default-src 'none'"));
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("script-src 'self'"));
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("connect-src 'none'"));
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("frame-ancestors 'self'"));
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("form-action 'none'"));
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("object-src 'none'"));
        assert!(PLUGIN_CONTENT_SECURITY_POLICY.contains("base-uri 'none'"));
    }

    /// Goal: a plugin page never runs in the daemon's origin, where it would hold the user's
    /// session, and cannot open a window or leave its frame. Method: the policy sandboxes
    /// with scripts allowed and nothing else.
    #[test]
    fn plugin_csp_sandboxes_with_scripts_only() {
        let sandbox = PLUGIN_CONTENT_SECURITY_POLICY
            .split("; ")
            .find(|directive| directive.starts_with("sandbox"));
        assert_eq!(sandbox, Some("sandbox allow-scripts"));
    }

    /// What the real handler answers each of a plugin's UI `files` with. Runs on the
    /// sidecar, as the servers do, since the file service needs its reactor. Nothing is
    /// asserted there: a panic would take the shared test sidecar down with it.
    async fn ui_file_responses(
        state: AppState,
        files: &'static [&'static str],
    ) -> Vec<axum::http::response::Parts> {
        let serve = move || async move {
            let app = axum::Router::new()
                .route(
                    "/plugins/{plugin_id}/ui/{*file_path}",
                    axum::routing::get(get_ui_files),
                )
                .with_state(state);
            let mut responses = Vec::with_capacity(files.len());
            for file in files {
                let request = Request::get(format!("/plugins/{PLUGIN_ID}/ui/{file}"))
                    .body(Body::empty())
                    .unwrap();
                let response = app.clone().oneshot(request).await.unwrap();
                responses.push(response.into_parts().0);
            }
            responses
        };
        let responses = crate::sidecar::handle().run(serve).await.unwrap();
        assert_eq!(responses.len(), files.len());
        responses
    }

    const PLUGIN_ID: &str = "test-plugin";

    /// A plugin with a UI under `plugin_dir`, known to a controller that manages no services.
    fn controller_with_ui(plugin_dir: &std::path::Path) -> PluginController {
        let ui_dir = plugin_dir.join("ui");
        std::fs::create_dir(&ui_dir).unwrap();
        std::fs::write(ui_dir.join("index.html"), "<html></html>").unwrap();
        std::fs::write(ui_dir.join("app.js"), "").unwrap();
        std::fs::write(ui_dir.join("icon.svg"), "<svg/>").unwrap();
        let manifest = ServiceManifest {
            id: PLUGIN_ID.to_string(),
            service_type: ServiceType::Integration,
            description: None,
            version: None,
            url: None,
            executable: None,
            args: Vec::new(),
            envs: Vec::new(),
            address: ConnectionType::None,
            tls: None,
            privileged: false,
            proxy: None,
            path: plugin_dir.to_path_buf(),
        };
        let controller = PluginController::new_disabled();
        controller.register(manifest);
        controller
    }

    /// Goal: the UI frames a plugin page, and a browser checks every ancestor, so once the UI
    /// is embedded the page must admit the same ancestors or it renders blank. Method: the
    /// real handler over a plugin on disk, with and without a configured ancestor. Without
    /// one the policy is the baseline, byte for byte.
    #[test]
    #[serial_test::serial(modes_file)]
    fn plugin_pages_admit_the_configured_frame_ancestors() {
        const ANCESTOR: &str = "https://cockpit.example.com:9090";
        const FILES: [&str; 1] = ["index.html"];
        crate::rt::test_runtime(async {
            let plugin_dir = tempfile::tempdir().unwrap();
            let controller = Rc::new(controller_with_ui(plugin_dir.path()));
            let cancel_token = CancellationToken::new();
            moro_local::async_scope!(|main_scope| -> anyhow::Result<()> {
                let state = AppState {
                    plugin_handle: PluginHandle::new(controller, cancel_token.clone(), main_scope),
                    ..crate::api::empty_app_state(&cancel_token, main_scope).await
                };
                let framed_state = AppState {
                    frame_policy: FramePolicy::from_config(&[ANCESTOR.to_string()]),
                    ..state.clone()
                };
                let unframed = ui_file_responses(state, &FILES).await;
                let framed = ui_file_responses(framed_state, &FILES).await;
                // Stops the actors so the scope can finish.
                cancel_token.cancel();

                for response in unframed.iter().chain(&framed) {
                    assert_eq!(response.status, axum::http::StatusCode::OK);
                }
                assert_eq!(
                    unframed[0].headers["content-security-policy"],
                    PLUGIN_CONTENT_SECURITY_POLICY
                );
                let csp = &framed[0].headers["content-security-policy"];
                let csp = csp.to_str().unwrap();
                assert!(csp.contains(&format!("; frame-ancestors 'self' {ANCESTOR}; ")));
                assert_eq!(csp.matches("frame-ancestors").count(), 1);
                Ok(())
            })
            .await
            .unwrap();
        });
    }

    /// Goal: a browser renders more than HTML as a document, SVG among them, and one served
    /// without the policy would run its scripts in the daemon's origin. Method: the real
    /// handler over a plugin on disk, with and without a configured ancestor, since the
    /// policy differs between the two. Every file carries the page's policy.
    #[test]
    #[serial_test::serial(modes_file)]
    fn every_plugin_ui_file_carries_the_page_policy() {
        const ANCESTOR: &str = "https://cockpit.example.com:9090";
        const FILES: [&str; 3] = ["index.html", "app.js", "icon.svg"];
        crate::rt::test_runtime(async {
            let plugin_dir = tempfile::tempdir().unwrap();
            let controller = Rc::new(controller_with_ui(plugin_dir.path()));
            let cancel_token = CancellationToken::new();
            moro_local::async_scope!(|main_scope| -> anyhow::Result<()> {
                let state = AppState {
                    plugin_handle: PluginHandle::new(controller, cancel_token.clone(), main_scope),
                    ..crate::api::empty_app_state(&cancel_token, main_scope).await
                };
                let framed_state = AppState {
                    frame_policy: FramePolicy::from_config(&[ANCESTOR.to_string()]),
                    ..state.clone()
                };
                let unframed = ui_file_responses(state, &FILES).await;
                let framed = ui_file_responses(framed_state, &FILES).await;
                // Stops the actors so the scope can finish.
                cancel_token.cancel();

                for responses in [&unframed, &framed] {
                    let page_csp = &responses[0].headers["content-security-policy"];
                    for file in &responses[1..] {
                        assert_eq!(file.status, axum::http::StatusCode::OK);
                        assert_eq!(&file.headers["content-security-policy"], page_csp);
                    }
                }
                assert_ne!(
                    unframed[0].headers["content-security-policy"],
                    framed[0].headers["content-security-policy"]
                );
                Ok(())
            })
            .await
            .unwrap();
        });
    }
}
