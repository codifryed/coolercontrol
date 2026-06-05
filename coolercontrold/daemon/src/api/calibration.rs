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

use crate::api::actor::{CalibrationBatchStatus, CalibrationStatus};
use crate::api::devices::DeviceChannelPath;
use crate::api::{AppState, CCError};
use crate::calibration::{Calibration, CalibrationEntry};
use crate::device::{ChannelName, DeviceUID};
use aide::NoApi;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Calibration as exposed via the REST API. Flattens the persistence
/// struct and adds derived fields the UI consumes.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationView {
    #[serde(flatten)]
    pub calibration: Calibration,
    /// Resolved kick-boost decision (override + heuristic). `true`
    /// when the dispatcher will apply the cold-start boost on the
    /// next Off->Kicking transition for this channel. Derived; the
    /// deserialize path is only used for JSON round-trips in tests
    /// and will be recomputed if `kick_boost_override` changes.
    #[serde(default)]
    pub kick_boost_active: bool,
}

impl From<Calibration> for CalibrationView {
    fn from(calibration: Calibration) -> Self {
        let kick_boost_active = calibration.kick_boost_active();
        Self {
            calibration,
            kick_boost_active,
        }
    }
}

/// Same wire shape as `crate::calibration::CalibrationEntry`, with
/// the wrapped view in place of the bare calibration.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationEntryView {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub calibration: CalibrationView,
}

impl From<CalibrationEntry> for CalibrationEntryView {
    fn from(entry: CalibrationEntry) -> Self {
        Self {
            device_uid: entry.device_uid,
            channel_name: entry.channel_name,
            calibration: entry.calibration.into(),
        }
    }
}

/// Start a calibration diagnosis on the channel. Returns 202 if the
/// diagnosis was queued, 409 if a diagnosis is already in flight for
/// the same channel.
pub async fn start(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Result<NoApi<StatusCode>, CCError> {
    calibration_handle
        .start(path.device_uid, path.channel_name)
        .await
        .map(|()| NoApi(StatusCode::ACCEPTED))
        .map_err(|err| CCError::Conflict {
            msg: err.to_string(),
        })
}

/// Cancel an in-flight calibration. Returns 404 if no diagnosis was
/// running for the channel.
pub async fn cancel(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Result<(), CCError> {
    let cancelled = calibration_handle
        .cancel(path.device_uid, path.channel_name)
        .await;
    if cancelled {
        Ok(())
    } else {
        Err(CCError::NotFound {
            msg: "no calibration in flight for this channel".to_string(),
        })
    }
}

/// Get the stored calibration for a channel. 404 when none exists.
pub async fn get(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Result<Json<CalibrationView>, CCError> {
    calibration_handle
        .get(path.device_uid, path.channel_name)
        .await
        .map(|c| Json(c.into()))
        .ok_or(CCError::NotFound {
            msg: "no calibration stored for this channel".to_string(),
        })
}

/// Get the latest calibration status (polling). Always returns 200;
/// channels that have never been diagnosed and have no persisted
/// calibration return a `NotStarted` status payload rather than 404.
pub async fn status(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Result<Json<CalibrationStatus>, CCError> {
    calibration_handle
        .status(path.device_uid, path.channel_name)
        .await
        .map(Json)
        .ok_or(CCError::InternalError {
            msg: "calibration actor is not responding".to_string(),
        })
}

/// One channel reference in a batch request body.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BatchChannel {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
}

/// Body for `POST /calibrations/batch/start`. `concurrency` is how many
/// sweeps run at once (1 = sequential); omitted or 0 is treated as 1, and
/// the daemon clamps it to the channel count.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StartCalibrationBatchRequest {
    pub channels: Vec<BatchChannel>,
    #[serde(default)]
    pub concurrency: usize,
}

/// Begin a calibration batch. 202 once queued, or 409 if a batch is
/// already active or the request is invalid (empty or over the channel
/// cap).
pub async fn batch_start(
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
    Json(request): Json<StartCalibrationBatchRequest>,
) -> Result<NoApi<StatusCode>, CCError> {
    let channels = request
        .channels
        .into_iter()
        .map(|channel| (channel.device_uid, channel.channel_name))
        .collect();
    calibration_handle
        .start_batch(channels, request.concurrency)
        .await
        .map(|()| NoApi(StatusCode::ACCEPTED))
        .map_err(|err| CCError::Conflict {
            msg: err.to_string(),
        })
}

/// Current calibration batch status. Always 200; the body is `null` when
/// no batch has run this session.
pub async fn batch_status(
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Json<Option<CalibrationBatchStatus>> {
    Json(calibration_handle.batch_status().await)
}

/// Cancel the active batch and stop its queue. 404 when none is active.
pub async fn batch_cancel(
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Result<(), CCError> {
    if calibration_handle.cancel_batch().await {
        Ok(())
    } else {
        Err(CCError::NotFound {
            msg: "no calibration batch is active".to_string(),
        })
    }
}

/// Snapshot of every persisted calibration. Empty list when nothing
/// is stored. Matches the wrapper shape used by `/profiles` and
/// `/alerts` so clients can iterate `dto.calibrations`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CalibrationsDto {
    pub calibrations: Vec<CalibrationEntryView>,
}

/// List every persisted calibration. Always returns 200; an empty
/// list signals that no channel has been calibrated yet. The UI
/// consumes this once at app load to mark calibrated channels in the
/// tree menu without one request per channel.
pub async fn list(
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Json<CalibrationsDto> {
    let calibrations = calibration_handle
        .get_all()
        .await
        .into_iter()
        .map(CalibrationEntryView::from)
        .collect();
    Json(CalibrationsDto { calibrations })
}

/// Per-fan calibration override values. `null` clears the override
/// and falls back to the auto-derived behavior (heuristic for the
/// boost, calibrated `kick_duration_ms` for the duration, walk-down
/// enabled by default).
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct CalibrationOverridesUpdate {
    pub kick_boost_override: Option<bool>,
    pub kick_duration_override_ms: Option<u32>,
    #[serde(default)]
    pub walk_after_kick_override: Option<bool>,
}

/// Replace the override fields on the persisted calibration for a
/// channel. Both fields are set unconditionally from the request body
/// (PUT-style on the overrides subset). 404 when no calibration is
/// stored for the channel. Returns the updated calibration so the UI
/// re-renders without a second GET.
pub async fn set_overrides(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
    Json(body): Json<CalibrationOverridesUpdate>,
) -> Result<Json<CalibrationView>, CCError> {
    calibration_handle
        .set_overrides(
            path.device_uid,
            path.channel_name,
            body.kick_boost_override,
            body.kick_duration_override_ms,
            body.walk_after_kick_override,
        )
        .await
        .map_err(|err| CCError::InternalError {
            msg: err.to_string(),
        })?
        .map(|c| Json(c.into()))
        .ok_or(CCError::NotFound {
            msg: "no calibration stored for this channel".to_string(),
        })
}

/// Delete the stored calibration for a channel. 404 when none exists.
pub async fn delete(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
) -> Result<(), CCError> {
    let removed = calibration_handle
        .delete(path.device_uid, path.channel_name)
        .await
        .map_err(|err| CCError::InternalError {
            msg: err.to_string(),
        })?;
    if removed {
        Ok(())
    } else {
        Err(CCError::NotFound {
            msg: "no calibration stored for this channel".to_string(),
        })
    }
}
