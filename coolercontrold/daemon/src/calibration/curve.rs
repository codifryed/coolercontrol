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

//! Calibration curve types and the pure RPM<->duty math. A `Calibration`
//! holds two sample arrays (up + down), a kick window, derived scalars,
//! and a `Smooth`/`Stepped` classification. Smooth maps; stepped returns
//! `None` so the dispatcher passes through.

use crate::device::{Duty, SpeedOptions, RPM};
use chrono::{DateTime, Local};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Sparse duty step (post kick-in on up-sweep, outside kick-zone on down).
pub const DUTY_STEP_SPARSE: u8 = 5;

/// Dense duty step (ramp from rest, inside kick-zone). Tighter resolution
/// where the duty-to-RPM gradient is steepest.
pub const DUTY_STEP_DENSE: u8 = 2;

/// Buffer above `min_start_duty` treated as the kick-in zone on the
/// down-sweep. Wide enough to cover hysteresis.
pub const KICK_ZONE_BUFFER_PERCENT: u8 = 10;

/// Up-sweep dense range. Dense (2%) below, sparse (5%) above. Wide
/// enough to capture firmware-floor / firmware-kick fans cleanly; the
/// real start-of-response can sit at ~20-25% on those.
pub const UP_SWEEP_DENSE_RANGE_END_DUTY: Duty = 30;

/// Up-sweep duty at which a non-spinning fan is declared unresponsive
/// and the sweep is aborted. 50% is well above any real PWM fan's start.
pub const UNRESPONSIVE_ABORT_DUTY: Duty = 50;

/// Sample cap enforced at deserialization to guard against pathological data.
pub const MAX_SAMPLES_PER_CURVE: usize = 128;

const RPM_START_THRESHOLD_ABSOLUTE: RPM = 50;
const RPM_START_THRESHOLD_FRACTION_PERCENT: u32 = 5;
const RPM_JITTER_ABSOLUTE: RPM = 50;
const RPM_JITTER_FRACTION_PERCENT: u32 = 3;

/// A (device-duty, RPM) sample. Curves are sorted ascending by `duty`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct DutySample {
    pub duty: Duty,
    pub rpm: RPM,
}

/// The duty/RPM curve and derived scalars for one channel. Always
/// represents a successful diagnosis.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Calibration {
    /// Up-sweep samples (duty 0..=100 ascending). Dense in kick-in, sparse above.
    pub up_curve: Vec<DutySample>,

    /// Down-sweep samples. Same shape, dense around the kick-in zone.
    pub down_curve: Vec<DutySample>,

    /// Measured time from writing `min_start_duty` to RPM crossing the
    /// start floor, plus a 50% safety margin. Used by the dispatcher's
    /// Off -> Kicking -> On hand-off. Bounded by the daemon's poll rate
    /// so values land on second-aligned increments.
    pub kick_duration_ms: u32,

    /// Lowest device-duty that reliably starts the fan from rest.
    pub min_start_duty: Duty,

    /// Lowest device-duty that keeps the fan spinning once started.
    pub min_sustain_duty: Duty,

    /// Lowest device-duty without oscillation. Equals `min_sustain_duty`
    /// on healthy fans; sits above it on firmware-kicked fans that
    /// oscillate just above sustain (dispatcher clamps kick + sustain
    /// to this value). Defaults to 0 on older records.
    #[serde(default)]
    pub min_stable_duty: Duty,

    /// "Near plateau" duty (within `jitter` of `rpm_max`). Informational:
    /// `true_to_device(100)` can still write above this on fans that
    /// keep gaining RPM past it.
    pub max_eff_duty: Duty,

    /// Peak RPM observed. Always positive.
    pub rpm_max: RPM,

    pub curve_kind: CurveKind,

    /// Non-fatal reliability findings. Empty for a healthy calibration.
    #[serde(default)]
    pub warnings: Vec<CalibrationWarning>,

    /// Diagnoser backfilled `duty: None` history entries with derived
    /// values (channel has no native duty reporting). Clears on delete;
    /// the UI uses it to decide whether to prompt for a chart reload.
    #[serde(default)]
    pub was_rpm_only: bool,

    pub timestamp: DateTime<Local>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CurveKind {
    Smooth,
    Stepped,
}

/// Non-fatal reliability findings. The calibration persists either way;
/// these inform the popover and the completion notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalibrationWarning {
    /// Tachometer reported 0 RPM throughout: fan disconnected, tach
    /// wire unplugged, or firmware holding the fan off.
    NoTachometer,
    /// Fan spins but does not track duty (RPM span within noise). Most
    /// often BIOS/firmware-controlled. Mapping disabled (forced Stepped).
    NotControllable,
    /// Usable RPM span is small enough that mapping resolution is coarse.
    /// Mapping stays active.
    LimitedRange { rpm_span: RPM, rpm_max: RPM },
    /// Fan never settled above `min_sustain_duty` (cheap firmware-kicked
    /// fan). `min_stable_duty` undefined; dispatcher falls back to
    /// `min_sustain_duty`. The range marks the observed oscillation band.
    Oscillating { lower_duty: Duty, upper_duty: Duty },
}

/// Result of a forward true-duty -> device-duty mapping.
///
/// (kick, sustain) device duties returned by the forward map.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappedDuty {
    pub kick: Duty,
    pub sustain: Duty,
}

impl Calibration {
    /// Maps true-duty to (kick, sustain). `None` for stepped channels.
    pub fn true_to_device(&self, true_duty: Duty) -> Option<MappedDuty> {
        if self.curve_kind == CurveKind::Stepped {
            return None;
        }
        Some(self.true_to_device_smooth(true_duty))
    }

    /// Maps measured RPM back to true-duty. `None` for stepped channels.
    pub fn rpm_to_true_duty(&self, rpm: RPM) -> Option<Duty> {
        if self.curve_kind == CurveKind::Stepped {
            return None;
        }
        Some(self.rpm_to_true_duty_smooth(rpm))
    }

    /// Stable inverse: device-duty -> down-curve RPM -> true-duty. Used
    /// by the status pipeline since device-duty does not jitter the way
    /// RPM does. `None` for stepped channels.
    pub fn device_to_true_duty(&self, device_duty: Duty) -> Option<Duty> {
        if self.curve_kind == CurveKind::Stepped {
            return None;
        }
        let dut = device_duty.min(100);
        // Mirror the forward `true_duty == 0 -> device 0` special case so
        // an off fan always displays as 0% regardless of where the
        // filtered curve's first sample sits.
        if dut == 0 {
            return Some(0);
        }
        // Skip kick-artifact samples below the sustain floor: the
        // firmware-kick dense region has non-monotonic rpms that would
        // otherwise pull `rpm_at_device_duty` to inflated values.
        let down_above = curve_above(&self.down_curve, self.min_sustain_duty);
        let rpm = rpm_at_device_duty(down_above, dut);
        Some(self.rpm_to_true_duty_smooth(rpm))
    }

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
        // Saturation tail: write full device duty rather than chasing
        // `rpm_max`, which may sit at a sample slightly below duty 100
        // due to measurement variance. Fans that actually throttle at
        // saturation are rare and the user can recalibrate; the common
        // case is the user expects 100% to give the fan's full speed.
        if true_clamped == 100 {
            return MappedDuty {
                kick: 100,
                sustain: 100,
            };
        }
        let rpm_floor = self.rpm_floor();
        assert!(self.rpm_max >= rpm_floor);
        let target_rpm = interpolate_rpm(rpm_floor, self.rpm_max, true_clamped);
        // Skip artifact samples below the floor on both curves. Without
        // this, kick spikes in the dense region (e.g., down_curve has
        // rpm 1012 at duty 4) would land as the first sample matching a
        // mid-range target_rpm and drag the lookup to a bogus low duty
        // before the `.max(min_stable_duty)` clamp.
        let up_above = curve_above(&self.up_curve, self.min_start_duty);
        let down_above = curve_above(&self.down_curve, self.min_sustain_duty);
        let kick = duty_for_rpm(up_above, target_rpm)
            .max(self.min_start_duty)
            .max(self.min_stable_duty);
        let sustain = duty_for_rpm(down_above, target_rpm).max(self.min_stable_duty);
        MappedDuty { kick, sustain }
    }

    /// Ceiling division so the inverse round-trips: setting 98% would
    /// otherwise display as 97% when integer truncation in `duty_for_rpm`
    /// shaves a few RPM off the target. `rpm == 0` is fan-off; any
    /// non-zero rpm at or below the floor is "fan running at minimum"
    /// and maps to 1 so the user can tell a spinning fan from a stopped
    /// one in the display.
    fn rpm_to_true_duty_smooth(&self, rpm: RPM) -> Duty {
        debug_assert_eq!(self.curve_kind, CurveKind::Smooth);
        assert!(self.rpm_max > 0);

        if rpm == 0 {
            return 0;
        }
        let rpm_floor = self.rpm_floor();
        if rpm <= rpm_floor {
            return 1;
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

    /// RPM at `min_sustain_duty` on the down-curve: lowest RPM the fan
    /// can hold once spinning. The reverse map uses this (not the
    /// up-curve's higher kick threshold) so hysteretic fans don't get
    /// their legitimate low-duty samples clamped to 0%.
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
/// Slice of `curve` at and above `floor`, in original order. Used by
/// the dispatcher's lookups to skip the kick-artifact zone below the
/// sustain floor; those samples have non-monotonic rpms that would
/// fool the linear scan in `duty_for_rpm` / `rpm_at_device_duty`.
fn curve_above(curve: &[DutySample], floor: Duty) -> &[DutySample] {
    let start = curve
        .iter()
        .position(|s| s.duty >= floor)
        .unwrap_or(curve.len());
    &curve[start..]
}

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

/// `Stepped` when fewer than half the inter-sample gaps show RPM
/// growth above the jitter tolerance (catches `ThinkPad` fans, step-pumps).
/// Jitter scales with step size so dense (2%) gaps are not unfairly
/// penalized vs. sparse (5%) gaps.
pub fn classify_curve(up_curve: &[DutySample], rpm_max: RPM) -> CurveKind {
    assert!(rpm_max > 0);
    if up_curve.len() < 2 {
        return CurveKind::Stepped;
    }

    let sparse_jitter = jitter_threshold(rpm_max);
    let mut transitions: u32 = 0;
    for pair in up_curve.windows(2) {
        let step = u32::from(pair[1].duty.saturating_sub(pair[0].duty)).max(1);
        let threshold =
            u32::try_from(u64::from(sparse_jitter) * u64::from(step) / u64::from(DUTY_STEP_SPARSE))
                .unwrap_or(sparse_jitter);
        if pair[1].rpm > pair[0].rpm.saturating_add(threshold) {
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

pub fn start_threshold(rpm_max: RPM) -> RPM {
    let relative_u64 = u64::from(rpm_max) * u64::from(RPM_START_THRESHOLD_FRACTION_PERCENT) / 100;
    let relative = u32::try_from(relative_u64).unwrap_or(u32::MAX);
    relative.max(RPM_START_THRESHOLD_ABSOLUTE)
}

fn jitter_threshold(rpm_max: RPM) -> RPM {
    let relative_u64 = u64::from(rpm_max) * u64::from(RPM_JITTER_FRACTION_PERCENT) / 100;
    let relative = u32::try_from(relative_u64).unwrap_or(u32::MAX);
    relative.max(RPM_JITTER_ABSOLUTE)
}

/// First sample in `curve` (low duty to high) whose `rpm` clears
/// `threshold` AND is not significantly above the minimum of all later
/// samples. Returning the higher value would make it a firmware-kick
/// transient that decays once the fan settles past the dense region.
fn first_monotonic_above_threshold(
    curve: &[DutySample],
    threshold: RPM,
    jitter: RPM,
) -> Option<&DutySample> {
    for (i, sample) in curve.iter().enumerate() {
        if sample.rpm < threshold {
            continue;
        }
        let later_min = curve[i + 1..]
            .iter()
            .map(|s| s.rpm)
            .min()
            .unwrap_or(sample.rpm);
        if sample.rpm <= later_min.saturating_add(jitter) {
            return Some(sample);
        }
    }
    None
}

/// Down-sweep sample with the lowest mean RPM across the matched
/// up-curve and down-curve at the same duty. Both samples must clear
/// `threshold`. A firmware-kick fan oscillates at low duty, so a single
/// down-sample can land on the oscillation valley (artificially low)
/// while the up-sample at the same duty lands on a peak (artificially
/// high). Averaging the two cancels the timing-of-sample bias and
/// identifies the duty where the fan actually sits at its floor.
fn lowest_mean_floor_duty<'a>(
    up_curve: &'a [DutySample],
    down_curve: &'a [DutySample],
    threshold: RPM,
) -> Option<&'a DutySample> {
    let mut best: Option<(&'a DutySample, u32)> = None;
    for down in down_curve {
        if down.rpm < threshold {
            continue;
        }
        let Some(up) = up_curve.iter().find(|u| u.duty == down.duty) else {
            continue;
        };
        if up.rpm < threshold {
            continue;
        }
        let mean = u32::midpoint(up.rpm, down.rpm);
        match best {
            None => best = Some((down, mean)),
            Some((_, prev)) if mean < prev => best = Some((down, mean)),
            _ => {}
        }
    }
    best.map(|(s, _)| s)
}

/// Returns `None` when the up-curve never crosses the start threshold.
pub fn derive_scalars(
    up_curve: &[DutySample],
    down_curve: &[DutySample],
) -> Option<DerivedScalars> {
    // Take rpm_max from both curves: a delayed RPM sensor often reads
    // slightly higher on the down-sweep near saturation, and the true
    // peak should reflect that.
    let rpm_max = up_curve
        .iter()
        .chain(down_curve.iter())
        .map(|s| s.rpm)
        .max()
        .unwrap_or(0);
    if rpm_max == 0 || up_curve.is_empty() {
        return None;
    }
    let threshold = start_threshold(rpm_max);
    let jitter = jitter_threshold(rpm_max);

    let min_start_duty = first_monotonic_above_threshold(up_curve, threshold, jitter)?.duty;
    // Lowest-mean over both curves: oscillation at low duty makes the
    // raw down-curve minimum unreliable (the sample lands wherever the
    // oscillation cycle is at sample time). The mean cancels that bias.
    let min_sustain_duty =
        lowest_mean_floor_duty(up_curve, down_curve, threshold).map_or(min_start_duty, |s| s.duty);
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

/// Linearly interpolates RPM at a duty in a variable-spaced curve.
/// Clamps outside the recorded range, returns 0 for an empty curve.
fn rpm_at_device_duty(curve: &[DutySample], device_duty: Duty) -> RPM {
    if curve.is_empty() {
        return 0;
    }
    if device_duty <= curve[0].duty {
        return curve[0].rpm;
    }
    let last = curve[curve.len() - 1];
    if device_duty >= last.duty {
        return last.rpm;
    }
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

/// Picks the displayed true-duty. The device-duty-derived value reflects
/// what the dispatcher actually wrote and is the user-facing source of
/// truth. The RPM-derived value is a fallback only for channels that
/// don't report duty back (some hwmon devices read RPM but not PWM).
///
/// An earlier version cross-checked the two and showed the RPM-derived
/// value on disagreement to surface stuck fans / broken PWM. That
/// triggered false positives on firmware-kick fans, where the fan
/// transiently runs below the calibrated rpm at low duties, and the
/// displayed value would not match what the user just set.
pub fn select_displayed_true_duty(
    device_derived: Option<Duty>,
    rpm_derived: Option<Duty>,
) -> Option<Duty> {
    device_derived.or(rpm_derived)
}

/// Reliability warnings. Mutates `curve_kind` to `Stepped` when the
/// fan looks BIOS-controlled (kills the mapping).
pub fn derive_warnings(
    up_curve: &[DutySample],
    scalars: DerivedScalars,
    curve_kind: &mut CurveKind,
) -> Vec<CalibrationWarning> {
    let mut warnings = Vec::new();
    let effective_span = effective_rpm_span(up_curve, scalars);
    let jitter = jitter_threshold(scalars.rpm_max);
    let not_controllable_limit = jitter.saturating_mul(2);
    if effective_span <= not_controllable_limit {
        warnings.push(CalibrationWarning::NotControllable);
        *curve_kind = CurveKind::Stepped;
        // Skip LimitedRange: NotControllable already implies tiny range.
        return warnings;
    }
    // LimitedRange fires regardless of curve_kind: stepped fans benefit
    // from the explicit "narrow band" cue alongside "mapping disabled".
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

/// Smooth-calibrated channels widen the user-facing range to [0, 100]
/// because the mapping absorbs the device's dead zone. Stepped /
/// uncalibrated channels keep their raw limits (passthrough).
pub fn effective_speed_options(
    raw: &SpeedOptions,
    calibration: Option<&Calibration>,
) -> SpeedOptions {
    let mut so = raw.clone();
    if let Some(cal) = calibration {
        if cal.curve_kind == CurveKind::Smooth {
            so.min_duty = 0;
            so.max_duty = 100;
        }
    }
    so
}

/// Scans the down-curve top-down for the contiguous run where each
/// sample is stable AND above the start threshold. The run's bottom is
/// the threshold (clamped to `min_sustain_duty`). Returns an
/// oscillation band only in the fully-unstable case.
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
    let mut floor: Option<Duty> = None;
    for (sample, &stable) in down_curve.iter().rev().zip(down_stable.iter().rev()) {
        if stable && sample.rpm >= threshold_rpm {
            floor = Some(sample.duty);
            continue;
        }
        break;
    }
    match floor {
        Some(duty) if duty <= min_sustain_duty => (min_sustain_duty, None),
        Some(duty) => (duty, None),
        None => {
            let upper = down_curve.last().map_or(min_sustain_duty, |s| s.duty);
            (min_sustain_duty, Some((min_sustain_duty, upper)))
        }
    }
}

/// RPM span from `min_start_duty` up to peak. Excluding the off-prefix
/// stops a toggle fan from looking like a healthy `0..rpm_max` range.
fn effective_rpm_span(up_curve: &[DutySample], scalars: DerivedScalars) -> RPM {
    let rpm_at_start = up_curve
        .iter()
        .find(|s| s.duty == scalars.min_start_duty)
        .map_or(0, |s| s.rpm);
    scalars.rpm_max.saturating_sub(rpm_at_start)
}

/// Scalars derived from the raw sweep curves. `max_eff_duty` is
/// informational, not a cap on the mapping.
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
    use std::ops::Not;

    const TEST_SAMPLE_COUNT: usize = 21;
    const TEST_STEP: u8 = 5;

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
            was_rpm_only: false,
            timestamp: Local::now(),
        }
    }

    #[test]
    fn classify_smooth_curve_is_smooth() {
        let curve = smooth_curve(2000);
        let kind = classify_curve(&curve, 2000);
        assert_eq!(kind, CurveKind::Smooth);
    }

    #[test]
    fn classify_stepped_curve_is_stepped() {
        let curve = stepped_curve(2000);
        let kind = classify_curve(&curve, 2000);
        assert_eq!(kind, CurveKind::Stepped);
    }

    #[test]
    fn classify_jittery_smooth_curve_stays_smooth() {
        // Jitter tolerance must absorb alternating +/-30 RPM noise.
        let mut curve = smooth_curve(2000);
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
        let cal = make_smooth_calibration();
        let mapped = cal.true_to_device(0).expect("smooth maps");
        assert_eq!(mapped.kick, 0);
        assert_eq!(mapped.sustain, 0);
    }

    #[test]
    fn forward_map_hundred_writes_full_device_duty() {
        // true=100 must reach the curve's top duty; `max_eff_duty` does
        // not cap the mapping, since fans often keep gaining RPM past it.
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
        // Write the sustain duty, sample RPM as hardware would, recover true-duty.
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
        let cal = make_smooth_calibration();
        let result = cal
            .rpm_to_true_duty(cal.rpm_max + 5000)
            .expect("smooth maps");
        assert_eq!(result, 100);
    }

    #[test]
    fn reverse_map_clamps_below_floor() {
        let cal = make_smooth_calibration();
        let result = cal.rpm_to_true_duty(0).expect("smooth maps");
        assert_eq!(result, 0);
    }

    #[test]
    fn stepped_calibration_returns_none_from_mapping() {
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
            was_rpm_only: false,
            timestamp: Local::now(),
        };
        assert!(cal.true_to_device(50).is_none());
        assert!(cal.rpm_to_true_duty(1000).is_none());
        assert!(cal.device_to_true_duty(50).is_none());
    }

    #[test]
    fn device_to_true_duty_zero_maps_to_zero() {
        let cal = make_smooth_calibration();
        assert_eq!(cal.device_to_true_duty(0).expect("smooth maps"), 0);
    }

    #[test]
    fn device_to_true_duty_saturated_returns_full() {
        let cal = make_smooth_calibration();
        let recovered = cal.device_to_true_duty(100).expect("smooth maps");
        assert_eq!(recovered, 100);
    }

    #[test]
    fn device_to_true_duty_round_trips_within_tolerance() {
        // Stable-display path used by the status pipeline.
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
    fn select_prefers_device_when_present() {
        // Device-derived is the source of truth: even if RPM-derived
        // disagrees (e.g., fan transiently below calibrated rpm at low
        // duty), the user sees what the dispatcher wrote.
        assert_eq!(select_displayed_true_duty(Some(50), Some(52)), Some(50));
        assert_eq!(select_displayed_true_duty(Some(50), Some(0)), Some(50));
        assert_eq!(select_displayed_true_duty(Some(100), Some(80)), Some(100));
        assert_eq!(select_displayed_true_duty(Some(40), None), Some(40));
    }

    #[test]
    fn select_falls_back_to_rpm_when_device_absent() {
        // Rpm-only devices (no PWM readback) have None for device-derived.
        assert_eq!(select_displayed_true_duty(None, Some(40)), Some(40));
    }

    #[test]
    fn select_returns_none_when_neither_derived() {
        assert_eq!(select_displayed_true_duty(None, None), None);
    }

    #[test]
    fn derive_warnings_empty_for_healthy_smooth_curve() {
        let up = smooth_curve(2000);
        let down = smooth_curve(2000);
        let scalars = derive_scalars(&up, &down).expect("derives");
        let mut kind = classify_curve(&up, scalars.rpm_max);
        let warnings = derive_warnings(&up, scalars, &mut kind);
        assert!(
            warnings.is_empty(),
            "expected no warnings, got {warnings:?}"
        );
        assert_eq!(kind, CurveKind::Smooth);
    }

    #[test]
    fn derive_warnings_flags_not_controllable_when_span_within_jitter() {
        // Near-constant RPM regardless of duty: NotControllable + force Stepped.
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
        let warnings = derive_warnings(&up, scalars, &mut kind);
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
        // Responding but total RPM range under 500: LimitedRange fires
        // regardless of curve_kind.
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
        let warnings = derive_warnings(&up, scalars, &mut kind);
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
            was_rpm_only: false,
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
                95 | 100 => 2000,
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
            was_rpm_only: false,
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
    fn derive_scalars_rejects_kick_artifact_at_low_duty() {
        // Firmware-kick fan: low-duty samples show transient kick spikes
        // while the real sustain floor sits below. min_sustain_duty must
        // land on the first post-artifact sample so the dispatcher's
        // rpm_floor reflects the real floor and true_duty=1% maps near
        // the bottom of the fan's RPM range.
        let up = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 5, rpm: 1130 }, // kick spike
            DutySample { duty: 10, rpm: 290 },
            DutySample { duty: 15, rpm: 290 },
            DutySample { duty: 20, rpm: 400 },
            DutySample { duty: 30, rpm: 700 },
            DutySample {
                duty: 50,
                rpm: 1200,
            },
            DutySample {
                duty: 70,
                rpm: 1600,
            },
            DutySample {
                duty: 100,
                rpm: 2000,
            },
        ];
        let down = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 5, rpm: 1230 }, // kick spike
            DutySample { duty: 10, rpm: 970 }, // kick decay
            DutySample { duty: 15, rpm: 290 },
            DutySample { duty: 20, rpm: 400 },
            DutySample { duty: 30, rpm: 700 },
            DutySample {
                duty: 50,
                rpm: 1200,
            },
            DutySample {
                duty: 70,
                rpm: 1600,
            },
            DutySample {
                duty: 100,
                rpm: 2000,
            },
        ];
        let scalars = derive_scalars(&up, &down).expect("derives");
        assert_eq!(
            scalars.min_start_duty, 10,
            "first up sample without kick spike is duty 10"
        );
        assert_eq!(
            scalars.min_sustain_duty, 15,
            "first down sample without kick spike or decay is duty 15"
        );
        let floor_rpm = down
            .iter()
            .find(|s| s.duty == scalars.min_sustain_duty)
            .expect("min_sustain sample present")
            .rpm;
        assert_eq!(floor_rpm, 290, "rpm_floor must be the real floor, not kick");
    }

    #[test]
    fn derive_scalars_min_sustain_uses_lowest_mean_across_oscillation() {
        // Firmware-kick fan oscillates at duty 4 so the down-sweep
        // happens to catch the valley (270) while the up-sweep catches
        // a higher point (310). The raw "lowest down sample" would pin
        // min_sustain to duty 4, but the fan isn't actually stable
        // there. At duty 6 both curves agree at the true floor, giving
        // the lowest mean and the correct min_sustain_duty.
        let up = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 4, rpm: 310 }, // oscillation peak
            DutySample { duty: 6, rpm: 280 },
            DutySample { duty: 8, rpm: 280 },
            DutySample { duty: 10, rpm: 290 },
            DutySample { duty: 30, rpm: 700 },
            DutySample {
                duty: 100,
                rpm: 2000,
            },
        ];
        let down = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 4, rpm: 270 }, // oscillation valley
            DutySample { duty: 6, rpm: 270 },
            DutySample { duty: 8, rpm: 280 },
            DutySample { duty: 10, rpm: 290 },
            DutySample { duty: 30, rpm: 700 },
            DutySample {
                duty: 100,
                rpm: 2000,
            },
        ];
        let scalars = derive_scalars(&up, &down).expect("derives");
        // Means: duty 4 = 290, duty 6 = 275, duty 8 = 280, duty 10 = 290.
        // Lowest is at duty 6.
        assert_eq!(scalars.min_sustain_duty, 6);
    }

    fn artifact_calibration() -> Calibration {
        // Models the firmware-kick fan from `/etc/coolercontrol/calibrations.json`
        // device e0d1df00: kick artifacts at duty 2 and 4 on both curves,
        // real floor at duty 6, monotonic from there to duty 100.
        let up = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 2, rpm: 1139 },
            DutySample { duty: 4, rpm: 1222 },
            DutySample { duty: 6, rpm: 335 },
            DutySample { duty: 10, rpm: 320 },
            DutySample { duty: 20, rpm: 427 },
            DutySample { duty: 30, rpm: 662 },
            DutySample { duty: 40, rpm: 883 },
            DutySample { duty: 50, rpm: 997 },
            DutySample {
                duty: 70,
                rpm: 1466,
            },
            DutySample {
                duty: 100,
                rpm: 1914,
            },
        ];
        let down = vec![
            DutySample { duty: 0, rpm: 0 },
            DutySample { duty: 2, rpm: 564 },
            DutySample { duty: 4, rpm: 1012 },
            DutySample { duty: 6, rpm: 311 },
            DutySample { duty: 10, rpm: 328 },
            DutySample { duty: 20, rpm: 554 },
            DutySample { duty: 30, rpm: 774 },
            DutySample {
                duty: 40,
                rpm: 1004,
            },
            DutySample {
                duty: 50,
                rpm: 1250,
            },
            DutySample {
                duty: 70,
                rpm: 1581,
            },
            DutySample {
                duty: 100,
                rpm: 1922,
            },
        ];
        Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 1125,
            min_start_duty: 6,
            min_sustain_duty: 6,
            min_stable_duty: 6,
            max_eff_duty: 100,
            rpm_max: 1930,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            was_rpm_only: false,
            timestamp: Local::now(),
        }
    }

    #[test]
    fn true_to_device_skips_low_duty_kick_artifacts() {
        // Without the kick-zone filter, all true_duty values from ~1 to
        // ~43 would map to the same device duty (the min_stable clamp)
        // because duty_for_rpm hits the artifact samples first. With
        // the filter, the lookup escapes the kick zone and the mapping
        // is monotonically increasing across the whole range.
        let cal = artifact_calibration();
        let mut last_sustain: Duty = 0;
        let mut sustains: Vec<Duty> = Vec::new();
        for t in [1u8, 10, 30, 50, 100] {
            let mapped = cal.true_to_device(t).expect("smooth maps");
            sustains.push(mapped.sustain);
            assert!(
                mapped.sustain >= last_sustain,
                "sustain non-monotonic across artifacts at t={t}: \
                 prev={last_sustain} now={} (all: {sustains:?})",
                mapped.sustain
            );
            last_sustain = mapped.sustain;
        }
        // sustains[0] is the floor end (low true_duty). It must escape
        // the kick zone (> min_sustain_duty=6) AND stay near the
        // floor (well below 100).
        assert!(
            sustains[0] > cal.min_sustain_duty,
            "true_duty=1 must map above the floor: got {}",
            sustains[0]
        );
        assert!(
            sustains[0] < 30,
            "true_duty=1 must map near the floor, not mid-range: got {}",
            sustains[0]
        );
        // sustains[last] at true_duty=100 must reach near saturation.
        assert!(
            *sustains.last().unwrap() >= 90,
            "true_duty=100 must reach near-100% device duty: got {sustains:?}"
        );
    }

    #[test]
    fn true_to_device_hundred_writes_full_device_duty_when_max_lands_at_lower_duty() {
        // rpm_max captured at duty 95 (down sample 1985) while duty 100
        // shows 1981. Without the special case, true_duty=100 would map
        // to device 95 (where rpm_max is); with it, true_duty=100 always
        // writes 100% so the fan reaches its actual hardware max.
        let mut cal = artifact_calibration();
        // Modify down_curve to put rpm_max at duty 95, slight dip at 100.
        let len = cal.down_curve.len();
        cal.down_curve[len - 2] = DutySample {
            duty: 95,
            rpm: 1930,
        };
        cal.down_curve[len - 1] = DutySample {
            duty: 100,
            rpm: 1922,
        };
        let mapped = cal.true_to_device(100).expect("smooth maps");
        assert_eq!(mapped.sustain, 100);
        assert_eq!(mapped.kick, 100);
    }

    #[test]
    fn rpm_to_true_duty_distinguishes_off_from_running_at_floor() {
        // rpm=0 is fan-off; any non-zero rpm at or below the calibrated
        // floor is "fan running at its minimum" and maps to 1 so the
        // UI can tell a stopped fan from one ticking over.
        let cal = artifact_calibration();
        assert_eq!(cal.rpm_to_true_duty(0), Some(0));
        // The floor for this cal is down_curve at duty 6 = 311 rpm.
        assert_eq!(cal.rpm_to_true_duty(311), Some(1));
        // Just below the floor: still considered "at floor" (1%).
        assert_eq!(cal.rpm_to_true_duty(280), Some(1));
    }

    #[test]
    fn device_to_true_duty_ignores_kick_artifacts_below_floor() {
        // The down-sweep lookup must skip the kick-artifact samples
        // below min_sustain_duty; otherwise device_to_true_duty at a
        // mid-range device duty would interpolate over the spikes and
        // return a nonsense true-duty.
        let cal = artifact_calibration();
        // device_duty 0 is a true off and must always show 0%.
        assert_eq!(cal.device_to_true_duty(0), Some(0));
        // At the floor duty the fan is running at minimum: display 1%
        // (distinguishes spinning at floor from fully off).
        assert_eq!(cal.device_to_true_duty(6), Some(1));
        // Mid-range: device_duty 50 has down_curve rpm 1250. With the
        // filter the lookup gets the correct rpm, and the inverse map
        // produces a reasonable mid-range true-duty (50-60%).
        let mid = cal
            .device_to_true_duty(50)
            .expect("device_to_true_duty maps");
        assert!(
            (50..=70).contains(&mid),
            "device_duty 50 should map to ~58% true-duty, got {mid}"
        );
    }

    #[test]
    fn derive_scalars_takes_rpm_max_from_both_curves() {
        // Down-sweep often reads slightly higher near saturation when the
        // RPM sensor lags, so rpm_max must reflect the union of both
        // sweeps rather than just up.
        let up = smooth_curve(1900);
        let down: Vec<DutySample> = up
            .iter()
            .map(|s| DutySample {
                duty: s.duty,
                rpm: if s.duty == 100 { 1925 } else { s.rpm },
            })
            .collect();
        let scalars = derive_scalars(&up, &down).expect("derives");
        assert_eq!(scalars.rpm_max, 1925);
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
            .map(|s| unstable_duties.contains(&s.duty).not())
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
        // Goal: rule (B). Descend only while EVERY sample above is
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

    // --- effective_speed_options -----------------------------------

    fn raw_speed_options(min_duty: Duty, max_duty: Duty) -> SpeedOptions {
        SpeedOptions {
            min_duty,
            max_duty,
            fixed_enabled: true,
            extension: None,
        }
    }

    #[test]
    fn effective_speed_options_returns_raw_when_no_calibration() {
        // Goal: an uncalibrated channel must keep its device-derived
        // limits exactly (the dispatcher passthrough case) and
        // any deviation here would silently change the manual-duty
        // slider bounds for every channel that hasn't been calibrated.
        let raw = raw_speed_options(15, 80);
        let out = effective_speed_options(&raw, None);
        assert_eq!(out.min_duty, 15);
        assert_eq!(out.max_duty, 80);
        assert_eq!(out.fixed_enabled, raw.fixed_enabled);
        assert_eq!(out.extension, raw.extension);
    }

    #[test]
    fn effective_speed_options_widens_both_ends_on_smooth_calibration() {
        // Goal: a Smooth calibration must widen the user-facing range
        // to [0, 100] regardless of where the raw limits sit. Both
        // ends matter: AMDGPU frequently exposes a non-zero min_duty
        // (fan_curve_info.speed_range.start), and some configs cap
        // max_duty below 100 as well. Confirms the helper does not
        // simply zero out min while leaving a too-low max in place.
        let raw = raw_speed_options(15, 80);
        let cal = make_smooth_calibration();
        let out = effective_speed_options(&raw, Some(&cal));
        assert_eq!(out.min_duty, 0);
        assert_eq!(out.max_duty, 100);
        assert_eq!(out.fixed_enabled, raw.fixed_enabled);
    }

    #[test]
    fn effective_speed_options_keeps_raw_on_stepped_calibration() {
        // Goal: a Stepped calibration means the dispatcher passes
        // device-duty through unchanged (`true_to_device` returns None
        // on Stepped). The user-facing range therefore stays at the
        // device limits. Widening to [0, 100] would falsely advertise
        // a range the dispatcher cannot actually deliver.
        let raw = raw_speed_options(15, 100);
        let mut cal = make_smooth_calibration();
        cal.curve_kind = CurveKind::Stepped;
        let out = effective_speed_options(&raw, Some(&cal));
        assert_eq!(out.min_duty, 15);
        assert_eq!(out.max_duty, 100);
    }

    #[test]
    fn effective_speed_options_preserves_fixed_enabled_and_extension() {
        // Goal: the helper only widens the duty range; it must not
        // touch the orthogonal fields (`fixed_enabled` / `extension`).
        // A bug here would, for example, lose the AmdRdnaGpu extension
        // marker on a calibrated AMDGPU channel and break HW fan-curve
        // routing in the engine.
        let raw = SpeedOptions {
            min_duty: 15,
            max_duty: 100,
            fixed_enabled: true,
            extension: Some(crate::device::ChannelExtensionNames::AmdRdnaGpu),
        };
        let cal = make_smooth_calibration();
        let out = effective_speed_options(&raw, Some(&cal));
        assert!(out.fixed_enabled);
        assert_eq!(
            out.extension,
            Some(crate::device::ChannelExtensionNames::AmdRdnaGpu)
        );
    }
}
