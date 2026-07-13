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
//! Owns the temporal workflow: pre-flight, snapshot, sweep, classify,
//! persist, restore. All I/O routes through [`DiagnosisHost`] so the
//! sweep is testable against a mock; the engine provides the prod impl.

use super::curve::{
    classify_curve, derive_scalars, Calibration, CurveKind, DerivedScalars, DutySample,
    DUTY_STEP_DENSE, DUTY_STEP_SPARSE, KICK_ZONE_BUFFER_PERCENT, MAX_SAMPLES_PER_CURVE,
    UNRESPONSIVE_ABORT_DUTY, UP_SWEEP_DENSE_RANGE_END_DUTY,
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
use std::ops::Not;
use tokio_util::sync::CancellationToken;

/// Tunable parameters for a single diagnosis run.
#[derive(Debug, Clone)]
pub struct DiagnosisSettings {
    pub start_temp_max_c: f64,
    pub abort_temp_max_c: f64,
    pub start_rpm_min: RPM,
    pub kick_duration_default_ms: u32,
    /// Post-write wait at the first non-zero up-sweep step. Longer than
    /// `kick_duration_default_ms` because the diagnoser must see the
    /// true sustain RPM, not the kick peak: a long-lasting firmware
    /// kick at consistent RPM otherwise fills the stability window and
    /// gets recorded as the steady-state.
    pub kick_decay_settle_ms: u32,
    pub min_settle_ms: u32,
    pub stability_window: u32,
    pub stability_tolerance_rpm: RPM,
    /// Tighter tolerance for dense (2%) steps and the saturation tail,
    /// where settling is slower so we wait for tighter agreement.
    pub stability_tolerance_rpm_extremes: RPM,
    /// Duty at/above which a step is treated as extreme regardless of
    /// step size (catches the saturation tail).
    pub saturation_extreme_duty_min: Duty,
    /// Relative tolerance (percent of largest sample in the window).
    pub stability_tolerance_percent: u8,
    /// Inner busy-wait granularity while watching the status timestamp.
    pub status_poll_interval_ms: u32,
    /// Fraction of `step_settle_cap_ms` to wait for the RPM value to
    /// change after a write before accepting the cache. A timestamp
    /// advance proves only that the loop ticked; an RPM change proves
    /// a fresh post-write read landed. Fallback covers non-controllable
    /// fans, faulty sensors, and genuinely-stopped low-duty cases.
    pub fresh_read_cap_percent: u8,
}

impl Default for DiagnosisSettings {
    fn default() -> Self {
        Self {
            start_temp_max_c: 75.0,
            abort_temp_max_c: 85.0,
            start_rpm_min: 50,
            kick_duration_default_ms: 3000,
            kick_decay_settle_ms: 6000,
            min_settle_ms: 400,
            stability_window: 3,
            stability_tolerance_rpm: 30,
            stability_tolerance_rpm_extremes: 15,
            saturation_extreme_duty_min: 90,
            stability_tolerance_percent: 3,
            status_poll_interval_ms: 50,
            fresh_read_cap_percent: 50,
        }
    }
}

/// Reasons a diagnosis can fail. The API layer maps these onto REST
/// error codes and `calibration_failed` SSE events.
#[derive(Debug, Clone, PartialEq)]
pub enum DiagnosisFailure {
    /// Pre-flight temperature crossed the gate. `sensor` names the
    /// hottest reading, which may belong to an unrelated device.
    PreflightTempTooHigh {
        observed: f64,
        limit: f64,
        sensor: String,
    },
    /// Temp crossed abort mid-sweep. Channel was zeroed and snapshot
    /// restored. `sensor` names the hottest reading.
    TempAbortedAt {
        observed: f64,
        limit: f64,
        sensor: String,
    },
    Cancelled,
    /// Hardware write failed; preserves the repo error verbatim.
    WriteFailed(String),
    /// Restore failed after the sweep. The calibration is kept if
    /// persistence already happened, discarded otherwise.
    RestoreFailed(String),
    /// Disk persistence failed; snapshot has already been restored.
    PersistFailed(String),
}

/// Snapshotted channel setting. Captured before the sweep, restored
/// afterwards (success or fail).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSnapshot {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub kind: SnapshotKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SnapshotKind {
    None,
    Manual(Duty),
    Profile(ProfileUID),
}

/// Progress event broadcast as `calibration_progress` SSE.
#[derive(Debug, Clone, PartialEq)]
pub struct DiagnosisProgress {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub phase: DiagnosisPhase,
    /// 0 at preflight, 100 on `Finalizing`.
    pub percent: u8,
    pub current_duty: Option<Duty>,
    pub current_rpm: Option<RPM>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosisPhase {
    Preflight,
    UpSweep,
    DownSweep,
    Finalizing,
}

/// Hottest temperature across all devices, paired with the identity of
/// the sensor it came from. Drives the pre-flight gate and mid-sweep
/// abort; `sensor` names the offending reading (often an unrelated CPU
/// or GPU) so a temp failure tells the user which sensor to cool.
#[derive(Debug, Clone, PartialEq)]
pub struct HottestTemp {
    pub celsius: f64,
    /// Override-resolved "Device | Sensor" label. Empty only when no
    /// device reports any temperature, in which case `celsius` is 0.0
    /// and no gate can trip.
    pub sensor: String,
}

/// I/O dependencies for the diagnoser. Engine provides the prod impl;
/// tests provide mocks with synthetic curves and no-op sleeps.
#[async_trait(?Send)]
pub trait DiagnosisHost {
    async fn current_rpm(&self, device_uid: &UID, channel_name: &str) -> Option<RPM>;

    /// Watched by the settle loop to confirm a fresh cache refresh
    /// landed before sampling RPM. Returns `None` until the first
    /// status arrives.
    async fn latest_status_timestamp_ms(&self, device_uid: &UID) -> Option<i64>;

    /// Per-step settle cap. Should be `write_timeout + read_timeout`
    /// so a step never waits longer than the daemon's own I/O budget.
    fn step_settle_cap_ms(&self, device_uid: &UID) -> u32;

    /// Status-cache poll period for the device's repository. Used by
    /// `measure_settled_kick_duration` to bucket the calibrated
    /// `kick_duration_ms` to a multiple of the poll period so
    /// identical-model fans land on the same value despite cache-
    /// alignment jitter. Default of 1 s matches the daemon's typical
    /// `poll_rate`; the prod impl reads the live setting.
    fn poll_period_ms(&self, _device_uid: &UID) -> u32 {
        1000
    }

    /// Enable manual (direct duty) control on the channel before the
    /// sweep writes any duty. On hwmon this sets `pwm_enable=1`; a
    /// board left in automatic mode (every channel Unmanaged, so the
    /// firmware drives its own fan curve) rejects or ignores raw duty
    /// writes on some drivers until then. Non-controllable device
    /// types treat this as a no-op.
    async fn enter_manual_control(&self, device_uid: &UID, channel_name: &str) -> Result<()>;

    /// Writes raw duty, bypassing dispatch (channel is `under_diagnosis`).
    async fn write_raw_duty(&self, device_uid: &UID, channel_name: &str, duty: Duty) -> Result<()>;

    /// Hottest temp across all devices with the offending sensor's
    /// identity. Drives the pre-flight gate and mid-sweep abort.
    async fn hottest_temp(&self) -> HottestTemp;

    fn snapshot_setting(&self, device_uid: &UID, channel_name: &str) -> SettingsSnapshot;

    async fn restore_setting(&self, snapshot: &SettingsSnapshot) -> Result<()>;

    /// Abstracted so tests can no-op the wall-clock waits.
    async fn sleep_millis(&self, millis: u32);

    /// Default no-op so minimal mocks don't have to implement it.
    fn emit_progress(&self, _progress: DiagnosisProgress) {}
}

/// Run a single calibration diagnosis on `(device_uid, channel_name)`.
/// On success the produced `Calibration` is inserted in-memory and
/// returned; the caller flushes via `store.save_to_disk()` if desired
/// (keeps the diagnoser free of filesystem deps for tests). On any
/// failure the snapshotted setting is restored and `under_diagnosis`
/// is cleared.
pub async fn run_diagnosis<H: DiagnosisHost + ?Sized>(
    state: &FanStateMap,
    store: &CalibrationStore,
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: DeviceUID,
    channel_name: ChannelName,
    cancellation: CancellationToken,
) -> Result<Calibration, DiagnosisFailure> {
    let key: ChannelKey = (device_uid.clone(), channel_name.clone());
    run_preflight(host, settings, &device_uid, &channel_name).await?;

    let snapshot = host.snapshot_setting(&device_uid, &channel_name);
    // Reset FanState to Off so a stale Kicking timer can't write the
    // prior sustain over the sweep, and so the post-sweep restore
    // dispatches a fresh kick under the new mapping.
    state.begin_diagnosis(key.clone());

    // Enable manual control before any duty write. A channel left in
    // automatic mode (all channels Unmanaged) rejects or ignores raw
    // duty writes on some drivers. The post-sweep restore returns the
    // channel to its prior mode. Failure here aborts before the sweep.
    if let Err(err) = host.enter_manual_control(&device_uid, &channel_name).await {
        state.set_under_diagnosis(key, false);
        let _ = host.restore_setting(&snapshot).await;
        return Err(DiagnosisFailure::WriteFailed(err.to_string()));
    }

    // 0% baseline: let prior momentum or a pending firmware kick decay.
    // Write failures here are not fatal; perform_sweep surfaces its own.
    let _ = host.write_raw_duty(&device_uid, &channel_name, 0).await;
    host.sleep_millis(settings.kick_duration_default_ms).await;

    let sweep_data =
        match perform_sweep(host, settings, &device_uid, &channel_name, &cancellation).await {
            Ok(curves) => curves,
            Err(failure) => {
                emit_phase(
                    host,
                    &device_uid,
                    &channel_name,
                    DiagnosisPhase::Finalizing,
                    100,
                );
                state.set_under_diagnosis(key, false);
                let _ = host.restore_setting(&snapshot).await;
                return Err(failure);
            }
        };
    let scalars = derive_scalars(&sweep_data.0, &sweep_data.1);
    let kick_duration_ms = if let Some(s) = scalars {
        measure_settled_kick_duration(
            host,
            settings,
            &device_uid,
            &channel_name,
            &cancellation,
            &sweep_data,
            s,
        )
        .await
        .unwrap_or(settings.kick_duration_default_ms)
    } else {
        settings.kick_duration_default_ms
    };

    finalize_diagnosis(
        host,
        state,
        store,
        key,
        &snapshot,
        &device_uid,
        &channel_name,
        sweep_data,
        scalars,
        kick_duration_ms,
    )
    .await
}

fn emit_phase<H: DiagnosisHost + ?Sized>(
    host: &H,
    device_uid: &DeviceUID,
    channel_name: &ChannelName,
    phase: DiagnosisPhase,
    percent: u8,
) {
    host.emit_progress(DiagnosisProgress {
        device_uid: device_uid.clone(),
        channel_name: channel_name.clone(),
        phase,
        percent,
        current_duty: None,
        current_rpm: None,
    });
}

async fn run_preflight<H: DiagnosisHost + ?Sized>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &DeviceUID,
    channel_name: &ChannelName,
) -> Result<(), DiagnosisFailure> {
    emit_phase(host, device_uid, channel_name, DiagnosisPhase::Preflight, 0);
    let hottest = host.hottest_temp().await;
    if hottest.celsius >= settings.start_temp_max_c {
        return Err(DiagnosisFailure::PreflightTempTooHigh {
            observed: hottest.celsius,
            limit: settings.start_temp_max_c,
            sensor: hottest.sensor,
        });
    }
    Ok(())
}

/// Emit the final progress event, clear `under_diagnosis` BEFORE restore
/// (so the restore path's dispatch isn't no-op'd), persist the new
/// calibration in-memory, then surface any restore error.
#[allow(clippy::too_many_arguments)]
async fn finalize_diagnosis<H: DiagnosisHost + ?Sized>(
    host: &H,
    state: &FanStateMap,
    store: &CalibrationStore,
    key: ChannelKey,
    snapshot: &SettingsSnapshot,
    device_uid: &DeviceUID,
    channel_name: &ChannelName,
    sweep_data: (Vec<DutySample>, Vec<DutySample>, Vec<bool>),
    scalars: Option<DerivedScalars>,
    kick_duration_ms: u32,
) -> Result<Calibration, DiagnosisFailure> {
    emit_phase(
        host,
        device_uid,
        channel_name,
        DiagnosisPhase::Finalizing,
        100,
    );
    state.set_under_diagnosis(key.clone(), false);
    let restore_result = host.restore_setting(snapshot).await;

    let (up_curve, down_curve, down_stable) = sweep_data;
    let calibration = build_calibration(
        up_curve,
        down_curve,
        &down_stable,
        scalars,
        kick_duration_ms,
    );
    store.insert_unsaved(key, calibration.clone());

    if let Err(err) = restore_result {
        return Err(DiagnosisFailure::RestoreFailed(err.to_string()));
    }
    Ok(calibration)
}

/// `scalars = None` persists a passthrough record with `NoTachometer`
/// so the popover keeps the warning visible.
fn build_calibration(
    up_curve: Vec<DutySample>,
    down_curve: Vec<DutySample>,
    down_stable: &[bool],
    scalars: Option<DerivedScalars>,
    kick_duration_ms: u32,
) -> Calibration {
    use crate::calibration::curve::{derive_min_stable_duty, derive_warnings, CalibrationWarning};
    match scalars {
        Some(scalars) => {
            let mut curve_kind =
                classify_curve(&up_curve, scalars.rpm_max, scalars.min_sustain_duty);
            let mut warnings = derive_warnings(&up_curve, scalars, &mut curve_kind);
            let (min_stable_duty, band) = derive_min_stable_duty(
                &down_curve,
                down_stable,
                scalars.rpm_max,
                scalars.min_sustain_duty,
            );
            if let Some((lower_duty, upper_duty)) = band {
                warnings.push(CalibrationWarning::Oscillating {
                    lower_duty,
                    upper_duty,
                });
            }
            Calibration {
                up_curve,
                down_curve,
                kick_duration_ms,
                min_start_duty: scalars.min_start_duty,
                min_sustain_duty: scalars.min_sustain_duty,
                min_stable_duty,
                max_eff_duty: scalars.max_eff_duty,
                rpm_max: scalars.rpm_max,
                curve_kind,
                warnings,
                was_rpm_only: false,
                kick_boost_override: None,
                kick_duration_override_ms: None,
                walk_after_kick_override: None,
                timestamp: Local::now(),
            }
        }
        None => Calibration {
            up_curve,
            down_curve,
            kick_duration_ms,
            min_start_duty: 100,
            min_sustain_duty: 100,
            min_stable_duty: 100,
            max_eff_duty: 100,
            rpm_max: 0,
            curve_kind: CurveKind::Stepped,
            warnings: vec![CalibrationWarning::NoTachometer],
            was_rpm_only: false,
            kick_boost_override: None,
            kick_duration_override_ms: None,
            walk_after_kick_override: None,
            timestamp: Local::now(),
        },
    }
}

/// True-duty used to compute the dispatcher's worst-case kick. At
/// low true-duty with boost on, `mapped.kick` lands at the boost
/// target (largest kick the dispatcher will write); above the
/// boost crossover the natural lookup takes over and produces an
/// even higher duty that spins up faster.
const KICK_MEASUREMENT_TRUE_DUTY: Duty = 1;

/// Measure how long the dispatcher must hold the actual kick duty
/// (as `true_to_device_smooth` would compute it for the worst-case
/// low true-duty) before the fan settles into a stable RPM band. The
/// dispatcher then drops to the calibrated sustain. Returns a value
/// bucketed to the device's poll-period so identical-model fans land
/// on the same calibrated `kick_duration` regardless of cache-tick
/// alignment.
///
/// Steps:
/// 1. Build a provisional `Calibration` from the sweep data and
///    force `kick_boost_override = Some(true)` so the test mirrors
///    the worst-case (largest) kick the dispatcher will write.
/// 2. Compute the dispatcher's `mapped.kick` for `KICK_MEASUREMENT_TRUE_DUTY`.
/// 3. Stop the fan, sleep, then write the computed kick duty.
/// 4. Watch the status cache until the RPM both crosses `start_rpm_min`
///    AND stabilizes within the diagnoser's relaxed tolerance window
///    (the mid-sweep non-extreme value). The kick measurement only
///    needs to know the fan is past its spin-up transient, so we do
///    not pay the tight-extremes cost here; firmware-kick fans dither
///    20-30 RPM at the kick duty and would otherwise inflate the
///    measured time-to-stable for no dispatch benefit.
/// 5. Bucket the elapsed time up to the next multiple of the device
///    poll period, plus one extra period of safety, clamped to
///    `[poll_period, cap_ms]`.
async fn measure_settled_kick_duration<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
    sweep_data: &(Vec<DutySample>, Vec<DutySample>, Vec<bool>),
    scalars: DerivedScalars,
) -> Result<u32, DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    if cancellation.is_cancelled() {
        return Err(DiagnosisFailure::Cancelled);
    }
    let (up, down, down_stable) = sweep_data;
    let mut provisional = build_calibration(
        up.clone(),
        down.clone(),
        down_stable,
        Some(scalars),
        settings.kick_duration_default_ms,
    );
    provisional.kick_boost_override = Some(true);
    let Some(mapped) = provisional.true_to_device(KICK_MEASUREMENT_TRUE_DUTY) else {
        return Ok(settings.kick_duration_default_ms);
    };

    // Cold-start from rest, mirroring the dispatcher's Off->Kicking entry.
    host.write_raw_duty(device_uid, channel_name, 0)
        .await
        .map_err(|err| DiagnosisFailure::WriteFailed(err.to_string()))?;
    host.sleep_millis(settings.kick_duration_default_ms).await;
    host.write_raw_duty(device_uid, channel_name, mapped.kick)
        .await
        .map_err(|err| DiagnosisFailure::WriteFailed(err.to_string()))?;

    let cap_ms = host.step_settle_cap_ms(device_uid);
    let poll_period_ms = host.poll_period_ms(device_uid).max(1);
    let window_size = (settings.stability_window as usize).max(1);
    let abs_tolerance_rpm = settings.stability_tolerance_rpm;
    let mut waited_ms: u32 = 0;
    let mut last_seen_ts = host.latest_status_timestamp_ms(device_uid).await;
    let mut window: VecDeque<RPM> = VecDeque::with_capacity(window_size);

    while waited_ms < cap_ms {
        if cancellation.is_cancelled() {
            return Err(DiagnosisFailure::Cancelled);
        }
        let current_ts = host.latest_status_timestamp_ms(device_uid).await;
        if current_ts != last_seen_ts && current_ts.is_some() {
            last_seen_ts = current_ts;
            let rpm = host
                .current_rpm(device_uid, channel_name)
                .await
                .unwrap_or(0);
            // Gate on `start_rpm_min` so "stably at 0 RPM" never
            // satisfies the exit (a broken tach or never-spinning fan
            // must time out at cap_ms instead of returning a tiny
            // duration). Below the threshold we don't push the sample,
            // so the stability window stays free of "fan not yet
            // spinning" noise.
            if rpm >= settings.start_rpm_min
                && push_and_check_stable(
                    &mut window,
                    rpm,
                    window_size,
                    abs_tolerance_rpm,
                    settings.stability_tolerance_percent,
                )
            {
                let ticks = waited_ms.div_ceil(poll_period_ms).max(1);
                let bucketed = ticks.saturating_add(1).saturating_mul(poll_period_ms);
                return Ok(bucketed.clamp(poll_period_ms, cap_ms));
            }
        }
        host.sleep_millis(settings.status_poll_interval_ms).await;
        waited_ms = waited_ms.saturating_add(settings.status_poll_interval_ms);
    }
    // Fan never stabilized within cap_ms: hand the dispatcher the
    // largest plausible kick window so it at least spins up
    // (cap_ms = poll_rate * 16 in production).
    Ok(cap_ms)
}

/// Up-sweep then down-sweep, returning both curves and the down-sweep
/// stability flags. Mid-sweep failures short-circuit; the caller still
/// restores the snapshot. Down-sweep is skipped when the up-curve
/// never crossed `start_rpm_min` (the passthrough + `NoTachometer` path).
async fn perform_sweep<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
) -> Result<(Vec<DutySample>, Vec<DutySample>, Vec<bool>), DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    let up_curve = perform_up_sweep(host, settings, device_uid, channel_name, cancellation).await?;
    let kick_in_duty = up_curve
        .iter()
        .find(|s| s.rpm >= settings.start_rpm_min)
        .map(|s| s.duty);
    let (down_curve, down_stable) = match kick_in_duty {
        Some(duty) => {
            perform_down_sweep(host, settings, device_uid, channel_name, cancellation, duty).await?
        }
        None => (Vec::new(), Vec::new()),
    };
    Ok((up_curve, down_curve, down_stable))
}

/// Up-sweep: dense (2%) steps below `UP_SWEEP_DENSE_RANGE_END_DUTY`,
/// sparse (5%) above, returning a sample list ascending from duty 0
/// to 100. The first non-zero step pauses for `kick_duration_default_ms`
/// so firmware kicks decay before sampling. Aborts early once duty
/// reaches `UNRESPONSIVE_ABORT_DUTY` without any sample crossing
/// `start_rpm_min`: the partial list signals the abort to
/// `perform_sweep` which then skips the down-sweep.
async fn perform_up_sweep<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
) -> Result<Vec<DutySample>, DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    let mut samples = Vec::with_capacity(64);
    let mut duty: Duty = 0;
    let mut kicked_in = false;
    loop {
        assert!(
            samples.len() < MAX_SAMPLES_PER_CURVE,
            "up-sweep sample bound"
        );
        let in_dense = duty < UP_SWEEP_DENSE_RANGE_END_DUTY;
        // First non-zero step on a firmware-kick fan: the kick fires
        // when duty leaves 0%, spiking RPM well above the steady-state.
        // Sleep post-write inside sweep_step so the kick has time to
        // decay before settle_and_sample's stability window starts.
        let extra_post_write_settle_ms = if duty == DUTY_STEP_DENSE {
            settings.kick_decay_settle_ms
        } else {
            0
        };
        let (rpm, _was_stable) = sweep_step(
            host,
            settings,
            device_uid,
            channel_name,
            cancellation,
            duty,
            DiagnosisPhase::UpSweep,
            in_dense,
            extra_post_write_settle_ms,
        )
        .await?;
        samples.push(DutySample { duty, rpm });
        if duty == 100 {
            break;
        }
        if kicked_in.not() && rpm >= settings.start_rpm_min {
            kicked_in = true;
        }
        if kicked_in.not() && duty >= UNRESPONSIVE_ABORT_DUTY {
            return Ok(samples);
        }
        let step = if in_dense {
            DUTY_STEP_DENSE
        } else {
            DUTY_STEP_SPARSE
        };
        duty = duty.saturating_add(step).min(100);
    }
    Ok(samples)
}

/// Down-sweep: sparse 5% steps from 100 to `kick_in_duty +
/// KICK_ZONE_BUFFER_PERCENT`, dense 2% steps below. Samples and parallel
/// stability flags both returned ascending. Cap-exhausted (unstable)
/// samples mark firmware-kick oscillation; the sustain floor must sit
/// above them (see `derive_min_stable_duty`).
async fn perform_down_sweep<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
    kick_in_duty: Duty,
) -> Result<(Vec<DutySample>, Vec<bool>), DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    let mut sample_pairs: Vec<(DutySample, bool)> = Vec::with_capacity(64);
    let zone_top = kick_in_duty
        .saturating_add(KICK_ZONE_BUFFER_PERCENT)
        .min(100);
    let mut duty: Duty = 100;
    loop {
        assert!(
            sample_pairs.len() < MAX_SAMPLES_PER_CURVE,
            "down-sweep sample bound"
        );
        let in_dense = duty <= zone_top;
        let (rpm, was_stable) = sweep_step(
            host,
            settings,
            device_uid,
            channel_name,
            cancellation,
            duty,
            DiagnosisPhase::DownSweep,
            in_dense,
            0,
        )
        .await?;
        sample_pairs.push((DutySample { duty, rpm }, was_stable));
        if duty == 0 {
            break;
        }
        let step = if duty <= zone_top {
            DUTY_STEP_DENSE
        } else {
            DUTY_STEP_SPARSE
        };
        duty = duty.saturating_sub(step);
    }
    sample_pairs.sort_by_key(|(s, _)| s.duty);
    Ok(sample_pairs.into_iter().unzip())
}

/// One step of the sweep: write `duty`, adaptively settle, return the
/// stabilized RPM. `extra_post_write_settle_ms` adds a wait after the
/// write, before settle starts, so firmware-kick transients can decay
/// without polluting the stability window.
async fn sweep_step<H>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
    duty: Duty,
    phase: DiagnosisPhase,
    in_dense: bool,
    extra_post_write_settle_ms: u32,
) -> Result<(RPM, bool), DiagnosisFailure>
where
    H: DiagnosisHost + ?Sized,
{
    if cancellation.is_cancelled() {
        return Err(DiagnosisFailure::Cancelled);
    }
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
    if extra_post_write_settle_ms > 0 {
        host.sleep_millis(extra_post_write_settle_ms).await;
    }

    // Tight tolerance for any 2%-step (dense) region: ramp-up from
    // rest and kick-in zone on the down-sweep both need more careful
    // stabilization than the middle of the range. Also pin the
    // saturation tail (duty >= saturation_extreme_duty_min) to the
    // tight tolerance, matching the prior fixed-index behavior.
    let is_extreme = in_dense || duty >= settings.saturation_extreme_duty_min;
    let abs_tolerance_rpm = if is_extreme {
        settings.stability_tolerance_rpm_extremes
    } else {
        settings.stability_tolerance_rpm
    };

    let (rpm, was_stable) = match settle_and_sample(
        host,
        settings,
        device_uid,
        channel_name,
        cancellation,
        pre_write_rpm,
        abs_tolerance_rpm,
    )
    .await
    {
        Ok(pair) => pair,
        Err(failure @ DiagnosisFailure::TempAbortedAt { .. }) => {
            let _ = host.write_raw_duty(device_uid, channel_name, 0).await;
            return Err(failure);
        }
        Err(err) => return Err(err),
    };

    host.emit_progress(DiagnosisProgress {
        device_uid: device_uid.clone(),
        channel_name: channel_name.to_string(),
        phase,
        percent: progress_percent(phase, duty),
        current_duty: Some(duty),
        current_rpm: Some(rpm),
    });
    Ok((rpm, was_stable))
}

/// Adaptive per-step settle. Sleeps `min_settle_ms`, then samples on
/// each status-refresh tick and declares stable when the window
/// agrees within tolerance. Caps at `step_settle_cap_ms`; on cap,
/// returns the median of whatever the window holds.
async fn settle_and_sample<H: DiagnosisHost + ?Sized>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    cancellation: &CancellationToken,
    pre_write_rpm: RPM,
    abs_tolerance_rpm: RPM,
) -> Result<(RPM, bool), DiagnosisFailure> {
    host.sleep_millis(settings.min_settle_ms).await;
    let cap_ms = host.step_settle_cap_ms(device_uid);
    let window_size = (settings.stability_window as usize).max(1);
    // Cache-acceptance cap for stuck sensor / stopped-fan / non-controllable.
    let fresh_read_cap_ms = u32::try_from(
        u64::from(cap_ms).saturating_mul(u64::from(settings.fresh_read_cap_percent.min(100))) / 100,
    )
    .unwrap_or(cap_ms);

    let mut waited_ms: u32 = settings.min_settle_ms;
    let mut last_seen_ts = host.latest_status_timestamp_ms(device_uid).await;
    let mut saw_rpm_change = false;
    let mut window: VecDeque<RPM> = VecDeque::with_capacity(window_size);

    loop {
        wait_for_fresh_status(
            host,
            settings,
            device_uid,
            cancellation,
            &mut last_seen_ts,
            &mut waited_ms,
            cap_ms,
        )
        .await?;
        let rpm = sample_with_temp_check(
            host,
            settings,
            device_uid,
            channel_name,
            pre_write_rpm,
            &mut saw_rpm_change,
        )
        .await?;

        // Fresh post-write read proven by an RPM change OR fresh-read cap
        // elapsing (assume the stuck reading is real).
        if saw_rpm_change || waited_ms >= fresh_read_cap_ms {
            if push_and_check_stable(
                &mut window,
                rpm,
                window_size,
                abs_tolerance_rpm,
                settings.stability_tolerance_percent,
            ) {
                return Ok((median_of(&window), true));
            }
        }
        if waited_ms >= cap_ms {
            // Cap exhausted. `was_stable = false` lifts the sustain
            // floor in `derive_min_stable_duty`.
            if window.is_empty() {
                window.push_back(rpm);
            }
            return Ok((median_of(&window), false));
        }
    }
}

async fn sample_with_temp_check<H: DiagnosisHost + ?Sized>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    channel_name: &str,
    pre_write_rpm: RPM,
    saw_rpm_change: &mut bool,
) -> Result<RPM, DiagnosisFailure> {
    let hottest = host.hottest_temp().await;
    if hottest.celsius >= settings.abort_temp_max_c {
        return Err(DiagnosisFailure::TempAbortedAt {
            observed: hottest.celsius,
            limit: settings.abort_temp_max_c,
            sensor: hottest.sensor,
        });
    }
    let rpm = host
        .current_rpm(device_uid, channel_name)
        .await
        .unwrap_or(0);
    if rpm != pre_write_rpm {
        *saw_rpm_change = true;
    }
    Ok(rpm)
}

fn push_and_check_stable(
    window: &mut VecDeque<RPM>,
    rpm: RPM,
    window_size: usize,
    abs_tolerance_rpm: RPM,
    pct_tolerance: u8,
) -> bool {
    if window.len() == window_size {
        window.pop_front();
    }
    window.push_back(rpm);
    window.len() == window_size && is_stable(window, abs_tolerance_rpm, pct_tolerance)
}

/// Wait until the host's status timestamp advances or `cap_ms` elapses.
/// Cancellation surfaces immediately as `Cancelled`.
async fn wait_for_fresh_status<H: DiagnosisHost + ?Sized>(
    host: &H,
    settings: &DiagnosisSettings,
    device_uid: &UID,
    cancellation: &CancellationToken,
    last_seen_ts: &mut Option<i64>,
    waited_ms: &mut u32,
    cap_ms: u32,
) -> Result<(), DiagnosisFailure> {
    loop {
        if cancellation.is_cancelled() {
            return Err(DiagnosisFailure::Cancelled);
        }
        if *waited_ms >= cap_ms {
            return Ok(());
        }
        let current = host.latest_status_timestamp_ms(device_uid).await;
        if current != *last_seen_ts && current.is_some() {
            *last_seen_ts = current;
            return Ok(());
        }
        host.sleep_millis(settings.status_poll_interval_ms).await;
        *waited_ms = waited_ms.saturating_add(settings.status_poll_interval_ms);
    }
}

/// Window agreement within `max(abs_tolerance, pct_tolerance%)`.
fn is_stable(window: &VecDeque<RPM>, abs_tolerance_rpm: RPM, pct_tolerance: u8) -> bool {
    let Some(&max) = window.iter().max() else {
        return false;
    };
    let Some(&min) = window.iter().min() else {
        return false;
    };
    let spread = max.saturating_sub(min);
    let pct = (u64::from(max) * u64::from(pct_tolerance) / 100)
        .try_into()
        .unwrap_or(RPM::MAX);
    let tolerance = abs_tolerance_rpm.max(pct);
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

/// Map a sweep step to a 0..=100 percent based on the current duty.
/// Duty progress is the only thing that monotonically advances across
/// a variable-resolution sweep (sample count is data-dependent), so we
/// use it as the progress signal. Up-sweep climbs duty 0 -> 100 (mapped
/// to percent 0 -> 50); down-sweep walks duty 100 -> 0 (mapped to
/// percent 50 -> 100).
fn progress_percent(phase: DiagnosisPhase, duty: Duty) -> u8 {
    let duty_u32 = u32::from(duty);
    let percent = match phase {
        DiagnosisPhase::UpSweep => duty_u32 / 2,
        DiagnosisPhase::DownSweep => 50 + (100 - duty_u32) / 2,
        DiagnosisPhase::Preflight => 0,
        DiagnosisPhase::Finalizing => 100,
    };
    u8::try_from(percent.min(100)).unwrap_or(100)
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
        // Auto-advances on each sleep_millis so the settle loop sees a
        // fresh cache refresh per tick without explicit timestamp wiring.
        status_timestamp_ms: Cell<i64>,
        // Sleeps are instant in tests; the cap only gates inner loop count.
        step_cap_ms: Cell<u32>,
        // Returns `stale_rpm` for the next N reads, simulating a cache
        // that has not yet refreshed after a duty write.
        stale_reads_remaining: Cell<usize>,
        stale_rpm: Cell<RPM>,
        // Records each enter_manual_control call and the step_counter at
        // call time, so tests can assert manual control was entered
        // before any duty write. `fail_manual_control` forces the call
        // to error, exercising the abort-and-restore path.
        manual_control_calls: RefCell<Vec<(String, String)>>,
        step_at_manual_control: Cell<Option<usize>>,
        fail_manual_control: Cell<bool>,
    }

    impl MockHost {
        fn new() -> Self {
            // step_cap_ms sized to ride out 5+ stale reads at the new
            // longer min_settle_ms. `fresh_read_cap` is 50% of cap, so
            // 2500 ms keeps the stale-fallback from firing prematurely.
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
                step_cap_ms: Cell::new(2500),
                stale_reads_remaining: Cell::new(0),
                stale_rpm: Cell::new(0),
                manual_control_calls: RefCell::new(Vec::new()),
                step_at_manual_control: Cell::new(None),
                fail_manual_control: Cell::new(false),
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

        /// Configure the host to map device-duty -> RPM linearly: RPM
        /// = 20 * duty, so duty 0 -> 0 RPM and duty 100 -> 2000 RPM.
        /// Populates every duty in 0..=100 since the new sweep visits
        /// 2%-step values too.
        fn with_smooth_fan(self) -> Self {
            for duty in 0u8..=100 {
                self.rpm_for_duty
                    .borrow_mut()
                    .insert(duty, 20 * u32::from(duty));
            }
            self
        }

        /// Configure the host as a stepped fan with three RPM
        /// plateaus by duty percentage.
        fn with_stepped_fan(self) -> Self {
            for duty in 0u8..=100 {
                let rpm = if duty < 25 {
                    0
                } else if duty < 65 {
                    1000
                } else {
                    2000
                };
                self.rpm_for_duty.borrow_mut().insert(duty, rpm);
            }
            self
        }

        /// Configure the host as an unresponsive fan (RPM=0 at every
        /// duty). The diagnoser persists a passthrough calibration
        /// carrying `NoTachometer` in the warnings.
        fn with_dead_fan(self) -> Self {
            for duty in 0u8..=100 {
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

        async fn latest_status_timestamp_ms(&self, _device_uid: &UID) -> Option<i64> {
            Some(self.status_timestamp_ms.get())
        }

        fn step_settle_cap_ms(&self, _device_uid: &UID) -> u32 {
            self.step_cap_ms.get()
        }

        async fn enter_manual_control(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
            self.step_at_manual_control
                .set(Some(self.step_counter.get()));
            self.manual_control_calls
                .borrow_mut()
                .push((device_uid.clone(), channel_name.to_string()));
            if self.fail_manual_control.get() {
                return Err(anyhow!("simulated manual-control failure"));
            }
            Ok(())
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

        async fn hottest_temp(&self) -> HottestTemp {
            let celsius = match self.temp_after_step.get() {
                Some((step, override_temp)) if self.step_counter.get() >= step => override_temp,
                _ => self.temp.get(),
            };
            HottestTemp {
                celsius,
                sensor: "mock-device | mock-temp".to_string(),
            }
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

        fn emit_progress(&self, progress: DiagnosisProgress) {
            self.progress_events.borrow_mut().push(progress);
        }
    }

    fn key(dev: &str, chan: &str) -> ChannelKey {
        (dev.to_string(), chan.to_string())
    }

    #[test]
    fn happy_path_smooth_fan_produces_smooth_calibration() {
        crate::rt::test_runtime(async {
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
            // Manual control must be entered exactly once, before any duty
            // write (step_counter still 0). Boards left in automatic mode
            // otherwise reject the sweep's raw writes.
            assert_eq!(host.manual_control_calls.borrow().len(), 1);
            assert_eq!(host.step_at_manual_control.get(), Some(0));
        });
    }

    #[test]
    fn manual_control_failure_aborts_before_sweep_and_restores() {
        crate::rt::test_runtime(async {
            // Goal: when entering manual control fails (a driver rejects
            // pwm_enable=1), the diagnosis aborts before writing any duty,
            // restores the snapshot, clears under_diagnosis, and persists
            // no calibration. The failure surfaces as WriteFailed.
            let state = FanStateMap::new();
            let store = CalibrationStore::empty();
            let host = MockHost::new().with_smooth_fan();
            host.fail_manual_control.set(true);
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
            .expect_err("manual-control failure aborts diagnosis");

            assert!(matches!(err, DiagnosisFailure::WriteFailed(_)));
            assert!(
                host.duty_writes.borrow().is_empty(),
                "no duty may be written when manual control could not be entered"
            );
            assert!(store.has(&key("dev-a", "fan1")).not());
            assert_eq!(host.restores_applied.borrow().len(), 1);
            assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
        });
    }

    #[test]
    fn stepped_fan_produces_stepped_calibration() {
        crate::rt::test_runtime(async {
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
        });
    }

    #[test]
    fn preflight_temp_too_high_short_circuits() {
        crate::rt::test_runtime(async {
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

            let DiagnosisFailure::PreflightTempTooHigh {
                observed, sensor, ..
            } = &err
            else {
                panic!("expected PreflightTempTooHigh, got {err:?}");
            };
            assert!((observed - 80.0).abs() < f64::EPSILON);
            // The offending sensor identity must reach the failure so the
            // notification and UI can name it, not just the number.
            assert_eq!(sensor, "mock-device | mock-temp");
            assert!(host.duty_writes.borrow().is_empty());
            assert!(host.snapshots_taken.borrow().is_empty());
            assert!(store.has(&key("dev-a", "fan1")).not());
        });
    }

    #[test]
    fn bios_controlled_fan_persists_with_not_controllable_warning() {
        crate::rt::test_runtime(async {
            // Goal: a fan stuck at a constant non-zero RPM regardless of
            // duty (BIOS / firmware override) must produce a saved
            // calibration with NotControllable in warnings and curve_kind
            // forced to Stepped.
            use crate::calibration::CalibrationWarning;
            let state = FanStateMap::new();
            let store = CalibrationStore::empty();
            let host = MockHost::new();
            // Every duty maps to a constant 1200 RPM, simulating a fan
            // that spins but does not respond to PWM changes.
            for duty in 0u8..=100 {
                host.rpm_for_duty.borrow_mut().insert(duty, 1200);
            }
            let settings = DiagnosisSettings::default();
            let cancellation = CancellationToken::new();

            let calibration = run_diagnosis(
                &state,
                &store,
                &host,
                &settings,
                "dev-a".to_string(),
                "fan1".to_string(),
                cancellation,
            )
            .await
            .expect("BIOS-stuck fan persists with warning, not error");

            assert!(
                calibration
                    .warnings
                    .contains(&CalibrationWarning::NotControllable),
                "expected NotControllable, got {:?}",
                calibration.warnings
            );
            assert_eq!(
                calibration.curve_kind,
                CurveKind::Stepped,
                "NotControllable must force passthrough"
            );
        });
    }

    #[test]
    fn dead_fan_persists_no_tachometer_warning() {
        crate::rt::test_runtime(async {
            // Goal: a fan whose RPM never crosses the start floor must
            // persist a passthrough calibration carrying NoTachometer so
            // the popover shows the warning on reopen and the user can
            // re-calibrate or clear from there.
            use crate::calibration::CalibrationWarning;
            let state = FanStateMap::new();
            let store = CalibrationStore::empty();
            let host = MockHost::new().with_dead_fan();
            let settings = DiagnosisSettings::default();
            let cancellation = CancellationToken::new();

            let calibration = run_diagnosis(
                &state,
                &store,
                &host,
                &settings,
                "dev-a".to_string(),
                "fan1".to_string(),
                cancellation,
            )
            .await
            .expect("dead fan persists with warning, not error");

            assert!(
                calibration
                    .warnings
                    .contains(&CalibrationWarning::NoTachometer),
                "expected NoTachometer warning, got {:?}",
                calibration.warnings
            );
            assert_eq!(calibration.curve_kind, CurveKind::Stepped);
            assert!(store.has(&key("dev-a", "fan1")));
            assert_eq!(host.restores_applied.borrow().len(), 1);
            assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
        });
    }

    #[test]
    fn dead_fan_aborts_at_unresponsive_threshold_and_skips_down_sweep() {
        crate::rt::test_runtime(async {
            // Goal: a fan with no RPM response must abort the up-sweep at
            // UNRESPONSIVE_ABORT_DUTY (50%) and skip the down-sweep
            // entirely. The persisted Calibration is still the passthrough
            // NoTachometer record (same end-state as a full dead-fan sweep)
            // but produced in ~half the up-sweep steps and zero down-sweep
            // steps. Asserts the step-count bound to lock the speed gain
            // against regression.
            use crate::calibration::curve::UNRESPONSIVE_ABORT_DUTY;
            use crate::calibration::CalibrationWarning;
            let state = FanStateMap::new();
            let store = CalibrationStore::empty();
            let host = MockHost::new().with_dead_fan();
            let settings = DiagnosisSettings::default();
            let cancellation = CancellationToken::new();

            let calibration = run_diagnosis(
                &state,
                &store,
                &host,
                &settings,
                "dev-a".to_string(),
                "fan1".to_string(),
                cancellation,
            )
            .await
            .expect("dead fan still persists passthrough on early abort");

            // Up-sweep stopped at UNRESPONSIVE_ABORT_DUTY (no higher duty
            // was written by the diagnoser).
            let max_up_duty = calibration
                .up_curve
                .iter()
                .map(|s| s.duty)
                .max()
                .expect("up-curve has at least one sample");
            assert_eq!(max_up_duty, UNRESPONSIVE_ABORT_DUTY);

            // Down-sweep was skipped entirely.
            assert!(calibration.down_curve.is_empty());

            // End-state matches the existing dead-fan passthrough.
            assert!(
                calibration
                    .warnings
                    .contains(&CalibrationWarning::NoTachometer),
                "expected NoTachometer warning, got {:?}",
                calibration.warnings
            );
            assert_eq!(calibration.curve_kind, CurveKind::Stepped);
            assert_eq!(calibration.rpm_max, 0);

            // The total duty writes are bounded: 26 up-sweep steps (0, 2,
            // ..., 50) plus the restore. Pre-change behaviour was ~51 up
            // + ~51 down = ~102 writes. Bound at 32 to detect regression
            // while leaving room for restore-path writes.
            assert!(
                host.step_counter.get() <= 32,
                "early abort should bound step_counter, got {}",
                host.step_counter.get()
            );

            assert!(store.has(&key("dev-a", "fan1")));
            assert_eq!(host.restores_applied.borrow().len(), 1);
            assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
        });
    }

    #[test]
    fn temp_abort_mid_sweep_zeros_channel_and_restores_snapshot() {
        crate::rt::test_runtime(async {
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

            let DiagnosisFailure::TempAbortedAt {
                observed, sensor, ..
            } = &err
            else {
                panic!("expected TempAbortedAt, got {err:?}");
            };
            assert!((observed - 90.0).abs() < f64::EPSILON);
            assert_eq!(sensor, "mock-device | mock-temp");
            let writes = host.duty_writes.borrow();
            let last_write = *writes.last().expect("at least one write");
            assert_eq!(last_write, 0, "safety write of 0% must be last");
            assert!(store.has(&key("dev-a", "fan1")).not());
            assert_eq!(host.restores_applied.borrow().len(), 1);
            assert!(state.is_under_diagnosis(&key("dev-a", "fan1")).not());
        });
    }

    #[test]
    fn cancellation_short_circuits_and_restores_snapshot() {
        crate::rt::test_runtime(async {
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
        });
    }

    #[test]
    fn write_failure_during_sweep_surfaces_as_write_failed() {
        crate::rt::test_runtime(async {
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
        });
    }

    #[test]
    fn restore_failure_surfaces_after_persistence() {
        crate::rt::test_runtime(async {
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
        });
    }

    #[test]
    fn under_diagnosis_flag_set_during_sweep() {
        crate::rt::test_runtime(async {
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
                async fn enter_manual_control(&self, d: &UID, c: &str) -> Result<()> {
                    self.inner.enter_manual_control(d, c).await
                }
                async fn write_raw_duty(&self, d: &UID, c: &str, duty: Duty) -> Result<()> {
                    self.state_seen_during_writes
                        .borrow_mut()
                        .push(self.state.is_under_diagnosis(&self.channel_key));
                    self.inner.write_raw_duty(d, c, duty).await
                }
                async fn hottest_temp(&self) -> HottestTemp {
                    self.inner.hottest_temp().await
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
        });
    }

    #[test]
    fn progress_events_cover_preflight_sweep_and_finalize() {
        crate::rt::test_runtime(async {
            // Goal: a successful diagnosis emits at least one preflight
            // event, one or more per-sweep events with monotonically
            // non-decreasing percent for the up-sweep half, and a final
            // finalizing event at 100%. This is what SSE clients consume
            // to render the progress bar.
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
            // The sweep is variable-resolution now, so we don't pin the
            // exact count. We just sanity-check that the up- and down-
            // sweeps each emit at least a dozen samples (dense + sparse
            // combined gives ~25-30 per direction) and that they are
            // similar in magnitude (within 2x).
            assert!(
                up_event_count >= 12,
                "expected at least 12 up-sweep events, got {up_event_count}"
            );
            assert!(
                down_event_count >= 12,
                "expected at least 12 down-sweep events, got {down_event_count}"
            );

            // The progress bar must rise monotonically across the entire
            // sweep, including the down-sweep half. Earlier the down-sweep
            // used `idx + 1` for its step counter and so counted DOWN from
            // 100% to ~52% (because the down-sweep iterates in reverse).
            // This walks every event in order and asserts non-decreasing
            // percent, which catches that class of regression.
            let mut last_percent: u8 = 0;
            for event in events.iter() {
                assert!(
                    event.percent >= last_percent,
                    "progress percent must not decrease: {} then {} (phase {:?})",
                    last_percent,
                    event.percent,
                    event.phase
                );
                last_percent = event.percent;
            }
            // The up-sweep half tops out at ~50%; the down-sweep then must
            // climb the remaining half. Pin the boundary so the bar always
            // reaches the second half during the down-sweep.
            let first_down = events
                .iter()
                .find(|e| e.phase == DiagnosisPhase::DownSweep)
                .expect("at least one down-sweep event");
            let last_down = events
                .iter()
                .rev()
                .find(|e| e.phase == DiagnosisPhase::DownSweep)
                .expect("at least one down-sweep event");
            assert!(
                first_down.percent >= 50,
                "down-sweep should start at or above 50%, got {}",
                first_down.percent
            );
            assert_eq!(
                last_down.percent, 100,
                "last down-sweep step should reach 100%"
            );
        });
    }

    #[test]
    fn pre_diagnosis_kicking_state_is_cleared_at_sweep_start() {
        crate::rt::test_runtime(async {
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
                    commanded_true_duty: None,
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
        });
    }

    #[test]
    fn stale_cache_does_not_lock_in_pre_write_value() {
        crate::rt::test_runtime(async {
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

            // The synthetic smooth fan returns rpm = 20 * duty. With
            // with_stale_reads(5, 800) the first 5 current_rpm calls
            // return 800. The early samples (low duty, real rpm < 200)
            // must therefore NOT contain a stale 800; settle must have
            // waited for the cache to refresh to the post-write value.
            let first = result
                .up_curve
                .iter()
                .find(|s| s.duty == 0)
                .expect("first sample at duty 0");
            assert_eq!(
            first.rpm, 0,
            "stale 800 must not be recorded at duty 0; settle should wait for the post-write change to 0"
        );
            let max_early_rpm = result
                .up_curve
                .iter()
                .filter(|s| s.duty <= 10)
                .map(|s| s.rpm)
                .max()
                .expect("at least one low-duty sample");
            assert!(
            max_early_rpm < 300,
            "no low-duty sample should carry the stale 800 RPM; max in duty<=10 was {max_early_rpm}"
        );
        });
    }

    #[test]
    fn is_stable_returns_true_when_window_fits_absolute_tolerance() {
        // Goal: when the spread across the window is at or below the
        // absolute RPM tolerance, the step is considered settled even
        // for very low RPMs where the percent tolerance would round to
        // zero.
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(100);
        window.push_back(125);
        window.push_back(115);
        // spread = 25, default abs tolerance = 30 -> stable.
        assert!(is_stable(&window, 30, 3));
    }

    #[test]
    fn is_stable_extreme_tolerance_is_stricter() {
        // Goal: at the sweep extremes the caller passes the tighter
        // 15-RPM floor; a 25-RPM spread that would pass under the
        // 30-RPM general tolerance must fail under the extreme one
        // so the diagnoser keeps waiting for the truly settled value.
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(100);
        window.push_back(125);
        window.push_back(115);
        assert!(is_stable(&window, 15, 3).not());
    }

    #[test]
    fn up_sweep_uses_dense_steps_before_kick_in() {
        crate::rt::test_runtime(async {
            // Goal: the up-sweep walks 2% steps below UP_SWEEP_DENSE_RANGE_END_DUTY
            // (30%) and 5% above, regardless of where the fan kicks in.
            // 2%-aligned duties must appear below 30%, 5%-aligned beyond.
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
            .expect("smooth fan diagnoses");

            let duties: Vec<Duty> = result.up_curve.iter().map(|s| s.duty).collect();
            // Every 2%-aligned duty 0..=28 must be present (dense range).
            for expected in (0u8..=28).step_by(2) {
                assert!(
                    duties.contains(&expected),
                    "dense duty {expected} missing: {duties:?}"
                );
            }
            // Above the dense range, the walker steps by 5.
            let post_dense: Vec<Duty> = duties.iter().copied().filter(|&d| d >= 30).collect();
            let consecutive_5_gap = post_dense.windows(2).any(|w| w[1] - w[0] == 5);
            assert!(
                consecutive_5_gap,
                "post-dense samples must include a 5% step: {post_dense:?}"
            );
            assert_eq!(
                result.up_curve[result.up_curve.len() - 1].duty,
                100,
                "up_curve must end at duty 100"
            );
            // A non-2%-aligned, non-5%-aligned mid-range duty must NOT appear.
            assert!(
                duties.contains(&47).not(),
                "duty 47 should not be sampled: {duties:?}"
            );
        });
    }

    #[test]
    fn up_sweep_stays_dense_through_firmware_kick_artifact() {
        crate::rt::test_runtime(async {
            // Goal: a firmware-kick fan that briefly spikes RPM at the first
            // non-zero duty (e.g. 850 RPM at 2% then settles to 300 RPM at
            // 4-28%) must still get dense sampling through the entire 0-30%
            // range. Old code would see the 850 RPM "kick-in", flip to sparse,
            // and miss the floor. New code is dense-by-duty so the firmware
            // kick decay is captured cleanly.
            let state = FanStateMap::new();
            let store = CalibrationStore::empty();
            let host = MockHost::new();
            {
                let mut map = host.rpm_for_duty.borrow_mut();
                for duty in 0u8..=100 {
                    let rpm = match duty {
                        0 => 0,
                        2 => 850,
                        d if d <= 30 => 300,
                        d => 300 + u32::from(d - 30) * 1700 / 70,
                    };
                    map.insert(duty, rpm);
                }
            }
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
            .expect("firmware-kick fan diagnoses");

            let duties: Vec<Duty> = result.up_curve.iter().map(|s| s.duty).collect();
            // Every dense duty from 0 to 28 must be captured.
            for expected in (0u8..=28).step_by(2) {
                assert!(
                    duties.contains(&expected),
                    "firmware-kick fan missed dense duty {expected}: {duties:?}"
                );
            }
            // The 850 RPM spike at duty 2 is preserved verbatim.
            let sample_at_2 = result
                .up_curve
                .iter()
                .find(|s| s.duty == 2)
                .expect("duty 2 sample present");
            assert_eq!(
                sample_at_2.rpm, 850,
                "spike at duty 2 must be preserved as-is"
            );
        });
    }

    #[test]
    fn kick_duration_is_measured_and_bounded() {
        crate::rt::test_runtime(async {
            // The saved kick_duration_ms must be the result of the live
            // measurement, not just the default. Asserts the value is
            // within [poll_period, cap_ms], is a multiple of the poll
            // period (bucketing eliminates cache-alignment jitter so
            // identical-model fans land on the same calibrated value),
            // and is at least one poll period (always one period of
            // safety on top of the measured time-to-stable).
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
            .expect("smooth fan diagnoses");

            let cap_ms = host.step_cap_ms.get();
            let poll_period = host.poll_period_ms(&"dev-a".to_string());
            assert!(
            result.kick_duration_ms >= poll_period,
            "kick_duration must be at least one poll period, got {} (poll_period={poll_period})",
            result.kick_duration_ms
        );
            assert_eq!(
            result.kick_duration_ms % poll_period,
            0,
            "kick_duration must be a multiple of the poll period for cross-fan consistency, got {} (poll_period={poll_period})",
            result.kick_duration_ms
        );
            assert!(
                result.kick_duration_ms <= cap_ms,
                "kick_duration must not exceed cap_ms ({cap_ms}), got {}",
                result.kick_duration_ms
            );
        });
    }

    #[test]
    fn kick_duration_falls_back_to_default_on_dead_fan() {
        crate::rt::test_runtime(async {
            // Dead fan has no derivable min_start_duty, so kick_duration
            // skips measurement and uses the configured default.
            let state = FanStateMap::new();
            let store = CalibrationStore::empty();
            let host = MockHost::new().with_dead_fan();
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
            .expect("dead fan persists passthrough");

            assert_eq!(result.kick_duration_ms, settings.kick_duration_default_ms);
        });
    }

    #[test]
    fn down_sweep_uses_dense_steps_in_kick_in_zone() {
        crate::rt::test_runtime(async {
            // Goal: the down-sweep walks duty 100 -> 0. 5%-steps above the
            // kick-in zone (kick_in_duty + KICK_ZONE_BUFFER_PERCENT), 2%-
            // steps inside the zone and continuing to 0. With kick-in at
            // duty 4 on the up-sweep (smooth synthetic), zone_top = 14;
            // duties below 14 must appear at 2%-step spacing and duties
            // above at 5%-step spacing.
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
            .expect("smooth fan diagnoses");

            let duties: Vec<Duty> = result.down_curve.iter().map(|s| s.duty).collect();
            assert!(duties.contains(&0), "down_curve must include 0");
            assert!(duties.contains(&100), "down_curve must include 100");
            // Inside the dense zone: a 2%-step duty must appear.
            assert!(duties.contains(&8), "down_curve missing duty 8: {duties:?}");
            // Outside the dense zone (above zone_top): a 5%-step duty must appear.
            assert!(
                duties.contains(&50),
                "down_curve missing duty 50: {duties:?}"
            );
        });
    }

    #[test]
    fn is_stable_returns_true_when_window_fits_percent_tolerance() {
        // Goal: at higher RPMs, the percent tolerance dominates and
        // small absolute drift around a fast-spinning fan is treated
        // as stable.
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(2000);
        window.push_back(2020);
        window.push_back(2050);
        // spread = 50, 3% of 2050 = 61, larger of (30, 61) = 61, spread <= 61.
        assert!(is_stable(&window, 30, 3));
    }

    #[test]
    fn is_stable_returns_false_when_window_exceeds_tolerance() {
        // Goal: while the fan is still ramping (samples diverging),
        // stability must return false so the diagnoser keeps waiting.
        let mut window: VecDeque<RPM> = VecDeque::with_capacity(3);
        window.push_back(0);
        window.push_back(400);
        window.push_back(800);
        assert!(is_stable(&window, 30, 3).not());
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

    #[test]
    fn cancellation_during_settle_short_circuits_step() {
        crate::rt::test_runtime(async {
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
                async fn enter_manual_control(&self, d: &UID, c: &str) -> Result<()> {
                    self.inner.enter_manual_control(d, c).await
                }
                async fn write_raw_duty(&self, d: &UID, c: &str, duty: Duty) -> Result<()> {
                    self.inner.write_raw_duty(d, c, duty).await
                }
                async fn hottest_temp(&self) -> HottestTemp {
                    self.inner.hottest_temp().await
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
        });
    }

    use std::rc::Rc;
}
