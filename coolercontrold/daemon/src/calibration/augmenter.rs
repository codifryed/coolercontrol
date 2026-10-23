// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rewrites each stored channel's `duty` into true-duty space, as the device stores it.

use std::cell::RefCell;
use std::rc::Rc;

use super::state::FanStateMap;
use super::store::CalibrationStore;
use super::ChannelKey;
use crate::device::{ChannelSample, Device, DeviceUID, Duty, StatusAugmenter};

pub struct CalibrationStatusAugmenter {
    calibration_store: Rc<CalibrationStore>,
    fan_state_map: Rc<FanStateMap>,
    /// Lookup key reused for every channel, so a tick allocates nothing once
    /// the longest device UID and channel name have been seen.
    key: RefCell<ChannelKey>,
}

impl CalibrationStatusAugmenter {
    pub fn new(calibration_store: Rc<CalibrationStore>, fan_state_map: Rc<FanStateMap>) -> Self {
        Self {
            calibration_store,
            fan_state_map,
            key: RefCell::new((String::new(), String::new())),
        }
    }
}

impl StatusAugmenter for CalibrationStatusAugmenter {
    fn augment_channel(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        channel: &mut ChannelSample,
    ) {
        let mut key = self.key.borrow_mut();
        if key.0 != *device_uid {
            key.0.clear();
            key.0.push_str(device_uid);
        }
        key.1.clear();
        key.1.push_str(channel_name);
        let key: &ChannelKey = &key;
        // Sweep is writing raw duty; don't mask it with the prior mapping.
        if self.fan_state_map.is_under_diagnosis(key) {
            return;
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
        //    readback, and for a sub-floor device-duty whose rpm says the
        //    fan is turning anyway.
        let device_duty = channel.duty.map(clamp_f64_to_duty);
        let commanded = self.fan_state_map.commanded_true_duty(key);
        let cache_hit = match (device_duty, commanded) {
            (Some(dut), Some(cmd))
                if self
                    .calibration_store
                    .preimage_contains_true_duty(key, dut, cmd) =>
            {
                Some(cmd)
            }
            _ => None,
        };
        let device_derived =
            device_duty.and_then(|d| self.calibration_store.device_to_true_duty(key, d));
        let rpm_derived = channel
            .rpm
            .and_then(|r| self.calibration_store.rpm_to_true_duty(key, r));
        let derived = Self::prefer_rpm_over_stall(device_derived, rpm_derived);
        if let Some(displayed) = cache_hit.or(derived) {
            channel.duty = Some(f64::from(displayed));
        }
    }
}

impl CalibrationStatusAugmenter {
    /// Resolves the device-duty and rpm answers against each other. A device-duty
    /// below the sustain floor reverses to 0, which is honest for a stalled fan but
    /// wrong for one the firmware is still turning below the floor we measured. A
    /// non-zero rpm proves it is turning, so that evidence wins.
    fn prefer_rpm_over_stall(
        device_derived: Option<Duty>,
        rpm_derived: Option<Duty>,
    ) -> Option<Duty> {
        match (device_derived, rpm_derived) {
            (Some(0), Some(rpm_duty)) if rpm_duty > 0 => Some(rpm_duty),
            (Some(duty), _) => Some(duty),
            (None, rpm_duty) => rpm_duty,
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
    use crate::device::{ChannelStatus, Status, RPM};
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

    fn status_with_rpm_channel(name: &str, duty: f64, rpm: RPM) -> Status {
        let mut status = status_with_channels(vec![(name, duty)]);
        status.channels[0].rpm = Some(rpm);
        status
    }

    #[test]
    fn prefer_rpm_over_stall_lets_a_turning_fan_beat_a_sub_floor_zero() {
        // Goal: pin the resolution order. Only the stall answer defers to rpm;
        // every other combination keeps the device-duty answer, which is the
        // more accurate of the two whenever it is not claiming the fan is off.
        assert_eq!(
            CalibrationStatusAugmenter::prefer_rpm_over_stall(Some(0), Some(3)),
            Some(3),
            "a non-zero rpm proves the fan is turning"
        );
        assert_eq!(
            CalibrationStatusAugmenter::prefer_rpm_over_stall(Some(0), Some(0)),
            Some(0),
            "both agree the fan is off"
        );
        assert_eq!(
            CalibrationStatusAugmenter::prefer_rpm_over_stall(Some(0), None),
            Some(0),
            "no rpm evidence leaves the stall answer standing"
        );
        assert_eq!(
            CalibrationStatusAugmenter::prefer_rpm_over_stall(Some(40), Some(3)),
            Some(40),
            "an above-floor device duty is never overridden"
        );
        assert_eq!(
            CalibrationStatusAugmenter::prefer_rpm_over_stall(None, Some(3)),
            Some(3),
            "rpm-only devices still resolve"
        );
        assert_eq!(
            CalibrationStatusAugmenter::prefer_rpm_over_stall(None, None),
            None
        );
    }

    #[test]
    fn augment_reports_a_spinning_fan_below_the_sustain_floor() {
        // Goal: a calibrated fan the firmware drives below the measured sustain
        // floor reads a sub-floor pwm, which reverses to 0. Its rpm says it is
        // turning, so the displayed duty must not claim the fan is stopped.
        let store = Rc::new(CalibrationStore::empty());
        let state = Rc::new(FanStateMap::new());
        let device = "dev-a".to_string();
        let key = (device.clone(), "fan1".to_string());
        // Nothing turns until 20%, so min_sustain_duty lands above the pwm below.
        store.insert_unsaved(
            key.clone(),
            cal_with_samples(&[
                (0, 0),
                (5, 0),
                (10, 0),
                (15, 0),
                (20, 400),
                (40, 800),
                (60, 1200),
                (80, 1600),
                (100, 2000),
            ]),
        );
        let augmenter = CalibrationStatusAugmenter::new(Rc::clone(&store), Rc::clone(&state));
        let mut status = status_with_rpm_channel("fan1", 12.0, 300);
        augmenter.augment(&mut status, &device);
        assert!(
            status.channels[0].duty.unwrap() > 0.0,
            "a fan turning at 300 rpm must not display as off"
        );
    }

    #[test]
    fn augment_reports_zero_for_a_stopped_fan_below_the_sustain_floor() {
        // Goal: negative space for the test above. With the fan actually stopped,
        // the sub-floor reverse stays 0 rather than inventing a floor reading.
        let store = Rc::new(CalibrationStore::empty());
        let state = Rc::new(FanStateMap::new());
        let device = "dev-a".to_string();
        let key = (device.clone(), "fan1".to_string());
        store.insert_unsaved(
            key.clone(),
            cal_with_samples(&[
                (0, 0),
                (5, 0),
                (10, 0),
                (15, 0),
                (20, 400),
                (40, 800),
                (60, 1200),
                (80, 1600),
                (100, 2000),
            ]),
        );
        let augmenter = CalibrationStatusAugmenter::new(Rc::clone(&store), Rc::clone(&state));
        let mut status = status_with_rpm_channel("fan1", 12.0, 0);
        augmenter.augment(&mut status, &device);
        assert_eq!(status.channels[0].duty, Some(0.0));
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

    #[test]
    #[cfg(feature = "gated-tests")]
    fn augment_reuses_its_key_across_devices_without_allocating() {
        // Goal: the reused lookup key names the current device and channel on
        // every lookup, and a warm call allocates nothing. One augmenter serves
        // two devices and only dev-a's fan2 is calibrated. The first call sees a
        // longer UID and channel name, so a stale suffix would break the shorter
        // lookups that follow.
        let store = Rc::new(CalibrationStore::empty());
        let state = Rc::new(FanStateMap::new());
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
        let raw = f64::from(device_dut);
        let expected = f64::from(cal.device_to_true_duty(device_dut).expect("smooth"));
        assert_ne!(raw, expected, "the rewrite must be observable");
        store.insert_unsaved(("dev-a".to_string(), "fan2".to_string()), cal);
        let augmenter = CalibrationStatusAugmenter::new(Rc::clone(&store), Rc::clone(&state));
        let reading =
            || status_with_channels(vec![("fan2-long", raw), ("fan1", raw), ("fan2", raw)]);
        let longer_uid = "dev-a-longer".to_string();
        let uid = "dev-a".to_string();

        let mut uncalibrated = reading();
        augmenter.augment(&mut uncalibrated, &longer_uid);
        let mut status = reading();
        let ((), allocations) =
            crate::alloc_count::count_allocations(|| augmenter.augment(&mut status, &uid));

        assert_eq!(allocations, 0);
        assert!(uncalibrated.channels.iter().all(|c| c.duty == Some(raw)));
        assert_eq!(status.channels[0].duty, Some(raw));
        assert_eq!(status.channels[1].duty, Some(raw));
        assert_eq!(status.channels[2].duty, Some(expected));
    }
}
