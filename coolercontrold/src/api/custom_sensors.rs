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
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::setting::{CustomSensor, CustomSensorType};

use super::validate_name_string;

/// Retrieves the persisted list of Custom Sensors
#[get("/custom-sensors")]
async fn get_custom_sensors(
    cs_repo: Data<Arc<CustomSensorsRepo>>,
) -> Result<impl Responder, CCError> {
    cs_repo
        .get_custom_sensors()
        .await
        .map(|custom_sensors| HttpResponse::Ok().json(Json(CustomSensorsDto { custom_sensors })))
        .map_err(handle_error)
}

#[get("/custom-sensors/{custom_sensor_id}")]
async fn get_custom_sensor(
    custom_sensor_id: Path<String>,
    cs_repo: Data<Arc<CustomSensorsRepo>>,
) -> Result<impl Responder, CCError> {
    cs_repo
        .get_custom_sensor(&custom_sensor_id)
        .await
        .map(|custom_sensor| HttpResponse::Ok().json(custom_sensor))
        .map_err(handle_error)
}

/// Set the custom sensors order in the array of custom sensors
#[post("/custom-sensors/order")]
async fn save_custom_sensors_order(
    cs_dto: Json<CustomSensorsDto>,
    cs_repo: Data<Arc<CustomSensorsRepo>>,
    config: Data<Arc<Config>>,
) -> Result<impl Responder, CCError> {
    cs_repo
        .set_custom_sensors_order(&cs_dto.custom_sensors)
        .await
        .map_err(handle_error)?;
    handle_simple_result(config.save_config_file().await)
}

#[post("/custom-sensors")]
async fn save_custom_sensor(
    custom_sensor: Json<CustomSensor>,
    cs_repo: Data<Arc<CustomSensorsRepo>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    validate_custom_sensor(&custom_sensor)?;
    cs_repo
        .set_custom_sensor(custom_sensor.into_inner())
        .await
        .map_err(handle_error)?;
    handle_simple_result(config.save_config_file().await)
}

#[put("/custom-sensors")]
async fn update_custom_sensor(
    custom_sensor: Json<CustomSensor>,
    cs_repo: Data<Arc<CustomSensorsRepo>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    validate_custom_sensor(&custom_sensor)?;
    cs_repo
        .update_custom_sensor(custom_sensor.into_inner())
        .await
        .map_err(handle_error)?;
    config.save_config_file().await.map_err(handle_error)?;
    Ok(HttpResponse::Ok().finish())
}

#[delete("/custom-sensors/{custom_sensor_id}")]
async fn delete_custom_sensor(
    custom_sensor_id: Path<String>,
    cs_repo: Data<Arc<CustomSensorsRepo>>,
    settings_controller: Data<Arc<SettingsController>>,
    config: Data<Arc<Config>>,
    session: Session,
) -> Result<impl Responder, CCError> {
    verify_admin_permissions(&session).await?;
    settings_controller
        .custom_sensor_deleted(&cs_repo.get_device_uid().await, &custom_sensor_id)
        .await?;
    cs_repo
        .delete_custom_sensor(&custom_sensor_id)
        .await
        .map_err(handle_error)?;
    config.save_config_file().await.map_err(handle_error)?;
    Ok(HttpResponse::Ok().finish())
}

fn validate_custom_sensor(custom_sensor: &CustomSensor) -> Result<(), CCError> {
    validate_name_string(&custom_sensor.id)?;
    let mut invalid_msg: Option<String> = None;
    if custom_sensor.sources.len() > 10 {
        invalid_msg = Some("sources cannot have more than 10 temps".to_string());
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
    }
    if let Some(msg) = invalid_msg {
        Err(CCError::UserError { msg })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CustomSensorsDto {
    custom_sensors: Vec<CustomSensor>,
}
