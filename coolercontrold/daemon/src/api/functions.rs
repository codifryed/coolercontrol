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
    if function.step_size_min < 1 {
        invalid_msg = Some("duty_minimum must be greater than 0".to_string());
    } else if function.step_size_min > 100 {
        invalid_msg = Some("duty_minimum must be less than 101".to_string());
    } else if function.step_size_max > 100 {
        invalid_msg = Some("duty_maximum must be less than 101".to_string());
    } else if function.step_size_min > function.step_size_max && function.step_size_max != 0 {
        invalid_msg = Some(
            "duty_minimum must be less than duty_maximum when using a non-fixed step size"
                .to_string(),
        );
    } else if function.step_size_min_decreasing > 100 {
        invalid_msg = Some("step_size_min_decreasing must be less than 101".to_string());
    } else if function.step_size_max_decreasing > 100 {
        invalid_msg = Some("step_size_max_decreasing must be less than 101".to_string());
    } else if function.step_size_min_decreasing > function.step_size_max_decreasing
        && function.step_size_max_decreasing != 0
    {
        invalid_msg = Some("step_size_min_decreasing must be less than step_size_max_decreasing when using a non-fixed step size".to_string());
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::FunctionType;

    fn valid_function() -> Function {
        Function {
            uid: "test-uid".to_string(),
            name: "Test Function".to_string(),
            f_type: FunctionType::Standard,
            step_size_min: 2,
            step_size_max: 100,
            step_size_min_decreasing: 0,
            step_size_max_decreasing: 0,
            response_delay: None,
            deviance: None,
            only_downward: None,
            sample_window: None,
            threshold_hopping: true,
        }
    }

    #[test]
    fn test_validate_function_valid() {
        let func = valid_function();
        assert!(validate_function(&func).is_ok());
    }

    #[test]
    fn test_validate_function_valid_fixed_step() {
        let mut func = valid_function();
        func.step_size_min = 5;
        func.step_size_max = 0; // 0 = fixed step size
        assert!(validate_function(&func).is_ok());
    }

    #[test]
    fn test_validate_function_step_size_min_zero() {
        let mut func = valid_function();
        func.step_size_min = 0;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_step_size_min_over_100() {
        let mut func = valid_function();
        func.step_size_min = 101;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_step_size_max_over_100() {
        let mut func = valid_function();
        func.step_size_max = 101;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_min_greater_than_max() {
        let mut func = valid_function();
        func.step_size_min = 50;
        func.step_size_max = 25;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_decreasing_min_over_100() {
        let mut func = valid_function();
        func.step_size_min_decreasing = 101;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_decreasing_max_over_100() {
        let mut func = valid_function();
        func.step_size_max_decreasing = 101;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_decreasing_min_greater_than_max() {
        let mut func = valid_function();
        func.step_size_min_decreasing = 50;
        func.step_size_max_decreasing = 25;
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_decreasing_fixed_step() {
        let mut func = valid_function();
        func.step_size_min_decreasing = 10;
        func.step_size_max_decreasing = 0; // 0 = fixed step size
        assert!(validate_function(&func).is_ok());
    }

    #[test]
    fn test_validate_function_empty_name() {
        let mut func = valid_function();
        func.name = String::new();
        assert!(validate_function(&func).is_err());
    }

    #[test]
    fn test_validate_function_boundary_values() {
        let mut func = valid_function();
        func.step_size_min = 1;
        func.step_size_max = 100;
        assert!(validate_function(&func).is_ok());

        func.step_size_min = 100;
        func.step_size_max = 100;
        assert!(validate_function(&func).is_ok());
    }
}
