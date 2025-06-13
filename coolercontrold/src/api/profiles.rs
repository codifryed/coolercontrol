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

use crate::api::auth::verify_admin_permissions;
use crate::api::{handle_error, validate_name_string, AppState, CCError};
use crate::setting::{Profile, ProfileType, ProfileUID};
use aide::NoApi;
use axum::extract::{Path, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

/// Retrieves the persisted Profile list
pub async fn get_all(
    State(AppState { profile_handle, .. }): State<AppState>,
) -> Result<Json<ProfilesDto>, CCError> {
    profile_handle
        .get_all()
        .await
        .map(|profiles| Json(ProfilesDto { profiles }))
        .map_err(handle_error)
}

/// Set the profile order in the array of profiles
pub async fn save_order(
    State(AppState { profile_handle, .. }): State<AppState>,
    Json(profiles_dto): Json<ProfilesDto>,
) -> Result<(), CCError> {
    profile_handle
        .save_order(profiles_dto.profiles)
        .await
        .map_err(handle_error)
}

pub async fn create(
    NoApi(session): NoApi<Session>,
    State(AppState { profile_handle, .. }): State<AppState>,
    Json(profile): Json<Profile>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    validate_profile(&profile)?;
    profile_handle.create(profile).await.map_err(handle_error)
}

pub async fn update(
    NoApi(session): NoApi<Session>,
    State(AppState { profile_handle, .. }): State<AppState>,
    Json(profile): Json<Profile>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    validate_profile(&profile)?;
    profile_handle.update(profile).await.map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<ProfilePath>,
    NoApi(session): NoApi<Session>,
    State(AppState { profile_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    profile_handle
        .delete(path.profile_uid)
        .await
        .map_err(handle_error)
}

fn validate_profile(profile: &Profile) -> Result<(), CCError> {
    validate_name_string(&profile.name)?;
    if profile.uid.is_empty() {
        return Err(CCError::UserError {
            msg: "Invalid Profile UID, cannot be empty".to_string(),
        });
    }
    if profile.p_type == ProfileType::Fixed && profile.speed_fixed.is_none() {
        return Err(CCError::UserError {
            msg: "A Fixed profile must have a fixed speed".to_string(),
        });
    }
    if profile.p_type == ProfileType::Mix {
        if profile.member_profile_uids.is_empty() {
            return Err(CCError::UserError {
                msg: "A Mix profile must have at least one member profile".to_string(),
            });
        }
        if profile.mix_function_type.is_none() {
            return Err(CCError::UserError {
                msg: "A Mix profile must have a mix function set".to_string(),
            });
        }
    }
    if profile.p_type == ProfileType::Graph {
        if profile.function_uid.is_empty() {
            // A Valid function_uid is verified upon entity creation/update
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Function".to_string(),
            });
        }
        if profile.temp_source.is_none() {
            // A Valid temp_source is verified upon entity creation/update
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Temp Source".to_string(),
            });
        }
        if profile.temp_source.as_ref().unwrap().temp_name.is_empty() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a valid Temp Source Name".to_string(),
            });
        }
        if profile.temp_source.as_ref().unwrap().device_uid.is_empty() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a valid Temp Source DeviceUID".to_string(),
            });
        }
        if profile.speed_profile.is_none() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Speed Profile set".to_string(),
            });
        }
        if profile.speed_profile.as_ref().unwrap().is_empty() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Speed Profile with values".to_string(),
            });
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProfilesDto {
    profiles: Vec<Profile>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ProfilePath {
    profile_uid: ProfileUID,
}
