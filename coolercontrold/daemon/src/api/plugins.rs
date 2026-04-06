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
use crate::repositories::service_plugin::service_management::manager::ServiceStatus;
use aide::axum::IntoApiResponse;
use axum::extract::{Path, Request, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
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
                has_full_page_ui: plugin_ui_dir.join("app.html").exists(),
            })
        },
    )
}

pub async fn start_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .start_plugin(path.plugin_id)
        .await
        .map_err(handle_error)
}

pub async fn stop_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .stop_plugin(path.plugin_id)
        .await
        .map_err(handle_error)
}

pub async fn restart_plugin(
    Path(path): Path<PluginPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    plugin_handle
        .restart_plugin(path.plugin_id)
        .await
        .map_err(handle_error)
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

pub async fn get_ui_files(
    Path(path): Path<PluginUiPath>,
    State(AppState { plugin_handle, .. }): State<AppState>,
    request: Request,
) -> Result<impl IntoApiResponse, CCError> {
    let safe_path = sanitize_file_path(&path.file_path)?;
    let plugin_ui_dir = plugin_handle.get_ui_dir(path.plugin_id).await?;
    ServeFile::new(plugin_ui_dir.join(safe_path))
        .oneshot(request)
        .await
        .map_err(|_infallible| CCError::InternalError {
            msg: "Failed to serve file".to_string(),
        })
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct HasUiDto {
    pub has_ui: bool,
    pub has_full_page_ui: bool,
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
}
