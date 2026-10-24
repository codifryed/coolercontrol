// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use anyhow::Result;
use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use crate::api::devices::{DeviceChannelPath, DevicePath};
use crate::api::{AppState, CCError};
use crate::device::{DeviceType, StatusSnapshot, UID};
use crate::Device;

/// Returns the status of all devices with the selected filters from the request body
/// This endpoint has the most options available.
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

pub async fn get_all(
    Query(status_query): Query<StatusQuery>,
    State(AppState { status_handle, .. }): State<AppState>,
) -> Result<Json<StatusResponse>, CCError> {
    let devices = if let Some(true) = status_query.all {
        status_handle.all().await
    } else {
        status_handle.recent().await
    }?;
    Ok(Json(StatusResponse { devices }))
}

pub async fn get_device(
    Path(path): Path<DevicePath>,
    Query(status_query): Query<StatusQuery>,
    State(AppState { status_handle, .. }): State<AppState>,
) -> Result<Json<DeviceStatusDto>, CCError> {
    let device = if let Some(true) = status_query.all {
        status_handle.all_device(path.device_uid).await
    } else {
        status_handle.recent_device(path.device_uid).await
    }?;
    Ok(Json(device))
}

pub async fn get_device_channel(
    Path(path): Path<DeviceChannelPath>,
    Query(status_query): Query<StatusQuery>,
    State(AppState { status_handle, .. }): State<AppState>,
) -> Result<Json<DeviceChannelStatusDto>, CCError> {
    let device = if let Some(true) = status_query.all {
        status_handle
            .all_device_channel(path.device_uid, path.channel_name)
            .await
    } else {
        status_handle
            .recent_device_channel(path.device_uid, path.channel_name)
            .await
    }?;
    Ok(Json(device))
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatusRequest {
    all: Option<bool>,
    since: Option<DateTime<Local>>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StatusQuery {
    #[serde(default, deserialize_with = "empty_string_as_none")]
    all: Option<bool>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct StatusResponse {
    pub devices: Vec<DeviceStatusDto>,
}

/// The SSE form of [`StatusResponse`], serialized the same: one tick's devices, shared by every
/// subscriber so none of them copies them.
#[derive(Debug, Clone, Serialize)]
pub struct SharedStatusResponse {
    #[serde(serialize_with = "serialize_shared_devices")]
    pub devices: Arc<[DeviceStatusDto]>,
}

fn serialize_shared_devices<S: Serializer>(
    devices: &Arc<[DeviceStatusDto]>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    devices.as_ref().serialize(serializer)
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DeviceStatusDto {
    #[serde(rename(serialize = "type"))]
    pub d_type: DeviceType,
    pub type_index: u8,
    pub uid: UID,
    /// The device's statuses, oldest first. A copy taken on request.
    pub status_history: StatusSnapshot,
}

impl From<&Device> for DeviceStatusDto {
    fn from(device: &Device) -> Self {
        Self {
            d_type: device.d_type,
            type_index: device.type_index,
            uid: device.uid.clone(),
            status_history: device.snapshot_all(),
        }
    }
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DeviceChannelStatusDto {
    pub status_history: StatusSnapshot,
}

/// Serde deserialization decorator to map empty Strings to None,
fn empty_string_as_none<'de, D, T>(de: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: fmt::Display,
{
    let opt = Option::<String>::deserialize(de)?;
    match opt.as_deref() {
        None | Some("") => Ok(None),
        Some(s) => FromStr::from_str(s).map_err(de::Error::custom).map(Some),
    }
}
