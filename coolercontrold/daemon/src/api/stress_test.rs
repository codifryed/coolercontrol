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

use std::path::Path;

use axum::extract::State;
use axum::Json;
use log::warn;
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

#[derive(Debug, Deserialize, JsonSchema)]
pub struct StartDriveStressRequest {
    /// Block device path, e.g. "/dev/sda" or "/dev/nvme0n1".
    pub device_path: String,
    /// Number of I/O threads. Defaults to 4.
    pub threads: Option<u16>,
    /// Duration in seconds. Defaults to 60, max 600.
    pub duration_secs: Option<u16>,
}

#[derive(Debug, Serialize, JsonSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct StressTestStatusResponse {
    pub cpu_active: bool,
    pub cpu_duration_secs: Option<u16>,
    pub gpu_active: bool,
    pub gpu_duration_secs: Option<u16>,
    pub ram_active: bool,
    pub ram_duration_secs: Option<u16>,
    pub drive_active: bool,
    pub drive_duration_secs: Option<u16>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DriveInfo {
    /// Block device path, e.g. "/dev/sda".
    pub device_path: String,
    /// Device model name if available.
    pub model: Option<String>,
    /// Device size in bytes.
    pub size_bytes: u64,
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

/// POST /stress-test/drive
pub async fn start_drive(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
    Json(request): Json<StartDriveStressRequest>,
) -> Result<Json<()>, CCError> {
    validate_device_path(&request.device_path)?;
    stress_test_handle
        .start_drive(request.device_path, request.threads, request.duration_secs)
        .await
        .map(Json)
        .map_err(|e| CCError::UserError { msg: e.to_string() })
}

/// Validates block device path at the API boundary before passing to actor.
fn validate_device_path(device_path: &str) -> Result<(), CCError> {
    if !device_path.starts_with("/dev/") {
        return Err(CCError::UserError {
            msg: "Device path must start with /dev/".to_string(),
        });
    }
    if device_path.contains("..") {
        return Err(CCError::UserError {
            msg: "Device path must not contain '..'".to_string(),
        });
    }
    if !std::path::Path::new(device_path).exists() {
        return Err(CCError::UserError {
            msg: format!("Device {device_path} does not exist"),
        });
    }
    Ok(())
}

/// DELETE /stress-test/drive
pub async fn stop_drive(
    State(AppState {
        stress_test_handle, ..
    }): State<AppState>,
) -> Result<Json<()>, CCError> {
    stress_test_handle
        .stop_drive()
        .await
        .map(Json)
        .map_err(|e| CCError::InternalError { msg: e.to_string() })
}

/// GET /stress-test/drives
pub async fn list_drives() -> Json<Vec<DriveInfo>> {
    let mut drives = Vec::new();
    let block_dir = Path::new("/sys/class/block");
    let entries = match std::fs::read_dir(block_dir) {
        Ok(e) => e,
        Err(e) => {
            warn!("Failed to read /sys/class/block: {e}");
            return Json(drives);
        }
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Filter to whole-disk block devices only (no partitions).
        let is_sd = name.starts_with("sd") && name.chars().skip(2).all(|c| c.is_ascii_lowercase());
        let is_nvme = name.starts_with("nvme")
            && name.contains('n')
            && !name.contains('p')
            && name != "nvme-fabrics";
        if !is_sd && !is_nvme {
            continue;
        }
        let device_path = format!("/dev/{name}");
        // Read model name.
        let model_path = block_dir.join(&name).join("device/model");
        let model = std::fs::read_to_string(model_path)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        // Read size in 512-byte sectors.
        let size_path = block_dir.join(&name).join("size");
        let size_bytes = std::fs::read_to_string(size_path)
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .map_or(0, |sectors| sectors * 512);
        drives.push(DriveInfo {
            device_path,
            model,
            size_bytes,
        });
    }
    drives.sort_by(|a, b| a.device_path.cmp(&b.device_path));
    Json(drives)
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
        drive_active: s.drive_active,
        drive_duration_secs: s.drive_duration_secs,
    })
}
