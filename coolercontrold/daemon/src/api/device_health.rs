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
use crate::device_health::DeviceHealthDto;
use axum::extract::State;
use axum::Json;

/// Retrieves the current device-health snapshot: failsafe channels and missing
/// temp-source references. Clients fetch this once on startup, then track
/// changes via the `failsafe` / `missing` events on the status SSE stream.
pub async fn get_all(
    State(AppState {
        device_health_handle,
        ..
    }): State<AppState>,
) -> Json<DeviceHealthDto> {
    Json(device_health_handle.get_all().await)
}
