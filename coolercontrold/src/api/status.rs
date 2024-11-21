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

use anyhow::Result;
use axum::extract::{Json, State};
use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::{AppState, CCError};
use crate::device::{DeviceType, Status, UID};
use crate::Device;

/// Returns the status of all devices with the selected filters from the request body
pub async fn retrieve(
    State(AppState { status_handle, .. }): State<AppState>,
    Json(status_request): Json<StatusRequest>,
) -> Result<Json<StatusResponse>, CCError> {
    let devices = if let Some(true) = status_request.all {
        status_handle.all().await
    } else if let Some(since_timestamp) = status_request.since {
        status_handle.since(since_timestamp).await
    } else {
        status_handle.recent().await
    }?;
    Ok(Json(StatusResponse { devices }))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusRequest {
    all: Option<bool>,
    since: Option<DateTime<Local>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusResponse {
    devices: Vec<DeviceStatusDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeviceStatusDto {
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    pub status_history: Vec<Status>,
}

impl From<&Device> for DeviceStatusDto {
    fn from(device: &Device) -> Self {
        Self {
            d_type: device.d_type.clone(),
            type_index: device.type_index,
            uid: device.uid.clone(),
            status_history: device.status_history.clone().into(),
        }
    }
}
