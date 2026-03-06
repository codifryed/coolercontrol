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
use crate::api::{handle_error, AppState, CCError};
use aide::axum::IntoApiResponse;
#[cfg(debug_assertions)]
use aide::openapi::OpenApi;
use anyhow::Result;
use axum::extract::Request;
use axum::extract::State;
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
    frame-ancestors 'none'; \
    object-src 'none'; \
    base-uri 'self'; \
    form-action 'self'";

async fn cache_control_middleware(request: Request, next: Next) -> axum::response::Response {
    let path = request.uri().path();
    // index.html should not be cached, all other assets have hashes and can be heavily cached.
    let is_index = path == "/" || path == "/index.html";
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    let cache_value = if is_index {
        headers.insert(
            axum::http::HeaderName::from_static("content-security-policy"),
            axum::http::HeaderValue::from_static(CONTENT_SECURITY_POLICY),
        );
        axum::http::HeaderValue::from_static("no-cache")
    } else {
        axum::http::HeaderValue::from_static("public, max-age=31536000, immutable")
    };
    headers.insert(axum::http::header::CACHE_CONTROL, cache_value);
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
        .check(warnings, errors)
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
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SystemDetails {
    pub(crate) name: String,
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert!(csp_str.contains("frame-ancestors 'none'"));
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
}
