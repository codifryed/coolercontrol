// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::actor::{CalibrationBatchStatus, CalibrationStatus};
use crate::api::devices::DeviceChannelPath;
use crate::api::Json;
use crate::api::{AppState, CCError};
use crate::calibration::{Calibration, CalibrationEntry};
use crate::device::{ChannelName, DeviceUID, Duty};
use aide::NoApi;
use axum::extract::{Path, State};
use axum::http::StatusCode;
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

/// Cap on one map request. A speed profile holds far fewer points than
/// this; the bound exists so an arbitrary body cannot size an allocation.
pub const MAX_MAP_DUTIES: usize = 256;

// A curve may hold one point per whole duty, so the cap must clear a
// full 0..=100 sweep or a legitimate profile would be rejected.
const _: () = assert!(MAX_MAP_DUTIES >= 101);

/// Body for the duty map. Device duties as authored before the channel
/// was calibrated.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MapDutiesRequest {
    pub device_duties: Vec<Duty>,
}

/// True duties, one per request entry, in the same order.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MapDutiesResponse {
    pub true_duties: Vec<Duty>,
}

/// Rejects rather than clamps: a duty over 100 is a client bug, and
/// clamping would hand back a mapping the caller never asked for.
fn validate_map_request(device_duties: &[Duty]) -> Result<(), CCError> {
    if device_duties.is_empty() {
        return Err(CCError::UserError {
            msg: "device_duties must not be empty".to_string(),
        });
    }
    if device_duties.len() > MAX_MAP_DUTIES {
        return Err(CCError::UserError {
            msg: format!("device_duties is limited to {MAX_MAP_DUTIES} entries"),
        });
    }
    if let Some(duty) = device_duties.iter().copied().find(|duty| *duty > 100) {
        return Err(CCError::UserError {
            msg: format!("device duty {duty} is over 100"),
        });
    }
    Ok(())
}

/// Applies the stable inverse to each entry, preserving order.
fn map_device_duties_to_true(
    calibration: &Calibration,
    device_duties: &[Duty],
) -> Result<Vec<Duty>, CCError> {
    let mut true_duties = Vec::with_capacity(device_duties.len());
    for &device_duty in device_duties {
        let Some(true_duty) = calibration.device_to_true_duty(device_duty) else {
            return Err(CCError::Conflict {
                msg: "stepped calibration is passthrough; nothing to convert".to_string(),
            });
        };
        true_duties.push(true_duty);
    }
    debug_assert_eq!(true_duties.len(), device_duties.len());
    debug_assert!(true_duties.iter().all(|duty| *duty <= 100));
    Ok(true_duties)
}

/// Map device duties to the true duties that reproduce them.
///
/// A calibrated channel reinterprets every stored duty as true-duty and
/// runs it back through `true_to_device` on write, so a curve authored
/// before calibration changes meaning. Feeding its points through here
/// yields the pre-image: writing those values makes the fan behave as it
/// did before. Pure computation, so this mutates nothing.
pub async fn map_duties(
    Path(path): Path<DeviceChannelPath>,
    State(AppState {
        calibration_handle, ..
    }): State<AppState>,
    Json(request): Json<MapDutiesRequest>,
) -> Result<Json<MapDutiesResponse>, CCError> {
    validate_map_request(&request.device_duties)?;
    let calibration = calibration_handle
        .get(path.device_uid, path.channel_name)
        .await
        .ok_or(CCError::NotFound {
            msg: "no calibration stored for this channel".to_string(),
        })?;
    let true_duties = map_device_duties_to_true(&calibration, &request.device_duties)?;
    Ok(Json(MapDutiesResponse { true_duties }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::{CurveKind, DutySample};
    use chrono::Local;

    /// Uniform 5%-step curve, up == down, top sample at `rpm_max`.
    /// Mirrors the store's fixture so the inverse saturates at 100.
    fn smooth_calibration() -> Calibration {
        let up: Vec<DutySample> = (0..21usize)
            .map(|i| DutySample {
                duty: u8::try_from(i).expect("fits in u8") * 5,
                rpm: 100 * u32::try_from(i).expect("fits in u32"),
            })
            .collect();
        let down = up.clone();
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 750,
            min_start_duty: 5,
            min_sustain_duty: 5,
            min_stable_duty: 5,
            max_eff_duty: 95,
            rpm_max: 2000,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            was_rpm_only: false,
            kick_boost_override: None,
            kick_duration_override_ms: None,
            walk_after_kick_override: None,
            timestamp: Local::now(),
        }
    }

    fn stepped_calibration() -> Calibration {
        Calibration {
            curve_kind: CurveKind::Stepped,
            ..smooth_calibration()
        }
    }

    fn user_error_message(error: CCError) -> String {
        match error {
            CCError::UserError { msg } => msg,
            other => panic!("expected UserError, got {other:?}"),
        }
    }

    #[test]
    fn validate_map_request_rejects_an_empty_list() {
        // Goal: an empty body is a client bug, not an empty-response case.
        // Method: validate an empty slice and read the rejection reason.
        let msg = user_error_message(validate_map_request(&[]).expect_err("empty is rejected"));
        assert!(msg.contains("empty"), "unexpected message: {msg}");
    }

    #[test]
    fn validate_map_request_rejects_over_the_cap() {
        // Goal: the cap must actually bound the allocation the handler
        // makes. Method: one entry past MAX_MAP_DUTIES must be refused
        // while exactly MAX_MAP_DUTIES passes.
        let at_cap = vec![50; MAX_MAP_DUTIES];
        validate_map_request(&at_cap).expect("the cap itself is accepted");
        let over_cap = vec![50; MAX_MAP_DUTIES + 1];
        let msg =
            user_error_message(validate_map_request(&over_cap).expect_err("over cap is rejected"));
        assert!(msg.contains("limited"), "unexpected message: {msg}");
    }

    #[test]
    fn validate_map_request_rejects_a_duty_over_one_hundred() {
        // Goal: reject rather than clamp, so a caller never receives a
        // mapping for a duty it did not send. Method: 101 in an
        // otherwise valid list, and confirm the bounds 0 and 100 pass.
        let msg =
            user_error_message(validate_map_request(&[0, 101, 50]).expect_err("101 is rejected"));
        assert!(msg.contains("101"), "unexpected message: {msg}");
        validate_map_request(&[0, 100]).expect("the bounds are accepted");
    }

    #[test]
    fn map_device_duties_to_true_preserves_order_and_length() {
        // Goal: the UI zips the response back onto its curve points by
        // index, so any reorder or drop would silently corrupt a profile.
        // Method: map a deliberately unsorted list and re-map each entry
        // on its own, expecting the same values in the same slots.
        let calibration = smooth_calibration();
        let device_duties = [80, 20, 55, 5];
        let mapped = map_device_duties_to_true(&calibration, &device_duties).expect("smooth maps");
        assert_eq!(mapped.len(), device_duties.len());
        for (index, &device_duty) in device_duties.iter().enumerate() {
            let expected = calibration
                .device_to_true_duty(device_duty)
                .expect("smooth maps");
            assert_eq!(mapped[index], expected, "slot {index} drifted");
        }
    }

    #[test]
    fn map_device_duties_to_true_keeps_zero_and_full_at_the_bounds() {
        // Goal: off must stay off (zero-RPM fans depend on it) and a
        // saturated point must not lose headroom. Method: map both
        // bounds on a fixture whose down-curve tops out at rpm_max.
        let calibration = smooth_calibration();
        let mapped = map_device_duties_to_true(&calibration, &[0, 100]).expect("smooth maps");
        assert_eq!(mapped[0], 0);
        assert_eq!(mapped[1], 100);
    }

    #[test]
    fn map_device_duties_to_true_keeps_a_sub_sustain_duty_off() {
        // Goal: a duty under the sustain floor stalls the fan, so the
        // conversion has to keep it stopped. Mapping it to the floor
        // instead would silently start a fan the user had set to idle.
        // Method: map a duty below min_sustain_duty, and the floor duty
        // itself, which does spin and so must stay on.
        let calibration = smooth_calibration();
        assert!(calibration.min_sustain_duty > 3);
        let mapped = map_device_duties_to_true(&calibration, &[3]).expect("smooth maps");
        assert_eq!(mapped[0], 0);
        let at_floor = map_device_duties_to_true(&calibration, &[calibration.min_sustain_duty])
            .expect("smooth maps");
        assert_eq!(at_floor[0], 1);
    }

    #[test]
    fn map_device_duties_to_true_stays_monotonic() {
        // Goal: a rising curve must stay rising after conversion, or a
        // converted profile would fight itself as temperature climbs.
        // Method: map every duty 0..=100 and check the output never dips.
        let calibration = smooth_calibration();
        let device_duties: Vec<Duty> = (0..=100).collect();
        let mapped = map_device_duties_to_true(&calibration, &device_duties).expect("smooth maps");
        for pair in mapped.windows(2) {
            assert!(pair[1] >= pair[0], "dipped from {} to {}", pair[0], pair[1]);
        }
    }

    #[test]
    fn map_device_duties_to_true_conflicts_on_a_stepped_calibration() {
        // Goal: a stepped channel is written through unmapped, so there
        // is nothing to convert and converting anyway would change the
        // fan's behavior. Method: map against a stepped calibration.
        let calibration = stepped_calibration();
        let error = map_device_duties_to_true(&calibration, &[50]).expect_err("stepped conflicts");
        match error {
            CCError::Conflict { msg } => assert!(msg.contains("stepped"), "unexpected: {msg}"),
            other => panic!("expected Conflict, got {other:?}"),
        }
    }

    #[test]
    fn map_duties_dtos_round_trip_through_json() {
        // Goal: the request and response are the wire contract with the
        // UI. Method: serialize both, parse back, compare the payloads.
        let request = MapDutiesRequest {
            device_duties: vec![0, 40, 100],
        };
        let encoded = serde_json::to_string(&request).expect("request serializes");
        let decoded: MapDutiesRequest = serde_json::from_str(&encoded).expect("request parses");
        assert_eq!(decoded.device_duties, request.device_duties);

        let response = MapDutiesResponse {
            true_duties: vec![0, 38, 100],
        };
        let encoded = serde_json::to_string(&response).expect("response serializes");
        let decoded: MapDutiesResponse = serde_json::from_str(&encoded).expect("response parses");
        assert_eq!(decoded.true_duties, response.true_duties);
    }
}
