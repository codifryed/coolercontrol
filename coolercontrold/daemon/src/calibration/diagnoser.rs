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

//! Per-channel diagnosis sweep that produces a `Calibration`.
//!
//! The diagnoser owns the temporal workflow: pre-flight thermal
//! checks, snapshotting the channel's current setting, marking the
//! channel `under_diagnosis` so the engine's dispatch becomes a
//! no-op, the up-sweep and down-sweep, classification, persistence,
//! and restoring the snapshotted setting at the end.
//!
//! All I/O is funnelled through the [`DiagnosisHost`] trait so the
//! sweep can be exercised against a mock in unit tests. The host is
//! implemented for real by a thin adapter living on the engine in a
//! later phase; Phase 3b-i only ships the diagnoser itself.

#![allow(dead_code)]

use super::curve::{
    classify_curve, derive_scalars, Calibration, CurveKind, DUTY_STEP_PERCENT, SAMPLE_COUNT,
};
use super::state::FanStateMap;
use super::store::CalibrationStore;
use super::ChannelKey;
use crate::device::{ChannelName, DeviceUID, Duty, RPM, UID};
use crate::setting::ProfileUID;
use anyhow::Result;
use async_trait::async_trait;
use chrono::Local;
use std::collections::VecDeque;
use tokio_util::sync::CancellationToken;

/// Tunable parameters for a single diagnosis run. Defaults: 75 C
/// pre-flight gate, 85 C abort, 50 RPM start-detection floor, a
/// conservative kick-duration default, and an adaptive per-step
/// settle. The settle parameters are documented inline below.
#[derive(Debug, Clone)]
pub struct DiagnosisSettings {
    pub start_temp_max_c: f64,
    pub abort_temp_max_c: f64,
    pub start_rpm_min: RPM,
    pub kick_duration_default_ms: u32,
    /// Initial sleep after a duty write before waiting for the cache
    /// to refresh. Insurance that the write has at least been queued
    /// at the repo layer. Cheap; even on slow devices, 200 ms is
    /// below any practical poll cycle.
    pub min_settle_ms: u32,
    /// Number of consecutive RPM samples that must agree within the
    /// stability tolerance for the step to be considered settled.
    pub stability_window: u32,
    /// Absolute RPM tolerance for stability (max - min over the
    /// stability window).
    pub stability_tolerance_rpm: RPM,
    /// Relative RPM tolerance for stability, in percent of the
    /// largest sample seen in the window.
    pub stability_tolerance_percent: u8,
    /// Inner busy-wait granularity while waiting for a fresh status
    /// timestamp. Independent of poll rate; just bounds the latency
    /// between cache refresh and us noticing.
    pub status_poll_interval_ms: u32,
    /// Fraction of `step_settle_cap_ms` to wait for an RPM-value
    /// change after a duty write before accepting the cache value
    /// as-is. A timestamp advance proves only that the main loop
    /// ticked; an RPM-value change additionally proves a fresh
    /// post-write read happened (real fans jitter every successful
    /// read). The fallback covers the exception cases: non-
    /// controllable fans, missing fans, faulty RPM sensors, and the
    /// genuinely-constant low-duty stopped-fan case.
    pub fresh_read_cap_percent: u8,
}

impl Default for DiagnosisSettings {
    fn default() -> Self {
        Self {
            start_temp_max_c: 75.0,
            abort_temp_max_c: 85.0,
            start_rpm_min: 50,
            kick_duration_default_ms: 1500,
            min_settle_ms: 200,
            stability_window: 3,
            stability_tolerance_rpm: 50,
            stability_tolerance_percent: 3,
            status_poll_interval_ms: 50,
            fresh_read_cap_percent: 50,
        }
    }
}

/// Reasons a diagnosis can fail. The caller surfaces these directly
/// to the user (the API layer in Phase 3b-ii will map them onto REST
/// error codes / SSE `calibration_failed` events).
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosisFailure {
    /// One or more temperature sensors crossed the pre-flight limit
    /// before the sweep started.
    PreflightTempTooHigh { observed: f64, limit: f64 },
    /// The up-sweep finished without any RPM sample crossing the
    /// start floor; the fan never produced detectable motion.
    FanUnresponsive,
    /// A temperature sensor crossed the abort limit mid-sweep. The
    /// channel was written to 0% and the snapshot restored.
    TempAbortedAt { observed: f64, limit: f64 },
    /// The caller's `CancellationToken` was triggered.
    Cancelled,
    /// A hardware write failed (repo I/O error). The diagnoser
    /// preserves the original error message verbatim.
    WriteFailed(String),
    /// Restoring the snapshotted setting after the sweep failed.
    /// The calibration itself is persisted if the failure happens
    /// after persistence; otherwise the calibration is discarded.
    RestoreFailed(String),
    /// Persisting the calibration to disk failed. The snapshot has
    /// already been restored at this point.
    PersistFailed(String),
}

/// Captured channel setting taken before a diagnosis starts. The
/// host returns this from `snapshot_setting` and consumes it on
/// `restore_setting` once the sweep is complete (success or fail).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub kind: SnapshotKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotKind {
    /// No prior duty setting for this channel; restore is a no-op.
    None,
    /// Channel was on a fixed manual duty.
    Manual(Duty),
    /// Channel was assigned a profile.
    Profile(ProfileUID),
}

/// A progress notification emitted by the diagnoser. Phase 4a-ii will
/// broadcast these as `calibration_progress` SSE events; Phase 4a-i
/// just plumbs the type through.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosisProgress {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub phase: DiagnosisPhase,
    /// Percent complete across both sweeps. 0 at preflight, 100 on
    /// `Finalizing`.
    pub percent: u8,
    /// Most recent device-duty the sweep wrote, if any.
    pub current_duty: Option<Duty>,
    /// Most recent RPM the sweep observed, if any.
    pub current_rpm: Option<RPM>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosisPhase {
    Preflight,
    UpSweep,
    DownSweep,
    Finalizing,
}

/// Single trait carrying every I/O dependency the diagnoser needs.
///
/// The production implementation lives on the engine (Phase 3b-ii)
/// and dispatches the calls to the repos / config / sleep timers.
/// Tests implement this trait directly with synthetic RPM curves and
/// instantly-elapsed sleeps.
#[async_trait(?Send)]
pub trait DiagnosisHost {
    /// Latest measured RPM for the channel, or `None` if no reading.
    async fn current_rpm(&self, device_uid: &UID, channel_name: &str) -> Option<RPM>;

    /// Timestamp (millis since epoch) of the most recent status entry
    /// for the device, or `None` if no status has landed yet.
    ///
    /// The diagnoser uses the advance of this value to detect that
    /// the main loop has refreshed the device's cached status, so
    /// per-step settling does not race ahead of the cache. Note that
    /// the main loop publishes a status every poll cycle regardless
    /// of whether the underlying read succeeded; a timestamp advance
    /// is not a strict guarantee of a fresh post-write value. It is
    /// however a strong proxy for healthy devices, and the
    /// stability-window check below mitigates the outlier where a
    /// wedged device's status keeps re-publishing the same value.
    async fn latest_status_timestamp_ms(&self, device_uid: &UID) -> Option<i64>;

    /// Per-step cap for the adaptive settle in milliseconds. Should
    /// be `device_write_permit_timeout + device_read_permit_timeout`
    /// so a step cannot wait longer than the daemon itself would
    /// tolerate a single write + read against this device.
    fn step_settle_cap_ms(&self, device_uid: &UID) -> u32;

    /// Write a device-duty value directly to the hardware, bypassing
    /// the calibration dispatch (it is currently paused for this
    /// channel because we marked it `under_diagnosis`).
    async fn write_raw_duty(&self, device_uid: &UID, channel_name: &str, duty: Duty) -> Result<()>;

    /// Highest current temperature in Celsius across every monitored
    /// device. Used for pre-flight gating and mid-sweep abort.
    async fn max_temp_celsius(&self) -> f64;

    /// Capture the channel's current setting so the diagnoser can
    /// restore it afterwards. Synchronous because reading from the
    /// in-memory config is cheap.
    fn snapshot_setting(&self, device_uid: &UID, channel_name: &str) -> SettingsSnapshot;

    /// Reapply a previously snapshotted setting. The async fn body
    /// will typically delegate to `engine.set_config_setting`.
    async fn restore_setting(&self, snapshot: &SettingsSnapshot) -> Result<()>;

    /// Sleep for the given number of milliseconds. Separately
    /// abstracted so unit tests can no-op the wall-clock waits.
    async fn sleep_millis(&self, millis: u32);

    /// Receive a progress notification from the diagnoser. Default
    /// no-op so unit tests using a minimal mock host do not have to
    /// implement it; the production engine wires this to an SSE
    /// broadcast in Phase 4a-ii.
    fn emit_progress(&self, _progress: DiagnosisProgress) {}
}

/// Run a single calibration diagnosis on `(device_uid, channel_name)`.
///
/// On success the produced `Calibration` is inserted into the store's
/// **in-memory** map and also returned to the caller; the caller is
/// then responsible for triggering `store.save_to_disk()` if disk
/// persistence is desired. Decoupling disk I/O from the sweep keeps
/// the diagnoser free of filesystem dependencies (so unit tests run
/// without write access to `/etc/coolercontrol`).
///
/// On any failure the snapshotted setting is reapplied via
/// `host.restore_setting` and the channel's `under_diagnosis` flag
/// is cleared.
pub async fn run_diagnosis<H>(
    state: &FanStateMap,
    store: &CalibrationStore,
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: DeviceUID,
    channel_name: ChannelName,
    cancellation: CancellationToken,
) -> Result<Calibration, DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    let key: ChannelKey = (device_uid.clone(), channel_name.clone());

    host.emit_progress(DiagnosisProgress {
        device_uid: device_uid.clone(),
        channel_name: channel_name.clone(),
        phase: DiagnosisPhase::Preflight,
        percent: 0,
        current_duty: None,
        current_rpm: None,
    });

    let preflight_temp = host.max_temp_celsius().await;
    if preflight_temp >= settings.start_temp_max_c {
        return Err(DiagnosisFailure::PreflightTempTooHigh {
            observed: preflight_temp,
            limit: settings.start_temp_max_c,
        });
    }

    let snapshot = host.snapshot_setting(&device_uid, &channel_name);
    // Reset FanState to Off alongside setting under_diagnosis. Stops a
    // stale Kicking timer from writing the prior sustain duty over the
    // sweep, and leaves Off carried into the post-sweep restore so the
    // restore-time dispatch can do a fresh kick under the new mapping.
    state.begin_diagnosis(key.clone());

    let sweep_outcome =
        perform_sweep(host, settings, &device_uid, &channel_name, &cancellation).await;

    host.emit_progress(DiagnosisProgress {
        device_uid: device_uid.clone(),
        channel_name: channel_name.clone(),
        phase: DiagnosisPhase::Finalizing,
        percent: 100,
        current_duty: None,
        current_rpm: None,
    });

    // Clear the flag BEFORE restore so the production restore path
    // (which routes through `engine.set_fixed_speed` -> `dispatch_local`)
    // is not no-op'd by the still-active under_diagnosis check.
    state.set_under_diagnosis(key.clone(), false);
    let restore_result = host.restore_setting(&snapshot).await;

    let (up_curve, down_curve) = match sweep_outcome {
        Ok(curves) => curves,
        Err(failure) => return Err(failure),
    };

    let Some(scalars) = derive_scalars(&up_curve, &down_curve) else {
        return Err(DiagnosisFailure::FanUnresponsive);
    };
    let curve_kind = classify_curve(&up_curve, scalars.rpm_max);

    let calibration = Calibration {
        up_curve,
        down_curve,
        kick_duration_ms: settings.kick_duration_default_ms,
        min_start_duty: scalars.min_start_duty,
        min_sustain_duty: scalars.min_sustain_duty,
        max_eff_duty: scalars.max_eff_duty,
        rpm_max: scalars.rpm_max,
        curve_kind,
        timestamp: Local::now(),
    };

    store.insert_unsaved(key, calibration.clone());

    if let Err(err) = restore_result {
        return Err(DiagnosisFailure::RestoreFailed(err.to_string()));
    }

    Ok(calibration)
}

/// Sweep the duty range up and down, returning the two RPM curves
/// (one per direction). Mid-sweep failures (temp abort, cancellation,
/// write failure) short-circuit the function; the caller still
/// reapplies the snapshot and clears `under_diagnosis` afterwards.
async fn perform_sweep<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
) -> Result<([RPM; SAMPLE_COUNT], [RPM; SAMPLE_COUNT]), DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    let mut up_curve = [0u32; SAMPLE_COUNT];
    let mut down_curve = [0u32; SAMPLE_COUNT];

    for i in 0..SAMPLE_COUNT {
        sweep_step(
            host,
            settings,
            device_uid,
            channel_name,
            cancellation,
            i,
            &mut up_curve,
            DiagnosisPhase::UpSweep,
        )
        .await?;
    }
    for i in (0..SAMPLE_COUNT).rev() {
        sweep_step(
            host,
            settings,
            device_uid,
            channel_name,
            cancellation,
            i,
            &mut down_curve,
            DiagnosisPhase::DownSweep,
        )
        .await?;
    }
    Ok((up_curve, down_curve))
}

/// One step of the sweep: write the duty at index `i`, adaptively
/// settle by watching the device's status timestamp advance, sample
/// RPM across a small window, and record the median once consecutive
/// samples agree within tolerance. Falls back to "median of what we
/// have" if the per-step cap elapses. Temp abort and cancellation
/// are checked inside the settle loop, not just at step boundaries,
/// so the diagnoser stays responsive on slow devices.
async fn sweep_step<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
    i: usize,
    target: &mut [RPM; SAMPLE_COUNT],
    phase: DiagnosisPhase,
) -> Result<(), DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    if cancellation.is_cancelled() {
        return Err(DiagnosisFailure::Cancelled);
    }
    let duty = index_to_duty(i);
    // Snapshot the cached RPM before issuing the write. The settle
    // loop uses this to confirm a fresh post-write read has landed
    // (the value must change, not just the status timestamp).
    let pre_write_rpm = host
        .current_rpm(device_uid, channel_name)
        .await
        .unwrap_or(0);
    host.write_raw_duty(device_uid, channel_name, duty)
        .await
        .map_err(|err| DiagnosisFailure::WriteFailed(err.to_string()))?;

    let rpm = match settle_and_sample(
        host,
        settings,
        device_uid,
        channel_name,
        cancellation,
        pre_write_rpm,
    )
    .await
    {
        Ok(rpm) => rpm,
        Err(DiagnosisFailure::TempAbortedAt { observed, limit }) => {
            let _ = host.write_raw_duty(device_uid, channel_name, 0).await;
            return Err(DiagnosisFailure::TempAbortedAt { observed, limit });
        }
        Err(err) => return Err(err),
    };

    target[i] = rpm;
    host.emit_progress(DiagnosisProgress {
        device_uid: device_uid.clone(),
        channel_name: channel_name.to_string(),
        phase,
        percent: progress_percent(phase, i),
        current_duty: Some(duty),
        current_rpm: Some(rpm),
    });
    Ok(())
}

/// Adaptive per-step settle. Returns the representative RPM for the
/// just-written duty.
///
/// Algorithm:
/// 1. Sleep `min_settle_ms` so the queued write has a chance to
///    actually reach hardware before we start watching the cache.
/// 2. Loop sampling at status-refresh boundaries:
///    - Wait until the device's status timestamp advances past the
///      last-seen one (with `status_poll_interval_ms` granularity).
///    - Read the latest cached RPM, push into a sliding window.
///    - Declare stable when the window is full and `max - min` over
///      it falls within `max(stability_tolerance_rpm,
///      stability_tolerance_percent% of largest)`.
/// 3. Hard cap at `step_settle_cap_ms` (write timeout + read timeout
///    for the device). On cap, return the median of whatever the
///    window holds.
async fn settle_and_sample<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
    pre_write_rpm: RPM,
) -> Result<RPM, DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    host.sleep_millis(settings.min_settle_ms).await;
    let cap_ms = host.step_settle_cap_ms(device_uid);
    let window_size = (settings.stability_window as usize).max(1);
    // After this much elapsed time without an observed RPM change,
    // accept the cache value as-is (covers stuck-sensor, non-
    // controllable, and stopped-fan cases). Saturating multiplication
    // avoids overflow on extreme settings; clamped to cap_ms.
    let fresh_read_cap_ms = (u64::from(cap_ms))
        .saturating_mul(u64::from(settings.fresh_read_cap_percent.min(100)))
        / 100;
    let fresh_read_cap_ms = u32::try_from(fresh_read_cap_ms).unwrap_or(cap_ms);

    let mut waited_ms: u32 = settings.min_settle_ms;
    let mut last_seen_ts = host.latest_status_timestamp_ms(device_uid).await;
    let mut saw_rpm_change = false;
    let mut window: VecDeque<RPM> = VecDeque::with_capacity(window_size);

    loop {
        // Wait for the next cache refresh, granular at status_poll_interval_ms.
        loop {
            if cancellation.is_cancelled() {
                return Err(DiagnosisFailure::Cancelled);
            }
            if waited_ms >= cap_ms {
                break;
            }
            let current = host.latest_status_timestamp_ms(device_uid).await;
            if current != last_seen_ts && current.is_some() {
                last_seen_ts = current;
                break;
            }
            host.sleep_millis(settings.status_poll_interval_ms).await;
            waited_ms = waited_ms.saturating_add(settings.status_poll_interval_ms);
        }

        let temp = host.max_temp_celsius().await;
        if temp >= settings.abort_temp_max_c {
            return Err(DiagnosisFailure::TempAbortedAt {
                observed: temp,
                limit: settings.abort_temp_max_c,
            });
        }

        let rpm = host
            .current_rpm(device_uid, channel_name)
            .await
            .unwrap_or(0);
        if rpm != pre_write_rpm {
            saw_rpm_change = true;
        }

        // Gate window updates on either an observed RPM change (proof
        // a fresh post-write read happened) or the fresh-read cap
        // elapsing (assume stuck reading is legitimate).
        let fresh_read_confirmed = saw_rpm_change || waited_ms >= fresh_read_cap_ms;
        if fresh_read_confirmed {
            if window.len() == window_size {
                window.pop_front();
            }
            window.push_back(rpm);
            if window.len() == window_size && is_stable(&window, settings) {
                return Ok(median_of(&window));
            }
        }
        if waited_ms >= cap_ms {
            // Cap exhausted: ensure we return something concrete even
            // if no fresh read was ever confirmed (extreme stuck-sensor
            // case where even the fresh-read fallback never fired).
            if window.is_empty() {
                window.push_back(rpm);
            }
            return Ok(median_of(&window));
        }
    }
}

/// True when the largest and smallest RPM in the window agree within
/// the larger of the absolute and percent tolerances.
fn is_stable(window: &VecDeque<RPM>, settings: &DiagnosisSettings) -> bool {
    let Some(&max) = window.iter().max() else {
        return false;
    };
    let Some(&min) = window.iter().min() else {
        return false;
    };
    let spread = max.saturating_sub(min);
    let pct_tolerance = (u64::from(max) * u64::from(settings.stability_tolerance_percent) / 100)
        .try_into()
        .unwrap_or(RPM::MAX);
    let tolerance = settings.stability_tolerance_rpm.max(pct_tolerance);
    spread <= tolerance
}

/// Median of the window. The window is small (default 3), so sorting
/// a clone is the simplest correct approach.
fn median_of(window: &VecDeque<RPM>) -> RPM {
    if window.is_empty() {
        return 0;
    }
    let mut buf: Vec<RPM> = window.iter().copied().collect();
    buf.sort_unstable();
    buf[buf.len() / 2]
}

/// Map a sweep step to a 0..=100 percent. The up-sweep occupies the
/// first half (0..50), the down-sweep the second half (50..100).
fn progress_percent(phase: DiagnosisPhase, idx: usize) -> u8 {
    const HALF: u32 = 50;
    let denom = u32::try_from(SAMPLE_COUNT).expect("SAMPLE_COUNT fits in u32");
    let step = u32::try_from(idx + 1).unwrap_or(denom);
    let half_progress = (step * HALF) / denom;
    let percent = match phase {
        DiagnosisPhase::UpSweep => half_progress,
        DiagnosisPhase::DownSweep => HALF + half_progress,
        DiagnosisPhase::Preflight => 0,
        DiagnosisPhase::Finalizing => 100,
    };
    u8::try_from(percent.min(100)).unwrap_or(100)
}

fn index_to_duty(idx: usize) -> Duty {
    assert!(idx < SAMPLE_COUNT);
    // SAMPLE_COUNT <= u8::MAX asserted at compile time in curve.rs.
    let idx_u8 = u8::try_from(idx).expect("SAMPLE_COUNT <= u8::MAX (const-asserted)");
    idx_u8 * DUTY_STEP_PERCENT
}

#[cfg(test)]
mod tests {
    use super::super::state::FanState;
    use super::*;
    use anyhow::anyhow;
    use std::cell::{Cell, RefCell};

    struct MockHost {
        rpm_for_duty: RefCell<std::collections::HashMap<Duty, RPM>>,
        duty_writes: RefCell<Vec<Duty>>,
        last_written_duty: Cell<Duty>,
        temp: Cell<f64>,
        temp_after_step: Cell<Option<(usize, f64)>>,
        step_counter: Cell<usize>,
        snapshots_taken: RefCell<Vec<SettingsSnapshot>>,
        restores_applied: RefCell<Vec<SettingsSnapshot>>,
        restore_should_fail: Cell<bool>,
        fail_write_at_step: Cell<Option<usize>>,
        progress_events: RefCell<Vec<DiagnosisProgress>>,
        // Synthetic status timestamp. Auto-advances on each sleep_millis
        // so the diagnoser's settle loop sees fresh "cache refreshes"
        // every tick without tests needing to wire timestamps explicitly.
        status_timestamp_ms: Cell<i64>,
        // Cap returned to the diagnoser. 1 s is generous for tests; the
        // sleeps are instant so the cap effectively gates only the
        // iteration count of the inner status-poll loop.
        step_cap_ms: Cell<u32>,
        // When `stale_reads_remaining > 0`, current_rpm returns
        // `stale_rpm` (simulating a cache that has not yet refreshed
        // after a duty write). Used to exercise the RPM-change-detection
        // path in settle_and_sample.
        stale_reads_remaining: Cell<usize>,
        stale_rpm: Cell<RPM>,
    }

    impl MockHost {
        fn new() -> Self {
            Self {
                rpm_for_duty: RefCell::new(std::collections::HashMap::new()),
                duty_writes: RefCell::new(Vec::new()),
                last_written_duty: Cell::new(0),
                temp: Cell::new(30.0),
                temp_after_step: Cell::new(None),
                step_counter: Cell::new(0),
                snapshots_taken: RefCell::new(Vec::new()),
                restores_applied: RefCell::new(Vec::new()),
                restore_should_fail: Cell::new(false),
                fail_write_at_step: Cell::new(None),
                progress_events: RefCell::new(Vec::new()),
                status_timestamp_ms: Cell::new(0),
                step_cap_ms: Cell::new(1000),
                stale_reads_remaining: Cell::new(0),
                stale_rpm: Cell::new(0),
            }
        }

        /// Force the next `count` `current_rpm` calls to return `value`,
        /// simulating a cache that has not refreshed since a duty write.
        /// After `count` calls, normal `rpm_for_duty` lookup resumes.
        fn with_stale_reads(self, count: usize, value: RPM) -> Self {
            self.stale_reads_remaining.set(count);
            self.stale_rpm.set(value);
            self
        }

        /// Configure the host to map device-duty -> RPM linearly in
        /// 100-RPM steps (mirrors the synthetic smooth curve from
        /// the unit tests in curve.rs).
        fn with_smooth_fan(self) -> Self {
            for i in 0..SAMPLE_COUNT {
                let duty = index_to_duty(i);
                let rpm = 100 * u32::try_from(i).expect("SAMPLE_COUNT fits in u32");
                self.rpm_for_duty.borrow_mut().insert(duty, rpm);
            }
            self
        }

        /// Configure the host as a stepped fan with three RPM
        /// plateaus (matches the curve.rs synthetic stepped fan).
        fn with_stepped_fan(self) -> Self {
            for i in 0..SAMPLE_COUNT {
                let duty = index_to_duty(i);
                let rpm = if i < 5 {
                    0
                } else if i < 13 {
                    1000
                } else {
                    2000
                };
                self.rpm_for_duty.borrow_mut().insert(duty, rpm);
            }
            self
        }

        /// Configure the host as an unresponsive fan (RPM=0 at every
        /// duty). The diagnoser must surface `FanUnresponsive`.
        fn with_dead_fan(self) -> Self {
            for i in 0..SAMPLE_COUNT {
                let duty = index_to_duty(i);
                self.rpm_for_duty.borrow_mut().insert(duty, 0);
            }
            self
        }
    }

    #[async_trait(?Send)]
    impl DiagnosisHost for MockHost {
        async fn current_rpm(&self, _device_uid: &UID, _channel_name: &str) -> Option<RPM> {
            let remaining = self.stale_reads_remaining.get();
            if remaining > 0 {
                self.stale_reads_remaining.set(remaining - 1);
                return Some(self.stale_rpm.get());
            }
            self.rpm_for_duty
                .borrow()
                .get(&self.last_written_duty.get())
                .copied()
        }

        async fn write_raw_duty(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
            duty: Duty,
        ) -> Result<()> {
            let step = self.step_counter.get();
            self.step_counter.set(step + 1);
            if let Some(fail_at) = self.fail_write_at_step.get() {
                if step == fail_at {
                    return Err(anyhow!("simulated write failure at step {step}"));
                }
            }
            self.duty_writes.borrow_mut().push(duty);
            self.last_written_duty.set(duty);
            Ok(())
        }

        async fn max_temp_celsius(&self) -> f64 {
            if let Some((step, override_temp)) = self.temp_after_step.get() {
                if self.step_counter.get() >= step {
                    return override_temp;
                }
            }
            self.temp.get()
        }

        fn snapshot_setting(&self, device_uid: &UID, channel_name: &str) -> SettingsSnapshot {
            let snapshot = SettingsSnapshot {
                device_uid: device_uid.clone(),
                channel_name: channel_name.to_string(),
                kind: SnapshotKind::Manual(50),
            };
            self.snapshots_taken.borrow_mut().push(snapshot.clone());
            snapshot
        }

        async fn restore_setting(&self, snapshot: &SettingsSnapshot) -> Result<()> {
            if self.restore_should_fail.get() {
                return Err(anyhow!("simulated restore failure"));
            }
            self.restores_applied.borrow_mut().push(snapshot.clone());
            Ok(())
        }

        async fn sleep_millis(&self, ms: u32) {
            // Advance the synthetic status timestamp so the diagnoser's
            // settle loop sees a fresh "cache refresh" each sleep. Real
            // sleep is a no-op for tests.
            self.status_timestamp_ms
                .set(self.status_timestamp_ms.get() + i64::from(ms.max(1)));
        }

        async fn latest_status_timestamp_ms(&self, _device_uid: &UID) -> Option<i64> {
            Some(self.status_timestamp_ms.get())
        }

        fn step_settle_cap_ms(&self, _device_uid: &UID) -> u32 {
            self.step_cap_ms.get()
        }

        fn emit_progress(&self, progress: DiagnosisProgress) {
            self.progress_events.borrow_mut().push(progress);
        }
    }

    fn key(dev: &str, chan: &str) -> ChannelKey {
        (dev.to_string(), chan.to_string())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn happy_path_smooth_fan_produces_smooth_calibration() {
        // Goal: a synthetic smooth fan completes the sweep, the
        // diagnoser classifies the curve as Smooth, persists a
        // Calibration into the store, and restores the snapshot.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let result = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect("smooth fan diagnoses successfully");

        assert_eq!(result.curve_kind, CurveKind::Smooth);
        assert!(result.rpm_max > 0);
        assert!(store.has(&key("dev-a", "fan1")));
        assert_eq!(host.snapshots_taken.borrow().len(), 1);
        assert_eq!(host.restores_applied.borrow().len(), 1);
        assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stepped_fan_produces_stepped_calibration() {
        // Goal: a synthetic stepped fan (sparse RPM plateaus) is
        // classified Stepped and persisted; the dispatch layer then
        // leaves the channel in passthrough mode (covered by curve
        // and dispatch tests).
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_stepped_fan();
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let result = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect("stepped fan diagnoses successfully");

        assert_eq!(result.curve_kind, CurveKind::Stepped);
        assert!(store.has(&key("dev-a", "fan1")));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn preflight_temp_too_high_short_circuits() {
        // Goal: when ambient temp already exceeds the pre-flight
        // gate, the diagnoser refuses to start. No writes, no
        // snapshot, no calibration persisted.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        host.temp.set(80.0);
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("preflight rejects hot system");

        assert!(matches!(err, DiagnosisFailure::PreflightTempTooHigh { .. }));
        assert!(host.duty_writes.borrow().is_empty());
        assert!(host.snapshots_taken.borrow().is_empty());
        assert!(store.has(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn dead_fan_yields_fan_unresponsive() {
        // Goal: a fan whose RPM never crosses the start floor
        // surfaces as FanUnresponsive. The snapshot is still
        // restored and the under_diagnosis flag is cleared.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_dead_fan();
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("dead fan rejected");

        assert_eq!(err, DiagnosisFailure::FanUnresponsive);
        assert!(store.has(&key("dev-a", "fan1")).not());
        assert_eq!(host.restores_applied.borrow().len(), 1);
        assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn temp_abort_mid_sweep_zeros_channel_and_restores_snapshot() {
        // Goal: a temperature climb past the abort gate during the
        // sweep terminates the run, writes 0 to the channel for
        // safety, restores the snapshot, and clears under_diagnosis.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        host.temp_after_step.set(Some((5, 90.0)));
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("abort surfaces on hot temp");

        assert!(matches!(err, DiagnosisFailure::TempAbortedAt { .. }));
        let writes = host.duty_writes.borrow();
        let last_write = *writes.last().expect("at least one write");
        assert_eq!(last_write, 0, "safety write of 0% must be last");
        assert!(store.has(&key("dev-a", "fan1")).not());
        assert_eq!(host.restores_applied.borrow().len(), 1);
        assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_short_circuits_and_restores_snapshot() {
        // Goal: a pre-triggered cancellation makes the sweep bail
        // before the first write completes. The snapshot is still
        // restored.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();
        cancellation.cancel();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("cancellation propagates");

        assert_eq!(err, DiagnosisFailure::Cancelled);
        assert_eq!(host.restores_applied.borrow().len(), 1);
        assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn write_failure_during_sweep_surfaces_as_write_failed() {
        // Goal: a hardware write error mid-sweep terminates with
        // WriteFailed carrying the inner error message; snapshot is
        // restored and no calibration is persisted.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        host.fail_write_at_step.set(Some(3));
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("write failure surfaces");

        assert!(matches!(err, DiagnosisFailure::WriteFailed(_)));
        assert!(store.has(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn restore_failure_surfaces_after_persistence() {
        // Goal: even when the snapshot restore fails, the
        // calibration produced by a successful sweep is persisted to
        // the store. The error surfaces as RestoreFailed so the
        // caller (and UI) can warn the user; calibration data is not
        // lost.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        host.restore_should_fail.set(true);
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("restore failure surfaces");

        assert!(matches!(err, DiagnosisFailure::RestoreFailed(_)));
        assert!(
            store.has(&key("dev-a", "fan1")),
            "calibration persisted even when restore fails"
        );
        assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn under_diagnosis_flag_set_during_sweep() {
        // Goal: the under_diagnosis flag must be set for the
        // duration of the sweep so the engine's dispatch becomes a
        // no-op on that channel. We observe this from inside a
        // probe trait method (`write_raw_duty`) by reading the
        // state map mid-sweep through a shared Rc.
        struct ProbingHost {
            inner: MockHost,
            state_seen_during_writes: RefCell<Vec<bool>>,
            state: Rc<FanStateMap>,
            channel_key: ChannelKey,
        }

        #[async_trait(?Send)]
        impl DiagnosisHost for ProbingHost {
            async fn current_rpm(&self, d: &UID, c: &str) -> Option<RPM> {
                self.inner.current_rpm(d, c).await
            }
            async fn latest_status_timestamp_ms(&self, d: &UID) -> Option<i64> {
                self.inner.latest_status_timestamp_ms(d).await
            }
            fn step_settle_cap_ms(&self, d: &UID) -> u32 {
                self.inner.step_settle_cap_ms(d)
            }
            async fn write_raw_duty(&self, d: &UID, c: &str, duty: Duty) -> Result<()> {
                self.state_seen_during_writes
                    .borrow_mut()
                    .push(self.state.is_under_diagnosis(&self.channel_key));
                self.inner.write_raw_duty(d, c, duty).await
            }
            async fn max_temp_celsius(&self) -> f64 {
                self.inner.max_temp_celsius().await
            }
            fn snapshot_setting(&self, d: &UID, c: &str) -> SettingsSnapshot {
                self.inner.snapshot_setting(d, c)
            }
            async fn restore_setting(&self, s: &SettingsSnapshot) -> Result<()> {
                self.inner.restore_setting(s).await
            }
            async fn sleep_millis(&self, m: u32) {
                self.inner.sleep_millis(m).await;
            }
        }

        let state = Rc::new(FanStateMap::new());
        let store = CalibrationStore::empty();
        let probing_host = ProbingHost {
            inner: MockHost::new().with_smooth_fan(),
            state_seen_during_writes: RefCell::new(Vec::new()),
            state: Rc::clone(&state),
            channel_key: key("dev-a", "fan1"),
        };
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        run_diagnosis(
            &state,
            &store,
            &probing_host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect("happy path");

        assert!(
            probing_host
                .state_seen_during_writes
                .borrow()
                .iter()
                .all(|seen| *seen),
            "under_diagnosis flag must be true on every write"
        );
        assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn progress_events_cover_preflight_sweep_and_finalize() {
        // Goal: a successful diagnosis emits at least one preflight
        // event, one or more per-sweep events with monotonically
        // non-decreasing percent for the up-sweep half, and a final
        // finalizing event at 100%. This is what SSE clients consume
        // to render the progress bar in Phase 4a-ii.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan();
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect("happy path");

        let events = host.progress_events.borrow();
        assert!(events.len() >= 3, "at least preflight + sweep + finalize");
        assert_eq!(
            events.first().expect("preflight").phase,
            DiagnosisPhase::Preflight
        );
        assert_eq!(
            events.last().expect("finalize").phase,
            DiagnosisPhase::Finalizing
        );
        assert_eq!(events.last().expect("finalize").percent, 100);
        let up_event_count = events
            .iter()
            .filter(|e| e.phase == DiagnosisPhase::UpSweep)
            .count();
        let down_event_count = events
            .iter()
            .filter(|e| e.phase == DiagnosisPhase::DownSweep)
            .count();
        assert_eq!(
            up_event_count, SAMPLE_COUNT,
            "one progress event per up-sweep step"
        );
        assert_eq!(
            down_event_count, SAMPLE_COUNT,
            "one progress event per down-sweep step"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn pre_diagnosis_kicking_state_is_cleared_at_sweep_start() {
        // Goal: when a re-calibration starts on a channel that was
        // mid-kick from a prior dispatch, the sweep must reset
        // FanState to Off. Otherwise a deferred complete_kick task
        // spawned by the prior dispatch would observe Kicking and
        // write the old sustain duty over the diagnoser's raw write
        // mid-sweep.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        state.replace(
            key("dev-a", "fan1"),
            crate::calibration::state::ChannelEntry {
                state: FanState::Kicking { sustain_target: 60 },
                under_diagnosis: false,
            },
        );
        let host = MockHost::new().with_smooth_fan();
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect("happy path");

        // After the sweep, FanState must be Off (the sweep ended at 0%)
        // and under_diagnosis must be cleared.
        let entry = state.entry(&key("dev-a", "fan1"));
        assert_eq!(entry.state, FanState::Off);
        assert!(entry.under_diagnosis.not());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stale_cache_does_not_lock_in_pre_write_value() {
        // Goal: when the cache returns the pre-write RPM for several
        // status updates after a duty write (e.g. main-loop read
        // failed or was coalesced away), the diagnoser must wait for
        // the value to actually change before treating samples as
        // valid. Otherwise the sweep would record the stale RPM as
        // the post-write reading, biasing the calibration curve.
        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let host = MockHost::new().with_smooth_fan().with_stale_reads(5, 800);
        let settings = DiagnosisSettings::default();
        let cancellation = CancellationToken::new();

        let result = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect("smooth fan diagnoses successfully even with stale reads");

        // The synthetic smooth fan returns rpm = 100 * idx for each
        // duty index. With_stale_reads(5, 800) makes the first 5
        // current_rpm calls return 800 instead. The very first sweep
        // step (duty=0) must therefore reach the post-stale read of 0,
        // not record the stale 800.
        assert_eq!(
            result.up_curve[0], 0,
            "stale 800 must not be recorded; settle should wait for the post-write change to 0"
        );
        // The rest of the up-curve should follow the synthetic linear
        // fan exactly, since stale_reads exhausted during the first
        // step.
        assert_eq!(result.up_curve[5], 500);
        assert_eq!(result.up_curve[10], 1000);
    }

    #[test]
    fn is_stable_returns_true_when_window_fits_absolute_tolerance() {
        // Goal: when the spread across the window is at or below the
        // absolute RPM tolerance, the step is considered settled even
        // for very low RPMs where the percent tolerance would round to
        // zero.
        let settings = DiagnosisSettings::default();
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(100);
        window.push_back(140);
        window.push_back(120);
        // spread = 40, abs tolerance = 50 -> stable.
        assert!(is_stable(&window, &settings));
    }

    #[test]
    fn is_stable_returns_true_when_window_fits_percent_tolerance() {
        // Goal: at higher RPMs, the percent tolerance dominates and
        // small absolute drift around a fast-spinning fan is treated
        // as stable.
        let settings = DiagnosisSettings::default();
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(1900);
        window.push_back(1950);
        window.push_back(1980);
        // spread = 80, 3% of 1980 = 59, abs tolerance = 50 -> larger
        // is 59, spread 80 exceeds it. Use a tighter case:
        window.clear();
        window.push_back(2000);
        window.push_back(2020);
        window.push_back(2050);
        // spread = 50, 3% of 2050 = 61, larger is 61, spread <= 61.
        assert!(is_stable(&window, &settings));
    }

    #[test]
    fn is_stable_returns_false_when_window_exceeds_tolerance() {
        // Goal: while the fan is still ramping (samples diverging),
        // stability must return false so the diagnoser keeps waiting.
        let settings = DiagnosisSettings::default();
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(0);
        window.push_back(400);
        window.push_back(800);
        assert!(is_stable(&window, &settings).not());
    }

    #[test]
    fn median_of_returns_middle_element_after_sort() {
        // Goal: median ignores arrival order and returns the middle
        // RPM, dampening single outlier reads when stability has
        // already been observed.
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(1500);
        window.push_back(900);
        window.push_back(1200);
        assert_eq!(median_of(&window), 1200);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancellation_during_settle_short_circuits_step() {
        // Goal: cancellation triggered mid-step is observed inside the
        // settle loop, not just at step boundaries. This keeps the
        // diagnoser responsive on slow devices where a single step
        // can otherwise wait up to step_settle_cap_ms.
        struct CancellingHost {
            inner: MockHost,
            cancel_on_status_check: Rc<RefCell<Option<CancellationToken>>>,
        }
        #[async_trait(?Send)]
        impl DiagnosisHost for CancellingHost {
            async fn current_rpm(&self, d: &UID, c: &str) -> Option<RPM> {
                self.inner.current_rpm(d, c).await
            }
            async fn latest_status_timestamp_ms(&self, d: &UID) -> Option<i64> {
                // Trip the cancel on the very first status check inside
                // settle_and_sample so the loop bails before sampling.
                if let Some(token) = self.cancel_on_status_check.borrow_mut().take() {
                    token.cancel();
                }
                self.inner.latest_status_timestamp_ms(d).await
            }
            fn step_settle_cap_ms(&self, d: &UID) -> u32 {
                self.inner.step_settle_cap_ms(d)
            }
            async fn write_raw_duty(&self, d: &UID, c: &str, duty: Duty) -> Result<()> {
                self.inner.write_raw_duty(d, c, duty).await
            }
            async fn max_temp_celsius(&self) -> f64 {
                self.inner.max_temp_celsius().await
            }
            fn snapshot_setting(&self, d: &UID, c: &str) -> SettingsSnapshot {
                self.inner.snapshot_setting(d, c)
            }
            async fn restore_setting(&self, s: &SettingsSnapshot) -> Result<()> {
                self.inner.restore_setting(s).await
            }
            async fn sleep_millis(&self, m: u32) {
                self.inner.sleep_millis(m).await;
            }
        }

        let state = FanStateMap::new();
        let store = CalibrationStore::empty();
        let cancellation = CancellationToken::new();
        let cancel_on_status_check = Rc::new(RefCell::new(Some(cancellation.clone())));
        // Force the timestamp NOT to advance, so the inner status loop
        // keeps spinning until cancellation is observed. Use a tiny cap
        // to ensure the test would take forever without the cancel hook.
        let inner = MockHost::new().with_smooth_fan();
        // freeze timestamps: override sleep to NOT advance ts. We do
        // this by giving cap that is much larger than the single
        // cancellation check we expect.
        inner.step_cap_ms.set(60_000);
        let host = CancellingHost {
            inner,
            cancel_on_status_check,
        };
        let settings = DiagnosisSettings::default();

        let err = run_diagnosis(
            &state,
            &store,
            &host,
            &settings,
            "dev-a".to_string(),
            "fan1".to_string(),
            cancellation,
        )
        .await
        .expect_err("cancellation surfaces");
        assert_eq!(err, DiagnosisFailure::Cancelled);
    }

    use std::ops::Not;
    use std::rc::Rc;
}
