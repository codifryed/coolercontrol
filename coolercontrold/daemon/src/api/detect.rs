// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::Json;
use axum::extract::State;
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
    pub blacklisted: Vec<String>,
    pub environment: EnvironmentDto,
    /// False when no probe was made at all, so an empty `detected_chips` means
    /// "not looked for" rather than "none present". Detection can be switched
    /// off by config or environment, and is unsupported off `x86_64`.
    pub probed: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct DetectedChipDto {
    pub name: String,
    pub driver: String,
    pub address: String,
    pub base_address: String,
    /// The chip's own id register. Reported so a chip line in a pasted report
    /// can be matched against a pasted log.
    pub device_id: String,
    pub features: Vec<String>,
    pub module_status: String,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct EnvironmentDto {
    pub is_container: bool,
    pub is_secure_boot: bool,
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
                    device_id: c.device_id,
                    features: c.features,
                    module_status: c.module_status,
                })
                .collect(),
            blacklisted: results.blacklisted,
            environment: EnvironmentDto {
                is_container: results.environment.is_container,
                is_secure_boot: results.environment.is_secure_boot,
                has_dev_port: results.environment.has_dev_port,
            },
            probed: true,
        }
    }
}

impl DetectResponse {
    /// No probe was made. Everything empty, and `probed` false so a client
    /// cannot mistake that for a clean scan.
    fn not_probed() -> Self {
        Self {
            detected_chips: Vec::new(),
            blacklisted: Vec::new(),
            environment: EnvironmentDto {
                is_container: false,
                is_secure_boot: false,
                has_dev_port: false,
            },
            probed: false,
        }
    }
}

/// GET /detect - Return what the startup probe found.
///
/// Deliberately does not re-probe. Startup is when module loading actually
/// happens, so a fresh scan would answer a different question than the one
/// being asked, and probing Super-I/O config registers on demand is real
/// hardware access for what reads like a page load. `POST /detect` remains the
/// explicit, user-initiated re-scan.
pub async fn get_detect(
    State(AppState {
        hardware_report_handle,
        ..
    }): State<AppState>,
) -> Result<Json<DetectResponse>, CCError> {
    hardware_report_handle
        .retained_detection()
        .await
        .map(|retained| {
            Json(retained.map_or_else(DetectResponse::not_probed, DetectResponse::from))
        })
        .map_err(|e| CCError::InternalError {
            msg: format!("Could not read retained detection results: {e}"),
        })
}

/// POST /detect — Run detection and optionally load modules.
pub async fn post_detect(
    State(AppState {
        detect_handle,
        hardware_report_handle,
        ..
    }): State<AppState>,
    Json(request): Json<DetectRequest>,
) -> Result<Json<DetectResponse>, CCError> {
    let results =
        detect_handle
            .run(request.load_modules)
            .await
            .map_err(|e| CCError::InternalError {
                msg: format!("Detection failed: {e}"),
            })?;
    // This scan is newer than the retained one and may have just loaded a
    // module, so the health snapshot has to be brought along with it. A failed
    // hand-off leaves the older results in place, which is stale but not wrong,
    // so it does not fail the request the user actually made.
    if let Err(err) = hardware_report_handle
        .refresh_detection(results.clone())
        .await
    {
        log::warn!("Could not refresh the retained detection results: {err}");
    }
    Ok(Json(DetectResponse::from(results)))
}
