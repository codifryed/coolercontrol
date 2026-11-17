// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::Json;
use crate::api::{handle_error, AppState, CCError};
use crate::device::UID;
use crate::modes::Mode;
use crate::setting::Setting;
use axum::extract::Path;
use axum::extract::State;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
                    msg: "Mode not found".to_string(),
                })
        })
}

pub async fn save_order(
    State(AppState { mode_handle, .. }): State<AppState>,
    Json(mode_order_dto): Json<ModeOrderDto>,
) -> Result<(), CCError> {
    mode_handle
        .save_order(mode_order_dto.mode_uids)
        .await
        .map_err(handle_error)
}

pub async fn create(
    State(AppState { mode_handle, .. }): State<AppState>,
    Json(create_mode_dto): Json<CreateModeDto>,
) -> Result<Json<ModeDto>, CCError> {
    mode_handle
        .create(create_mode_dto.name)
        .await
        .map(|mode| Json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

pub async fn update(
    State(AppState { mode_handle, .. }): State<AppState>,
    Json(update_mode_dto): Json<UpdateModeDto>,
) -> Result<(), CCError> {
    mode_handle
        .update(update_mode_dto.uid, update_mode_dto.name)
        .await
        .map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<ModePath>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    mode_handle
        .delete(path.mode_uid)
        .await
        .map_err(handle_error)
}

pub async fn duplicate(
    Path(path): Path<ModePath>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ModeDto>, CCError> {
    mode_handle
        .duplicate(path.mode_uid)
        .await
        .map(|mode| Json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

pub async fn update_mode_settings(
    Path(path): Path<ModePath>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ModeDto>, CCError> {
    mode_handle
        .update_settings(path.mode_uid)
        .await
        .map(|mode| Json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

pub async fn get_active(
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<Json<ActiveModesDto>, CCError> {
    mode_handle
        .get_active()
        .await
        .map(Json)
        .map_err(handle_error)
}

pub async fn activate(
    Path(path): Path<ModePath>,
    State(AppState { mode_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
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
    pub current_mode_uid: Option<UID>,
    pub previous_mode_uid: Option<UID>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ModePath {
    mode_uid: UID,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ActiveMode {
    pub uid: Option<UID>,
    pub name: Option<String>,
    pub previous_uid: Option<UID>,
}
