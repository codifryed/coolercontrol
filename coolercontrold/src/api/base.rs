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
use crate::api::auth::verify_admin_permissions;
use crate::api::{AppState, CCError};
use aide::axum::IntoApiResponse;
use aide::openapi::OpenApi;
use aide::NoApi;
use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Extension, Json};
use include_dir::{include_dir, Dir};
use nix::sys::signal;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
use serde_json::json;
use std::sync::Arc;
use tower_serve_static::ServeDir;
use tower_sessions::Session;

static ASSETS_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/resources/app");

pub async fn handshake() -> impl IntoApiResponse {
    Json(json!({"shake": true}))
}

pub fn web_app_service() -> ServeDir {
    ServeDir::new(&ASSETS_DIR)
}

pub async fn serve_api_doc(Extension(api): Extension<Arc<OpenApi>>) -> impl IntoApiResponse {
    Json(api).into_response()
}

pub async fn logs(State(AppState { log_buf_handle, .. }): State<AppState>) -> impl IntoApiResponse {
    log_buf_handle.get_logs().await
}

pub async fn shutdown(NoApi(session): NoApi<Session>) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    signal::kill(Pid::this(), Signal::SIGQUIT).map_err(|err| CCError::InternalError {
        msg: err.to_string(),
    })
}
