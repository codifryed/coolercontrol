/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon and contributors
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

import type { UID } from '@/models/Device'

/**
 * A single (device-duty, RPM) sample from a calibration sweep.
 * Mirrors `coolercontrold::calibration::DutySample`. Sampling is dense
 * (2% steps) around the kick-in zone and sparse (5% steps) elsewhere,
 * so duty values are NOT regularly spaced.
 */
export interface DutySample {
    duty: number
    rpm: number
}

/**
 * One persisted calibration record returned by the bulk
 * `GET /calibrations` endpoint. Mirrors
 * `coolercontrold::calibration::CalibrationEntry`.
 */
export interface CalibrationEntry {
    device_uid: UID
    channel_name: string
    calibration: Calibration
}

/**
 * Persisted per-channel calibration produced by a successful
 * diagnosis sweep. Mirrors `coolercontrold::calibration::Calibration`.
 *
 * `curve_kind === 'Stepped'` means the channel has discrete RPM
 * plateaus and the dispatch layer leaves true-duty == device-duty
 * (passthrough). For Smooth channels the dispatcher applies the
 * full RPM-normalized true-duty mapping.
 */
export interface Calibration {
    /** Up-sweep (duty, RPM) samples, sorted by duty ascending. */
    up_curve: DutySample[]
    /** Down-sweep (duty, RPM) samples, sorted by duty ascending. */
    down_curve: DutySample[]
    /** Kick window applied when transitioning out of Off. */
    kick_duration_ms: number
    /** Lowest duty that reliably starts the fan (multiple of 5). */
    min_start_duty: number
    /** Lowest duty at which the fan continues spinning (multiple of 5). */
    min_sustain_duty: number
    /**
     * Lowest duty at which the fan operates stably (no firmware-kick
     * oscillation). Equals `min_sustain_duty` for healthy fans.
     * Defaults to 0 on older persisted calibrations that pre-date this
     * field (the dispatcher's clamp then degenerates to a no-op).
     */
    min_stable_duty: number
    /** Lowest duty where the fan hits its saturation plateau. */
    max_eff_duty: number
    /** Peak RPM observed during the sweep. */
    rpm_max: number
    /** `Smooth` for active mapping, `Stepped` for passthrough. */
    curve_kind: 'Smooth' | 'Stepped'
    /**
     * Non-fatal reliability findings produced by `derive_warnings`.
     * Empty for a healthy calibration. Mirrors
     * `coolercontrold::calibration::CalibrationWarning`.
     */
    warnings: CalibrationWarning[]
    /**
     * True iff the channel's status_history had `duty: None` entries
     * that the daemon backfilled with calibration-derived true-duty
     * values at the end of the sweep. On completion the UI uses this
     * bit to decide whether to prompt for a reload (so the chart
     * series list is rebuilt against the now-populated history). On
     * delete the daemon uses the same bit to clear stale derived
     * duty values from history before the calibration is removed.
     *
     * Optional because older persisted calibrations pre-date the
     * field; serde defaults to false on the daemon side.
     */
    was_rpm_only?: boolean
    /**
     * User override for the cold-start kick boost. `null` defers to
     * the daemon heuristic (auto). `true` forces the boost on for
     * every cold start; `false` silences it.
     */
    kick_boost_override: boolean | null
    /**
     * User override for the kick-write duration (ms). `null` uses the
     * calibrated `kick_duration_ms`. Set to force the dispatcher to
     * hold the kick for a specific duration before dropping to
     * sustain.
     */
    kick_duration_override_ms: number | null
    /**
     * User override for the post-kick walk-down. `null` defers to the
     * default (walk down in small steps from kick to sustain). `true`
     * forces the walk on; `false` jumps from kick straight to sustain.
     * Skipping the walk is safe on hardware that tolerates an abrupt
     * drop and removes the visible 1-22 second ramp on every cold
     * start. Optional because older persisted calibrations pre-date
     * the field; the daemon defaults to None (walk enabled).
     */
    walk_after_kick_override?: boolean | null
    /**
     * Resolved cold-start kick-boost decision for this channel
     * (override or heuristic). `true` means the dispatcher will issue
     * the boost on the next Off->Kicking transition. Computed by the
     * daemon on every read; the UI uses it to clarify what `Auto`
     * resolves to per-fan.
     */
    kick_boost_active: boolean
    /** ISO 8601 timestamp of when the diagnosis finished. */
    timestamp: string
}

/**
 * Non-fatal reliability finding on a calibration. Surfaced in the
 * popover status text and (via the SSE notifications channel) as a
 * desktop notification. Discriminated union tagged by `kind`,
 * matching the daemon's `#[serde(tag = "kind", rename_all = "snake_case")]`.
 */
export type CalibrationWarning =
    | { kind: 'no_tachometer' }
    | { kind: 'not_controllable' }
    | { kind: 'limited_range'; rpm_span: number; rpm_max: number }
    | { kind: 'oscillating'; lower_duty: number; upper_duty: number }

/**
 * The stage label embedded in an in-progress status. The values
 * match `coolercontrold::calibration::DiagnosisPhase` after
 * `serde(rename_all = "snake_case")`.
 */
export type CalibrationStage = 'preflight' | 'up_sweep' | 'down_sweep' | 'finalizing'

/**
 * Latest known calibration status for one channel. Discriminated
 * union tagged by `phase`. Polled by the UI at ~1 Hz while
 * `phase === 'in_progress'`. After a terminal transition the value
 * stays sticky until the next diagnosis starts on the same channel.
 *
 * `NotStarted` is returned by the daemon when neither an in-memory
 * snapshot nor a persisted calibration exists for the channel, so
 * the polling endpoint always returns 200.
 *
 * Mirrors `coolercontrold::api::actor::CalibrationStatus`.
 */
export type CalibrationStatus =
    | CalibrationStatusNotStarted
    | CalibrationStatusInProgress
    | CalibrationStatusCompleted
    | CalibrationStatusFailed

export interface CalibrationStatusNotStarted {
    phase: 'not_started'
    device_uid: UID
    channel_name: string
}

export interface CalibrationStatusInProgress {
    phase: 'in_progress'
    device_uid: UID
    channel_name: string
    stage: CalibrationStage
    percent: number
    current_duty: number | null
    current_rpm: number | null
    updated_at: string
}

export interface CalibrationStatusCompleted {
    phase: 'completed'
    device_uid: UID
    channel_name: string
    completed_at: string
    calibration: Calibration
}

export interface CalibrationStatusFailed {
    phase: 'failed'
    device_uid: UID
    channel_name: string
    failed_at: string
    /**
     * Machine-readable code matching one of the
     * `coolercontrold::calibration::DiagnosisFailure` variants:
     * `preflight_temp_too_high`, `fan_unresponsive`, `temp_aborted`,
     * `user_cancelled`, `write_failed`, `restore_failed`,
     * `persist_failed`.
     */
    reason: string
    /** Human-readable explanation for display. */
    message: string
}
