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

use actix_web::{delete, get, HttpResponse, post, put, Responder};
use actix_web::web::{Data, Json, Path};
use serde::{Deserialize, Serialize};

use crate::api::{handle_error, handle_simple_result};
use crate::config::Config;
use crate::setting::Profile;
use crate::processors::SettingsProcessor;

/// Retrieves the persisted Profile list
#[get("/profiles")]
async fn get_profiles(
    config: Data<Arc<Config>>
) -> impl Responder {
    match config.get_profiles().await {
        Ok(profiles) => HttpResponse::Ok().json(Json(ProfilesDto { profiles })),
        Err(err) => handle_error(err)
    }
}

/// Set the profile order in the array of profiles
#[post("/profiles/order")]
async fn save_profiles_order(
    profiles_dto: Json<ProfilesDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    if let Err(err) = config.set_profiles_order(&profiles_dto.profiles).await {
        return handle_error(err);
    }
    handle_simple_result(config.save_config_file().await)
}

#[post("/profiles")]
async fn save_profile(
    profile: Json<Profile>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    if let Err(err) = config.set_profile(profile.into_inner()).await {
        return handle_error(err);
    }
    handle_simple_result(config.save_config_file().await)
}

#[put("/profiles")]
async fn update_profile(
    profile: Json<Profile>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    let profile_uid = profile.uid.clone();
    if let Err(err) = config.update_profile(profile.into_inner()).await {
        return handle_error(err);
    }
    if let Err(err) = config.save_config_file().await {
        return handle_error(err);
    }
    settings_processor.profile_updated(&profile_uid).await;
    handle_simple_result(Ok(()))
}

#[delete("/profiles/{profile_uid}")]
async fn delete_profile(
    profile_uid: Path<String>,
    settings_processor: Data<Arc<SettingsProcessor>>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    if let Err(err) = config.delete_profile(&profile_uid).await {
        return handle_error(err);
    }
    if let Err(err) = config.save_config_file().await {
        return handle_error(err);
    }
    settings_processor.profile_deleted(&profile_uid).await;
    handle_simple_result(Ok(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesDto {
    profiles: Vec<Profile>,
}
