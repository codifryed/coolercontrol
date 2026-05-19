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

use crate::api::AppState;
use crate::device::{ChannelDataType, ChannelName, ChannelStats, TempName, UID};
use axum::extract::State;
use axum::Json;
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
