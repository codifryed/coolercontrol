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
use crate::setting::{CustomSensor, CustomSensorType};
use axum::extract::{Path, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::validate_name_string;

/// Retrieves the persisted list of Custom Sensors
pub async fn get_all(
    State(AppState {
        custom_sensor_handle,
        ..
    }): State<AppState>,
) -> Result<Json<CustomSensorsDto>, CCError> {
    custom_sensor_handle
        .get_all()
        .await
        .map(|custom_sensors| Json(CustomSensorsDto { custom_sensors }))
        .map_err(handle_error)
}

pub async fn get(
    Path(path): Path<CSPath>,
    State(AppState {
        custom_sensor_handle,
        ..
    }): State<AppState>,
) -> Result<Json<CustomSensor>, CCError> {
    custom_sensor_handle
        .get(path.custom_sensor_id)
        .await
        .map(Json)
        .map_err(handle_error)
}

/// Set the custom sensors order in the array of custom sensors
pub async fn save_order(
    State(AppState {
        custom_sensor_handle,
        ..
    }): State<AppState>,
    Json(cs_dto): Json<CustomSensorsDto>,
) -> Result<(), CCError> {
    custom_sensor_handle
        .save_order(cs_dto.custom_sensors)
        .await
        .map_err(handle_error)
}

pub async fn create(
    State(AppState {
        custom_sensor_handle,
        ..
    }): State<AppState>,
    Json(custom_sensor): Json<CustomSensor>,
) -> Result<(), CCError> {
    validate_custom_sensor(&custom_sensor)?;
    custom_sensor_handle
        .create(custom_sensor)
        .await
        .map_err(handle_error)
}

pub async fn update(
    State(AppState {
        custom_sensor_handle,
        ..
    }): State<AppState>,
    Json(custom_sensor): Json<CustomSensor>,
) -> Result<(), CCError> {
    validate_custom_sensor(&custom_sensor)?;
    custom_sensor_handle
        .update(custom_sensor)
        .await
        .map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<CSPath>,
    State(AppState {
        custom_sensor_handle,
        ..
    }): State<AppState>,
) -> Result<(), CCError> {
    custom_sensor_handle
        .delete(path.custom_sensor_id)
        .await
        .map_err(handle_error)
}

fn validate_custom_sensor(custom_sensor: &CustomSensor) -> Result<(), CCError> {
    validate_name_string(&custom_sensor.id)?;
    let mut invalid_msg: Option<String> = None;
    // This limit is not a hard-limit, but to protect the API.
    if custom_sensor.sources.len() > 50 {
        invalid_msg = Some("sources cannot have more than 50 temps".to_string());
    } else if custom_sensor.sources.iter().any(|s| s.weight > 254) {
        invalid_msg = Some("sources cannot have a weight greater than 254".to_string());
    } else if custom_sensor
        .sources
        .iter()
        .any(|s| s.temp_source.device_uid.is_empty())
    {
        invalid_msg =
            Some("sources cannot have a temp_source with an empty device UID".to_string());
    } else if custom_sensor
        .sources
        .iter()
        .any(|s| s.temp_source.temp_name.is_empty())
    {
        invalid_msg = Some("sources cannot have a temp_source with an empty Temp Name".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::Mix && custom_sensor.file_path.is_some() {
        invalid_msg = Some("Custom Sensor Mix type cannot have a file path".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::File && custom_sensor.file_path.is_none() {
        invalid_msg = Some("Custom Sensor File type must have a file path".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::File && !custom_sensor.sources.is_empty() {
        invalid_msg = Some("Custom Sensor File type should not have sources".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::Offset && custom_sensor.offset.is_none() {
        invalid_msg = Some("Custom Sensor Offset type must have an offset".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::Offset
        && custom_sensor.offset.unwrap() < -100
    {
        invalid_msg = Some("Custom Sensor Offset type offset cannot be less than -100".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::Offset
        && custom_sensor.offset.unwrap() > 100
    {
        invalid_msg =
            Some("Custom Sensor Offset type offset cannot be greater than 100".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::Offset && custom_sensor.sources.len() != 1
    {
        invalid_msg = Some("Custom Sensor Offset type must have exactly 1 temp source".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::TimeAverage
        && custom_sensor.sources.len() != 1
    {
        invalid_msg =
            Some("Custom Sensor TimeAverage type must have exactly 1 temp source".to_string());
    } else if custom_sensor.cs_type == CustomSensorType::TimeAverage
        && custom_sensor.time_window_seconds.is_none()
    {
        invalid_msg = Some(
            "Custom Sensor TimeAverage type must have a time_window_seconds value".to_string(),
        );
    } else if custom_sensor.cs_type == CustomSensorType::TimeAverage
        && !(1..=60).contains(&custom_sensor.time_window_seconds.unwrap())
    {
        invalid_msg = Some(
            "Custom Sensor TimeAverage time_window_seconds must be between 1 and 60".to_string(),
        );
    }
    if let Some(msg) = invalid_msg {
        Err(CCError::UserError { msg })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CustomSensorsDto {
    custom_sensors: Vec<CustomSensor>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CSPath {
    custom_sensor_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setting::{CustomSensorMixFunctionType, CustomTempSourceData, TempSource};

    fn time_average_sensor() -> CustomSensor {
        CustomSensor {
            id: "ta".to_string(),
            cs_type: CustomSensorType::TimeAverage,
            mix_function: CustomSensorMixFunctionType::Min,
            sources: vec![CustomTempSourceData {
                temp_source: TempSource {
                    device_uid: "device-uid".to_string(),
                    temp_name: "cpu_temp".to_string(),
                },
                weight: 1,
            }],
            file_path: None,
            offset: None,
            time_window_seconds: Some(10),
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    // Valid TimeAverage sensor with one source and a window in [1, 60] passes validation.
    #[test]
    fn time_average_valid_passes() {
        let sensor = time_average_sensor();
        assert!(validate_custom_sensor(&sensor).is_ok());
    }

    // time_window_seconds == 0 is rejected (lower bound is 1).
    #[test]
    fn time_average_rejects_zero_window() {
        let mut sensor = time_average_sensor();
        sensor.time_window_seconds = Some(0);
        assert!(validate_custom_sensor(&sensor).is_err());
    }

    // time_window_seconds > 60 is rejected (upper bound is 60).
    #[test]
    fn time_average_rejects_window_above_60() {
        let mut sensor = time_average_sensor();
        sensor.time_window_seconds = Some(61);
        assert!(validate_custom_sensor(&sensor).is_err());
    }

    // Missing time_window_seconds is rejected for TimeAverage type (required field).
    #[test]
    fn time_average_rejects_missing_window() {
        let mut sensor = time_average_sensor();
        sensor.time_window_seconds = None;
        assert!(validate_custom_sensor(&sensor).is_err());
    }

    // TimeAverage requires exactly one source. Zero or two are both rejected.
    #[test]
    fn time_average_rejects_zero_sources() {
        let mut sensor = time_average_sensor();
        sensor.sources.clear();
        assert!(validate_custom_sensor(&sensor).is_err());
    }

    #[test]
    fn time_average_rejects_two_sources() {
        let mut sensor = time_average_sensor();
        let extra = sensor.sources[0].clone();
        sensor.sources.push(extra);
        assert!(validate_custom_sensor(&sensor).is_err());
    }
}
