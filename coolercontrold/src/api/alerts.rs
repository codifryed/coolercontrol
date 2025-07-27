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

use crate::alerts::{Alert, AlertLog, AlertName, AlertState};
use crate::api::auth::verify_admin_permissions;
use crate::api::{handle_error, validate_name_string, AppState, CCError};
use crate::device::UID;
use crate::setting::ChannelSource;
use aide::NoApi;
use axum::extract::{Path, State};
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;

/// Retrieves the persisted Alert list
pub async fn get_all(
    State(AppState { alert_handle, .. }): State<AppState>,
) -> Result<Json<AlertsDto>, CCError> {
    alert_handle
        .get_all()
        .await
        .map(|all| {
            Json(AlertsDto {
                alerts: all.0.into_iter().map(AlertDto::from).collect(),
                logs: all.1,
            })
        })
        .map_err(handle_error)
}

pub async fn create(
    NoApi(session): NoApi<Session>,
    State(AppState { alert_handle, .. }): State<AppState>,
    Json(alert_dto): Json<AlertDto>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    validate_alert(&alert_dto)?;
    alert_handle
        .create(Alert::from(alert_dto))
        .await
        .map_err(handle_error)
}

pub async fn update(
    NoApi(session): NoApi<Session>,
    State(AppState { alert_handle, .. }): State<AppState>,
    Json(alert_dto): Json<AlertDto>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    validate_alert(&alert_dto)?;
    alert_handle
        .update(Alert::from(alert_dto))
        .await
        .map_err(handle_error)
}

pub async fn delete(
    Path(path): Path<AlertPath>,
    NoApi(session): NoApi<Session>,
    State(AppState { alert_handle, .. }): State<AppState>,
) -> Result<(), CCError> {
    verify_admin_permissions(&session).await?;
    alert_handle
        .delete(path.alert_uid)
        .await
        .map_err(handle_error)
}

#[allow(clippy::float_cmp)]
fn validate_alert(alert: &AlertDto) -> Result<(), CCError> {
    validate_name_string(&alert.name)?;
    if alert.channel_source.device_uid.is_empty() {
        return Err(CCError::UserError {
            msg: "channel_source.device_uid cannot be empty".to_string(),
        });
    }
    if alert.channel_source.channel_name.is_empty() {
        return Err(CCError::UserError {
            msg: "channel_source.channel_name cannot be empty".to_string(),
        });
    }
    if alert.max < alert.min {
        return Err(CCError::UserError {
            msg: "max must be greater than min".to_string(),
        });
    }
    if alert.max == alert.min {
        return Err(CCError::UserError {
            msg: "max and min cannot be equal".to_string(),
        });
    }
    if alert.max < 0.0 {
        return Err(CCError::UserError {
            msg: "max cannot be negative".to_string(),
        });
    }
    if alert.min < 0.0 {
        return Err(CCError::UserError {
            msg: "min cannot be negative".to_string(),
        });
    }
    if alert.uid.is_empty() {
        return Err(CCError::UserError {
            msg: "uid cannot be empty".to_string(),
        });
    }
    if alert.warmup_duration.is_sign_negative() {
        return Err(CCError::UserError {
            msg: "warmup_threshold cannot be negative".to_string(),
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertPath {
    alert_uid: UID,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertsDto {
    alerts: Vec<AlertDto>,
    logs: Vec<AlertLog>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AlertDto {
    pub uid: UID,
    pub name: AlertName,
    pub channel_source: ChannelSource,
    pub min: f64,
    pub max: f64,
    // We send the current state, but we don't receive it when creating or updating:
    pub state: Option<AlertState>,
    /// Time in seconds throughout which the alert conditidon must hold before the alert is
    /// activated
    pub warmup_duration: f64,
}

impl From<Alert> for AlertDto {
    fn from(alert: Alert) -> Self {
        AlertDto {
            uid: alert.uid,
            name: alert.name,
            channel_source: alert.channel_source,
            min: alert.min,
            max: alert.max,
            state: Some(alert.state),
            warmup_duration: alert.warmup_duration,
        }
    }
}

impl From<AlertDto> for Alert {
    fn from(alert_dto: AlertDto) -> Self {
        Alert {
            uid: alert_dto.uid,
            name: alert_dto.name,
            channel_source: alert_dto.channel_source,
            min: alert_dto.min,
            max: alert_dto.max,
            state: AlertState::Inactive,
            warmup_duration: alert_dto.warmup_duration,
        }
    }
}
