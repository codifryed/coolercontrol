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

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::ops::Not;

use log::{error, trace};
use serde::{Deserialize, Serialize};
use yata::methods::TMA;
use yata::prelude::Method;

use crate::device::{Duty, Temp};
use crate::engine::{NormalizedGraphProfile, Processor, SpeedProfileData};
use crate::repositories::repository::DeviceLock;
use crate::setting::{Function, FunctionType, ProfileUID};
use crate::AllDevices;

pub const TMA_DEFAULT_WINDOW_SIZE: u8 = 8;
const TEMP_SAMPLE_SIZE: isize = 16;
const MIN_TEMP_HIST_STACK_SIZE: u8 = 2;
const MAX_DUTY_SAMPLE_SIZE: usize = 20;
const DEFAULT_MAX_NO_DUTY_SET_SECONDS: f64 = 30.;
const MIN_NO_DUTY_SET_SECONDS: f64 = 30.;
const MAX_NO_DUTY_SET_SECONDS: f64 = 60.;
const EMERGENCY_MISSING_TEMP: Temp = 100.;

/// The default function returns the source temp as-is.
pub struct FunctionIdentityPreProcessor {
    all_devices: AllDevices,
}

impl FunctionIdentityPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self { all_devices }
    }
}

impl Processor for FunctionIdentityPreProcessor {
    fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.function.f_type == FunctionType::Identity && data.temp.is_none()
        // preprocessor only
    }

    fn init_state(&self, _: &ProfileUID) {}

    fn clear_state(&self, _: &ProfileUID) {}

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_option = self
            .all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if temp_source_device_option.is_none() {
            log_missing_temp_device(data);
            data.temp = Some(EMERGENCY_MISSING_TEMP);
            return data;
        }
        data.temp = temp_source_device_option
            .unwrap()
            .borrow()
            .status_history
            .iter()
            .last() // last = latest temp
            .and_then(|status| {
                status
                    .temps
                    .iter()
                    .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                    .map(|temp_status| temp_status.temp)
                    .next_back()
                    .or_else(|| {
                        log_missing_temp_sensor(data);
                        Some(EMERGENCY_MISSING_TEMP)
                    })
            });
        data
    }
}

/// The standard Function with Hysteresis control
pub struct FunctionStandardPreProcessor {
    all_devices: AllDevices,
    channel_settings_metadata: RefCell<HashMap<ProfileUID, ChannelSettingMetadata>>,
}

impl FunctionStandardPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices,
            channel_settings_metadata: RefCell::new(HashMap::new()),
        }
    }

    fn data_is_sane(data: &SpeedProfileData) -> bool {
        if data.profile.function.response_delay.is_none()
            || data.profile.function.deviance.is_none()
            || data.profile.function.only_downward.is_none()
        {
            error!(
                "All required fields must be set for the standard Function: {:?}, {:?}, {:?}",
                data.profile.function.response_delay,
                data.profile.function.deviance,
                data.profile.function.only_downward,
            );
            return false;
        }
        true
    }

    fn fill_temp_stack(
        metadata: &mut ChannelSettingMetadata,
        data: &mut SpeedProfileData,
        temp_source_device_option: Option<&DeviceLock>,
    ) {
        if temp_source_device_option.is_none() {
            log_missing_temp_device(data);
            if metadata.last_applied_temp == 0. {
                metadata.temp_hist_stack.clear();
            }
            metadata.temp_hist_stack.push_back(EMERGENCY_MISSING_TEMP);
            return;
        }
        let temp_source_device = temp_source_device_option.unwrap().borrow();
        if metadata.last_applied_temp == 0. {
            // this is needed for the first application
            let mut latest_temps = temp_source_device
                .status_history
                .iter()
                .rev() // reverse so that take() takes the latest
                .take(metadata.ideal_stack_size)
                .flat_map(|status| status.temps.as_slice())
                .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                .map(|temp_status| temp_status.temp)
                .collect::<Vec<f64>>();
            latest_temps.reverse(); // re-order temps to proper Vec order
            if latest_temps.is_empty() {
                log_missing_temp_sensor(data);
                metadata.temp_hist_stack.clear();
                metadata.temp_hist_stack.push_back(EMERGENCY_MISSING_TEMP);
                return;
            }
            metadata.temp_hist_stack.clear();
            metadata.temp_hist_stack.extend(latest_temps);
        } else {
            // the normal operation
            let current_temp = temp_source_device
                .status_history
                .back()
                .and_then(|status| {
                    status
                        .temps
                        .as_slice()
                        .iter()
                        .filter(|temp_status| {
                            temp_status.name == data.profile.temp_source.temp_name
                        })
                        .map(|temp_status| temp_status.temp)
                        .next_back()
                        .or_else(|| {
                            log_missing_temp_sensor(data);
                            Some(EMERGENCY_MISSING_TEMP)
                        })
                })
                .unwrap();
            metadata.temp_hist_stack.push_back(current_temp);
        }
    }

    fn temp_within_tolerance(temp_to_verify: f64, last_applied_temp: f64, deviance: f64) -> bool {
        temp_to_verify <= (last_applied_temp + deviance)
            && temp_to_verify >= (last_applied_temp - deviance)
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn calc_ideal_stack_size(profile: &NormalizedGraphProfile) -> u8 {
        (f64::from(profile.function.response_delay.unwrap()) / profile.poll_rate).ceil() as u8 + 1
    }
}

impl Processor for FunctionStandardPreProcessor {
    fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.function.f_type == FunctionType::Standard && data.temp.is_none()
        // preprocessor only
    }

    fn init_state(&self, profile_uid: &ProfileUID) {
        self.channel_settings_metadata
            .borrow_mut()
            .insert(profile_uid.clone(), ChannelSettingMetadata::new());
    }

    fn clear_state(&self, profile_uid: &ProfileUID) {
        self.channel_settings_metadata
            .borrow_mut()
            .remove(profile_uid);
    }

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_option = self
            .all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if Self::data_is_sane(data).not() {
            return data;
        }

        // setup metadata:
        let mut metadata_lock = self.channel_settings_metadata.borrow_mut();
        let metadata = metadata_lock.get_mut(&data.profile.profile_uid).unwrap();
        if metadata.ideal_stack_size == 0 {
            // set ideal size on initial run:
            metadata.ideal_stack_size =
                MIN_TEMP_HIST_STACK_SIZE.max(Self::calc_ideal_stack_size(&data.profile)) as usize;
        }
        Self::fill_temp_stack(metadata, data, temp_source_device_option);

        if metadata.temp_hist_stack.len() > metadata.ideal_stack_size {
            metadata.temp_hist_stack.pop_front();
        } else if metadata.last_applied_temp == 0.
            && metadata.temp_hist_stack.len() < metadata.ideal_stack_size
        {
            // Very first run after boot/wakeup, let's apply something right away
            let temp_to_apply = metadata.temp_hist_stack.front().copied().unwrap();
            data.temp = Some(temp_to_apply);
            metadata.last_applied_temp = temp_to_apply;
            return data;
        }

        // main processor logic:
        if data.profile.function.only_downward.unwrap() {
            let newest_temp = *metadata.temp_hist_stack.back().unwrap();
            if newest_temp > metadata.last_applied_temp {
                metadata.temp_hist_stack.clear();
                metadata.temp_hist_stack.push_back(newest_temp);
                data.temp = Some(newest_temp);
                metadata.last_applied_temp = newest_temp;
                return data;
            }
        }
        let oldest_temp = metadata.temp_hist_stack.front().copied().unwrap();
        let oldest_temp_within_tolerance = Self::temp_within_tolerance(
            oldest_temp,
            metadata.last_applied_temp,
            data.profile.function.deviance.unwrap(),
        );
        if metadata.temp_hist_stack.len() > MIN_TEMP_HIST_STACK_SIZE as usize {
            let newest_temp_within_tolerance = Self::temp_within_tolerance(
                *metadata.temp_hist_stack.back().unwrap(),
                metadata.last_applied_temp,
                data.profile.function.deviance.unwrap(),
            );
            if oldest_temp_within_tolerance && newest_temp_within_tolerance {
                // normalize the stack, as we want to skip any spikes that happened within the delay period
                let adjust_count = metadata.temp_hist_stack.len() - 1; // we leave the newest temp as is
                metadata
                    .temp_hist_stack
                    .iter_mut()
                    .take(adjust_count)
                    .for_each(|temp| *temp = oldest_temp);
            }
        }
        if data.safety_latch_triggered {
            if data.profile.function.threshold_hopping {
                // bypass thresholds
                data.temp = Some(oldest_temp);
                metadata.last_applied_temp = oldest_temp;
                data
            } else {
                // If hopping is disabled, we want to re-apply the last applied temp NOT the
                // oldest temp, which would bypass the hysteresis thresholds.
                data.temp = Some(metadata.last_applied_temp);
                data
            }
        } else if oldest_temp_within_tolerance {
            data // nothing to apply
        } else {
            // should use temp from hysteresis stack
            data.temp = Some(oldest_temp);
            metadata.last_applied_temp = oldest_temp;
            data
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSettingMetadata {
    pub temp_hist_stack: VecDeque<f64>,
    pub ideal_stack_size: usize,
    pub last_applied_temp: f64,
}

impl ChannelSettingMetadata {
    pub fn new() -> Self {
        Self {
            temp_hist_stack: VecDeque::new(),
            ideal_stack_size: 0,
            last_applied_temp: 0.,
        }
    }
}

/// The EMA function calculates an Exponential Moving Average over recent temperatures and
/// returns the most recent value. (Dynamically affected by temp history)
pub struct FunctionEMAPreProcessor {
    all_devices: AllDevices,
}

impl FunctionEMAPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self { all_devices }
    }

    /// Computes an exponential moving average from give temps and returns the final/current value from that average.
    /// Exponential moving average gives the most recent values more weight. This is particularly helpful
    /// for setting duty for dynamic temperature sources like CPU. (Good reaction but also averaging)
    /// Will panic if `sample_size` is 0.
    /// Rounded to the nearest 100th decimal place
    fn current_temp_from_exponential_moving_average(
        all_temps: &[f64],
        window_size: Option<u8>,
    ) -> f64 {
        (TMA::new_over(
            window_size.unwrap_or(TMA_DEFAULT_WINDOW_SIZE),
            Self::get_temps_slice(all_temps),
        )
        .unwrap()
        .last()
        .unwrap()
            * 100.)
            .round()
            / 100.
    }

    #[allow(clippy::cast_possible_wrap, clippy::cast_sign_loss)]
    fn get_temps_slice(all_temps: &[f64]) -> &[f64] {
        // keeping the sample size low allows the average to be more forward-aggressive,
        // otherwise the actual reading and the EMA take quite a while before they are the same value
        // todo: we could auto-size the sample size, if the window is larger than the default sample size,
        //  but should test what the actual outcome with be and if that's a realistic value for users.
        let sample_delta = all_temps.len() as isize - TEMP_SAMPLE_SIZE;
        if sample_delta > 0 {
            all_temps.split_at(sample_delta as usize).1
        } else {
            all_temps
        }
    }
}

impl Processor for FunctionEMAPreProcessor {
    fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.function.f_type == FunctionType::ExponentialMovingAvg && data.temp.is_none()
        // preprocessor only
    }

    fn init_state(&self, _: &ProfileUID) {}

    fn clear_state(&self, _: &ProfileUID) {}

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_option = self
            .all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if temp_source_device_option.is_none() {
            log_missing_temp_device(data);
            data.temp = Some(EMERGENCY_MISSING_TEMP);
            return data;
        }
        let mut temps = {
            // scoped for the device read lock
            let temp_source_device = temp_source_device_option.unwrap().borrow();
            temp_source_device
                .status_history
                .iter()
                .rev() // reverse so that take() takes the end part
                // we only need the last (sample_size ) temps for EMA:
                .take(TEMP_SAMPLE_SIZE as usize)
                .flat_map(|status| status.temps.as_slice())
                .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                .map(|temp_status| temp_status.temp)
                .collect::<Vec<f64>>()
        };
        temps.reverse(); // re-order temps so last temp is again last
        data.temp = if temps.is_empty() {
            log_missing_temp_sensor(data);
            Some(EMERGENCY_MISSING_TEMP)
        } else {
            Some(Self::current_temp_from_exponential_moving_average(
                &temps,
                data.profile.function.sample_window,
            ))
        };
        data
    }
}

/// This post-processor keeps a set of last-applied-duties and applies only duties within set upper and
/// lower thresholds. It also handles improvements for edge cases.
pub struct FunctionDutyThresholdPostProcessor {
    scheduled_settings_metadata: RefCell<HashMap<ProfileUID, DutySettingMetadata>>,
}

impl FunctionDutyThresholdPostProcessor {
    pub fn new() -> Self {
        Self {
            scheduled_settings_metadata: RefCell::new(HashMap::new()),
        }
    }

    /// Returns the duty to apply based on step size thresholds and settings.
    fn apply_step_size_thresholds(&self, data: &SpeedProfileData) -> Option<u8> {
        if self.scheduled_settings_metadata.borrow()[&data.profile.profile_uid]
            .last_manual_speeds_set
            .is_empty()
        {
            return data.duty; // first application (startup)
        }
        let last_duty = self.get_appropriate_last_duty(&data.profile.profile_uid);
        let (step_increase_min, step_increase_max, step_decrease_min, step_decrease_max) =
            Self::determine_step_sizes(&data.profile.function);

        #[allow(clippy::cast_possible_wrap)]
        let diff_to_last_duty: i8 = data.duty.unwrap() as i8 - last_duty as i8;
        let duty_has_decreased = diff_to_last_duty < 0;
        let abs_diff_to_last_duty = diff_to_last_duty.unsigned_abs();

        if data.safety_latch_triggered {
            // If the safety-latch is triggered, we want to bypass only the MIN step size thresholds
            // as that is the only case where a duty is possibly not applied.
            // MAX step size limits we will respect in all cases.
            if data.profile.function.threshold_hopping {
                // For threshold hopping, we only want to bypass the min thresholds, as
                // that is the only case where duty is not applied.
                // For duty increases or zero, we handling it like normal - always applying
                if duty_has_decreased {
                    if abs_diff_to_last_duty < step_decrease_min {
                        return data.duty;
                    }
                } else if abs_diff_to_last_duty < step_increase_min {
                    return data.duty;
                }
            } else {
                // if hopping is disabled, we apply the last duty and NOT bypass the min step size thresholds.
                // The last_duty is guaranteed to be within threshold limits and the purpose here to
                // apply the last duty again to make sure the device is doing what it's supposed to.
                if duty_has_decreased {
                    if abs_diff_to_last_duty < step_decrease_min {
                        return Some(last_duty);
                    }
                } else if abs_diff_to_last_duty < step_increase_min {
                    return Some(last_duty);
                }
            }
        }

        // Normal flow
        if duty_has_decreased {
            if abs_diff_to_last_duty < step_decrease_min {
                None
            } else if abs_diff_to_last_duty > step_decrease_max {
                Some(last_duty - step_decrease_max) // limit to max step size
            } else {
                // within range
                data.duty
            }
        } else if abs_diff_to_last_duty < step_increase_min {
            None
        } else if abs_diff_to_last_duty > step_increase_max {
            Some(last_duty + step_increase_max) // limit to max step size
        } else {
            // within range
            data.duty
        }
    }

    /// This returns the last duty that was set manually. This used to also do extra work to
    /// determine if it was a true value of the device, but with the introduction of the
    /// safety-latch, that is superfluous.
    fn get_appropriate_last_duty(&self, profile_uid: &ProfileUID) -> u8 {
        *self.scheduled_settings_metadata.borrow()[profile_uid]
            .last_manual_speeds_set
            .back()
            .unwrap() // already checked to exist
    }

    fn determine_step_sizes(data_function: &Function) -> (Duty, Duty, Duty, Duty) {
        let step_is_symmetric = data_function.step_size_min_decreasing == 0;
        let step_has_fixed_increase = data_function.step_size_max == 0;
        let step_has_fixed_decrease = data_function.step_size_max_decreasing == 0;
        let step_increase_min = data_function.step_size_min;
        let step_increase_max = if step_has_fixed_increase {
            step_increase_min
        } else {
            data_function.step_size_max
        };
        let step_decrease_min = if step_is_symmetric {
            step_increase_min
        } else {
            data_function.step_size_min_decreasing
        };
        let step_decrease_max = if step_is_symmetric {
            step_increase_max
        } else if step_has_fixed_decrease {
            step_decrease_min
        } else {
            data_function.step_size_max_decreasing
        };
        (
            step_increase_min,
            step_increase_max,
            step_decrease_min,
            step_decrease_max,
        )
    }
}

impl Processor for FunctionDutyThresholdPostProcessor {
    fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.duty.is_some()
    }

    fn init_state(&self, profile_uid: &ProfileUID) {
        self.scheduled_settings_metadata
            .borrow_mut()
            .insert(profile_uid.clone(), DutySettingMetadata::new());
    }

    fn clear_state(&self, profile_uid: &ProfileUID) {
        self.scheduled_settings_metadata
            .borrow_mut()
            .remove(profile_uid);
    }

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        if let Some(duty_to_set) = self.apply_step_size_thresholds(data) {
            {
                let mut metadata_lock = self.scheduled_settings_metadata.borrow_mut();
                let metadata = metadata_lock.get_mut(&data.profile.profile_uid).unwrap();
                metadata.last_manual_speeds_set.push_back(duty_to_set);
                if metadata.last_manual_speeds_set.len() > MAX_DUTY_SAMPLE_SIZE {
                    metadata.last_manual_speeds_set.pop_front();
                }
            }
            data.duty = Some(duty_to_set);
            data
        } else {
            data.duty = None;
            trace!("Duty not above threshold to be applied to device. Skipping");
            trace!(
                "Last applied duties: {:?}",
                self.scheduled_settings_metadata.borrow()[&data.profile.profile_uid]
                    .last_manual_speeds_set
            );
            data
        }
    }
}

/// This is used to help in deciding exactly when to apply a setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DutySettingMetadata {
    /// (internal use) the last duty speeds that we set manually. This keeps track of applied settings
    /// to not re-apply the same setting over and over again needlessly. eg: [20, 25, 30]
    #[serde(skip_serializing, skip_deserializing)]
    pub last_manual_speeds_set: VecDeque<u8>,
}

impl DutySettingMetadata {
    pub fn new() -> Self {
        Self {
            last_manual_speeds_set: VecDeque::with_capacity(MAX_DUTY_SAMPLE_SIZE + 1),
        }
    }
}

/// This processor handles a so-called Safety-Latch. The makes sure that actual fan profile targets
/// are hit, regardless of thresholds set. It also makes sure that the device is actually doing
/// what it should. This processor needs to run at both the start and end of the processing chain.
pub struct FunctionSafetyLatchProcessor {
    scheduled_settings_metadata: RefCell<HashMap<ProfileUID, SafetyLatchMetadata>>,
}

impl FunctionSafetyLatchProcessor {
    pub fn new() -> Self {
        Self {
            scheduled_settings_metadata: RefCell::new(HashMap::new()),
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn initial_max_no_duty_set_count(profile: &NormalizedGraphProfile) -> u8 {
        if let Some(response_delay) = profile.function.response_delay {
            let response_delay_secs = f64::from(response_delay);
            let response_delay_count = response_delay_secs / profile.poll_rate;
            // use response_delay but within a reasonable limit
            let min_count = (MIN_NO_DUTY_SET_SECONDS / profile.poll_rate).ceil();
            let max_count = (MAX_NO_DUTY_SET_SECONDS / profile.poll_rate).ceil();
            response_delay_count.clamp(min_count, max_count) as u8
        } else {
            (DEFAULT_MAX_NO_DUTY_SET_SECONDS / profile.poll_rate).ceil() as u8
        }
    }
}

impl Processor for FunctionSafetyLatchProcessor {
    fn is_applicable(&self, _data: &SpeedProfileData) -> bool {
        // applies to all function types (they all have a minimum duty change setting)
        true
    }

    fn init_state(&self, profile_uid: &ProfileUID) {
        self.scheduled_settings_metadata
            .borrow_mut()
            .insert(profile_uid.clone(), SafetyLatchMetadata::new());
    }

    fn clear_state(&self, profile_uid: &ProfileUID) {
        self.scheduled_settings_metadata
            .borrow_mut()
            .remove(profile_uid);
    }

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let mut metadata_lock = self.scheduled_settings_metadata.borrow_mut();
        let metadata = metadata_lock.get_mut(&data.profile.profile_uid).unwrap();
        if data.processing_started.not() {
            // Check whether to trigger the latch at the start of processing
            if metadata.max_no_duty_set_count == 0 {
                // first run, set the max_count
                metadata.max_no_duty_set_count = Self::initial_max_no_duty_set_count(&data.profile);
            }
            if metadata.no_duty_set_counter >= metadata.max_no_duty_set_count {
                data.safety_latch_triggered = true;
            }
            data.processing_started = true;
            return data;
        }
        // end of processing logic
        if data.duty.is_some() {
            metadata.no_duty_set_counter = 0;
        } else {
            if data.safety_latch_triggered {
                error!("No Duty Set AND Safety latch triggered. This should not happen.");
            }
            metadata.no_duty_set_counter += 1;
        }
        data
    }
}

/// Metadata used for the Safety Latch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyLatchMetadata {
    /// (internal use) a counter to be able to know how many times the to-be-applied duty was under
    /// the various processor thresholds. This will help hit the target profile duty regardless of
    /// various threshold settings
    #[serde(skip_serializing, skip_deserializing)]
    pub no_duty_set_counter: u8,

    /// The max count allowed for a particular channel's settings configuration
    #[serde(skip_serializing, skip_deserializing)]
    pub max_no_duty_set_count: u8,
}

impl SafetyLatchMetadata {
    pub fn new() -> Self {
        Self {
            // This will force the SafetyLatch to trigger on latch initialization. (such as when
            // applying the profile to a second device channel)
            no_duty_set_counter: u8::MAX,
            max_no_duty_set_count: 0,
        }
    }
}

fn log_missing_temp_device(data: &SpeedProfileData) {
    error!(
        "Temperature Source Device: {} is missing for Profile: {}, \
         using emergency default temp: {EMERGENCY_MISSING_TEMP}C",
        data.profile.temp_source.device_uid, data.profile.profile_name,
    );
}

fn log_missing_temp_sensor(data: &SpeedProfileData) {
    error!(
        "Temperature Sensor: {} - {} is missing for Profile: {}, \
         using emergency default temp: {EMERGENCY_MISSING_TEMP}C",
        data.profile.temp_source.device_uid,
        data.profile.temp_source.temp_name,
        data.profile.profile_name,
    );
}

#[cfg(test)]
mod tests {
    use crate::engine::processors::functions::{
        FunctionDutyThresholdPostProcessor, FunctionEMAPreProcessor,
    };
    use crate::engine::{NormalizedGraphProfile, SpeedProfileData, TempSource};
    use crate::setting::Function;

    #[test]
    #[allow(clippy::float_cmp)]
    fn current_temp_from_exponential_moving_average_test() {
        let given_expected: Vec<(&[f64], f64)> = vec![
            // these are just samples. Tested with real hardware for expected results,
            // which are not so clear in numbers here.
            (&[20., 25.], 20.05),
            (&[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.], 35.86),
            (&[30., 30., 30., 30.], 30.),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                FunctionEMAPreProcessor::current_temp_from_exponential_moving_average(given, None),
                expected
            );
        }
    }

    // Helper to create a test function with specific step size settings
    fn create_test_function(
        step_size_min: u8,
        step_size_max: u8,
        step_size_min_decreasing: u8,
        step_size_max_decreasing: u8,
        threshold_hopping: bool,
    ) -> Function {
        Function {
            step_size_min,
            step_size_max,
            step_size_min_decreasing,
            step_size_max_decreasing,
            threshold_hopping,
            ..Default::default()
        }
    }

    fn create_test_profile(function: Function) -> std::rc::Rc<NormalizedGraphProfile> {
        std::rc::Rc::new(NormalizedGraphProfile {
            profile_uid: "test-profile".to_string(),
            profile_name: "Test Profile".to_string(),
            speed_profile: vec![],
            temp_source: TempSource {
                device_uid: "test-device".to_string(),
                temp_name: "test-temp".to_string(),
            },
            function,
            poll_rate: 1.0,
        })
    }

    // ==================== determine_step_sizes tests ====================

    #[test]
    fn determine_step_sizes_symmetric_variable() {
        // Default case: symmetric step sizes (step_size_min_decreasing = 0)
        let function = create_test_function(2, 100, 0, 0, true);
        let (inc_min, inc_max, dec_min, dec_max) =
            FunctionDutyThresholdPostProcessor::determine_step_sizes(&function);

        assert_eq!(inc_min, 2, "increase min should be step_size_min");
        assert_eq!(inc_max, 100, "increase max should be step_size_max");
        assert_eq!(
            dec_min, 2,
            "decrease min should mirror increase min (symmetric)"
        );
        assert_eq!(
            dec_max, 100,
            "decrease max should mirror increase max (symmetric)"
        );
    }

    #[test]
    fn determine_step_sizes_symmetric_fixed() {
        // Fixed step size: step_size_max = 0 means use step_size_min for both
        let function = create_test_function(5, 0, 0, 0, true);
        let (inc_min, inc_max, dec_min, dec_max) =
            FunctionDutyThresholdPostProcessor::determine_step_sizes(&function);

        assert_eq!(inc_min, 5);
        assert_eq!(inc_max, 5, "fixed step: max should equal min");
        assert_eq!(dec_min, 5, "symmetric: decrease min mirrors increase");
        assert_eq!(
            dec_max, 5,
            "symmetric + fixed: decrease max mirrors increase"
        );
    }

    #[test]
    fn determine_step_sizes_asymmetric_variable() {
        // Asymmetric: different step sizes for increase vs decrease
        let function = create_test_function(2, 50, 5, 30, true);
        let (inc_min, inc_max, dec_min, dec_max) =
            FunctionDutyThresholdPostProcessor::determine_step_sizes(&function);

        assert_eq!(inc_min, 2);
        assert_eq!(inc_max, 50);
        assert_eq!(dec_min, 5, "asymmetric: uses step_size_min_decreasing");
        assert_eq!(dec_max, 30, "asymmetric: uses step_size_max_decreasing");
    }

    #[test]
    fn determine_step_sizes_asymmetric_fixed_decrease() {
        // Asymmetric with fixed decrease step (step_size_max_decreasing = 0)
        let function = create_test_function(2, 50, 10, 0, true);
        let (inc_min, inc_max, dec_min, dec_max) =
            FunctionDutyThresholdPostProcessor::determine_step_sizes(&function);

        assert_eq!(inc_min, 2);
        assert_eq!(inc_max, 50);
        assert_eq!(dec_min, 10);
        assert_eq!(dec_max, 10, "fixed decrease: max equals min");
    }

    #[test]
    fn determine_step_sizes_asymmetric_fixed_increase() {
        // Asymmetric with fixed increase step (step_size_max = 0)
        let function = create_test_function(3, 0, 5, 20, true);
        let (inc_min, inc_max, dec_min, dec_max) =
            FunctionDutyThresholdPostProcessor::determine_step_sizes(&function);

        assert_eq!(inc_min, 3);
        assert_eq!(inc_max, 3, "fixed increase: max equals min");
        assert_eq!(dec_min, 5);
        assert_eq!(dec_max, 20);
    }

    // ==================== apply_step_size_thresholds tests ====================

    fn setup_processor_with_last_duty(last_duty: u8) -> FunctionDutyThresholdPostProcessor {
        use crate::engine::Processor;
        let processor = FunctionDutyThresholdPostProcessor::new();
        let profile_uid = "test-profile".to_string();
        processor.init_state(&profile_uid);
        // Add a last duty to the metadata
        processor
            .scheduled_settings_metadata
            .borrow_mut()
            .get_mut(&profile_uid)
            .unwrap()
            .last_manual_speeds_set
            .push_back(last_duty);
        processor
    }

    fn create_test_data(
        function: Function,
        duty: u8,
        safety_latch_triggered: bool,
    ) -> SpeedProfileData {
        SpeedProfileData {
            profile: create_test_profile(function),
            temp: None,
            duty: Some(duty),
            processing_started: true,
            safety_latch_triggered,
        }
    }

    #[test]
    fn apply_step_size_first_application_returns_duty() {
        use crate::engine::Processor;
        let processor = FunctionDutyThresholdPostProcessor::new();
        let profile_uid = "test-profile".to_string();
        processor.init_state(&profile_uid);
        // No last duty set - simulates first application

        let function = create_test_function(2, 100, 0, 0, true);
        let data = create_test_data(function, 50, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(50),
            "first application should return duty as-is"
        );
    }

    #[test]
    fn apply_step_size_increase_below_min_threshold() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(5, 100, 0, 0, true);
        // Duty increase of 3 (50 -> 53), below min threshold of 5
        let data = create_test_data(function, 53, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result, None,
            "increase below min threshold should return None"
        );
    }

    #[test]
    fn apply_step_size_increase_within_range() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(5, 100, 0, 0, true);
        // Duty increase of 10 (50 -> 60), within range [5, 100]
        let data = create_test_data(function, 60, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(result, Some(60), "increase within range should return duty");
    }

    #[test]
    fn apply_step_size_increase_above_max_threshold() {
        let processor = setup_processor_with_last_duty(30);
        let function = create_test_function(2, 20, 0, 0, true);
        // Duty increase of 50 (30 -> 80), above max threshold of 20
        let data = create_test_data(function, 80, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(50),
            "increase above max should be limited to last_duty + max"
        );
    }

    #[test]
    fn apply_step_size_decrease_below_min_threshold() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(5, 100, 0, 0, true);
        // Duty decrease of 3 (50 -> 47), below min threshold of 5
        let data = create_test_data(function, 47, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result, None,
            "decrease below min threshold should return None"
        );
    }

    #[test]
    fn apply_step_size_decrease_within_range() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(5, 100, 0, 0, true);
        // Duty decrease of 10 (50 -> 40), within range
        let data = create_test_data(function, 40, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(result, Some(40), "decrease within range should return duty");
    }

    #[test]
    fn apply_step_size_decrease_above_max_threshold() {
        let processor = setup_processor_with_last_duty(70);
        let function = create_test_function(2, 20, 0, 0, true);
        // Duty decrease of 50 (70 -> 20), above max threshold of 20
        let data = create_test_data(function, 20, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(50),
            "decrease above max should be limited to last_duty - max"
        );
    }

    #[test]
    fn apply_step_size_asymmetric_decrease() {
        let processor = setup_processor_with_last_duty(50);
        // Asymmetric: increase min=2, decrease min=10
        let function = create_test_function(2, 100, 10, 100, true);
        // Duty decrease of 5 (50 -> 45), below asymmetric decrease min of 10
        let data = create_test_data(function, 45, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result, None,
            "asymmetric decrease below min should return None"
        );
    }

    #[test]
    fn apply_step_size_asymmetric_decrease_within_range() {
        let processor = setup_processor_with_last_duty(50);
        // Asymmetric: increase min=2, decrease min=10
        let function = create_test_function(2, 100, 10, 100, true);
        // Duty decrease of 15 (50 -> 35), within asymmetric range
        let data = create_test_data(function, 35, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(35),
            "asymmetric decrease within range should return duty"
        );
    }

    // ==================== Safety latch with threshold hopping tests ====================

    #[test]
    fn apply_step_size_safety_latch_hopping_bypasses_min_increase() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(10, 100, 0, 0, true); // hopping enabled
                                                                  // Duty increase of 5 (50 -> 55), below min threshold of 10
                                                                  // With safety latch + hopping, should bypass min and return duty
        let data = create_test_data(function, 55, true);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(55),
            "safety latch + hopping should bypass min threshold"
        );
    }

    #[test]
    fn apply_step_size_safety_latch_hopping_bypasses_min_decrease() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(10, 100, 0, 0, true); // hopping enabled
                                                                  // Duty decrease of 5 (50 -> 45), below min threshold of 10
        let data = create_test_data(function, 45, true);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(45),
            "safety latch + hopping should bypass min threshold for decrease"
        );
    }

    #[test]
    fn apply_step_size_safety_latch_hopping_respects_max() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(2, 20, 0, 0, true); // hopping enabled
                                                                // Duty increase of 40 (50 -> 90), above max threshold of 20
                                                                // Even with safety latch, max should be respected
        let data = create_test_data(function, 90, true);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(70),
            "safety latch + hopping should still respect max threshold"
        );
    }

    #[test]
    fn apply_step_size_safety_latch_no_hopping_returns_last_duty() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(10, 100, 0, 0, false); // hopping disabled
                                                                   // Duty increase of 5 (50 -> 55), below min threshold of 10
                                                                   // With safety latch but NO hopping, should return last_duty
        let data = create_test_data(function, 55, true);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(50),
            "safety latch without hopping should return last_duty"
        );
    }

    #[test]
    fn apply_step_size_safety_latch_no_hopping_decrease_returns_last_duty() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(10, 100, 0, 0, false); // hopping disabled
                                                                   // Duty decrease of 5 (50 -> 45), below min threshold
        let data = create_test_data(function, 45, true);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(50),
            "safety latch without hopping should return last_duty for decrease"
        );
    }

    #[test]
    fn apply_step_size_fixed_step_size() {
        let processor = setup_processor_with_last_duty(50);
        // Fixed step size: step_size_max = 0 means min == max
        let function = create_test_function(5, 0, 0, 0, true);
        // Duty increase of 10 (50 -> 60), above fixed step of 5
        let data = create_test_data(function, 60, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(55),
            "fixed step size should limit to exactly step_size_min"
        );
    }

    #[test]
    fn apply_step_size_fixed_step_size_exact_match() {
        let processor = setup_processor_with_last_duty(50);
        let function = create_test_function(5, 0, 0, 0, true);
        // Duty increase of exactly 5 (50 -> 55), matches fixed step
        let data = create_test_data(function, 55, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(55),
            "exact fixed step size match should return duty"
        );
    }
}
