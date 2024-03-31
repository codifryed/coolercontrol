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
use crate::config::Config;
use crate::processing::settings::SettingsController;
use crate::setting::Function;

use super::validate_name_string;

/// Retrieves the persisted Function list
#[get("/functions")]
async fn get_functions(config: Data<Arc<Config>>) -> Result<impl Responder, CCError> {
    config
        .get_functions()
        .await
        .map(|functions| HttpResponse::Ok().json(Json(FunctionsDto { functions })))
        .map_err(handle_error)
}

/// Set the function order in the array of functions
#[post("/functions/order")]
async fn save_functions_order(
    functions_dto: Json<FunctionsDto>,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    config
        .set_functions_order(&functions_dto.functions)
        .await
        .map_err(handle_error)?;
    handle_simple_result(config.save_config_file().await)
}

#[post("/functions")]
async fn save_function(
    function: Json<Function>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    validate_function(&function)?;
    config
        .set_function(function.into_inner())
        .await
        .map_err(handle_error)?;
    handle_simple_result(config.save_config_file().await)
}

#[put("/functions")]
async fn update_function(
    function: Json<Function>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    validate_function(&function)?;
    let function_uid = function.uid.clone();
    config
        .update_function(function.into_inner())
        .await
        .map_err(handle_error)?;
    settings_controller.function_updated(&function_uid).await;
    config.save_config_file().await.map_err(handle_error)?;
    Ok(HttpResponse::Ok().finish())
}

#[delete("/functions/{function_uid}")]
async fn delete_function(
    function_uid: Path<String>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    config
        .delete_function(&function_uid)
        .await
        .map_err(handle_error)?;
    settings_controller.function_deleted(&function_uid).await;
    config.save_config_file().await.map_err(handle_error)?;
    Ok(HttpResponse::Ok().finish())
}

fn validate_function(function: &Function) -> Result<(), CCError> {
    validate_name_string(&function.name)?;
    let mut invalid_msg: Option<String> = None;
    if function.duty_minimum < 1 {
        invalid_msg = Some("duty_minimum must be greater than 0".to_string());
    } else if function.duty_minimum > 99 {
        invalid_msg = Some("duty_minimum must be less than 100".to_string());
    } else if function.duty_maximum < 2 {
        invalid_msg = Some("duty_maximum must be greater than 1".to_string());
    } else if function.duty_maximum > 100 {
        invalid_msg = Some("duty_maximum must be less than 101".to_string());
    } else if function.duty_minimum >= function.duty_maximum {
        invalid_msg = Some("duty_minimum must be less than duty_maximum".to_string());
    } else if function.duty_maximum <= function.duty_minimum {
        invalid_msg = Some("duty_maximum must be greater than duty_minimum".to_string());
    }
    if let Some(msg) = invalid_msg {
        Err(CCError::UserError { msg })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FunctionsDto {
    functions: Vec<Function>,
}
