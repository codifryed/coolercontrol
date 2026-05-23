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

//! Rewrites each pushed status's `duty` into true-duty space. Runs
//! inside `set_status`'s `Arc::make_mut`, avoiding a second pass that
//! would deep-clone the history when a reader holds the Arc.

use std::rc::Rc;

use super::state::FanStateMap;
use super::store::CalibrationStore;
use super::ChannelKey;
use crate::device::{Device, DeviceUID, Duty, Status, StatusAugmenter};

pub struct CalibrationStatusAugmenter {
    calibration_store: Rc<CalibrationStore>,
    fan_state_map: Rc<FanStateMap>,
}

impl CalibrationStatusAugmenter {
    pub fn new(calibration_store: Rc<CalibrationStore>, fan_state_map: Rc<FanStateMap>) -> Self {
        Self {
            calibration_store,
            fan_state_map,
        }
    }
}

impl StatusAugmenter for CalibrationStatusAugmenter {
    fn augment(&self, status: &mut Status, device_uid: &DeviceUID) {
        for channel in &mut status.channels {
            let key: ChannelKey = (device_uid.clone(), channel.name.clone());
            // Sweep is writing raw duty; don't mask it with the prior mapping.
            if self.fan_state_map.is_under_diagnosis(&key) {
                continue;
            }
            // Three-tier resolution:
            // 1. Hybrid cache: if the dispatcher recently commanded a
            //    true-duty AND the hardware's read-back device-duty's
            //    preimage still contains that commanded value, display
            //    the commanded value exactly. This closes the cross-fan
            //    drift the stateless midpoint reverse cannot remove.
            // 2. Midpoint reverse via `device_to_true_duty`: hardware's
            //    device-duty alone, used for un-cached channels (fresh
            //    startup) and when hardware has diverged from command.
            // 3. RPM-derived: fallback for rpm-only devices with no PWM
            //    readback.
            let device_duty = channel.duty.map(clamp_f64_to_duty);
            let commanded = self.fan_state_map.commanded_true_duty(&key);
            let cache_hit = match (device_duty, commanded) {
                (Some(dut), Some(cmd))
                    if self
                        .calibration_store
                        .preimage_contains_true_duty(&key, dut, cmd) =>
                {
                    Some(cmd)
                }
                _ => None,
            };
            let device_derived =
                device_duty.and_then(|d| self.calibration_store.device_to_true_duty(&key, d));
            let rpm_derived = channel
                .rpm
                .and_then(|r| self.calibration_store.rpm_to_true_duty(&key, r));
            if let Some(displayed) = cache_hit.or(device_derived).or(rpm_derived) {
                channel.duty = Some(f64::from(displayed));
            }
        }
    }
}

/// Round and clamp an `f64` duty into 0..=100 percent. NaN coerces to 0.
fn clamp_f64_to_duty(value: f64) -> Duty {
    if value.is_nan() {
        return 0;
    }
    let rounded = value.round().clamp(0.0, 100.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let as_u8 = rounded as Duty;
    as_u8
}

/// Install the augmenter on every known device. Devices added later
/// (e.g. custom sensors) carry no fan channels, so they don't need it.
pub fn install_augmenter_on_all_devices(
    all_devices: &std::collections::HashMap<DeviceUID, Rc<std::cell::RefCell<Device>>>,
    augmenter: &Rc<dyn StatusAugmenter>,
) {
    for device_lock in all_devices.values() {
        device_lock
            .borrow_mut()
            .set_status_augmenter(Rc::clone(augmenter));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calibration::curve::{Calibration, CurveKind, DutySample};
    use crate::calibration::state::{ChannelEntry, FanState};
    use crate::device::{ChannelStatus, RPM};
    use chrono::Local;

    fn cal_with_samples(samples: &[(u8, RPM)]) -> Calibration {
        let pairs: Vec<DutySample> = samples
            .iter()
            .map(|&(d, r)| DutySample { duty: d, rpm: r })
            .collect();
        let up = pairs.clone();
        let down = pairs;
        let scalars = crate::calibration::curve::derive_scalars(&up, &down).expect("derives");
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
            kick_boost_override: None,
            kick_duration_override_ms: None,
            walk_after_kick_override: None,
            timestamp: Local::now(),
        }
    }

    fn status_with_channels(channels: Vec<(&str, f64)>) -> Status {
        Status {
            timestamp: Local::now(),
            temps: Vec::new(),
            channels: channels
                .into_iter()
                .map(|(name, duty)| ChannelStatus {
                    name: name.to_string(),
                    rpm: None,
                    duty: Some(duty),
                    freq: None,
                    watts: None,
                    pwm_mode: None,
                })
                .collect(),
        }
    }

    #[test]
    fn hybrid_cache_displays_commanded_value_for_same_profile_across_curves() {
        // Goal: the user's exact bug. Two fans on the same Profile target
        // (commanded true_duty=13) but with different calibration curves.
        // The dispatcher writes a different device-duty to each (curve
        // truncation). The augmenter, with the cached commanded value in
        // each fan's FanStateMap entry, must rewrite both to 13.
        let store = Rc::new(CalibrationStore::empty());
        let state = Rc::new(FanStateMap::new());
        let device = "dev-a".to_string();
        let key_gentle = (device.clone(), "fan-gentle".to_string());
        let key_steep = (device.clone(), "fan-steep".to_string());
        store.insert_unsaved(
            key_gentle.clone(),
            cal_with_samples(&[
                (0, 0),
                (5, 100),
                (10, 200),
                (15, 300),
                (20, 400),
                (30, 600),
                (50, 1000),
                (75, 1500),
                (100, 2000),
            ]),
        );
        store.insert_unsaved(
            key_steep.clone(),
            cal_with_samples(&[
                (0, 0),
                (5, 100),
                (10, 200),
                (15, 400),
                (20, 500),
                (30, 700),
                (50, 1100),
                (75, 1600),
                (100, 2000),
            ]),
        );

        // Compute each fan's forward-truncated device-duty for commanded=13.
        // The augmenter receives these as the hardware read-back. The
        // dispatcher would have recorded commanded=13 in each entry.
        let gentle_dev = store
            .get(&key_gentle)
            .expect("inserted")
            .true_to_device(13)
            .expect("smooth maps")
            .sustain;
        let steep_dev = store
            .get(&key_steep)
            .expect("inserted")
            .true_to_device(13)
            .expect("smooth maps")
            .sustain;
        // Pre-populate the state map as if the dispatcher had just written.
        state.replace(
            key_gentle.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: Some(13),
            },
        );
        state.replace(
            key_steep.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: Some(13),
            },
        );

        let mut status = status_with_channels(vec![
            ("fan-gentle", f64::from(gentle_dev)),
            ("fan-steep", f64::from(steep_dev)),
        ]);
        let augmenter = CalibrationStatusAugmenter::new(Rc::clone(&store), Rc::clone(&state));
        augmenter.augment(&mut status, &device);

        assert_eq!(status.channels[0].duty, Some(13.0));
        assert_eq!(status.channels[1].duty, Some(13.0));
    }

    #[test]
    fn hybrid_falls_back_to_midpoint_when_no_cached_command() {
        // Goal: channels never written by the daemon (no entry in
        // FanStateMap, or commanded_true_duty=None) must still produce
        // a sensible displayed duty via the existing midpoint reverse.
        let store = Rc::new(CalibrationStore::empty());
        let state = Rc::new(FanStateMap::new());
        let device = "dev-a".to_string();
        let key = (device.clone(), "fan".to_string());
        let cal = cal_with_samples(&[
            (0, 0),
            (5, 100),
            (10, 200),
            (15, 300),
            (20, 400),
            (30, 600),
            (50, 1000),
            (75, 1500),
            (100, 2000),
        ]);
        let device_dut = cal.true_to_device(50).expect("smooth").sustain;
        let expected = cal.device_to_true_duty(device_dut).expect("smooth");
        store.insert_unsaved(key.clone(), cal);

        let mut status = status_with_channels(vec![("fan", f64::from(device_dut))]);
        let augmenter = CalibrationStatusAugmenter::new(Rc::clone(&store), Rc::clone(&state));
        augmenter.augment(&mut status, &device);

        assert_eq!(status.channels[0].duty, Some(f64::from(expected)));
    }

    #[test]
    fn hybrid_falls_back_when_hardware_diverged_from_command() {
        // Goal: when hardware reads a device duty whose preimage does
        // NOT contain the cached commanded value (the user wrote PWM
        // externally, or the fan controller overrode our value), the
        // augmenter must fall back to midpoint reverse so the user sees
        // the hardware truth rather than a stale command.
        let store = Rc::new(CalibrationStore::empty());
        let state = Rc::new(FanStateMap::new());
        let device = "dev-a".to_string();
        let key = (device.clone(), "fan".to_string());
        let cal = cal_with_samples(&[
            (0, 0),
            (5, 100),
            (10, 200),
            (15, 300),
            (20, 400),
            (30, 600),
            (50, 1000),
            (75, 1500),
            (100, 2000),
        ]);
        let commanded = 13_u8;
        // External write pushed device way above what we asked for.
        let hijacked_dev = cal.true_to_device(80).expect("smooth").sustain;
        let expected = cal.device_to_true_duty(hijacked_dev).expect("smooth");
        store.insert_unsaved(key.clone(), cal);
        state.replace(
            key.clone(),
            ChannelEntry {
                state: FanState::On,
                under_diagnosis: false,
                commanded_true_duty: Some(commanded),
            },
        );

        let mut status = status_with_channels(vec![("fan", f64::from(hijacked_dev))]);
        let augmenter = CalibrationStatusAugmenter::new(Rc::clone(&store), Rc::clone(&state));
        augmenter.augment(&mut status, &device);

        // Not the stale commanded value; the hardware-derived midpoint.
        assert_eq!(status.channels[0].duty, Some(f64::from(expected)));
        assert_ne!(status.channels[0].duty, Some(f64::from(commanded)));
    }
}
