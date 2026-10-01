// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::api::{handle_error, AppState, CCError};
use crate::logger::LogLevelSource;
use aide::axum::IntoApiResponse;
#[cfg(debug_assertions)]
use aide::openapi::OpenApi;
use anyhow::Result;
use axum::extract::Request;
use axum::extract::State;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::IntoResponse;
#[cfg(debug_assertions)]
use axum::Extension;
use axum::Json;
use chrono::{DateTime, Local};
use include_dir::{include_dir, Dir};
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
#[cfg(debug_assertions)]
use std::sync::Arc;
use tower_serve_static::ServeDir;

static ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/resources/app");

pub async fn handshake() -> impl IntoApiResponse {
    Json(json!({"shake": true})).into_response()
}

pub fn web_app_service() -> axum::routing::MethodRouter {
    axum::routing::get_service(ServeDir::new(&ASSETS_DIR))
        .layer(middleware::from_fn(cache_control_middleware))
}

const CONTENT_SECURITY_POLICY: &str = "default-src 'self'; \
    script-src 'self' qrc:; \
    style-src 'self' 'unsafe-inline'; \
    img-src 'self' blob: data:; \
    font-src 'self' data:; \
    connect-src 'self'; \
    object-src 'none'; \
    base-uri 'self'; \
    form-action 'self'";

/// Vite hashes every bundle it emits under this prefix, so those can be pinned for a year.
/// Paths outside it keep their names across releases and have to revalidate: a pinned
/// service worker would freeze the notification logic at whatever build first cached it,
/// and a pinned manifest would keep an installed app on stale metadata.
const HASHED_ASSETS_PREFIX: &str = "/assets/";

const CSP_HEADER: HeaderName = HeaderName::from_static("content-security-policy");
const CSP_VALUE: HeaderValue = HeaderValue::from_static(CONTENT_SECURITY_POLICY);
const CACHE_PINNED: HeaderValue = HeaderValue::from_static("public, max-age=31536000, immutable");
const CACHE_REVALIDATE: HeaderValue = HeaderValue::from_static("no-cache");
const TYPE_CSS: HeaderValue = HeaderValue::from_static("text/css; charset=utf-8");
const TYPE_JS: HeaderValue = HeaderValue::from_static("text/javascript; charset=utf-8");

/// How a served file may be cached. Derived from the path alone, because what the fallback
/// serves is fixed at compile time by `include_dir!`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CachePolicy {
    /// The document. Carries the CSP and must never be pinned, or clients strand on an old build.
    Document,
    /// Unhashed and stable across releases, so it has to be revalidated.
    Revalidate,
    /// Content-hashed, so a new build means a new URL and this one can be pinned for a year.
    Pinned,
}

impl CachePolicy {
    /// Hashed bundles are nearly all of this traffic, so they settle on one prefix comparison
    /// without touching the named-path arms.
    fn for_path(path: &str) -> Self {
        if path.starts_with(HASHED_ASSETS_PREFIX) {
            return Self::Pinned;
        }
        match path {
            "/" | "/index.html" => Self::Document,
            // Everything else comes from public/ under a name that never changes, so
            // pinning it would strand clients on the build that first cached it. The
            // static service answers If-Modified-Since with a 304, so revalidating
            // costs a conditional request rather than the body.
            _ => Self::Revalidate,
        }
    }
}

/// Only the fallback service is layered with this, so API and SSE requests never reach it.
/// Everything is classified up front: `path` borrows `request`, which the next layer consumes.
async fn cache_control_middleware(request: Request, next: Next) -> axum::response::Response {
    let path = request.uri().path();
    debug_assert!(path.starts_with('/'), "an axum request path is absolute");
    let policy = CachePolicy::for_path(path);
    // Ensure text-based assets declare UTF-8 encoding explicitly.
    // lightningcss converts CSS escapes (e.g. \e909) to raw UTF-8 bytes,
    // which requires correct charset to render in sandboxed plugin iframes.
    // Keyed off the extension rather than the policy so a future unhashed .css keeps it.
    let extension = std::path::Path::new(path).extension();
    let is_css = extension.is_some_and(|ext| ext.eq_ignore_ascii_case("css"));
    let is_js = extension.is_some_and(|ext| ext.eq_ignore_ascii_case("js"));
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    if is_css {
        headers.insert(CONTENT_TYPE, TYPE_CSS);
    } else if is_js {
        headers.insert(CONTENT_TYPE, TYPE_JS);
    }
    let cache_value = match policy {
        CachePolicy::Document => {
            headers.insert(CSP_HEADER, CSP_VALUE);
            CACHE_REVALIDATE
        }
        CachePolicy::Revalidate => CACHE_REVALIDATE,
        CachePolicy::Pinned => CACHE_PINNED,
    };
    headers.insert(CACHE_CONTROL, cache_value);
    response
}

#[cfg(debug_assertions)]
pub async fn serve_api_doc(Extension(api): Extension<Arc<OpenApi>>) -> impl IntoApiResponse {
    Json(api).into_response()
}

pub async fn health(
    State(AppState {
        log_buf_handle,
        health,
        ..
    }): State<AppState>,
) -> Result<Json<HealthCheck>, CCError> {
    let (warnings, errors) = log_buf_handle.warning_errors().await;
    health
        .check(warnings, errors, log_buf_handle.level_info())
        .await
        .map(Json)
        .map_err(handle_error)
}

pub async fn acknowledge_issues(
    State(AppState { log_buf_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    log_buf_handle
        .acknowledge_issues()
        .await
        .map_err(handle_error)
}

pub async fn logs(State(AppState { log_buf_handle, .. }): State<AppState>) -> impl IntoApiResponse {
    log_buf_handle.get_logs().await
}

pub async fn shutdown() -> Result<(), CCError> {
    signal::kill(Pid::this(), Signal::SIGQUIT).map_err(|err| CCError::InternalError {
        msg: err.to_string(),
    })
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthCheck {
    pub status: String,
    pub description: String,
    pub current_timestamp: DateTime<Local>,
    pub details: HealthDetails,
    pub system: SystemDetails,
    pub links: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HealthDetails {
    pub uptime: String,
    pub version: String,
    pub pid: u32,
    pub memory_mb: f64,
    pub warnings: usize,
    pub errors: usize,
    pub liquidctl_connected: bool,
    /// The log level the daemon started with, e.g. "INFO" or "DEBUG".
    pub log_level: String,
    /// Where the log level came from.
    pub log_level_source: LogLevelSource,
    /// Whether logs go to the systemd journal rather than stderr.
    pub log_to_journal: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SystemDetails {
    pub name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    use axum::body::Body;
    use axum::http;
    use axum::routing::get;
    use axum::Router;
    use tower::ServiceExt;

    #[tokio::test]
    async fn test_csp_header_set_on_index() {
        let app = Router::new()
            .route("/", get(|| async { "index" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let csp = response
            .headers()
            .get("content-security-policy")
            .expect("CSP header should be set on index");
        let csp_str = csp.to_str().unwrap();
        assert!(csp_str.contains("default-src 'self'"));
        assert!(csp_str.contains("script-src 'self' qrc:"));
    }

    #[tokio::test]
    async fn test_csp_header_absent_on_assets() {
        let app = Router::new()
            .route("/assets/app.js", get(|| async { "js" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/assets/app.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().get("content-security-policy").is_none());
    }

    #[tokio::test]
    async fn test_csp_header_set_on_index_html() {
        let app = Router::new()
            .route("/index.html", get(|| async { "index" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/index.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert!(response.headers().get("content-security-policy").is_some());
        assert_eq!(response.headers().get("cache-control").unwrap(), "no-cache");
    }

    // Goal: an installed app has to notice a new manifest. Method: serve the manifest path
    // through the middleware and assert it revalidates rather than pinning for a year,
    // which is what every unhashed path got before.
    #[tokio::test]
    async fn test_manifest_is_revalidated() {
        let app = Router::new()
            .route("/manifest.webmanifest", get(|| async { "{}" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/manifest.webmanifest")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.headers().get("cache-control").unwrap(), "no-cache");
        assert!(response.headers().get("content-security-policy").is_none());
    }

    // Goal: a year-pinned service worker would freeze notification behavior at whatever
    // build first cached it. Method: assert it revalidates, and that lifting the CSP
    // branch out of the cache branch did not cost it the JS charset it had before.
    #[tokio::test]
    async fn test_service_worker_revalidates_and_keeps_charset() {
        let app = Router::new()
            .route("/notification-sw.js", get(|| async { "self" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/notification-sw.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.headers().get("cache-control").unwrap(), "no-cache");
        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/javascript; charset=utf-8"
        );
    }

    // Goal: negative space for the widened revalidation list. A hashed bundle whose name
    // merely resembles one of those paths must stay pinned, or every load refetches the
    // whole app. Method: two near-miss paths that must not match.
    #[tokio::test]
    async fn test_hashed_assets_stay_pinned() {
        for path in [
            "/assets/notification-sw-abc123.js",
            "/assets/index-abc123.js",
        ] {
            let app = Router::new()
                .route(path, get(|| async { "js" }))
                .layer(middleware::from_fn(cache_control_middleware));

            let response = app
                .oneshot(
                    http::Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                response.headers().get("cache-control").unwrap(),
                "public, max-age=31536000, immutable",
                "{path} must stay pinned"
            );
        }
    }

    // Goal: the whole cache decision is one pure function, so cover it directly instead of
    // paying a Router per case. Method: the paths the build actually emits, plus the
    // near-misses that must not be mistaken for the unhashed ones.
    #[test]
    fn test_cache_policy_for_path() {
        let cases = [
            ("/", CachePolicy::Document),
            ("/index.html", CachePolicy::Document),
            ("/manifest.webmanifest", CachePolicy::Revalidate),
            ("/notification-sw.js", CachePolicy::Revalidate),
            ("/assets/index-abc123.js", CachePolicy::Pinned),
            ("/assets/index-abc123.css", CachePolicy::Pinned),
            // A hashed bundle named after an unhashed path must not steal its policy.
            ("/assets/notification-sw-abc123.js", CachePolicy::Pinned),
            ("/assets/manifest.webmanifest", CachePolicy::Pinned),
            // Unhashed and served under a fixed name, so they have to revalidate.
            ("/favicon.ico", CachePolicy::Revalidate),
            ("/icons/app-512.png", CachePolicy::Revalidate),
            ("/icons/alert-triggered.png", CachePolicy::Revalidate),
        ];
        for (path, expected) in cases {
            assert_eq!(CachePolicy::for_path(path), expected, "{path}");
        }
    }

    // Goal: prove the metadata feature is actually on. Without it the static service sends
    // no validator, so `no-cache` on an unhashed asset means the whole body every load
    // rather than a 304 - the opposite of what the revalidate policy is for, and nothing
    // else in this suite would notice. Method: drive the real fallback service, which is
    // the only place `include_dir` metadata reaches, and require the header.
    #[tokio::test]
    async fn test_unhashed_assets_carry_a_validator() {
        // A daemon-only build embeds an empty dir (build.rs warns rather than fails in
        // debug, and CI's ci-test runs cargo build without sync-app), so there is
        // nothing to serve and nothing to assert.
        if ASSETS_DIR.get_file("index.html").is_none() {
            return;
        }
        let app = Router::new().fallback_service(web_app_service());
        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/index.html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), http::StatusCode::OK);
        assert!(
            response.headers().contains_key("last-modified"),
            "the metadata feature must stay enabled, or revalidation costs a full body"
        );
    }

    // Goal: the charset override keys off the extension, not off /assets/, so an unhashed
    // stylesheet added to public/ later still declares UTF-8. Method: a .css path outside
    // the hashed prefix, which takes the named-path fallthrough arm.
    #[tokio::test]
    async fn test_unhashed_css_still_declares_charset() {
        let app = Router::new()
            .route("/theme.css", get(|| async { "body{}" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/theme.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/css; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_css_charset_utf8() {
        let app = Router::new()
            .route("/assets/style-abc123.css", get(|| async { "body{}" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/assets/style-abc123.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            response.headers().get("cache-control").unwrap(),
            "public, max-age=31536000, immutable"
        );
    }

    #[tokio::test]
    async fn test_js_charset_utf8() {
        let app = Router::new()
            .route("/assets/app-abc123.js", get(|| async { "console.log(1)" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/assets/app-abc123.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(
            response.headers().get("content-type").unwrap(),
            "text/javascript; charset=utf-8"
        );
    }

    #[tokio::test]
    async fn test_non_text_assets_no_charset_override() {
        let app = Router::new()
            .route("/assets/primeicons-abc123.svg", get(|| async { "<svg/>" }))
            .layer(middleware::from_fn(cache_control_middleware));

        let response = app
            .oneshot(
                http::Request::builder()
                    .uri("/assets/primeicons-abc123.svg")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        // The middleware should not override Content-Type for non-CSS/JS assets.
        let content_type = response
            .headers()
            .get("content-type")
            .map(|ct| ct.to_str().unwrap().to_owned())
            .unwrap_or_default();
        assert!(content_type.starts_with("text/css").not());
        assert!(content_type.starts_with("text/javascript").not());
    }
}
