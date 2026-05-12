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

/// Duty resolution used by the diagnosis sweep and the lookup curves.
///
/// Changes here break backwards compatibility with persisted JSON files.
pub const DUTY_STEP_PERCENT: u8 = 5;

/// Number of samples in each direction. Equal to `(100 / DUTY_STEP_PERCENT) + 1`.
pub const SAMPLE_COUNT: usize = 21;

const _: () = assert!((DUTY_STEP_PERCENT as usize) * (SAMPLE_COUNT - 1) == 100);
const _: () = assert!(SAMPLE_COUNT <= u8::MAX as usize);

/// Absolute RPM floor below which we treat readings as noise.
const RPM_START_THRESHOLD_ABSOLUTE: RPM = 50;

/// Fraction of `rpm_max` (in percent) used as a relative noise floor.
const RPM_START_THRESHOLD_FRACTION_PERCENT: u32 = 5;

/// Absolute RPM jitter tolerance used during step-curve classification.
const RPM_JITTER_ABSOLUTE: RPM = 50;

/// Fraction of `rpm_max` (in percent) used as the relative jitter tolerance.
const RPM_JITTER_FRACTION_PERCENT: u32 = 3;

/// The duty/RPM curve and derived working-range scalars for one channel.
///
/// Always represents a successful diagnosis. A fan that never spins is
/// rejected by the diagnoser before a `Calibration` is constructed.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct Calibration {
    /// RPM readings at duty 0, 5, ..., 100 while sweeping the duty upward.
    pub up_curve: [RPM; SAMPLE_COUNT],

    /// RPM readings at duty 0, 5, ..., 100 while sweeping the duty downward.
    pub down_curve: [RPM; SAMPLE_COUNT],

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

    /// RPM at `min_start_duty` along the up-curve.
    fn rpm_floor(&self) -> RPM {
        let idx = usize::from(self.min_start_duty / DUTY_STEP_PERCENT);
        assert!(idx < SAMPLE_COUNT);
        self.up_curve[idx]
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

/// Binary-ish scan of the curve for the lowest duty where RPM >= target,
/// linearly interpolating between the surrounding samples.
fn duty_for_rpm(curve: &[RPM; SAMPLE_COUNT], rpm: RPM) -> Duty {
    let mut idx = SAMPLE_COUNT - 1;
    for (i, &v) in curve.iter().enumerate() {
        if v >= rpm {
            idx = i;
            break;
        }
    }
    if idx == 0 {
        return 0;
    }
    let lo_rpm = curve[idx - 1];
    let hi_rpm = curve[idx];
    let lo_duty = index_to_duty(idx - 1);
    let hi_duty = index_to_duty(idx);
    if hi_rpm <= lo_rpm {
        return lo_duty;
    }
    if rpm <= lo_rpm {
        return lo_duty;
    }
    let numerator = (rpm - lo_rpm) * u32::from(hi_duty - lo_duty);
    let denominator = hi_rpm - lo_rpm;
    let frac = numerator / denominator;
    // frac is bounded by (hi_duty - lo_duty), which is at most DUTY_STEP_PERCENT.
    lo_duty + u8::try_from(frac).unwrap_or(hi_duty - lo_duty)
}

/// Classify the up-curve as `Smooth` or `Stepped`.
///
/// Counts inter-sample increases that exceed the jitter tolerance.
/// Below half the maximum possible transitions is `Stepped`; this catches
/// devices like `ThinkPad` fans and step-pumps that only change RPM at a
/// few discrete duty values.
pub fn classify_curve(up_curve: &[RPM; SAMPLE_COUNT], rpm_max: RPM) -> CurveKind {
    assert!(rpm_max > 0);

    let jitter = jitter_threshold(rpm_max);
    let mut transitions: u32 = 0;
    for i in 0..(SAMPLE_COUNT - 1) {
        if up_curve[i + 1] > up_curve[i].saturating_add(jitter) {
            transitions += 1;
        }
    }
    let half = u32::try_from((SAMPLE_COUNT - 1) / 2).unwrap_or(0);
    if transitions < half {
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
    up_curve: &[RPM; SAMPLE_COUNT],
    down_curve: &[RPM; SAMPLE_COUNT],
) -> Option<DerivedScalars> {
    let rpm_max = *up_curve.iter().max().unwrap_or(&0);
    if rpm_max == 0 {
        return None;
    }
    let threshold = start_threshold(rpm_max);
    let jitter = jitter_threshold(rpm_max);

    let start_idx = up_curve.iter().position(|&v| v >= threshold)?;
    let sustain_idx = down_curve
        .iter()
        .position(|&v| v >= threshold)
        .unwrap_or(start_idx);
    let plateau_target = rpm_max.saturating_sub(jitter);
    let plateau_idx = up_curve
        .iter()
        .position(|&v| v >= plateau_target)
        .unwrap_or(SAMPLE_COUNT - 1);

    Some(DerivedScalars {
        min_start_duty: index_to_duty(start_idx),
        min_sustain_duty: index_to_duty(sustain_idx),
        max_eff_duty: index_to_duty(plateau_idx),
        rpm_max,
    })
}

fn index_to_duty(idx: usize) -> Duty {
    assert!(idx < SAMPLE_COUNT);
    // SAMPLE_COUNT <= u8::MAX is asserted at compile time, so the
    // conversion below is total.
    let idx_u8 = u8::try_from(idx).expect("SAMPLE_COUNT <= u8::MAX (const-asserted)");
    idx_u8 * DUTY_STEP_PERCENT
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

    /// Build a synthetic smooth up-curve: RPM ramps linearly from 0 to `rpm_top`.
    fn smooth_curve(rpm_top: RPM) -> [RPM; SAMPLE_COUNT] {
        let mut curve = [0u32; SAMPLE_COUNT];
        let denominator = u32::try_from(SAMPLE_COUNT - 1).expect("SAMPLE_COUNT - 1 fits in u32");
        for (i, slot) in curve.iter_mut().enumerate() {
            let frac = u32::try_from(i).expect("SAMPLE_COUNT fits in u32");
            *slot = (rpm_top * frac) / denominator;
        }
        curve
    }

    /// Build a stepped curve: three RPM plateaus at low/middle/high duty.
    fn stepped_curve(rpm_top: RPM) -> [RPM; SAMPLE_COUNT] {
        let mut curve = [0u32; SAMPLE_COUNT];
        for (i, slot) in curve.iter_mut().enumerate() {
            *slot = match i {
                0..=4 => 0,
                5..=12 => rpm_top / 2,
                _ => rpm_top,
            };
        }
        curve
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
        for (i, slot) in curve.iter_mut().enumerate() {
            if i.is_multiple_of(2) {
                *slot = slot.saturating_add(30);
            } else {
                *slot = slot.saturating_sub(30);
            }
        }
        let rpm_max = *curve.iter().max().unwrap();
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

    /// Linearly interpolate the RPM a real fan would produce at an arbitrary
    /// device-duty, given its sampled curve. Models how the hardware behaves
    /// between sample points; the bare `curve[idx]` lookup only samples the
    /// grid, which is not what reaches the status pipeline.
    fn simulated_rpm_at(curve: &[RPM; SAMPLE_COUNT], device_duty: Duty) -> RPM {
        let lo_idx = usize::from(device_duty / DUTY_STEP_PERCENT);
        let hi_idx = (lo_idx + 1).min(SAMPLE_COUNT - 1);
        let frac = u32::from(device_duty % DUTY_STEP_PERCENT);
        let lo_rpm = curve[lo_idx];
        let hi_rpm = curve[hi_idx];
        let delta = hi_rpm.saturating_sub(lo_rpm);
        lo_rpm + delta * frac / u32::from(DUTY_STEP_PERCENT)
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
            let rpm = simulated_rpm_at(&cal.down_curve, mapped.sustain);
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
    }

    #[test]
    fn derive_scalars_rejects_zero_curve() {
        // Goal: a fan that never produces RPM must surface as None so the
        // diagnoser fails with fan_unresponsive instead of saving garbage.
        let zero = [0u32; SAMPLE_COUNT];
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
        let curve = [1500u32; SAMPLE_COUNT];
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
