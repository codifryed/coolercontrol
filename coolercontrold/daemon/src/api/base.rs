// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::api::{handle_error, AppState, CCError};
use crate::logger::LogLevelSource;
use aide::axum::IntoApiResponse;
#[cfg(debug_assertions)]
use aide::openapi::OpenApi;
use anyhow::Result;
use axum::body::Body;
use axum::extract::Request;
use axum::extract::State;
use axum::http::header::{
    ACCEPT_ENCODING, CACHE_CONTROL, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_TYPE, VARY,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
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
use std::ops::Not;
#[cfg(debug_assertions)]
use std::sync::Arc;
use tower_serve_static::ServeDir;

static ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/resources/app");
/// Gzip copies of the compressible files in `ASSETS_DIR`, made by build.rs, each at its
/// original's path plus `.gz`.
static GZIP_ASSETS: Dir<'static> = include_dir!("$OUT_DIR/app-gzip");

pub async fn handshake() -> impl IntoApiResponse {
    Json(json!({"shake": true})).into_response()
}

pub fn web_app_service() -> axum::routing::MethodRouter {
    axum::routing::get_service(ServeDir::new(&ASSETS_DIR))
        .layer(middleware::from_fn(|request, next| {
            precompressed_middleware(&GZIP_ASSETS, request, next)
        }))
        .layer(middleware::from_fn(cache_control_middleware))
}

const VARY_ACCEPT_ENCODING: HeaderValue = HeaderValue::from_static("accept-encoding");
const ENCODING_GZIP: HeaderValue = HeaderValue::from_static("gzip");

/// Swaps in the build-time gzip of a file for clients that accept it.
///
/// Everything else stays with `ServeDir`: the lookup, redirects, 404s, `If-Modified-Since`,
/// the content type and `Last-Modified`, all of which describe the original file. Only a
/// 200 has its body replaced, and only when a gzip copy exists, so the daemon never
/// compresses a static file per request.
async fn precompressed_middleware(
    variants: &'static Dir<'static>,
    request: Request,
    next: Next,
) -> axum::response::Response {
    let gzipped = gzip_variant(variants, request.uri().path());
    let accepts_gzip = accepts_gzip(request.headers());
    let mut response = next.run(request).await;
    let Some(gzipped) = gzipped else {
        return response;
    };
    // A file with two representations must say so on every response for it, so no cache
    // hands the gzip to a client that never asked for it.
    response.headers_mut().append(VARY, VARY_ACCEPT_ENCODING);
    if response.status() != StatusCode::OK {
        return response;
    }
    if accepts_gzip.not() {
        return response;
    }
    let headers = response.headers_mut();
    headers.insert(CONTENT_ENCODING, ENCODING_GZIP);
    headers.remove(CONTENT_LENGTH);
    *response.body_mut() = Body::from(gzipped);
    response
}

/// The gzip copy of the file `ServeDir` serves for `request_path`, if the build made one.
///
/// `ServeDir` percent-decodes the path. Bundle names are plain ASCII, so an encoded path is
/// simply served uncompressed rather than decoded twice.
fn gzip_variant(variants: &'static Dir<'static>, request_path: &str) -> Option<&'static [u8]> {
    let relative = request_path.trim_start_matches('/');
    if relative.contains('%') {
        return None;
    }
    let variant_path = if relative.is_empty() || relative.ends_with('/') {
        format!("{relative}index.html.gz")
    } else {
        format!("{relative}.gz")
    };
    debug_assert!(
        variant_path.starts_with('/').not(),
        "variants are keyed relative"
    );
    debug_assert!(
        variant_path.starts_with(relative),
        "a variant extends its original path"
    );
    variants
        .get_file(variant_path)
        .map(include_dir::File::contents)
}

/// Whether `Accept-Encoding` allows gzip: named outright, or covered by `*`, at a nonzero
/// quality. A named entry wins over the wildcard, as RFC 9110 specifies.
fn accepts_gzip(headers: &HeaderMap) -> bool {
    let mut wildcard = false;
    let entries = headers
        .get_all(ACCEPT_ENCODING)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','));
    for entry in entries {
        let mut parts = entry.split(';');
        let coding = parts.next().unwrap_or_default().trim();
        let allowed = parts.all(|parameter| is_zero_quality(parameter).not());
        if coding.eq_ignore_ascii_case("gzip") || coding.eq_ignore_ascii_case("x-gzip") {
            return allowed;
        }
        if coding == "*" {
            wildcard = allowed;
        }
    }
    wildcard
}

fn is_zero_quality(parameter: &str) -> bool {
    let Some((name, value)) = parameter.split_once('=') else {
        return false;
    };
    if name.trim().eq_ignore_ascii_case("q").not() {
        return false;
    }
    value
        .trim()
        .parse::<f32>()
        .is_ok_and(|quality| quality <= 0.0)
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

/// Marks a response from the embedded web app, so the API's compression layer leaves it alone.
///
/// The fallback is reachable without credentials, and its bundles run to megabytes. Compressing
/// them per request would let any client spend the sidecar thread's CPU at will, while hashed
/// bundles are cached for a year and the rest revalidate for free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StaticAsset;

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
    response.extensions_mut().insert(StaticAsset);
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

    fn accept(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append(ACCEPT_ENCODING, value.parse().unwrap());
        }
        headers
    }

    // Goal: gzip is accepted when named or wildcarded at a nonzero quality, and refused when
    // absent or zeroed, with a named entry overriding the wildcard.
    #[test]
    fn test_accepts_gzip() {
        for (values, expected) in [
            (&["gzip, deflate, br"][..], true),
            (&["GZIP"][..], true),
            (&["br", "gzip;q=0.5"][..], true),
            (&["*"][..], true),
            (&["br, *;q=0.5"][..], true),
            (&[][..], false),
            (&["br, deflate"][..], false),
            (&["gzip;q=0"][..], false),
            (&["gzip; q=0.000"][..], false),
            (&["*;q=0"][..], false),
            (&["gzip;q=0, *"][..], false),
            (&["identity"][..], false),
        ] {
            assert_eq!(accepts_gzip(&accept(values)), expected, "{values:?}");
        }
    }

    static TEST_VARIANTS: Dir<'static> = Dir::new(
        "",
        &[
            include_dir::DirEntry::File(include_dir::File::new("index.html.gz", b"GZ-INDEX")),
            include_dir::DirEntry::Dir(Dir::new(
                "assets",
                &[include_dir::DirEntry::File(include_dir::File::new(
                    "assets/app.js.gz",
                    b"GZ-APP",
                ))],
            )),
        ],
    );

    // Goal: a request path finds its gzip copy, directories map to index.html as ServeDir
    // does, and paths without a copy or with percent-encoding find none.
    #[test]
    fn test_gzip_variant_lookup() {
        assert_eq!(
            gzip_variant(&TEST_VARIANTS, "/assets/app.js"),
            Some(&b"GZ-APP"[..])
        );
        assert_eq!(gzip_variant(&TEST_VARIANTS, "/"), Some(&b"GZ-INDEX"[..]));
        assert_eq!(gzip_variant(&TEST_VARIANTS, "/assets/other.js"), None);
        assert_eq!(gzip_variant(&TEST_VARIANTS, "/assets/app%2Ejs"), None);
        assert_eq!(gzip_variant(&TEST_VARIANTS, "/assets/../index.html"), None);
    }

    /// The precompressed layer in front of a stand-in for ServeDir that answers every path
    /// with the given status and a plain body.
    async fn serve_precompressed(
        path: &str,
        accept_encoding: Option<&str>,
        status: http::StatusCode,
    ) -> axum::response::Response {
        let app = Router::new()
            .fallback(
                move || async move { (status, [(CONTENT_TYPE, "text/javascript")], "original") },
            )
            .layer(middleware::from_fn(|request, next| {
                precompressed_middleware(&TEST_VARIANTS, request, next)
            }));
        let mut builder = http::Request::builder().uri(path);
        if let Some(value) = accept_encoding {
            builder = builder.header(ACCEPT_ENCODING, value);
        }
        app.oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn body_bytes(response: axum::response::Response) -> Vec<u8> {
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap()
            .to_vec()
    }

    // Goal: a client that accepts gzip gets the build-time copy, labelled as gzip, with the
    // original's content type kept and caches told the response varies.
    #[tokio::test]
    async fn test_gzip_copy_is_served_when_accepted() {
        let response =
            serve_precompressed("/assets/app.js", Some("gzip, br"), http::StatusCode::OK).await;
        assert_eq!(response.headers()[CONTENT_ENCODING], "gzip");
        assert_eq!(response.headers()[CONTENT_TYPE], "text/javascript");
        assert_eq!(response.headers()[VARY], "accept-encoding");
        assert_eq!(body_bytes(response).await, b"GZ-APP");
    }

    // Goal: a client that does not accept gzip gets the original, still marked as varying.
    #[tokio::test]
    async fn test_original_is_served_without_gzip() {
        for accept_encoding in [None, Some("br"), Some("gzip;q=0")] {
            let response =
                serve_precompressed("/assets/app.js", accept_encoding, http::StatusCode::OK).await;
            assert!(response.headers().get(CONTENT_ENCODING).is_none());
            assert_eq!(response.headers()[VARY], "accept-encoding");
            assert_eq!(body_bytes(response).await, b"original");
        }
    }

    // Goal: a file with no gzip copy passes through untouched, with no Vary added.
    #[tokio::test]
    async fn test_file_without_a_copy_is_untouched() {
        let response =
            serve_precompressed("/assets/other.js", Some("gzip"), http::StatusCode::OK).await;
        assert!(response.headers().get(CONTENT_ENCODING).is_none());
        assert!(response.headers().get(VARY).is_none());
        assert_eq!(body_bytes(response).await, b"original");
    }

    // Goal: only a 200 is swapped. A 304 keeps its empty meaning, though it still carries
    // Vary, which a 304 must repeat from the 200 it stands for.
    #[tokio::test]
    async fn test_not_modified_is_not_swapped() {
        let response = serve_precompressed(
            "/assets/app.js",
            Some("gzip"),
            http::StatusCode::NOT_MODIFIED,
        )
        .await;
        assert_eq!(response.status(), http::StatusCode::NOT_MODIFIED);
        assert!(response.headers().get(CONTENT_ENCODING).is_none());
        assert_eq!(response.headers()[VARY], "accept-encoding");
    }

    // Goal: every gzip copy the build embedded decompresses to exactly the file it stands
    // for, so a client can never get stale or corrupt content. Method: walk every copy; a
    // daemon-only build embeds none, and the loop is then empty.
    #[test]
    fn test_embedded_gzip_copies_match_their_originals() {
        use std::io::Read;
        let mut pending = vec![&GZIP_ASSETS];
        while let Some(dir) = pending.pop() {
            pending.extend(dir.dirs());
            for copy in dir.files() {
                let copy_path = copy.path().to_str().unwrap();
                let original_path = copy_path.strip_suffix(".gz").unwrap();
                let original = ASSETS_DIR
                    .get_file(original_path)
                    .unwrap_or_else(|| panic!("no original for {copy_path}"));
                let mut decompressed = Vec::new();
                flate2::read::GzDecoder::new(copy.contents())
                    .read_to_end(&mut decompressed)
                    .unwrap();
                assert!(decompressed == original.contents(), "{copy_path} is stale");
                assert!(copy.contents().len() < original.contents().len());
            }
        }
    }

    // Goal: end to end through the production fallback, every file with a gzip copy is
    // served gzipped to a client that accepts it and decompresses to the original, while a
    // client that does not gets the original. Method: every embedded copy; a daemon-only
    // build embeds none, and the loop is then empty.
    #[tokio::test]
    async fn test_fallback_serves_embedded_gzip_copies() {
        use std::io::Read;
        let mut pending = vec![&GZIP_ASSETS];
        while let Some(dir) = pending.pop() {
            pending.extend(dir.dirs());
            for copy in dir.files() {
                let original_path = copy.path().to_str().unwrap().strip_suffix(".gz").unwrap();
                let original = ASSETS_DIR.get_file(original_path).unwrap().contents();
                let uri = format!("/{original_path}");
                let app = Router::new().fallback_service(web_app_service());
                let request = http::Request::builder()
                    .uri(&uri)
                    .header(ACCEPT_ENCODING, "gzip, deflate")
                    .body(Body::empty())
                    .unwrap();
                let response = app.clone().oneshot(request).await.unwrap();
                assert_eq!(response.status(), http::StatusCode::OK, "{uri}");
                assert_eq!(response.headers()[CONTENT_ENCODING], "gzip", "{uri}");
                let mut decompressed = Vec::new();
                flate2::read::GzDecoder::new(&body_bytes(response).await[..])
                    .read_to_end(&mut decompressed)
                    .unwrap();
                assert!(decompressed == original, "{uri}");

                let plain = http::Request::builder()
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap();
                let response = app.oneshot(plain).await.unwrap();
                assert!(response.headers().get(CONTENT_ENCODING).is_none(), "{uri}");
                assert!(body_bytes(response).await == original, "{uri}");
            }
        }
    }

    // Goal: every file the fallback serves carries the `StaticAsset` marker, which is what keeps
    // the API compression layer off it. Method: a route behind the same middleware.
    #[tokio::test]
    async fn test_static_responses_carry_the_marker() {
        let app = Router::new()
            .route("/assets/app-abc123.js", get(|| async { "x" }))
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
            response.extensions().get::<StaticAsset>(),
            Some(&StaticAsset)
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
