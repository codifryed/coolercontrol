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
use axum::extract::{Path, Request, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::ServeFile;
use tower_serve_static::include_file;

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

pub async fn get_ui_files(
    Path(path): Path<PluginUiPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
    request: Request,
) -> Result<impl IntoApiResponse, CCError> {
    let ui_file_name = sanitize_file_name(path.file_name)?;
    let plugin_ui_dir = plugin_handle.get_ui_dir(path.plugin_id).await?;
    ServeFile::new(plugin_ui_dir.join(ui_file_name))
        .oneshot(request)
        .await
        .map_err(|_infallible| CCError::InternalError {
            msg: "Failed to serve file".to_string(),
        })
}

fn sanitize_file_name(file_name: String) -> Result<OsString, CCError> {
    let file_path = PathBuf::from(file_name);
    if file_path.is_absolute() || file_path.extension().is_none() {
        Err(invalid_file_name())
    } else {
        file_path
            .file_name()
            .map(ToOwned::to_owned)
            .ok_or_else(invalid_file_name)
    }
}

fn invalid_file_name() -> CCError {
    CCError::UserError {
        msg: "Invalid file name".to_string(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginPath {
    pub plugin_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PluginUiPath {
    pub plugin_id: String,
    pub file_name: String,
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
    pub address: String,
    pub privileged: bool,
    pub path: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct HasUiDto {
    pub has_ui: bool,
}
