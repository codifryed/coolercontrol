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

use std::sync::Arc;

use actix_session::Session;
use actix_web::web::{Data, Json, Path};
use actix_web::{delete, get, post, put, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::api::{handle_error, handle_simple_result, verify_admin_permissions, CCError};
use crate::device::UID;
use crate::modes::{Mode, ModeController};
use crate::setting::Setting;

#[get("/modes")]
async fn get_modes(mode_controller: Data<Arc<ModeController>>) -> Result<impl Responder, CCError> {
    let mode_dtos: Vec<ModeDto> = mode_controller
        .get_modes()
        .await
        .into_iter()
        .map(convert_mode_to_dto)
        .collect();
    let modes_dto = ModesDto { modes: mode_dtos };
    Ok(HttpResponse::Ok().json(modes_dto))
}

#[get("/modes/{mode_uid}")]
async fn get_mode(
    mode_uid: Path<String>,
    mode_controller: Data<Arc<ModeController>>,
) -> Result<impl Responder, CCError> {
    mode_controller
        .get_mode(&mode_uid)
        .await
        .map(|mode| HttpResponse::Ok().json(convert_mode_to_dto(mode)))
        .ok_or_else(|| CCError::NotFound {
            msg: format!("Mode with UID {mode_uid} not found"),
        })
}

#[post("/modes/order")]
async fn set_modes_order(
    mode_order_dto: Json<ModeOrderDto>,
    mode_controller: Data<Arc<ModeController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    handle_simple_result(
        mode_controller
            .update_mode_order(mode_order_dto.into_inner().mode_uids)
            .await,
    )
}

#[post("/modes")]
async fn create_mode(
    create_mode_dto: Json<CreateModeDto>,
    mode_controller: Data<Arc<ModeController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    mode_controller
        .create_mode(create_mode_dto.into_inner().name)
        .await
        .map(|mode| HttpResponse::Ok().json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

#[put("/modes")]
async fn update_mode(
    update_mode_dto: Json<UpdateModeDto>,
    mode_controller: Data<Arc<ModeController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    let update_dto = update_mode_dto.into_inner();
    handle_simple_result(
        mode_controller
            .update_mode(&update_dto.uid, update_dto.name)
            .await,
    )
}

#[put("/modes/{mode_uid}/settings")]
async fn update_mode_settings(
    mode_uid: Path<String>,
    mode_controller: Data<Arc<ModeController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    mode_controller
        .update_mode_with_current_settings(&mode_uid)
        .await
        .map(|mode| HttpResponse::Ok().json(convert_mode_to_dto(mode)))
        .map_err(handle_error)
}

#[delete("/modes/{mode_uid}")]
async fn delete_mode(
    mode_uid: Path<String>,
    mode_controller: Data<Arc<ModeController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    handle_simple_result(mode_controller.delete_mode(&mode_uid).await)
}

#[get("/modes-active")]
async fn get_active_mode(
    mode_controller: Data<Arc<ModeController>>,
) -> Result<impl Responder, CCError> {
    let response_body = mode_controller.get_active_mode_uid().await.map_or_else(
        || ActiveModeDto { mode_uid: None },
        |mode_uid| ActiveModeDto {
            mode_uid: Some(mode_uid),
        },
    );
    Ok(HttpResponse::Ok().json(response_body))
}

#[post("/modes-active/{mode_uid}")]
async fn activate_mode(
    mode_uid: Path<String>,
    mode_controller: Data<Arc<ModeController>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    handle_simple_result(mode_controller.activate_mode(&mode_uid).await)
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModesDto {
    modes: Vec<ModeDto>,
}

// We have to use nested arrays instead of maps because the class-transformer in the frontend has
// some difficulties with maps.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModeDto {
    uid: String,
    name: String,
    device_settings: Vec<(UID, Vec<Setting>)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ModeOrderDto {
    mode_uids: Vec<UID>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CreateModeDto {
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct UpdateModeDto {
    uid: UID,
    name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ActiveModeDto {
    mode_uid: Option<UID>,
}
