// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::AppState;
use crate::device::health::DeviceHealthDto;
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
