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

use axum::extract::State;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::{AppState, CCError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartCpuStressRequest {
    /// Number of threads. Defaults to all available CPU cores.
    pub thread_count: Option<u16>,
    /// Duration in seconds. Defaults to 60, max 600.
    pub duration_secs: Option<u16>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartGpuStressRequest {
    /// Duration in seconds. Defaults to 60, max 600.
    pub duration_secs: Option<u16>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartRamStressRequest {
    /// Duration in seconds. Defaults to 60, max 600.
    pub duration_secs: Option<u16>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct StressTestStatusResponse {
    pub cpu_active: bool,
    pub cpu_duration_secs: Option<u16>,
    pub gpu_active: bool,
    pub gpu_duration_secs: Option<u16>,
    pub ram_active: bool,
    pub ram_duration_secs: Option<u16>,
}

/// POST /stress-test/cpu/start
pub async fn start_cpu(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
    Json(request): Json<StartCpuStressRequest>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .start_cpu(request.thread_count, request.duration_secs)
        .await
        .map(Json)
        .map_err(|e| CCError::UserError { msg: e.to_string() })
}

/// POST /stress-test/cpu/stop
pub async fn stop_cpu(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .stop_cpu()
        .await
        .map(Json)
        .map_err(|e| CCError::InternalError { msg: e.to_string() })
}

/// POST /stress-test/gpu/start
pub async fn start_gpu(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
    Json(request): Json<StartGpuStressRequest>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .start_gpu(request.duration_secs)
        .await
        .map(Json)
        .map_err(|e| CCError::UserError { msg: e.to_string() })
}

/// POST /stress-test/gpu/stop
pub async fn stop_gpu(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .stop_gpu()
        .await
        .map(Json)
        .map_err(|e| CCError::InternalError { msg: e.to_string() })
}

/// POST /stress-test/ram
pub async fn start_ram(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
    Json(request): Json<StartRamStressRequest>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .start_ram(request.duration_secs)
        .await
        .map(Json)
        .map_err(|e| CCError::UserError { msg: e.to_string() })
}

/// DELETE /stress-test/ram
pub async fn stop_ram(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .stop_ram()
        .await
        .map(Json)
        .map_err(|e| CCError::InternalError { msg: e.to_string() })
}

/// POST /stress-test/stop
pub async fn stop_all(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .stop_all()
        .await
        .map(Json)
        .map_err(|e| CCError::InternalError { msg: e.to_string() })
}

/// GET /stress-test/status
pub async fn status(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
) -> Json<StressTestStatusResponse> {
    let s = stress_test_handle.status().await;
    Json(StressTestStatusResponse {
        cpu_active: s.cpu_active,
        cpu_duration_secs: s.cpu_duration_secs,
        gpu_active: s.gpu_active,
        gpu_duration_secs: s.gpu_duration_secs,
        ram_active: s.ram_active,
        ram_duration_secs: s.ram_duration_secs,
    })
}
