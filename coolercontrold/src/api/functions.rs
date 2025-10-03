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
use crate::setting::{Function, FunctionUID};
use axum::extract::{Path, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::validate_name_string;

/// Retrieves the persisted Function list
pub async fn get_all(
    State(AppState {
        function_handle, ..
    }): State<AppState>,
) -> Result<Json<FunctionsDto>, CCError> {
    function_handle
        .get_all()
        .await
        .map(|functions| Json(FunctionsDto { functions }))
        .map_err(handle_error)
}

/// Set the function order in the array of functions
pub async fn save_order(
    State(AppState {
        function_handle, ..
    }): State<AppState>,
    Json(functions_dto): Json<FunctionsDto>,
) -> Result<(), CCError> {
    function_handle
        .save_order(functions_dto.functions)
        .await
        .map_err(handle_error)
}

pub async fn create(
    State(AppState {
        function_handle, ..
    }): State<AppState>,
    Json(function): Json<Function>,
) -> Result<(), CCError> {
    validate_function(&function)?;
    function_handle.create(function).await.map_err(handle_error)
}

pub async fn update(
    State(AppState {
        function_handle, ..
    }): State<AppState>,
    Json(function): Json<Function>,
) -> Result<(), CCError> {
    validate_function(&function)?;
    function_handle.update(function).await.map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<FunctionPath>,
    State(AppState {
        function_handle, ..
    }): State<AppState>,
) -> Result<(), CCError> {
    function_handle
        .delete(path.function_uid)
        .await
        .map_err(handle_error)
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FunctionsDto {
    functions: Vec<Function>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FunctionPath {
    function_uid: FunctionUID,
}
