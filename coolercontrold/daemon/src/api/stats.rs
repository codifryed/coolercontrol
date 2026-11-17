// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::AppState;
use crate::api::Json;
use crate::device::{ChannelDataType, ChannelName, ChannelStats, TempName, UID};
use axum::extract::State;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::HashMap;

/// Top-level response for `GET /stats` and `DELETE /stats`. One entry per
/// known device. Channels/temps are only present once they have been
/// observed at least once since daemon start (or last reset).
#[derive(Debug, Clone, Default, Serialize, JsonSchema)]
pub struct StatsResponse {
    pub devices: Vec<DeviceStatsDto>,
}

#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DeviceStatsDto {
    pub uid: UID,
    pub temps: HashMap<TempName, ChannelStats>,
    pub channels: HashMap<ChannelName, HashMap<ChannelDataType, ChannelStats>>,
}

pub async fn get_all(State(AppState { stats_handle, .. }): State<AppState>) -> Json<StatsResponse> {
    Json(stats_handle.all().await)
}

pub async fn delete_all(
    State(AppState { stats_handle, .. }): State<AppState>,
) -> Json<StatsResponse> {
    Json(stats_handle.reset_all().await)
}
