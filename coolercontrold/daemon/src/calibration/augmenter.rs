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

//! `StatusAugmenter` impl that rewrites a freshly-pushed status's
//! per-channel `duty` into RPM-normalized true-duty space. Installed
//! on every device by `Engine::new`, fires inside `Device::set_status`
//! while the history `Arc` is being mutated, so there is no extra deep
//! clone when a REST or gRPC reader holds an `Arc` clone of the history.

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
            // Skip channels currently being swept: the diagnoser is
            // writing raw device duty and the UI must see those raw
            // values, not the prior calibration's mapped values.
            if self.fan_state_map.is_under_diagnosis(&key) {
                continue;
            }
            // Prefer the device-duty-derived value (stable across
            // reads). Fall back to the RPM-derived value when the
            // two diverge by more than the sanity threshold: that gap
            // surfaces stuck fans, dead fans, and broken PWM lines
            // that the stable path would otherwise hide behind the
            // duty the daemon last wrote.
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

/// Round and clamp an `f64` duty into the 0..=100 percent range. NaN
/// reads from hardware are coerced to 0. Mirrors the helper that
/// previously lived in the engine module.
fn clamp_f64_to_duty(value: f64) -> Duty {
    if value.is_nan() {
        return 0;
    }
    let rounded = value.round().clamp(0.0, 100.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let as_u8 = rounded as Duty;
    as_u8
}

/// Install a fresh `CalibrationStatusAugmenter` on every device in
/// `all_devices`. Called by `Engine::new` so the calibration rewrite
/// runs inside each device's `set_status` `Arc::make_mut` rather than
/// in a separate hot-path pass. Devices added later (e.g. user-created
/// custom sensors) carry no fan channels, so missing the install on
/// them is a no-op.
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
