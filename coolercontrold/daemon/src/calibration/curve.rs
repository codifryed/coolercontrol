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

//! Calibration curve types and the pure RPM<->duty math.
//!
//! A `Calibration` is the persisted result of a successful diagnosis sweep.
//! It carries two RPM samples-vs-duty arrays (one each direction), the
//! kick window, the derived working-range scalars, and a classification of
//! the curve as `Smooth` or `Stepped`.
//!
//! Smooth curves get RPM-normalized true-duty mapping. Stepped curves are
//! returned to the caller via `None` so the dispatch layer passes through.

// Phase 1 staging: math and types here are consumed in later phases
// (dispatch, diagnoser). Until then they are dead in production but
// fully exercised by the test suite at the bottom of this file.
#![allow(dead_code)]

use crate::device::{Duty, RPM};
use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Sparse step size: duty increment used after kick-in on the up-sweep
/// and outside the kick-in zone on the down-sweep.
pub const DUTY_STEP_SPARSE: u8 = 5;

/// Dense step size: duty increment used while ramping up from rest on
/// the up-sweep, and inside the kick-in zone on the down-sweep. The
/// extra resolution captures the steep duty-to-RPM gradient at low
/// duty and produces a more accurate mapping in that region.
pub const DUTY_STEP_DENSE: u8 = 2;

/// Buffer above `min_start_duty` that the down-sweep treats as the
/// kick-in zone (where dense sampling is used). Wide enough to catch
/// hysteresis (fan keeps spinning at duties slightly below where it
/// started).
pub const KICK_ZONE_BUFFER_PERCENT: u8 = 10;

/// Maximum samples we will accept in a single curve. Loose upper
/// bound; the sweep produces ~30 samples per direction with default
/// step sizes. Enforced at deserialization time to guard against
/// pathological persisted data.
pub const MAX_SAMPLES_PER_CURVE: usize = 128;

/// Absolute RPM floor below which we treat readings as noise.
const RPM_START_THRESHOLD_ABSOLUTE: RPM = 50;

/// Fraction of `rpm_max` (in percent) used as a relative noise floor.
const RPM_START_THRESHOLD_FRACTION_PERCENT: u32 = 5;

/// Absolute RPM jitter tolerance used during step-curve classification.
const RPM_JITTER_ABSOLUTE: RPM = 50;

/// Fraction of `rpm_max` (in percent) used as the relative jitter tolerance.
const RPM_JITTER_FRACTION_PERCENT: u32 = 3;

/// Maximum percentage-point disagreement between the device-duty-derived
/// and RPM-derived true-duty before the displayed value falls back to the
/// RPM-derived one. The fallback exists to surface failures (stuck fan,
/// dead fan, broken PWM) that pure device-duty mapping would otherwise
/// hide behind the value the daemon last wrote.
pub const SANITY_THRESHOLD_PERCENT: u8 = 10;

/// A single measured (device-duty, RPM) sample taken during a sweep.
/// Curves are stored as variable-length `Vec<DutySample>` sorted by
/// `duty` ascending, with the dense (2%) sampling concentrated in the
/// kick-in region and the sparse (5%) sampling elsewhere.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DutySample {
    pub duty: Duty,
    pub rpm: RPM,
}

/// The duty/RPM curve and derived working-range scalars for one channel.
///
/// Always represents a successful diagnosis. A fan that never spins is
/// rejected by the diagnoser before a `Calibration` is constructed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Calibration {
    /// Up-sweep RPM samples by device-duty, sorted by duty ascending.
    /// First sample is at duty 0, last is at duty 100. Dense (2%-step)
    /// sampling in the kick-in region, sparse (5%-step) elsewhere.
    pub up_curve: Vec<DutySample>,

    /// Down-sweep RPM samples. Same shape as `up_curve`, with the
    /// dense sampling around the kick-in zone.
    pub down_curve: Vec<DutySample>,

    /// Measured time from writing `min_start_duty` until RPM crossed the
    /// start threshold, with a safety margin already applied. Bounded.
    pub kick_duration_ms: u32,

    /// Lowest device-duty (multiple of `DUTY_STEP_PERCENT`) that reliably
    /// starts the fan from rest.
    pub min_start_duty: Duty,

    /// Lowest device-duty (multiple of `DUTY_STEP_PERCENT`) at which the
    /// fan continues to spin once already in motion.
    pub min_sustain_duty: Duty,

    /// Lowest device-duty at which the fan operates stably (no oscillation).
    ///
    /// On well-behaved fans this equals `min_sustain_duty` — every spinning
    /// duty is also stable. Some controllers apply a kick in firmware that
    /// keeps RPM above an internal floor, producing an audible oscillation
    /// in a band immediately above `min_sustain_duty`; on those fans the
    /// threshold sits above the band and the dispatcher clamps the
    /// post-kick sustain (and the kick target itself) to this value so the
    /// fan never operates inside the oscillation zone.
    ///
    /// Defaults to 0 on older persisted records that pre-date this field;
    /// the dispatcher's `max` clamp then degenerates to a no-op, preserving
    /// pre-existing behaviour until the user recalibrates.
    #[serde(default)]
    pub min_stable_duty: Duty,

    /// Lowest device-duty at which the up-curve reaches within
    /// `jitter` of `rpm_max` — the "near plateau" point where
    /// additional duty produces diminishing RPM gains. Informational
    /// only: the mapping math uses `rpm_max` as the upper bound, so
    /// `true_to_device(100)` may still write a device-duty above
    /// this value when the fan continues to gain RPM past it.
    pub max_eff_duty: Duty,

    /// Peak RPM observed across the sweep. Always positive.
    pub rpm_max: RPM,

    /// Whether the curve is smooth (mapping active) or stepped (passthrough).
    pub curve_kind: CurveKind,

    /// Non-fatal reliability findings: missing tachometer, fan not
    /// responding to duty, limited RPM range. Empty for a healthy
    /// calibration. Defaults to empty on older persisted records that
    /// pre-date this field.
    #[serde(default)]
    pub warnings: Vec<CalibrationWarning>,

    /// Wall-clock time when the diagnosis that produced this calibration finished.
    pub timestamp: DateTime<Local>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CurveKind {
    Smooth,
    Stepped,
}

/// Non-fatal reliability findings recorded on a `Calibration` to be
/// surfaced to the user. The diagnoser still persists the calibration
/// when these fire; the warnings just inform the popover and the
/// completion notification that the fan is not as well-behaved as a
/// fully healthy one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalibrationWarning {
    /// The fan tachometer reported 0 RPM across the entire sweep. The
    /// fan may be disconnected, its tach wire unplugged, or the
    /// firmware may be holding it off.
    NoTachometer,
    /// The fan is spinning but its RPM does not track duty changes
    /// (effective RPM span across the responding portion of the curve
    /// is within noise). Most often a BIOS / firmware-controlled fan.
    /// Mapping is disabled (`curve_kind` is forced to `Stepped`).
    NotControllable,
    /// The fan responds to duty but the usable RPM span is small
    /// enough that the mapping resolution is coarse. Mapping stays
    /// active; the popover surfaces the span so the user can decide
    /// whether to clear.
    LimitedRange { rpm_span: RPM, rpm_max: RPM },
    /// The fan never settled into a stable RPM anywhere above
    /// `min_sustain_duty` — typically a cheap firmware-kicked fan
    /// whose internal floor sits well above its mechanical minimum.
    /// `min_stable_duty` could not be derived; the dispatcher falls
    /// back to `min_sustain_duty` (no clamp). `lower_duty`/`upper_duty`
    /// describe the duty range observed to oscillate so the UI can
    /// surface what the diagnoser saw.
    Oscillating { lower_duty: Duty, upper_duty: Duty },
}

/// Result of a forward true-duty -> device-duty mapping.
///
/// The two values correspond to the up-curve (kick from rest) and the
/// down-curve (sustain once spinning), used by the dispatch state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedDuty {
    pub kick: Duty,
    pub sustain: Duty,
}

impl Calibration {
    /// Map a target true-duty into the (kick, sustain) device duties.
    ///
    /// Returns `None` for stepped channels so the caller passes through.
    pub fn true_to_device(&self, true_duty: Duty) -> Option<MappedDuty> {
        if self.curve_kind == CurveKind::Stepped {
            return None;
        }
        Some(self.true_to_device_smooth(true_duty))
    }

    /// Map a measured RPM back to its true-duty equivalent.
    ///
    /// Returns `None` for stepped channels so the caller passes through.
    pub fn rpm_to_true_duty(&self, rpm: RPM) -> Option<Duty> {
        if self.curve_kind == CurveKind::Stepped {
            return None;
        }
        Some(self.rpm_to_true_duty_smooth(rpm))
    }

    /// Map a cached **device-duty** (the raw PWM percent currently being
    /// driven) back to its true-duty equivalent. Steady-state sustain
    /// is the dominant case, so we interpolate in `down_curve` to
    /// recover the RPM the fan would produce at this device-duty, then
    /// reuse the existing RPM->true-duty math.
    ///
    /// Used by the status pipeline as the **stable** source of the
    /// displayed true-duty (device-duty does not jitter the way RPM
    /// does). Returns `None` for stepped channels (caller passes the
    /// raw device-duty through unchanged).
    pub fn device_to_true_duty(&self, device_duty: Duty) -> Option<Duty> {
        if self.curve_kind == CurveKind::Stepped {
            return None;
        }
        let rpm = rpm_at_device_duty(&self.down_curve, device_duty.min(100));
        Some(self.rpm_to_true_duty_smooth(rpm))
    }

    /// Internal smooth-path forward map. Caller guarantees `Smooth`.
    fn true_to_device_smooth(&self, true_duty: Duty) -> MappedDuty {
        debug_assert_eq!(self.curve_kind, CurveKind::Smooth);
        assert!(self.rpm_max > 0);

        let true_clamped = true_duty.min(100);
        if true_clamped == 0 {
            return MappedDuty {
                kick: 0,
                sustain: 0,
            };
        }
        let rpm_floor = self.rpm_floor();
        assert!(self.rpm_max >= rpm_floor);
        let target_rpm = interpolate_rpm(rpm_floor, self.rpm_max, true_clamped);
        // Kick must clear two floors: `min_start_duty` (lowest duty that
        // reliably spins up from rest) and `min_stable_duty` (lowest duty
        // at which the fan operates without firmware-kick oscillation).
        // On healthy fans `min_stable_duty <= min_sustain_duty`, so the
        // second clamp is a no-op; on firmware-kick fans it raises the
        // kick target above the oscillation band so the post-kick
        // sustain landing also stays clear of the zone.
        let kick = duty_for_rpm(&self.up_curve, target_rpm)
            .max(self.min_start_duty)
            .max(self.min_stable_duty);
        // Same clamp on sustain: never let the dispatcher write a
        // device-duty that sits inside the oscillation zone. With
        // `min_stable_duty == 0` (the default for older persisted
        // calibrations that pre-date this field) the clamp degenerates
        // to a no-op and prior behaviour is preserved exactly.
        let sustain = duty_for_rpm(&self.down_curve, target_rpm).max(self.min_stable_duty);
        MappedDuty { kick, sustain }
    }

    /// Internal smooth-path reverse map. Caller guarantees `Smooth`.
    ///
    /// Uses **ceiling** integer division (not round-to-nearest) so the
    /// displayed true-duty round-trips back to what the user set even
    /// when truncation in `duty_for_rpm` shaves a few RPM off the
    /// target. Without this, setting 98% commonly displays as 97%
    /// because the integer-truncated device-duty corresponds to an
    /// RPM that falls just below the 98% boundary.
    fn rpm_to_true_duty_smooth(&self, rpm: RPM) -> Duty {
        debug_assert_eq!(self.curve_kind, CurveKind::Smooth);
        assert!(self.rpm_max > 0);

        let rpm_floor = self.rpm_floor();
        if rpm <= rpm_floor {
            return 0;
        }
        if rpm >= self.rpm_max {
            return 100;
        }
        let range = u64::from(self.rpm_max - rpm_floor);
        assert!(range > 0);
        let above = u64::from(rpm - rpm_floor);
        let result = (above * 100).div_ceil(range);
        u8::try_from(result.min(100)).unwrap_or(100)
    }

    /// RPM at `min_sustain_duty` along the **down**-curve: the lowest
    /// RPM the fan can hold once already spinning. This is the right
    /// floor for true-duty mapping in both directions: the dispatcher
    /// writes a sustain device-duty (from the down-curve) after the
    /// kick, so the reverse map must reference the down-curve's
    /// lowest spinning RPM, not the up-curve's higher kick threshold.
    ///
    /// Using the up-curve threshold here on hysteretic fans causes
    /// the reverse map to clamp legitimate low-duty samples (e.g.
    /// `down_curve[10] = 713` RPM when `up_curve[10] = 720` RPM) to
    /// 0% true-duty, even though the fan is clearly spinning. The
    /// forward path now clamps `kick` to `min_start_duty` so the
    /// kick is still strong enough to spin the fan up from rest.
    fn rpm_floor(&self) -> RPM {
        self.down_curve
            .iter()
            .find(|s| s.duty == self.min_sustain_duty)
            .map_or_else(
                || rpm_at_device_duty(&self.down_curve, self.min_sustain_duty),
                |s| s.rpm,
            )
    }
}

/// Linear interpolation of a target RPM from a true-duty in 1..=100.
fn interpolate_rpm(rpm_floor: RPM, rpm_max: RPM, true_duty: Duty) -> RPM {
    assert!(true_duty > 0);
    assert!(true_duty <= 100);
    assert!(rpm_max >= rpm_floor);

    let range = u64::from(rpm_max - rpm_floor);
    let pct = u64::from(true_duty);
    let offset = (range * pct + 50) / 100;
    rpm_floor + u32::try_from(offset).unwrap_or(u32::MAX - rpm_floor)
}

/// Linear scan of a variable-spaced curve for the lowest sample where
/// RPM >= target, then linear interpolation by RPM between that sample
/// and the previous one. Returns the upper-bound duty when the target
/// is above the entire curve, the lower-bound duty when below, and 0
/// for an empty curve.
fn duty_for_rpm(curve: &[DutySample], rpm: RPM) -> Duty {
    if curve.is_empty() {
        return 0;
    }
    let Some(idx) = curve.iter().position(|s| s.rpm >= rpm) else {
        return curve[curve.len() - 1].duty;
    };
    if idx == 0 {
        return curve[0].duty;
    }
    let lo = curve[idx - 1];
    let hi = curve[idx];
    if hi.rpm <= lo.rpm || rpm <= lo.rpm {
        return lo.duty;
    }
    let span = hi.duty - lo.duty;
    let numerator = (rpm - lo.rpm) * u32::from(span);
    let denominator = hi.rpm - lo.rpm;
    let frac = numerator / denominator;
    lo.duty + u8::try_from(frac).unwrap_or(span)
}

/// Classify the up-curve as `Smooth` or `Stepped`.
///
/// Counts inter-sample increases that exceed the jitter tolerance.
/// Below half the inter-sample gaps having a meaningful increase is
/// `Stepped`; this catches devices like `ThinkPad` fans and step-pumps
/// that only change RPM at a few discrete duty values, regardless of
/// the sample spacing.
pub fn classify_curve(up_curve: &[DutySample], rpm_max: RPM) -> CurveKind {
    assert!(rpm_max > 0);
    if up_curve.len() < 2 {
        return CurveKind::Stepped;
    }

    let jitter = jitter_threshold(rpm_max);
    let mut transitions: u32 = 0;
    for pair in up_curve.windows(2) {
        if pair[1].rpm > pair[0].rpm.saturating_add(jitter) {
            transitions += 1;
        }
    }
    let total_gaps = u32::try_from(up_curve.len() - 1).unwrap_or(u32::MAX);
    if transitions * 2 < total_gaps {
        CurveKind::Stepped
    } else {
        CurveKind::Smooth
    }
}

/// RPM noise floor used when locating the start of fan rotation.
pub fn start_threshold(rpm_max: RPM) -> RPM {
    let relative_u64 = u64::from(rpm_max) * u64::from(RPM_START_THRESHOLD_FRACTION_PERCENT) / 100;
    let relative = u32::try_from(relative_u64).unwrap_or(u32::MAX);
    relative.max(RPM_START_THRESHOLD_ABSOLUTE)
}

/// RPM tolerance treated as "no real change" by the step classifier.
fn jitter_threshold(rpm_max: RPM) -> RPM {
    let relative_u64 = u64::from(rpm_max) * u64::from(RPM_JITTER_FRACTION_PERCENT) / 100;
    let relative = u32::try_from(relative_u64).unwrap_or(u32::MAX);
    relative.max(RPM_JITTER_ABSOLUTE)
}

/// Derive the working-range scalars from a pair of sweep curves.
///
/// Returns `None` when the up-curve never crosses the start threshold,
/// which the diagnoser surfaces as a `fan_unresponsive` failure rather
/// than persisting a degenerate calibration.
pub fn derive_scalars(
    up_curve: &[DutySample],
    down_curve: &[DutySample],
) -> Option<DerivedScalars> {
    let rpm_max = up_curve.iter().map(|s| s.rpm).max().unwrap_or(0);
    if rpm_max == 0 || up_curve.is_empty() {
        return None;
    }
    let threshold = start_threshold(rpm_max);
    let jitter = jitter_threshold(rpm_max);

    let min_start_duty = up_curve.iter().find(|s| s.rpm >= threshold)?.duty;
    let min_sustain_duty = down_curve
        .iter()
        .find(|s| s.rpm >= threshold)
        .map_or(min_start_duty, |s| s.duty);
    let plateau_target = rpm_max.saturating_sub(jitter);
    let max_eff_duty = up_curve
        .iter()
        .find(|s| s.rpm >= plateau_target)
        .map_or_else(|| up_curve[up_curve.len() - 1].duty, |s| s.duty);

    Some(DerivedScalars {
        min_start_duty,
        min_sustain_duty,
        max_eff_duty,
        rpm_max,
    })
}

/// Linearly interpolate the RPM at an arbitrary device-duty in a
/// variable-spaced sample list. Clamps to the boundary RPMs outside
/// the recorded range and returns 0 for an empty curve.
fn rpm_at_device_duty(curve: &[DutySample], device_duty: Duty) -> RPM {
    if curve.is_empty() {
        return 0;
    }
    // Below the lowest recorded duty: clamp to its RPM.
    if device_duty <= curve[0].duty {
        return curve[0].rpm;
    }
    // Above the highest recorded duty: clamp to its RPM.
    let last = curve[curve.len() - 1];
    if device_duty >= last.duty {
        return last.rpm;
    }
    // Find the first sample with duty > device_duty; interpolate
    // between it and the previous sample.
    let hi_idx = curve
        .iter()
        .position(|s| s.duty > device_duty)
        .unwrap_or(curve.len() - 1);
    let lo = curve[hi_idx - 1];
    let hi = curve[hi_idx];
    let span = u32::from(hi.duty - lo.duty);
    if span == 0 {
        return lo.rpm;
    }
    let frac = u32::from(device_duty - lo.duty);
    let delta = hi.rpm.saturating_sub(lo.rpm);
    lo.rpm + delta * frac / span
}

/// Pick the displayed true-duty from the two reverse-mapped values.
///
/// Prefer the device-duty-derived value because it is stable across
/// reads (the PWM sysfs file does not jitter the way the fan tachometer
/// does). Fall back to the RPM-derived value when the two disagree by
/// more than `threshold` percentage points: that gap signals a real
/// failure (stuck fan, broken sensor, fan not responding to PWM) which
/// the user should see surfaced in the duty display.
pub fn select_displayed_true_duty(
    device_derived: Option<Duty>,
    rpm_derived: Option<Duty>,
    threshold: u8,
) -> Option<Duty> {
    match (device_derived, rpm_derived) {
        (Some(dd), Some(rd)) if dd.abs_diff(rd) > threshold => Some(rd),
        (Some(dd), _) => Some(dd),
        (None, Some(rd)) => Some(rd),
        (None, None) => None,
    }
}

/// Examine the calibration curves and produce any reliability warnings
/// that should surface to the user. Warnings do not stop the sweep
/// from persisting; they just colour the post-completion message and
/// the popover status text.
///
/// Inputs are the up-sweep curve and the scalars already derived from
/// it. The `curve_kind` is taken by mutable reference so this helper
/// can force passthrough when the fan looks BIOS-controlled.
pub fn derive_warnings(
    up_curve: &[DutySample],
    scalars: &DerivedScalars,
    curve_kind: &mut CurveKind,
) -> Vec<CalibrationWarning> {
    let mut warnings = Vec::new();
    let effective_span = effective_rpm_span(up_curve, scalars);
    let jitter = jitter_threshold(scalars.rpm_max);
    let not_controllable_limit = jitter.saturating_mul(2);
    if effective_span <= not_controllable_limit {
        warnings.push(CalibrationWarning::NotControllable);
        // BIOS / firmware-controlled fan: forcibly disable mapping so
        // the dispatcher does not pretend the fan responds to true-duty.
        *curve_kind = CurveKind::Stepped;
        // Don't also stack LimitedRange on top — NotControllable is
        // the more pointed message and already conveys "tiny range".
        return warnings;
    }
    // LimitedRange is informational: it fires regardless of curve_kind
    // because a narrow-range stepped fan also benefits from the
    // explicit "the usable RPM band is small" cue alongside the
    // "mapping disabled" status text.
    let limited_relative = scalars.rpm_max.saturating_mul(25) / 100;
    let limited_limit = limited_relative.max(500);
    if effective_span < limited_limit {
        warnings.push(CalibrationWarning::LimitedRange {
            rpm_span: effective_span,
            rpm_max: scalars.rpm_max,
        });
    }
    warnings
}

/// Derive `min_stable_duty` from the down-sweep + parallel stability
/// flags collected by the diagnoser.
///
/// Rule: scan the down-curve from highest duty to lowest; descend while
/// each sample is `was_stable = true` AND `rpm >= start_threshold(rpm_max)`;
/// stop at the first sample that fails either gate. The lowest duty in
/// that contiguous-stable run is the threshold.
///
/// Returns `(threshold, oscillation_band)`:
/// - `threshold` is the lowest stable duty (clamped at `min_sustain_duty`
///   so a stable-all-the-way-down fan reports `min_sustain_duty` and the
///   dispatcher's `max` clamp is a no-op).
/// - `oscillation_band` is `Some((lower, upper))` only when **no**
///   contiguous-stable run exists from the top (fully-unstable case).
///   In the partial-unstable case (firmware-kick fans, the usual one)
///   the threshold is set above the band and no warning is needed: the
///   threshold itself is the user-facing signal.
pub fn derive_min_stable_duty(
    down_curve: &[DutySample],
    down_stable: &[bool],
    rpm_max: RPM,
    min_sustain_duty: Duty,
) -> (Duty, Option<(Duty, Duty)>) {
    assert!(rpm_max > 0);
    assert_eq!(down_curve.len(), down_stable.len());
    if down_curve.is_empty() {
        return (min_sustain_duty, None);
    }
    let threshold_rpm = start_threshold(rpm_max);
    // Scan top-down; remember the lowest duty in the contiguous-stable run.
    let mut floor: Option<Duty> = None;
    for (sample, &stable) in down_curve.iter().rev().zip(down_stable.iter().rev()) {
        if stable && sample.rpm >= threshold_rpm {
            floor = Some(sample.duty);
            continue;
        }
        break;
    }
    match floor {
        // All stable, or stable down to (and including) min_sustain_duty:
        // collapse to min_sustain_duty so the dispatcher clamp degenerates.
        Some(duty) if duty <= min_sustain_duty => (min_sustain_duty, None),
        // Partial: oscillation zone exists but a stable region sits above
        // it. Threshold is the bottom of that stable region. No warning.
        Some(duty) => (duty, None),
        // Fully unstable above start-threshold: every sample failed at
        // least one gate. Threshold falls back to min_sustain_duty and we
        // surface the band so the UI can explain what happened.
        None => {
            let upper = down_curve.last().map_or(min_sustain_duty, |s| s.duty);
            (min_sustain_duty, Some((min_sustain_duty, upper)))
        }
    }
}

/// RPM span across the responding portion of the up-sweep: the
/// distance from the sample at `min_start_duty` up to the curve's
/// peak. Excluding the "off" prefix prevents `0 → rpm_max` from
/// looking like a healthy range when the fan is actually a toggle.
fn effective_rpm_span(up_curve: &[DutySample], scalars: &DerivedScalars) -> RPM {
    let rpm_at_start = up_curve
        .iter()
        .find(|s| s.duty == scalars.min_start_duty)
        .map_or(0, |s| s.rpm);
    scalars.rpm_max.saturating_sub(rpm_at_start)
}

/// Scalars derived from the raw sweep curves.
///
/// `max_eff_duty` is informational (where the up-curve approaches its peak
/// within `jitter`); the mapping math does NOT use it as a duty cap. See
/// `Calibration::max_eff_duty` for the full doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DerivedScalars {
    pub min_start_duty: Duty,
    pub min_sustain_duty: Duty,
    pub max_eff_duty: Duty,
    pub rpm_max: RPM,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Number of samples in the synthetic 5%-step curves used by the
    /// pre-existing tests. The new sweep produces variable spacing in
    /// real use, but uniform 5%-step samples are sufficient to test
    /// the mapping math.
    const TEST_SAMPLE_COUNT: usize = 21;
    const TEST_STEP: u8 = 5;

    /// Build a synthetic smooth up-curve: RPM ramps linearly from 0 to `rpm_top`.
    fn smooth_curve(rpm_top: RPM) -> Vec<DutySample> {
        let denominator =
            u32::try_from(TEST_SAMPLE_COUNT - 1).expect("TEST_SAMPLE_COUNT - 1 fits in u32");
        (0..TEST_SAMPLE_COUNT)
            .map(|i| {
                let frac = u32::try_from(i).expect("TEST_SAMPLE_COUNT fits in u32");
                let duty = u8::try_from(i).expect("fits in u8") * TEST_STEP;
                DutySample {
                    duty,
                    rpm: (rpm_top * frac) / denominator,
                }
            })
            .collect()
    }

    /// Build a stepped curve: three RPM plateaus at low/middle/high duty.
    fn stepped_curve(rpm_top: RPM) -> Vec<DutySample> {
        (0..TEST_SAMPLE_COUNT)
            .map(|i| {
                let duty = u8::try_from(i).expect("fits in u8") * TEST_STEP;
                let rpm = match i {
                    0..=4 => 0,
                    5..=12 => rpm_top / 2,
                    _ => rpm_top,
                };
                DutySample { duty, rpm }
            })
            .collect()
    }

    fn make_smooth_calibration() -> Calibration {
        // Build a fully-derived smooth calibration to test mapping math.
        let rpm_top = 2000;
        let up = smooth_curve(rpm_top);
        let down = smooth_curve(rpm_top);
        let scalars = derive_scalars(&up, &down).expect("smooth curve derives");
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 800,
            min_start_duty: scalars.min_start_duty,
            min_sustain_duty: scalars.min_sustain_duty,
            min_stable_duty: scalars.min_sustain_duty,
            max_eff_duty: scalars.max_eff_duty,
            rpm_max: scalars.rpm_max,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            timestamp: Local::now(),
        }
    }

    #[test]
    fn classify_smooth_curve_is_smooth() {
        // Goal: a strictly-monotonic synthetic curve should classify as Smooth.
        let curve = smooth_curve(2000);
        let kind = classify_curve(&curve, 2000);
        assert_eq!(kind, CurveKind::Smooth);
    }

    #[test]
    fn classify_stepped_curve_is_stepped() {
        // Goal: a curve with only a handful of RPM plateaus must classify as Stepped.
        let curve = stepped_curve(2000);
        let kind = classify_curve(&curve, 2000);
        assert_eq!(kind, CurveKind::Stepped);
    }

    #[test]
    fn classify_jittery_smooth_curve_stays_smooth() {
        // Goal: small per-sample noise on a generally rising curve must not
        // be misclassified as stepped (jitter tolerance must absorb it).
        let mut curve = smooth_curve(2000);
        // Add alternating +/-30 RPM jitter; below the absolute floor of 50.
        for (i, sample) in curve.iter_mut().enumerate() {
            if i.is_multiple_of(2) {
                sample.rpm = sample.rpm.saturating_add(30);
            } else {
                sample.rpm = sample.rpm.saturating_sub(30);
            }
        }
        let rpm_max = curve.iter().map(|s| s.rpm).max().unwrap();
        let kind = classify_curve(&curve, rpm_max);
        assert_eq!(kind, CurveKind::Smooth);
    }

    #[test]
    fn forward_map_zero_is_zero() {
        // Goal: true-duty 0 must always map to device-duty 0 for both kick and sustain
        // (fan off is fan off regardless of curve shape).
        let cal = make_smooth_calibration();
        let mapped = cal.true_to_device(0).expect("smooth maps");
        assert_eq!(mapped.kick, 0);
        assert_eq!(mapped.sustain, 0);
    }

    #[test]
    fn forward_map_hundred_writes_full_device_duty() {
        // Goal: true-duty 100 targets `rpm_max` and writes a device-duty
        // up to the curve's top. `max_eff_duty` is informational and
        // does NOT cap the forward mapping — real fans often keep
        // gaining RPM beyond their `max_eff_duty` and the algorithm
        // honors that. Asserts the mapping reaches 100% device-duty
        // for a curve whose rpm_max sits at duty 100.
        let cal = make_smooth_calibration();
        let mapped = cal.true_to_device(100).expect("smooth maps");
        assert_eq!(
            mapped.sustain, 100,
            "true=100 must drive the full device-duty range, not cap at max_eff_duty"
        );
        assert_eq!(mapped.kick, 100);
    }

    #[test]
    fn forward_map_is_monotonic_in_true_duty() {
        // Goal: increasing true-duty must produce non-decreasing device-duty
        // along both kick and sustain dimensions.
        let cal = make_smooth_calibration();
        let mut last_kick = 0;
        let mut last_sustain = 0;
        for t in 0..=100 {
            let mapped = cal.true_to_device(t).expect("smooth maps");
            assert!(
                mapped.kick >= last_kick,
                "kick non-monotonic at true={t}: prev={last_kick} now={}",
                mapped.kick
            );
            assert!(
                mapped.sustain >= last_sustain,
                "sustain non-monotonic at true={t}: prev={last_sustain} now={}",
                mapped.sustain
            );
            last_kick = mapped.kick;
            last_sustain = mapped.sustain;
        }
    }

    #[test]
    fn reverse_map_round_trips_within_tolerance() {
        // Goal: a true-duty written as sustain device-duty must, when read
        // back via the RPM the fan would actually produce at that device
        // duty (interpolated between samples, as real hardware does),
        // recover close to the original true-duty.
        let cal = make_smooth_calibration();
        for t in (5..=95).step_by(5) {
            let mapped = cal.true_to_device(t).expect("smooth maps");
            let rpm = rpm_at_device_duty(&cal.down_curve, mapped.sustain);
            let recovered = cal.rpm_to_true_duty(rpm).expect("smooth maps");
            assert!(
                recovered.abs_diff(t) <= 3,
                "round-trip drifted: input={t} sustain_duty={} rpm={rpm} recovered={recovered}",
                mapped.sustain
            );
        }
    }

    #[test]
    fn reverse_map_clamps_above_max() {
        // Goal: RPM beyond the calibrated maximum must clamp at true-duty 100.
        let cal = make_smooth_calibration();
        let result = cal
            .rpm_to_true_duty(cal.rpm_max + 5000)
            .expect("smooth maps");
        assert_eq!(result, 100);
    }

    #[test]
    fn reverse_map_clamps_below_floor() {
        // Goal: RPM at or below the noise floor must clamp at true-duty 0.
        let cal = make_smooth_calibration();
        let result = cal.rpm_to_true_duty(0).expect("smooth maps");
        assert_eq!(result, 0);
    }

    #[test]
    fn stepped_calibration_returns_none_from_mapping() {
        // Goal: a stepped calibration must signal passthrough via None on
        // both forward and reverse mapping (the dispatcher then keeps the
        // user value untouched).
        let up = stepped_curve(2000);
        let down = stepped_curve(2000);
        let scalars = derive_scalars(&up, &down).expect("derives");
        let cal = Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 800,
            min_start_duty: scalars.min_start_duty,
            min_sustain_duty: scalars.min_sustain_duty,
            min_stable_duty: scalars.min_sustain_duty,
            max_eff_duty: scalars.max_eff_duty,
            rpm_max: scalars.rpm_max,
            curve_kind: CurveKind::Stepped,
            warnings: Vec::new(),
            timestamp: Local::now(),
        };
        assert!(cal.true_to_device(50).is_none());
        assert!(cal.rpm_to_true_duty(1000).is_none());
        assert!(cal.device_to_true_duty(50).is_none());
    }

    #[test]
    fn device_to_true_duty_zero_maps_to_zero() {
        // Goal: at 0% device-duty the fan is at rest; the displayed
        // true-duty must therefore also be 0.
        let cal = make_smooth_calibration();
        assert_eq!(cal.device_to_true_duty(0).expect("smooth maps"), 0);
    }

    #[test]
    fn device_to_true_duty_saturated_returns_full() {
        // Goal: at 100% device-duty the fan is at its max effective speed;
        // the displayed true-duty must therefore land at 100.
        let cal = make_smooth_calibration();
        let recovered = cal.device_to_true_duty(100).expect("smooth maps");
        assert_eq!(recovered, 100);
    }

    #[test]
    fn device_to_true_duty_round_trips_within_tolerance() {
        // Goal: for each user-facing true-duty t, the device-duty written
        // as sustain must round-trip back through `device_to_true_duty`
        // within the same tolerance as the RPM round-trip. This is the
        // stable-display path used by the status pipeline.
        let cal = make_smooth_calibration();
        for t in (5..=95).step_by(5) {
            let mapped = cal.true_to_device(t).expect("smooth maps");
            let recovered = cal
                .device_to_true_duty(mapped.sustain)
                .expect("smooth maps");
            assert!(
                recovered.abs_diff(t) <= 3,
                "round-trip drifted: input={t} sustain_duty={} recovered={recovered}",
                mapped.sustain
            );
        }
    }

    #[test]
    fn select_uses_device_when_close() {
        // Goal: when device-duty-derived and RPM-derived agree within the
        // threshold, the stable device-duty value wins. This is the
        // common path for a healthy fan with natural RPM jitter.
        let chosen = select_displayed_true_duty(Some(50), Some(52), SANITY_THRESHOLD_PERCENT);
        assert_eq!(chosen, Some(50));
    }

    #[test]
    fn select_falls_back_to_rpm_when_diverged() {
        // Goal: a stuck or dead fan keeps the device-duty value the daemon
        // wrote but produces no RPM. The cross-check must trip and the
        // displayed value must be the RPM-derived one so the user sees
        // the failure instead of a misleading "50%".
        let chosen = select_displayed_true_duty(Some(50), Some(0), SANITY_THRESHOLD_PERCENT);
        assert_eq!(chosen, Some(0));
    }

    #[test]
    fn select_uses_device_when_only_device_available() {
        // Goal: a channel with no RPM reading at all (sensor missing or
        // never reported) falls back to the device-duty-derived value
        // instead of dropping the display.
        let chosen = select_displayed_true_duty(Some(40), None, SANITY_THRESHOLD_PERCENT);
        assert_eq!(chosen, Some(40));
    }

    #[test]
    fn select_uses_rpm_when_only_rpm_available() {
        // Goal: a channel with no cached device-duty (some repos report
        // RPM-only) still gets a true-duty display via the RPM path.
        let chosen = select_displayed_true_duty(None, Some(40), SANITY_THRESHOLD_PERCENT);
        assert_eq!(chosen, Some(40));
    }

    #[test]
    fn select_returns_none_when_neither_derived() {
        // Goal: an uncalibrated or stepped channel produces None on both
        // reverse maps; the helper must report None so the caller leaves
        // the raw device-duty in place (passthrough).
        let chosen = select_displayed_true_duty(None, None, SANITY_THRESHOLD_PERCENT);
        assert_eq!(chosen, None);
    }

    #[test]
    fn derive_warnings_empty_for_healthy_smooth_curve() {
        // Goal: a synthetic 0..=2000 linear smooth curve gives no
        // warnings; the typical case must not produce false positives.
        let up = smooth_curve(2000);
        let down = smooth_curve(2000);
        let scalars = derive_scalars(&up, &down).expect("derives");
        let mut kind = classify_curve(&up, scalars.rpm_max);
        let warnings = derive_warnings(&up, &scalars, &mut kind);
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
        assert_eq!(kind, CurveKind::Smooth);
    }

    #[test]
    fn derive_warnings_flags_not_controllable_when_span_within_jitter() {
        // Goal: a fan spinning at a near-constant 1500 RPM regardless of
        // duty must surface as NotControllable, and curve_kind must be
        // forced to Stepped so the dispatcher passes through.
        let up: Vec<DutySample> = (0..TEST_SAMPLE_COUNT)
            .map(|i| {
                let duty = u8::try_from(i).expect("fits in u8") * TEST_STEP;
                let rpm = if duty == 0 { 0 } else { 1500 };
                DutySample { duty, rpm }
            })
            .collect();
        let down = up.clone();
        let scalars = derive_scalars(&up, &down).expect("derives");
        let mut kind = CurveKind::Smooth; // pretend classify said smooth
        let warnings = derive_warnings(&up, &scalars, &mut kind);
        assert!(
            warnings.contains(&CalibrationWarning::NotControllable),
            "expected NotControllable in {warnings:?}"
        );
        assert_eq!(
            kind,
            CurveKind::Stepped,
            "NotControllable must force passthrough"
        );
    }

    #[test]
    fn derive_warnings_flags_limited_range_when_span_too_small() {
        // Goal: a fan that does respond to duty (effective_span beyond
        // the NotControllable noise band) but whose total RPM range
        // is under the absolute 500-RPM threshold must surface
        // LimitedRange. We use a step-curve here because that is the
        // natural shape produced by a real fan with this little range,
        // but the warning still fires (LimitedRange is independent of
        // curve_kind).
        let up: Vec<DutySample> = (0..TEST_SAMPLE_COUNT)
            .map(|i| {
                let duty = u8::try_from(i).expect("fits in u8") * TEST_STEP;
                let rpm = if duty == 0 {
                    0
                } else if duty < 50 {
                    600
                } else {
                    900
                };
                DutySample { duty, rpm }
            })
            .collect();
        let down = up.clone();
        let scalars = derive_scalars(&up, &down).expect("derives");
        let mut kind = classify_curve(&up, scalars.rpm_max);
        let warnings = derive_warnings(&up, &scalars, &mut kind);
        let limited = warnings.iter().find_map(|w| match w {
            CalibrationWarning::LimitedRange { rpm_span, rpm_max } => Some((*rpm_span, *rpm_max)),
            _ => None,
        });
        assert!(limited.is_some(), "expected LimitedRange in {warnings:?}");
        let (span, _) = limited.expect("limited variant");
        assert_eq!(span, 300, "expected 900 - 600 = 300 RPM span, got {span}");
    }

    #[test]
    fn hysteretic_fan_low_true_duty_does_not_clamp_to_zero() {
        // Goal: regression for the user-reported bug where a hysteretic
        // real-world fan (down_curve[min_start_duty] < up_curve[min_start_duty])
        // would display 0% true-duty across the entire 1-5% set range.
        //
        // The shape mirrors the user's fan1 in calibrations.json:
        // min_start_duty=10, min_sustain_duty=6, rpm at down_curve[10]=713,
        // rpm at down_curve[6]=648, rpm_max=3518.
        let up = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 2, rpm: 0 },
            DutySample { duty: 4, rpm: 0 },
            DutySample { duty: 6, rpm: 0 },
            DutySample { duty: 8, rpm: 0 },
            DutySample { duty: 10, rpm: 720 },
            DutySample { duty: 15, rpm: 960 },
            DutySample {
                duty: 20,
                rpm: 1257,
            },
            DutySample {
                duty: 50,
                rpm: 2391,
            },
            DutySample {
                duty: 100,
                rpm: 3518,
            },
        ];
        let down = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 2, rpm: 0 },
            DutySample { duty: 4, rpm: 0 },
            DutySample { duty: 6, rpm: 648 },
            DutySample { duty: 8, rpm: 662 },
            DutySample { duty: 10, rpm: 713 },
            DutySample { duty: 12, rpm: 867 },
            DutySample {
                duty: 20,
                rpm: 1285,
            },
            DutySample {
                duty: 50,
                rpm: 2378,
            },
            DutySample {
                duty: 100,
                rpm: 3517,
            },
        ];
        let cal = Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 1500,
            min_start_duty: 10,
            min_sustain_duty: 6,
            min_stable_duty: 6,
            max_eff_duty: 95,
            rpm_max: 3518,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            timestamp: Local::now(),
        };
        // For each low true-duty the round-trip device_to_true_duty
        // must not collapse to 0.
        for t in 1..=5u8 {
            let mapped = cal.true_to_device(t).expect("smooth maps");
            assert!(
                mapped.kick >= cal.min_start_duty,
                "kick must clamp to min_start_duty; got {} for true={t}",
                mapped.kick
            );
            let recovered = cal
                .device_to_true_duty(mapped.sustain)
                .expect("smooth maps");
            assert!(
                recovered > 0,
                "true={t} -> sustain={} -> recovered={recovered} must not collapse to 0",
                mapped.sustain
            );
        }
    }

    #[test]
    fn rpm_to_true_duty_rounds_up_on_curve_truncation() {
        // Goal: when `duty_for_rpm` truncates the device-duty
        // calculation (the device-duty space is integer, but the
        // computed target lands between samples), the reverse
        // `rpm_to_true_duty` must round UP so the displayed true-duty
        // matches what the user originally set. Regression for the
        // "set 98%, display 97%" bug.
        //
        // Build a synthetic curve whose down-curve at duty 90 sits
        // below where the up-curve at the same duty would (a normal
        // hysteretic fan). 98% true_duty -> ~1962 RPM target ->
        // truncated to duty 93 -> read back as ~1952 RPM -> previously
        // mapped to 97% via round-to-nearest. Ceiling pulls it back
        // up to 98%.
        let mut up = vec![DutySample { duty: 0, rpm: 0 }];
        for d in (5..=100).step_by(5) {
            let rpm = match d {
                90 => 1880,
                95 => 2000,
                100 => 2000,
                _ => 20 * u32::from(d), // linear ramp
            };
            up.push(DutySample { duty: d, rpm });
        }
        let down = up.clone();
        let scalars = derive_scalars(&up, &down).expect("derives");
        let cal = Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 500,
            min_start_duty: scalars.min_start_duty,
            min_sustain_duty: scalars.min_sustain_duty,
            min_stable_duty: scalars.min_sustain_duty,
            max_eff_duty: scalars.max_eff_duty,
            rpm_max: scalars.rpm_max,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            timestamp: Local::now(),
        };
        // 98% device-duty round-trip: write the sustain device-duty
        // for 98%, then verify the reverse mapping comes back at 98%.
        let mapped = cal.true_to_device(98).expect("smooth maps");
        let recovered = cal
            .device_to_true_duty(mapped.sustain)
            .expect("smooth maps");
        assert_eq!(
            recovered, 98,
            "98% true must round-trip to 98% display; got {recovered}"
        );
    }

    #[test]
    fn rpm_at_device_duty_interpolates_across_variable_spacing() {
        // Goal: a curve with mixed 2%/5% spacing must interpolate
        // linearly at any in-between device-duty. Mirrors what the new
        // sweep produces: dense around kick-in, sparse beyond. We
        // verify that a duty between two samples comes back as the
        // expected linearly-interpolated RPM regardless of whether
        // the surrounding samples are 2 or 5 apart.
        let curve = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 2, rpm: 40 },
            DutySample { duty: 4, rpm: 80 },
            DutySample { duty: 9, rpm: 180 },
            DutySample { duty: 14, rpm: 280 },
            DutySample { duty: 19, rpm: 380 },
            DutySample {
                duty: 100,
                rpm: 2000,
            },
        ];
        // Between 2% samples (at duty 3, mid of 2 and 4): expect 60.
        assert_eq!(rpm_at_device_duty(&curve, 3), 60);
        // Between 5% samples (at duty 11, between 9 and 14 at +2/5 of
        // the way): 180 + (280-180) * 2/5 = 220.
        assert_eq!(rpm_at_device_duty(&curve, 11), 220);
        // At a sample duty: returns the sample RPM exactly.
        assert_eq!(rpm_at_device_duty(&curve, 14), 280);
        // Above the highest sample: clamps to that sample's RPM.
        assert_eq!(rpm_at_device_duty(&curve, 100), 2000);
    }

    #[test]
    fn derive_scalars_rejects_zero_curve() {
        // Goal: a fan that never produces RPM must surface as None so the
        // diagnoser fails with fan_unresponsive instead of saving garbage.
        let zero: Vec<DutySample> = (0..TEST_SAMPLE_COUNT)
            .map(|i| DutySample {
                duty: u8::try_from(i).expect("fits in u8") * TEST_STEP,
                rpm: 0,
            })
            .collect();
        assert!(derive_scalars(&zero, &zero).is_none());
    }

    #[test]
    fn derive_scalars_finds_start_and_plateau() {
        // Goal: on a synthetic smooth curve with linear ramp from 0 to 2000
        // RPM, min_start_duty must be the first index above the noise floor
        // and max_eff_duty must be at or near the plateau index.
        let up = smooth_curve(2000);
        let down = smooth_curve(2000);
        let scalars = derive_scalars(&up, &down).expect("derives");
        assert!(scalars.min_start_duty > 0);
        assert!(scalars.min_start_duty <= 25);
        assert!(scalars.max_eff_duty >= 95);
        assert_eq!(scalars.rpm_max, 2000);
    }

    #[test]
    fn jitter_threshold_uses_relative_when_large() {
        // Goal: at high rpm_max (10000), the 3% relative threshold (300) must
        // beat the absolute floor (50).
        let t = jitter_threshold(10_000);
        assert_eq!(t, 300);
    }

    #[test]
    fn jitter_threshold_uses_absolute_when_small() {
        // Goal: at low rpm_max (500), 3% relative (15) falls below the
        // absolute floor (50), so the floor wins.
        let t = jitter_threshold(500);
        assert_eq!(t, RPM_JITTER_ABSOLUTE);
    }

    #[test]
    fn classify_does_not_panic_on_uniform_curve() {
        // Goal: a constant non-zero curve (which would mean a stuck-on fan)
        // must classify cleanly without arithmetic surprises. Uniform = no
        // transitions = Stepped.
        let curve: Vec<DutySample> = (0..TEST_SAMPLE_COUNT)
            .map(|i| DutySample {
                duty: u8::try_from(i).expect("fits in u8") * TEST_STEP,
                rpm: 1500,
            })
            .collect();
        let kind = classify_curve(&curve, 1500);
        assert_eq!(kind, CurveKind::Stepped);
    }

    #[test]
    fn smooth_calibration_kick_is_at_or_above_sustain() {
        // Goal: kick must never be lower than sustain. Synthetic curves
        // have identical up and down samples so the two values are equal;
        // real hysteretic hardware will have kick > sustain. The ordering
        // invariant is what the dispatch state machine relies on.
        let cal = make_smooth_calibration();
        for t in 1..=100 {
            let mapped = cal.true_to_device(t).expect("smooth maps");
            assert!(
                mapped.kick >= mapped.sustain,
                "expected kick >= sustain at true={t}, got kick={} sustain={}",
                mapped.kick,
                mapped.sustain
            );
        }
    }

    // --- derive_min_stable_duty -------------------------------------

    /// Build a synthetic down-curve at 5%-step spacing and a parallel
    /// stable-flag vec. Helper for the `derive_min_stable_duty` tests so
    /// each case can declare just "RPM per duty step" + which duties are
    /// flagged unstable.
    fn down_curve_with_flags(
        per_duty_rpm: &[(Duty, RPM)],
        unstable_duties: &[Duty],
    ) -> (Vec<DutySample>, Vec<bool>) {
        let samples: Vec<DutySample> = per_duty_rpm
            .iter()
            .map(|&(duty, rpm)| DutySample { duty, rpm })
            .collect();
        let flags: Vec<bool> = samples
            .iter()
            .map(|s| !unstable_duties.contains(&s.duty))
            .collect();
        (samples, flags)
    }

    #[test]
    fn derive_min_stable_duty_all_stable_collapses_to_min_sustain() {
        // Goal: a healthy fan whose entire down-curve settles within
        // tolerance must report `min_stable_duty == min_sustain_duty`
        // and produce no oscillation band, so the dispatcher's clamp
        // is a no-op and the UI dashed line does not render.
        let samples: Vec<(Duty, RPM)> = (0..=20)
            .map(|i| {
                (
                    u8::try_from(i).unwrap() * 5,
                    100 * u32::try_from(i).unwrap(),
                )
            })
            .collect();
        let (down, stable) = down_curve_with_flags(&samples, &[]);
        let (floor, band) = derive_min_stable_duty(&down, &stable, 2000, 5);
        assert_eq!(floor, 5, "all-stable: floor collapses to min_sustain_duty");
        assert!(band.is_none(), "all-stable: no oscillation band reported");
    }

    #[test]
    fn derive_min_stable_duty_firmware_kick_lifts_floor_above_band() {
        // Goal: the canonical firmware-kick scenario. The fan is stable
        // from ~30%+ but oscillates between min_sustain_duty (5%) and
        // ~25%. The threshold must land at the bottom of the contiguous
        // stable run from the top of the down-curve. No band is
        // surfaced because the threshold ITSELF is the user-facing
        // signal (partial-unstable case).
        let mut samples: Vec<(Duty, RPM)> = Vec::new();
        for i in 0..=20 {
            samples.push((
                u8::try_from(i).unwrap() * 5,
                100 * u32::try_from(i).unwrap(),
            ));
        }
        let unstable: Vec<Duty> = vec![10, 15, 20, 25]; // oscillation band
        let (down, stable) = down_curve_with_flags(&samples, &unstable);
        let (floor, band) = derive_min_stable_duty(&down, &stable, 2000, 5);
        assert_eq!(
            floor, 30,
            "floor lifts to the bottom of the contiguous-stable run"
        );
        assert!(
            band.is_none(),
            "partial-unstable: no Oscillating warning (threshold conveys it)"
        );
    }

    #[test]
    fn derive_min_stable_duty_fully_unstable_falls_back_and_warns() {
        // Goal: when no contiguous-stable run exists from the top of
        // the down-curve, the floor must fall back to min_sustain_duty
        // (no clamp change) and the oscillation band must be surfaced
        // so the popover can emit a CalibrationWarning::Oscillating.
        let mut samples: Vec<(Duty, RPM)> = Vec::new();
        for i in 0..=20 {
            samples.push((
                u8::try_from(i).unwrap() * 5,
                100 * u32::try_from(i).unwrap(),
            ));
        }
        // Every sample above min_sustain_duty is unstable.
        let unstable: Vec<Duty> = (5..=100).step_by(5).collect();
        let (down, stable) = down_curve_with_flags(&samples, &unstable);
        let (floor, band) = derive_min_stable_duty(&down, &stable, 2000, 5);
        assert_eq!(
            floor, 5,
            "fully-unstable: floor falls back to min_sustain_duty"
        );
        let (lower, upper) = band.expect("fully-unstable: oscillation band surfaced");
        assert_eq!(lower, 5);
        assert_eq!(upper, 100);
    }

    #[test]
    fn derive_min_stable_duty_ignores_phase_coincidence_stable_island() {
        // Goal: rule (B) — descend only while EVERY sample above is
        // stable. A single phase-coincidence stable sample sitting
        // inside an otherwise-unstable region must NOT be picked as
        // the floor; the run is broken by the unstable samples above it.
        let mut samples: Vec<(Duty, RPM)> = Vec::new();
        for i in 0..=20 {
            samples.push((
                u8::try_from(i).unwrap() * 5,
                100 * u32::try_from(i).unwrap(),
            ));
        }
        // 15 is a stable island; 20 and 25 unstable; 30+ stable. The
        // contiguous run from the top stops at 30 (because 25 broke it).
        let unstable: Vec<Duty> = vec![10, 20, 25];
        let (down, stable) = down_curve_with_flags(&samples, &unstable);
        let (floor, band) = derive_min_stable_duty(&down, &stable, 2000, 5);
        assert_eq!(
            floor, 30,
            "phase-coincidence stable at 15 must NOT be chosen; floor is bottom of run above 25"
        );
        assert!(band.is_none());
    }

    #[test]
    fn derive_min_stable_duty_low_rpm_samples_block_floor() {
        // Goal: a sample marked stable but whose RPM is below
        // start_threshold(rpm_max) must NOT count as the floor. This
        // guards against the "fan stopped at low duty so the reading
        // is noise-stable around 0 RPM" failure mode.
        let mut samples: Vec<(Duty, RPM)> = Vec::new();
        // Fan is below start threshold (50 RPM or 5% of 2000 = 100) for
        // duty 0-15, then climbs. All samples are tagged stable.
        for i in 0..=20 {
            let duty = u8::try_from(i).unwrap() * 5;
            let rpm = if duty < 20 { 30 } else { 100 * u32::from(duty) };
            samples.push((duty, rpm));
        }
        let (down, stable) = down_curve_with_flags(&samples, &[]);
        let (floor, _band) = derive_min_stable_duty(&down, &stable, 2000, 5);
        assert_eq!(
            floor, 20,
            "low-RPM samples (below start_threshold) must not be eligible as the floor"
        );
    }

    // --- dispatch clamp via true_to_device_smooth -------------------

    #[test]
    fn true_to_device_smooth_clamps_sustain_to_min_stable_duty() {
        // Goal: a calibration whose min_stable_duty sits ABOVE the
        // natural down-curve interpolation point for low true-duty
        // values must surface min_stable_duty as the sustain. The
        // post-kick fan lands above the oscillation zone instead of
        // inside it.
        let mut cal = make_smooth_calibration();
        // Force a firmware-kick-style threshold: clamp sustain at 40%.
        cal.min_stable_duty = 40;
        // 1% true duty would otherwise produce a sustain near
        // min_sustain_duty; the clamp must lift it to 40%.
        let mapped = cal.true_to_device(1).expect("smooth maps");
        assert!(
            mapped.sustain >= 40,
            "sustain must clamp at min_stable_duty; got {}",
            mapped.sustain
        );
        // Kick also lifts at least to 40% so the post-kick landing
        // isn't ABOVE the kick value (avoiding a transient downstep).
        assert!(
            mapped.kick >= 40,
            "kick must lift to min_stable_duty too; got {}",
            mapped.kick
        );
    }

    #[test]
    fn true_to_device_smooth_zero_still_off_with_clamp() {
        // Goal: the min_stable_duty clamp must not turn off-state into
        // a forced minimum. true_duty == 0 always writes (0, 0) so the
        // dispatcher can transition the fan to fully off.
        let mut cal = make_smooth_calibration();
        cal.min_stable_duty = 40;
        let mapped = cal.true_to_device(0).expect("smooth maps");
        assert_eq!(mapped.sustain, 0);
        assert_eq!(mapped.kick, 0);
    }

    #[test]
    fn true_to_device_smooth_no_clamp_when_threshold_at_sustain() {
        // Goal: the healthy-fan path. min_stable_duty == min_sustain_duty
        // means there's no oscillation zone; the clamp must be a no-op
        // and behaviour must match the pre-feature mapping exactly.
        // Snapshot the mapping with the threshold at min_sustain, then
        // at 0 (the default for old persisted calibrations), and require
        // both produce identical sustains for every true-duty.
        let cal = make_smooth_calibration();
        let baseline = cal.min_sustain_duty;
        let mut at_default = cal.clone();
        at_default.min_stable_duty = 0;
        let mut at_sustain = cal.clone();
        at_sustain.min_stable_duty = baseline;
        for t in 1..=100 {
            let a = at_default.true_to_device(t).expect("smooth maps");
            let b = at_sustain.true_to_device(t).expect("smooth maps");
            assert_eq!(a.sustain, b.sustain, "no-op clamp mismatch at true={t}");
            assert_eq!(a.kick, b.kick, "no-op clamp mismatch at true={t}");
        }
        // Also verify the post-fix sustain at high duty is unchanged
        // from what the natural interpolation would produce.
        let mapped = cal.true_to_device(80).expect("smooth maps");
        assert!(
            mapped.sustain >= baseline,
            "sustain at 80% must remain at or above min_sustain (no regression)"
        );
        let _ = at_default;
        let _ = mapped;
    }
}
