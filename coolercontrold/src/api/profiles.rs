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

use actix_web::{get, HttpResponse, post, Responder};
use actix_web::web::{Data, Json};
use log::error;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::api::ErrorResponse;
use crate::config::Config;
use crate::setting::Profile;

/// Retrieves the persisted Profile list
#[get("/profiles")]
async fn get_profiles(
    config: Data<Arc<Config>>
) -> impl Responder {
    match config.get_profiles().await {
        Ok(profiles) => HttpResponse::Ok().json(Json(ProfilesDto { profiles })),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

/// Set the given profiles, overwriting any existing
#[post("/profiles")]
async fn save_profiles(
    profiles_dto: Json<ProfilesDto>,
    config: Data<Arc<Config>>,
) -> impl Responder {
    config.set_profiles(&profiles_dto.profiles).await;
    match config.save_config_file().await {
        Ok(_) => HttpResponse::Ok().json(json!({"success": true})),
        Err(err) => {
            error!("{:?}", err);
            HttpResponse::InternalServerError()
                .json(Json(ErrorResponse { error: err.to_string() }))
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ProfilesDto {
    profiles: Vec<Profile>,
}
