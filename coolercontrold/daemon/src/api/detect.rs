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

/// Request body for POST /detect
#[derive(Debug, Deserialize, JsonSchema)]
pub struct DetectRequest {
    #[serde(default)]
    pub load_modules: bool,
}

/// Response for GET/POST /detect
#[derive(Debug, Serialize, JsonSchema)]
pub struct DetectResponse {
    pub detected_chips: Vec<DetectedChipDto>,
    pub skipped: Vec<SkippedDriverDto>,
    pub blacklisted: Vec<String>,
    pub environment: EnvironmentDto,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DetectedChipDto {
    pub name: String,
    pub driver: String,
    pub address: String,
    pub base_address: String,
    pub features: Vec<String>,
    pub module_status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SkippedDriverDto {
    pub driver: String,
    pub reason: String,
    pub preferred: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EnvironmentDto {
    pub is_container: bool,
    pub has_dev_port: bool,
}

impl From<cc_detect::DetectionResults> for DetectResponse {
    fn from(results: cc_detect::DetectionResults) -> Self {
        Self {
            detected_chips: results
                .detected_chips
                .into_iter()
                .map(|c| DetectedChipDto {
                    name: c.name,
                    driver: c.driver,
                    address: c.address,
                    base_address: c.base_address,
                    features: c.features,
                    module_status: c.module_status,
                })
                .collect(),
            skipped: results
                .skipped
                .into_iter()
                .map(|s| SkippedDriverDto {
                    driver: s.driver,
                    reason: s.reason,
                    preferred: s.preferred,
                })
                .collect(),
            blacklisted: results.blacklisted,
            environment: EnvironmentDto {
                is_container: results.environment.is_container,
                has_dev_port: results.environment.has_dev_port,
            },
        }
    }
}

/// GET /detect — Run detection and return results (no module loading).
pub async fn get_detect(
    State(AppState { detect_handle, .. }): State<AppState>,
) -> Result<Json<DetectResponse>, CCError> {
    detect_handle
        .run(false)
        .await
        .map(|r| Json(DetectResponse::from(r)))
        .map_err(|e| CCError::InternalError {
            msg: format!("Detection failed: {e}"),
        })
}

/// POST /detect — Run detection and optionally load modules.
pub async fn post_detect(
    State(AppState { detect_handle, .. }): State<AppState>,
    Json(request): Json<DetectRequest>,
) -> Result<Json<DetectResponse>, CCError> {
    detect_handle
        .run(request.load_modules)
        .await
        .map(|r| Json(DetectResponse::from(r)))
        .map_err(|e| CCError::InternalError {
            msg: format!("Detection failed: {e}"),
        })
}
