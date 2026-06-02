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
use crate::setting::{CustomSensor, CustomSensorKind, CustomTempSourceData};
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
    // The enum already enforces that each variant carries exactly its own fields (file_path
    // for File, offset for Offset, time_window_seconds for the smoothing variants), so only
    // the value ranges and source cardinality the type cannot express remain here.
    match &custom_sensor.kind {
        CustomSensorKind::Mix { sources, .. } => validate_custom_sensor_sources(sources),
        CustomSensorKind::File { .. } => Ok(()),
        CustomSensorKind::Offset { offset, sources } => {
            validate_single_source(sources)?;
            if (-100..=100).contains(offset) {
                Ok(())
            } else {
                Err(CCError::UserError {
                    msg: "Custom Sensor Offset type offset must be between -100 and 100"
                        .to_string(),
                })
            }
        }
        CustomSensorKind::TimeAverage {
            time_window_seconds,
            sources,
        }
        | CustomSensorKind::ExponentialMovingAvg {
            time_window_seconds,
            sources,
        } => {
            validate_single_source(sources)?;
            if (1..=300).contains(time_window_seconds) {
                Ok(())
            } else {
                Err(CCError::UserError {
                    msg: "Custom Sensor time_window_seconds must be between 1 and 300".to_string(),
                })
            }
        }
    }
}

/// Validates the `sources` constraints the type cannot express: a cap that protects the API,
/// per-source weight, and non-empty source identifiers.
fn validate_custom_sensor_sources(sources: &[CustomTempSourceData]) -> Result<(), CCError> {
    // Not a hard limit, just protects the API.
    if sources.len() > 50 {
        return Err(CCError::UserError {
            msg: "sources cannot have more than 50 temps".to_string(),
        });
    }
    for source in sources {
        if source.weight > 254 {
            return Err(CCError::UserError {
                msg: "sources cannot have a weight greater than 254".to_string(),
            });
        }
        if source.temp_source.device_uid.is_empty() {
            return Err(CCError::UserError {
                msg: "sources cannot have a temp_source with an empty device UID".to_string(),
            });
        }
        if source.temp_source.temp_name.is_empty() {
            return Err(CCError::UserError {
                msg: "sources cannot have a temp_source with an empty Temp Name".to_string(),
            });
        }
    }
    Ok(())
}

/// Validates the variants derived from a single source: exactly one source, plus the shared
/// source constraints.
fn validate_single_source(sources: &[CustomTempSourceData]) -> Result<(), CCError> {
    if sources.len() != 1 {
        return Err(CCError::UserError {
            msg: "Custom Sensor must have exactly 1 temp source".to_string(),
        });
    }
    validate_custom_sensor_sources(sources)
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
    use crate::setting::TempSource;

    fn source() -> CustomTempSourceData {
        CustomTempSourceData {
            temp_source: TempSource {
                device_uid: "device-uid".to_string(),
                temp_name: "cpu_temp".to_string(),
            },
            weight: 1,
        }
    }

    fn time_average(time_window_seconds: u16, sources: Vec<CustomTempSourceData>) -> CustomSensor {
        CustomSensor {
            id: "ta".to_string(),
            kind: CustomSensorKind::TimeAverage {
                time_window_seconds,
                sources,
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    fn ema(time_window_seconds: u16, sources: Vec<CustomTempSourceData>) -> CustomSensor {
        CustomSensor {
            id: "ema".to_string(),
            kind: CustomSensorKind::ExponentialMovingAvg {
                time_window_seconds,
                sources,
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    // Note: "missing time_window_seconds" is no longer testable here because the type makes
    // it a required field; that rejection is now a deserialize-time failure covered in
    // setting.rs. The same applies to "missing sources".

    // A TimeAverage with one source and an in-range window passes.
    #[test]
    fn time_average_valid_passes() {
        assert!(validate_custom_sensor(&time_average(10, vec![source()])).is_ok());
    }

    // time_window_seconds == 0 is rejected (lower bound is 1).
    #[test]
    fn time_average_rejects_zero_window() {
        assert!(validate_custom_sensor(&time_average(0, vec![source()])).is_err());
    }

    // time_window_seconds == 300 is the upper bound and must pass.
    #[test]
    fn time_average_accepts_window_300() {
        assert!(validate_custom_sensor(&time_average(300, vec![source()])).is_ok());
    }

    // time_window_seconds > 300 is rejected (upper bound is 300).
    #[test]
    fn time_average_rejects_window_above_300() {
        assert!(validate_custom_sensor(&time_average(301, vec![source()])).is_err());
    }

    // TimeAverage requires exactly one source. Zero or two are both rejected.
    #[test]
    fn time_average_rejects_zero_sources() {
        assert!(validate_custom_sensor(&time_average(10, vec![])).is_err());
    }

    #[test]
    fn time_average_rejects_two_sources() {
        assert!(validate_custom_sensor(&time_average(10, vec![source(), source()])).is_err());
    }

    // EMA mirrors TimeAverage: the validator path is parallel.
    #[test]
    fn ema_valid_passes() {
        assert!(validate_custom_sensor(&ema(10, vec![source()])).is_ok());
    }

    #[test]
    fn ema_rejects_zero_window() {
        assert!(validate_custom_sensor(&ema(0, vec![source()])).is_err());
    }

    #[test]
    fn ema_accepts_window_300() {
        assert!(validate_custom_sensor(&ema(300, vec![source()])).is_ok());
    }

    #[test]
    fn ema_rejects_window_above_300() {
        assert!(validate_custom_sensor(&ema(301, vec![source()])).is_err());
    }

    #[test]
    fn ema_rejects_zero_sources() {
        assert!(validate_custom_sensor(&ema(10, vec![])).is_err());
    }

    #[test]
    fn ema_rejects_two_sources() {
        assert!(validate_custom_sensor(&ema(10, vec![source(), source()])).is_err());
    }
}
