// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon-served hardware report.
//!
//! The daemon is the only thing that can enumerate liquidctl devices without
//! touching hardware, and it already holds the device list in memory, so the
//! liquidctl section costs no extra hardware access.

use crate::api::Json;
use axum::extract::{Query, State};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::api::{AppState, CCError};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReportQuery {
    /// Include the whole hwmon tree instead of the compact summary.
    #[serde(default)]
    pub full: bool,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ReportResponse {
    /// The report as plain text, ready to paste.
    pub report: String,
}

/// GET /hardware-report
pub async fn get_hardware_report(
    Query(query): Query<ReportQuery>,
    State(AppState {
        hardware_report_handle,
        ..
    }): State<AppState>,
) -> Result<Json<ReportResponse>, CCError> {
    // Generation runs on the main runtime: it reads sysfs through `cc_fs`,
    // whose futures are not `Send` and so cannot be awaited in a handler.
    // Liquidctl devices and the startup detection both come from what the
    // daemon retained, so nothing is re-enumerated or re-probed here.
    let report = hardware_report_handle
        .generate(query.full)
        .await
        .map_err(|err| CCError::InternalError {
            msg: format!("Could not generate the hardware report: {err}"),
        })?;
    Ok(Json(ReportResponse { report }))
}
