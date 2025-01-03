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
use crate::api::{handle_error, AppState, CCError};
use crate::device::UID;
use crate::modes::Mode;
use crate::setting::Setting;
use aide::NoApi;
use axum::extract::Path;
use axum::extract::State;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

pub async fn get_all(
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ModesDto>, CCError> {
    mode_handle
        .get_all()
        .await
        .map(|modes| modes.into_iter().map(convert_mode_to_dto).collect())
        .map(|mode_dtos| Json(ModesDto { modes: mode_dtos }))
        .map_err(handle_error)
}

pub async fn get(
    Path(path): Path<ModePath>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ModeDto>, CCError> {
    mode_handle
        .get(path.mode_uid.clone())
        .await
        .map_err(handle_error)
        .and_then(|mode| {
            mode.map(|mode| Json(convert_mode_to_dto(mode)))
                .ok_or_else(|| CCError::NotFound {
                    msg: format!("Mode with UID {} not found", path.mode_uid),
                })
        })
}

pub async fn save_order(
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
    Json(mode_order_dto): Json<ModeOrderDto>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .save_order(mode_order_dto.mode_uids)
        .await
        .map_err(handle_error)
}

pub async fn create(
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
    Json(create_mode_dto): Json<CreateModeDto>,
) -> Result<Json<ModeDto>, CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .create(create_mode_dto.name)
        .await
        .map(|mode| Json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

pub async fn update(
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
    Json(update_mode_dto): Json<UpdateModeDto>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .update(update_mode_dto.uid, update_mode_dto.name)
        .await
        .map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<ModePath>,
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .delete(path.mode_uid)
        .await
        .map_err(handle_error)
}

pub async fn duplicate(
    Path(path): Path<ModePath>,
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ModeDto>, CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .duplicate(path.mode_uid)
        .await
        .map(|mode| Json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

pub async fn update_mode_settings(
    Path(path): Path<ModePath>,
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ModeDto>, CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .update_settings(path.mode_uid)
        .await
        .map(|mode| Json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

pub async fn get_all_active(
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ActiveModesDto>, CCError> {
    mode_handle
        .get_all_active()
        .await
        .map(|mode_uids| Json(ActiveModesDto { mode_uids }))
        .map_err(handle_error)
}

pub async fn activate(
    Path(path): Path<ModePath>,
    NoApi(session): NoApi<Session>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    mode_handle
        .activate(path.mode_uid)
        .await
        .map_err(handle_error)
}

fn convert_mode_to_dto(mode: Mode) -> ModeDto {
    let device_settings = mode
        .all_device_settings
        .into_iter()
        .map(|(uid, settings)| (uid, settings.into_values().collect()))
        .collect();
    ModeDto {
        uid: mode.uid,
        name: mode.name,
        device_settings,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModesDto {
    modes: Vec<ModeDto>,
}

// We have to use nested arrays instead of maps because the class-transformer in the frontend has
// some difficulties with maps.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModeDto {
    uid: String,
    name: String,
    device_settings: Vec<(UID, Vec<Setting>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModeOrderDto {
    mode_uids: Vec<UID>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CreateModeDto {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct UpdateModeDto {
    uid: UID,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, JsonSchema)]
pub struct ActiveModesDto {
    mode_uids: Vec<UID>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModePath {
    mode_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModeActivated {
    pub uid: UID,
    pub name: String,
    pub already_active: bool,
}

impl Default for ModeActivated {
    fn default() -> Self {
        Self {
            uid: "Unknown".to_string(),
            name: "Unknown".to_string(),
            already_active: false,
        }
    }
}
