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

use super::curve::{select_displayed_true_duty, SANITY_THRESHOLD_PERCENT};
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
            // Device-duty is the stable source; fall back to RPM when the two
            // diverge so stuck fans and broken PWM lines surface in the readout.
            let device_derived = channel.duty.and_then(|d| {
                self.calibration_store
                    .device_to_true_duty(&key, clamp_f64_to_duty(d))
            });
            let rpm_derived = channel
                .rpm
                .and_then(|r| self.calibration_store.rpm_to_true_duty(&key, r));
            if let Some(displayed) =
                select_displayed_true_duty(device_derived, rpm_derived, SANITY_THRESHOLD_PERCENT)
            {
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
