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

    /// Lowest device-duty (multiple of `DUTY_STEP_PERCENT`) at which
    /// `up_curve` reaches the saturation plateau.
    pub max_eff_duty: Duty,

    /// Peak RPM observed across the sweep. Always positive.
    pub rpm_max: RPM,

    /// Whether the curve is smooth (mapping active) or stepped (passthrough).
    pub curve_kind: CurveKind,

    /// Wall-clock time when the diagnosis that produced this calibration finished.
    pub timestamp: DateTime<Local>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum CurveKind {
    Smooth,
    Stepped,
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
        let kick = duty_for_rpm(&self.up_curve, target_rpm);
        let sustain = duty_for_rpm(&self.down_curve, target_rpm);
        MappedDuty { kick, sustain }
    }

    /// Internal smooth-path reverse map. Caller guarantees `Smooth`.
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
        let result = (above * 100 + range / 2) / range;
        u8::try_from(result.min(100)).unwrap_or(100)
    }

    /// RPM at `min_start_duty` along the up-curve. Looks up the
    /// sample with the matching duty exactly; on a well-formed
    /// calibration this is always present (the sweep records the
    /// sample that crossed the start threshold at this duty value).
    fn rpm_floor(&self) -> RPM {
        self.up_curve
            .iter()
            .find(|s| s.duty == self.min_start_duty)
            .map_or_else(
                || rpm_at_device_duty(&self.up_curve, self.min_start_duty),
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

/// Scalars derived from the raw sweep curves.
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
            max_eff_duty: scalars.max_eff_duty,
            rpm_max: scalars.rpm_max,
            curve_kind: CurveKind::Smooth,
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
    fn forward_map_hundred_is_at_or_below_max_eff() {
        // Goal: true-duty 100 must map to a device duty <= max_eff_duty
        // (we don't waste duty beyond saturation).
        let cal = make_smooth_calibration();
        let mapped = cal.true_to_device(100).expect("smooth maps");
        assert!(mapped.kick <= cal.max_eff_duty);
        assert!(mapped.sustain <= cal.max_eff_duty);
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
            max_eff_duty: scalars.max_eff_duty,
            rpm_max: scalars.rpm_max,
            curve_kind: CurveKind::Stepped,
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
}
