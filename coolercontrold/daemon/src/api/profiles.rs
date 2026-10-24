// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::{handle_error, validate_name_string, AppState, CCError};
use crate::setting::{Profile, ProfileType, ProfileUID};
use axum::extract::{Path, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
    State(AppState { profile_handle, .. }): State<AppState>,
    Json(profile): Json<Profile>,
) -> Result<(), CCError> {
    validate_profile(&profile)?;
    profile_handle.create(profile).await.map_err(handle_error)
}

pub async fn update(
    State(AppState { profile_handle, .. }): State<AppState>,
    Json(profile): Json<Profile>,
) -> Result<(), CCError> {
    validate_profile(&profile)?;
    profile_handle.update(profile).await.map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<ProfilePath>,
    State(AppState { profile_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
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
    if profile.p_type() == ProfileType::Fixed {
        let Some(speed_fixed) = profile.speed_fixed() else {
            return Err(CCError::UserError {
                msg: "A Fixed profile must have a fixed speed".to_string(),
            });
        };
        if speed_fixed > 100 {
            return Err(CCError::UserError {
                msg: "A Fixed profile must have a fixed speed of 100 or less".to_string(),
            });
        }
    } else if profile.p_type() == ProfileType::Mix {
        if profile.member_profile_uids().is_empty() {
            return Err(CCError::UserError {
                msg: "A Mix profile must have at least one member profile".to_string(),
            });
        }
        if profile.mix_function_type().is_none() {
            return Err(CCError::UserError {
                msg: "A Mix profile must have a mix function set".to_string(),
            });
        }
    } else if profile.p_type() == ProfileType::Graph {
        if profile.function_uid.is_empty() {
            // A Valid function_uid is verified upon entity creation/update
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Function".to_string(),
            });
        }
        if profile.temp_source().is_none() {
            // A Valid temp_source is verified upon entity creation/update
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Temp Source".to_string(),
            });
        }
        if profile.temp_source().unwrap().temp_name.is_empty() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a valid Temp Source Name".to_string(),
            });
        }
        if profile.temp_source().unwrap().device_uid.is_empty() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a valid Temp Source DeviceUID".to_string(),
            });
        }
        if profile.speed_profile().is_none() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Speed Profile set".to_string(),
            });
        }
        if profile.speed_profile().unwrap().is_empty() {
            return Err(CCError::UserError {
                msg: "A Graph profile must have a Speed Profile with values".to_string(),
            });
        }
    } else if profile.p_type() == ProfileType::Overlay {
        if profile.member_profile_uids().is_empty() {
            return Err(CCError::UserError {
                msg: "An Overlay profile must have at least one member profile".to_string(),
            });
        }
        if profile.offset_profile().is_none() {
            return Err(CCError::UserError {
                msg: "An Overlay profile must have an Offset Profile set".to_string(),
            });
        }
        if profile.offset_profile().unwrap().is_empty() {
            return Err(CCError::UserError {
                msg: "An Overlay profile must have an Offset Profile with values".to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::ProfileKind;

    fn fixed_profile(speed_fixed: Option<u8>) -> Profile {
        Profile {
            uid: "prof-1".to_string(),
            name: "Fixed".to_string(),
            function_uid: "fn-1".to_string(),
            kind: ProfileKind::Fixed { speed_fixed },
        }
    }

    #[test]
    fn a_fixed_speed_from_0_to_100_is_valid() {
        assert!(validate_profile(&fixed_profile(Some(0))).is_ok());
        assert!(validate_profile(&fixed_profile(Some(100))).is_ok());
    }

    #[test]
    fn a_fixed_speed_above_100_is_rejected() {
        assert!(validate_profile(&fixed_profile(Some(101))).is_err());
        assert!(validate_profile(&fixed_profile(Some(u8::MAX))).is_err());
    }

    #[test]
    fn a_fixed_profile_without_a_speed_is_rejected() {
        assert!(validate_profile(&fixed_profile(None)).is_err());
    }
}
