/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::sync::Arc;

use actix_session::Session;
use actix_web::web::{Data, Json, Path};
use actix_web::{delete, get, post, put, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::api::{
    handle_error, handle_simple_result, validate_name_string, verify_admin_permissions, CCError,
};
use crate::config::Config;
use crate::processors::SettingsProcessor;
use crate::setting::Profile;

/// Retrieves the persisted Profile list
#[get("/profiles")]
async fn get_profiles(config: Data<Arc<Config>>) -> Result<impl Responder, CCError> {
    config
        .get_profiles()
        .await
        .map(|profiles| HttpResponse::Ok().json(Json(ProfilesDto { profiles })))
        .map_err(handle_error)
}

/// Set the profile order in the array of profiles
#[post("/profiles/order")]
async fn save_profiles_order(
    profiles_dto: Json<ProfilesDto>,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    config
        .set_profiles_order(&profiles_dto.profiles)
        .await
        .map_err(handle_error)?;
    handle_simple_result(config.save_config_file().await)
}

#[post("/profiles")]
async fn save_profile(
    profile: Json<Profile>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    validate_name_string(&profile.name)?;
    config
        .set_profile(profile.into_inner())
        .await
        .map_err(handle_error)?;
    handle_simple_result(config.save_config_file().await)
}

#[put("/profiles")]
async fn update_profile(
    profile: Json<Profile>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    validate_name_string(&profile.name)?;
    let profile_uid = profile.uid.clone();
    config
        .update_profile(profile.into_inner())
        .await
        .map_err(handle_error)?;
    settings_processor.profile_updated(&profile_uid).await;
    config.save_config_file().await.map_err(handle_error)?;
    handle_simple_result(Ok(()))
}

#[delete("/profiles/{profile_uid}")]
async fn delete_profile(
    profile_uid: Path<String>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    config
        .delete_profile(&profile_uid)
        .await
        .map_err(handle_error)?;
    settings_processor.profile_deleted(&profile_uid).await;
    config.save_config_file().await.map_err(handle_error)?;
    Ok(HttpResponse::Ok().finish())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesDto {
    profiles: Vec<Profile>,
}
