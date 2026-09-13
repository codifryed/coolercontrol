// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};
use std::ops::Not;
use std::rc::Rc;

use crate::device::{Duty, Temp};
use crate::engine::{utils, NormalizedGraphProfile, Processor, SpeedProfileData};
use crate::repositories::repository::DeviceLock;
use crate::setting::{Function, FunctionType, ProfileUID};
use crate::AllDevices;
use log::{error, trace};
use serde::{Deserialize, Serialize};

const MIN_TEMP_HIST_STACK_SIZE: u8 = 1;
const MAX_DUTY_SAMPLE_SIZE: usize = 20;
const DEFAULT_MAX_NO_DUTY_SET_SECONDS: f64 = 30.;
const MIN_NO_DUTY_SET_SECONDS: f64 = 30.;
const MAX_NO_DUTY_SET_SECONDS: f64 = 60.;
const EMERGENCY_MISSING_TEMP: Temp = 100.;
const MAX_DUTY: Duty = 100;

/// The depth of the hysteresis temp stack that holds a Function's response delay,
/// in poll cycles. Derived once per schedule into `NormalizedGraphProfile` so that a
/// live Function edit cannot leave a stale depth behind, and so the per-tick path
/// stays free of this arithmetic.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn calc_ideal_stack_size(function: &Function, poll_rate: f64) -> usize {
    debug_assert!(poll_rate > 0.0, "poll_rate must be positive");
    let response_delay_secs = f64::from(
        function
            .response_delay()
            .unwrap_or(DEFAULT_MAX_NO_DUTY_SET_SECONDS as u8),
    );
    let stack_size = (response_delay_secs / poll_rate).ceil() as u8;
    MIN_TEMP_HIST_STACK_SIZE.max(stack_size) as usize
}

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
        data.profile.function.f_type() == FunctionType::Identity && data.temp.is_none()
        // preprocessor only
    }

    fn init_state(&self, _: &ProfileUID) {}

    fn clear_state(&self, _: &ProfileUID) {}

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let Some(temp_source_device) = self
            .all_devices
            .get(data.profile.temp_source.device_uid.as_str())
        else {
            log_missing_temp_device(data);
            data.temp = Some(EMERGENCY_MISSING_TEMP);
            return data;
        };
        data.temp = temp_source_device
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
    channel_settings_metadata: Rc<RefCell<HashMap<ProfileUID, ChannelSettingMetadata>>>,
}

impl FunctionStandardPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices,
            channel_settings_metadata: Rc::new(RefCell::new(HashMap::new())),
        }
    }

    /// Returns a post-processor that shares metadata with this pre-processor.
    pub fn create_post_processor(&self) -> FunctionStandardPostProcessor {
        FunctionStandardPostProcessor {
            channel_settings_metadata: Rc::clone(&self.channel_settings_metadata),
        }
    }

    fn data_is_sane(data: &SpeedProfileData) -> bool {
        if data.profile.function.response_delay().is_none()
            || data.profile.function.deviance().is_none()
            || data.profile.function.only_downward().is_none()
        {
            error!(
                "All required fields must be set for the standard Function: {:?}, {:?}, {:?}",
                data.profile.function.response_delay(),
                data.profile.function.deviance(),
                data.profile.function.only_downward(),
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
        let Some(temp_source_device_lock) = temp_source_device_option else {
            log_missing_temp_device(data);
            if metadata.last_applied_temp == 0. {
                metadata.temp_hist_stack.clear();
            }
            metadata.temp_hist_stack.push_back(EMERGENCY_MISSING_TEMP);
            return;
        };
        let temp_source_device = temp_source_device_lock.borrow();
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
            let current_temp_celsius = temp_source_device
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
                .unwrap_or(EMERGENCY_MISSING_TEMP);
            metadata.temp_hist_stack.push_back(current_temp_celsius);
        }
        debug_assert!(
            !metadata.temp_hist_stack.is_empty(),
            "temp stack must not be empty after fill"
        );
    }

    fn apply_hysteresis<'a>(
        metadata: &mut ChannelSettingMetadata,
        data: &'a mut SpeedProfileData,
    ) -> &'a mut SpeedProfileData {
        let only_downward = data.profile.function.only_downward().unwrap_or(false);
        let deviance = data.profile.function.deviance().unwrap_or(2.0);
        debug_assert!(deviance >= 0.0, "deviance must not be negative");

        if only_downward {
            if Self::should_bypass_for_upward_temp(metadata, data) {
                return data;
            }
        }
        let Some(&oldest_temp_celsius) = metadata.temp_hist_stack.front() else {
            return data;
        };
        let oldest_temp_within_tolerance =
            Self::temp_within_tolerance(oldest_temp_celsius, metadata.last_applied_temp, deviance);
        if metadata.temp_hist_stack.len() > MIN_TEMP_HIST_STACK_SIZE as usize {
            let newest_temp_within_tolerance = metadata.temp_hist_stack.back().is_some_and(|&t| {
                Self::temp_within_tolerance(t, metadata.last_applied_temp, deviance)
            });
            if oldest_temp_within_tolerance && newest_temp_within_tolerance {
                // Normalize the stack to skip spikes within the delay period.
                let adjust_count = metadata.temp_hist_stack.len() - 1;
                metadata
                    .temp_hist_stack
                    .iter_mut()
                    .take(adjust_count)
                    .for_each(|temp| *temp = oldest_temp_celsius);
            }
        }
        if data.safety_latch_triggered {
            if data.profile.function.threshold_hopping {
                // Bypass thresholds.
                data.temp = Some(oldest_temp_celsius);
                metadata.last_applied_temp = oldest_temp_celsius;
                data
            } else {
                // Re-apply the last applied temp, not the oldest temp,
                // which would bypass the hysteresis thresholds.
                data.temp = Some(metadata.last_applied_temp);
                data
            }
        } else if oldest_temp_within_tolerance {
            if Self::should_continue_stepping(metadata, data) {
                // Replay the accepted temp so the step limiter can take its
                // next step. The stack and last_applied_temp stay untouched,
                // so the hysteresis deadband is unaffected.
                data.temp = Some(metadata.last_applied_temp);
            }
            data
        } else {
            // Should use temp from hysteresis stack.
            data.temp = Some(oldest_temp_celsius);
            metadata.last_applied_temp = oldest_temp_celsius;
            data
        }
    }

    /// Bypasses the hysteresis stack when `only_downward` is set and
    /// either the temperature is rising or the target duty has not dropped
    /// meaningfully (within `step_decrease_min` of the last applied duty).
    /// This prevents the stack from allowing an unwanted duty drop when
    /// temps dip and then slowly rise again.
    fn should_bypass_for_upward_temp(
        metadata: &mut ChannelSettingMetadata,
        data: &mut SpeedProfileData,
    ) -> bool {
        debug_assert!(
            data.profile.speed_profile.is_empty().not(),
            "speed profile must not be empty for interpolation"
        );
        let Some(&newest_temp_celsius) = metadata.temp_hist_stack.back() else {
            return false;
        };
        let Some(last_applied_duty) = metadata.last_applied_duty else {
            return false; // No duty history yet, let normal path handle it.
        };
        debug_assert!(last_applied_duty <= 100, "duty must be in valid range");
        let temp_rising = newest_temp_celsius > metadata.last_applied_temp;
        let duty_not_dropping = || {
            let target_duty =
                utils::interpolate_profile(&data.profile.speed_profile, newest_temp_celsius);
            let step_decrease_min = if data.profile.function.step_size_min_decreasing == 0 {
                data.profile.function.step_size_min
            } else {
                data.profile.function.step_size_min_decreasing
            };
            target_duty > last_applied_duty.saturating_sub(step_decrease_min)
        };
        if temp_rising || duty_not_dropping() {
            metadata.temp_hist_stack.clear();
            metadata.temp_hist_stack.push_back(newest_temp_celsius);
            metadata.last_applied_temp = newest_temp_celsius;
            data.temp = Some(newest_temp_celsius);
            return true;
        }
        false
    }

    /// True when the duty step limiter clamped the last write short of the
    /// target duty for the already-accepted temp. Without this the chain stops
    /// running as soon as the temp goes flat, which leaves the 30 second safety
    /// latch as the only thing that advances a rate-limited ramp.
    /// Compares against `last_applied_temp` rather than the live temp so that
    /// the target is fixed for the length of the ramp and the deadband holds.
    /// Each continuation cycle sets a duty, which resets the safety latch
    /// counter, so threshold hopping cannot hop mid-ramp. The target stays
    /// pinned until the temp leaves the deviance band, which is the deadband
    /// working as intended.
    fn should_continue_stepping(
        metadata: &ChannelSettingMetadata,
        data: &SpeedProfileData,
    ) -> bool {
        let Some(last_applied_duty) = metadata.last_applied_duty else {
            return false; // No duty history yet, nothing to continue.
        };
        debug_assert!(last_applied_duty <= MAX_DUTY, "duty must be in valid range");
        if data.profile.speed_profile.is_empty() {
            return false;
        }
        let (step_increase_min, step_increase_max, step_decrease_min, step_decrease_max) =
            FunctionDutyThresholdPostProcessor::determine_step_sizes(&data.profile.function);
        // A duty diff can never exceed 100, so a max step of 100 never clamps
        // and there is never a remainder to continue. This is the default
        // function, so keep it off the interpolation below entirely.
        if step_increase_max >= MAX_DUTY {
            if step_decrease_max >= MAX_DUTY {
                return false;
            }
        }
        let target_duty =
            utils::interpolate_profile(&data.profile.speed_profile, metadata.last_applied_temp);
        debug_assert!(
            target_duty <= MAX_DUTY,
            "interpolated duty must be in valid range"
        );
        // The post-processor applies a sub-min step at the extremes when
        // bypass_min_at_extremes is set, so the continuation has to keep
        // feeding it or that last remainder falls back to the safety latch.
        let extreme_bypass_active = data.profile.function.bypass_min_at_extremes
            && (target_duty == 0 || target_duty == MAX_DUTY);
        // An equal target needs no step. Anything short of the min step is the
        // limiter's own deadband and belongs to the safety latch, not here.
        match target_duty.cmp(&last_applied_duty) {
            Ordering::Less => {
                extreme_bypass_active || last_applied_duty - target_duty >= step_decrease_min
            }
            Ordering::Greater => {
                extreme_bypass_active || target_duty - last_applied_duty >= step_increase_min
            }
            Ordering::Equal => false,
        }
    }

    fn temp_within_tolerance(temp_to_verify: f64, last_applied_temp: f64, deviance: f64) -> bool {
        temp_to_verify <= (last_applied_temp + deviance)
            && temp_to_verify >= (last_applied_temp - deviance)
    }

    /// Keeps the hysteresis stack sized to the Profile's response delay. A Profile
    /// that stays scheduled on another device channel keeps this metadata across a
    /// Function edit, so the size has to follow the Profile rather than be set once,
    /// otherwise the old response delay stays in force until the daemon restarts.
    /// Shrinking drops the oldest temps, which are the ones a shorter delay must no
    /// longer wait on.
    fn sync_ideal_stack_size(
        metadata: &mut ChannelSettingMetadata,
        profile: &NormalizedGraphProfile,
    ) {
        debug_assert!(
            profile.ideal_stack_size > 0,
            "stack must hold at least one temp"
        );
        if metadata.ideal_stack_size == profile.ideal_stack_size {
            return;
        }
        metadata.ideal_stack_size = profile.ideal_stack_size;
        while metadata.temp_hist_stack.len() > metadata.ideal_stack_size {
            metadata.temp_hist_stack.pop_front();
        }
        debug_assert!(
            metadata.temp_hist_stack.len() <= metadata.ideal_stack_size,
            "stack must be within the ideal size after sync"
        );
    }
}

impl Processor for FunctionStandardPreProcessor {
    fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.function.f_type() == FunctionType::Standard && data.temp.is_none()
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
        let Some(metadata) = metadata_lock.get_mut(&data.profile.profile_uid) else {
            error!("Missing metadata for profile: {}", data.profile.profile_uid);
            return data;
        };
        Self::sync_ideal_stack_size(metadata, &data.profile);
        Self::fill_temp_stack(metadata, data, temp_source_device_option);

        if metadata.temp_hist_stack.len() > metadata.ideal_stack_size {
            metadata.temp_hist_stack.pop_front();
        } else if metadata.last_applied_temp == 0.
            && metadata.temp_hist_stack.len() < metadata.ideal_stack_size
        {
            // Very first run after boot/wakeup, let's apply something right away
            if let Some(&temp_to_apply) = metadata.temp_hist_stack.front() {
                data.temp = Some(temp_to_apply);
                metadata.last_applied_temp = temp_to_apply;
            }
            return data;
        }

        Self::apply_hysteresis(metadata, data)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSettingMetadata {
    pub temp_hist_stack: VecDeque<f64>,
    pub ideal_stack_size: usize,
    pub last_applied_temp: f64,
    pub last_applied_duty: Option<Duty>,
}

impl ChannelSettingMetadata {
    pub fn new() -> Self {
        Self {
            temp_hist_stack: VecDeque::new(),
            ideal_stack_size: 0,
            last_applied_temp: 0.,
            last_applied_duty: None,
        }
    }
}

/// Records the applied duty back into the Standard function's metadata
/// so that `only_downward` bypass decisions use the actual fan duty.
pub struct FunctionStandardPostProcessor {
    channel_settings_metadata: Rc<RefCell<HashMap<ProfileUID, ChannelSettingMetadata>>>,
}

impl Processor for FunctionStandardPostProcessor {
    fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.function.f_type() == FunctionType::Standard && data.duty.is_some()
    }

    fn init_state(&self, _profile_uid: &ProfileUID) {
        // State is managed by the pre-processor.
    }

    fn clear_state(&self, _profile_uid: &ProfileUID) {
        // State is managed by the pre-processor.
    }

    fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let Some(duty) = data.duty else {
            // is_applicable guarantees duty.is_some(); this is unreachable.
            debug_assert!(false, "process called with duty=None");
            return data;
        };
        debug_assert!(duty <= 100, "duty must be in valid range");
        let mut metadata_lock = self.channel_settings_metadata.borrow_mut();
        let Some(metadata) = metadata_lock.get_mut(&data.profile.profile_uid) else {
            error!("Missing metadata for profile: {}", data.profile.profile_uid);
            return data;
        };
        metadata.last_applied_duty = Some(duty);
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

        let target_duty = data.duty.unwrap();
        let extreme_bypass_active = data.profile.function.bypass_min_at_extremes
            && (target_duty == 0 || target_duty == 100);

        #[allow(clippy::cast_possible_wrap)]
        let diff_to_last_duty: i8 = target_duty as i8 - last_duty as i8;
        let duty_has_decreased = diff_to_last_duty < 0;
        let abs_diff_to_last_duty = diff_to_last_duty.unsigned_abs();

        if data.safety_latch_triggered {
            // If the safety-latch is triggered, we want to bypass only the MIN step size thresholds
            // as that is the only case where a duty is possibly not applied.
            // MAX step size limits we will respect in all cases.
            if data.profile.function.threshold_hopping || extreme_bypass_active {
                // For threshold hopping or extreme bypass, we only want to bypass the min thresholds,
                // as that is the only case where duty is not applied.
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

        // Normal flow. Extreme bypass skips the min check; max clamp still applies.
        if duty_has_decreased {
            if abs_diff_to_last_duty < step_decrease_min && extreme_bypass_active.not() {
                None
            } else if abs_diff_to_last_duty > step_decrease_max {
                Some(last_duty - step_decrease_max) // limit to max step size
            } else {
                // within range
                data.duty
            }
        } else if abs_diff_to_last_duty < step_increase_min && extreme_bypass_active.not() {
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
        if let Some(response_delay) = profile.function.response_delay() {
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
    use crate::engine::processors::functions::MIN_TEMP_HIST_STACK_SIZE;
    use crate::engine::processors::functions::{
        calc_ideal_stack_size, FunctionDutyThresholdPostProcessor, FunctionStandardPreProcessor,
    };
    use crate::engine::{NormalizedGraphProfile, SpeedProfileData, TempSource};
    use crate::setting::{Function, FunctionKind};
    use std::ops::Not;

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
        let ideal_stack_size = calc_ideal_stack_size(&function, 1.0);
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
            ideal_stack_size,
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

    // ==================== bypass_min_at_extremes tests ====================

    fn create_test_function_with_bypass_extremes(
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
            bypass_min_at_extremes: true,
            ..Default::default()
        }
    }

    #[test]
    fn apply_step_size_bypass_increase_to_100_below_min() {
        // Goal: with bypass enabled, an increase to exactly 100 should be applied
        // even when the diff is below step_size_min.
        let processor = setup_processor_with_last_duty(98);
        let function = create_test_function_with_bypass_extremes(5, 100, 0, 0, true);
        // Duty increase of 2 (98 -> 100), below step_size_min of 5.
        let data = create_test_data(function, 100, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(100),
            "bypass should apply target=100 even when below min threshold"
        );
    }

    #[test]
    fn apply_step_size_bypass_decrease_to_0_below_min() {
        // Goal: with bypass enabled, a decrease to exactly 0 should be applied
        // even when the diff is below step_size_min.
        let processor = setup_processor_with_last_duty(2);
        let function = create_test_function_with_bypass_extremes(5, 100, 0, 0, true);
        // Duty decrease of 2 (2 -> 0), below step_size_min of 5.
        let data = create_test_data(function, 0, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(0),
            "bypass should apply target=0 even when below min threshold"
        );
    }

    #[test]
    fn apply_step_size_bypass_target_99_not_extreme() {
        // Goal: bypass is strict at exactly 100, so target=99 should still
        // be filtered out by the min threshold.
        let processor = setup_processor_with_last_duty(98);
        let function = create_test_function_with_bypass_extremes(5, 100, 0, 0, true);
        // Duty increase of 1 (98 -> 99), below min, but 99 is not an extreme.
        let data = create_test_data(function, 99, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result, None,
            "bypass should not fire for target=99 (not strictly extreme)"
        );
    }

    #[test]
    fn apply_step_size_bypass_target_1_not_extreme() {
        // Goal: bypass is strict at exactly 0, so target=1 should still
        // be filtered out by the min threshold.
        let processor = setup_processor_with_last_duty(2);
        let function = create_test_function_with_bypass_extremes(5, 100, 0, 0, true);
        // Duty decrease of 1 (2 -> 1), below min, but 1 is not an extreme.
        let data = create_test_data(function, 1, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result, None,
            "bypass should not fire for target=1 (not strictly extreme)"
        );
    }

    #[test]
    fn apply_step_size_bypass_disabled_unchanged_behavior() {
        // Goal: with bypass disabled, an increase to 100 below the min
        // threshold should still be filtered (current pre-feature behavior).
        let processor = setup_processor_with_last_duty(98);
        let function = create_test_function(5, 100, 0, 0, true);
        // bypass_min_at_extremes defaults to false via create_test_function.
        let data = create_test_data(function, 100, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result, None,
            "with bypass disabled, increase below min should still return None"
        );
    }

    #[test]
    fn apply_step_size_bypass_increase_above_max_still_clamps() {
        // Goal: bypass-min-only invariant. Even with bypass on and target=100,
        // a large increase must be clamped to last_duty + step_size_max.
        let processor = setup_processor_with_last_duty(80);
        let function = create_test_function_with_bypass_extremes(2, 10, 0, 0, true);
        // Duty increase of 20 (80 -> 100), above max of 10.
        let data = create_test_data(function, 100, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(90),
            "max clamp must still apply when target=100 and diff > step_size_max"
        );
    }

    #[test]
    fn apply_step_size_bypass_decrease_above_max_still_clamps() {
        // Goal: bypass-min-only invariant on the decrease side.
        let processor = setup_processor_with_last_duty(20);
        let function = create_test_function_with_bypass_extremes(2, 100, 2, 10, true);
        // Duty decrease of 20 (20 -> 0), above decrease max of 10.
        let data = create_test_data(function, 0, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(10),
            "max clamp must still apply when target=0 and diff > step_size_max_decreasing"
        );
    }

    #[test]
    fn apply_step_size_bypass_asymmetric_decrease_to_0() {
        // Goal: with asymmetric step sizes, the decrease bypass uses
        // step_size_min_decreasing. last=10, target=0 with decrease min=15
        // would normally be filtered; bypass should let it through.
        let processor = setup_processor_with_last_duty(10);
        let function = create_test_function_with_bypass_extremes(5, 100, 15, 100, true);
        // Duty decrease of 10 (10 -> 0), below decrease min of 15.
        let data = create_test_data(function, 0, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(0),
            "bypass should apply target=0 below the asymmetric decrease min"
        );
    }

    #[test]
    fn apply_step_size_bypass_safety_latch_no_hopping_extreme_applies() {
        // Goal: bypass is independent of threshold_hopping. With
        // threshold_hopping=false and safety_latch_triggered, the normal
        // path replays last_duty when below min. Bypass should override
        // and apply the extreme target.
        let processor = setup_processor_with_last_duty(98);
        // hopping=false, bypass=true.
        let function = create_test_function_with_bypass_extremes(10, 100, 0, 0, false);
        // Duty increase of 2 (98 -> 100), below min.
        let data = create_test_data(function, 100, true);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(100),
            "bypass should apply target=100 even with safety_latch and threshold_hopping=false"
        );
    }

    #[test]
    fn apply_step_size_bypass_last_equals_target_extreme() {
        // Goal: when last_duty already equals the extreme target (no diff),
        // bypass still fires and re-applies the same value. The device-level
        // dedup handles redundant writes; the post-processor just needs to
        // not return None.
        let processor = setup_processor_with_last_duty(100);
        let function = create_test_function_with_bypass_extremes(5, 100, 0, 0, true);
        let data = create_test_data(function, 100, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(100),
            "bypass should re-apply target=100 even when diff is zero"
        );
    }

    #[test]
    fn apply_step_size_bypass_first_application_target_100() {
        // Goal: regression check. The first-application short-circuit must
        // still fire regardless of bypass state.
        use crate::engine::Processor;
        let processor = FunctionDutyThresholdPostProcessor::new();
        let profile_uid = "test-profile".to_string();
        processor.init_state(&profile_uid);
        let function = create_test_function_with_bypass_extremes(5, 100, 0, 0, true);
        let data = create_test_data(function, 100, false);

        let result = processor.apply_step_size_thresholds(&data);
        assert_eq!(
            result,
            Some(100),
            "first application should apply target=100 (bypass irrelevant here)"
        );
    }

    // ==================== should_bypass_for_upward_temp tests ====================

    fn create_bypass_test_data(
        speed_profile: Vec<(f64, u8)>,
        step_size_min: u8,
    ) -> SpeedProfileData {
        let function = Function {
            step_size_min,
            kind: FunctionKind::Standard {
                deviance: Some(2.0),
                only_downward: Some(true),
                response_delay: Some(5),
            },
            ..Default::default()
        };
        let ideal_stack_size = calc_ideal_stack_size(&function, 1.0);
        SpeedProfileData {
            profile: std::rc::Rc::new(NormalizedGraphProfile {
                profile_uid: "test-profile".to_string(),
                profile_name: "Test Profile".to_string(),
                speed_profile,
                temp_source: TempSource {
                    device_uid: "test-device".to_string(),
                    temp_name: "test-temp".to_string(),
                },
                function,
                poll_rate: 1.0,
                ideal_stack_size,
            }),
            temp: None,
            duty: None,
            processing_started: true,
            safety_latch_triggered: false,
        }
    }

    #[test]
    fn bypass_returns_false_when_temp_steady_and_duty_dropping_beyond_step() {
        // Temp unchanged AND target duty drops beyond step_decrease_min.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 50.0;
        metadata.last_applied_duty = Some(80);
        metadata.temp_hist_stack.push_back(50.0);
        // At 50C with profile (20,20)-(80,100), target duty = 60.
        // step_size_min=2 (symmetric), so threshold = 80-2 = 78.
        // 60 < 78, so neither condition fires.
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            !FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should not bypass when duty drops beyond step_decrease_min"
        );
    }

    #[test]
    fn bypass_returns_true_when_temp_steady_and_duty_within_step() {
        // Temp unchanged but duty drop is within step_decrease_min tolerance.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 50.0;
        metadata.last_applied_duty = Some(62);
        metadata.temp_hist_stack.push_back(50.0);
        // At 50C with profile (20,20)-(80,100), target duty = 60.
        // step_size_min=5 (symmetric), so threshold = 62-5 = 57.
        // 60 >= 57, so duty condition fires.
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 5);

        assert!(
            FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should bypass when duty drop is within step_decrease_min"
        );
    }

    #[test]
    fn bypass_returns_true_when_temp_steady_and_duty_equal() {
        // Temp unchanged but target duty equals last applied duty.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 50.0;
        metadata.last_applied_duty = Some(60);
        metadata.temp_hist_stack.push_back(50.0);
        // At 50C with profile (20,20)-(80,100), target duty = 60.
        // 60 >= 60-2 = 58, so duty condition fires.
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should bypass when target duty equals last applied duty"
        );
    }

    #[test]
    fn bypass_returns_false_when_temp_decreasing_and_duty_dropping_beyond_step() {
        // Temp decreasing AND target duty drops beyond step_decrease_min.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 50.0;
        metadata.last_applied_duty = Some(80);
        metadata.temp_hist_stack.push_back(45.0);
        // At 45C with profile (20,20)-(80,100), target duty = 53.
        // step_size_min=2 (symmetric), so threshold = 80-2 = 78.
        // 53 < 78, so neither condition fires.
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            !FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should not bypass when temp decreasing and duty drops beyond step"
        );
    }

    #[test]
    fn bypass_returns_true_when_temp_rising_even_if_duty_dropping() {
        // Temp is rising - bypass fires regardless of duty.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 49.0;
        metadata.last_applied_duty = Some(80);
        metadata.temp_hist_stack.push_back(50.0);
        // At 50C with profile (20,20)-(80,100), target duty = 60.
        // 60 < 80-2 = 78 (duty drops beyond step), but temp is rising
        // so bypass fires anyway.
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should bypass when temp is rising even if duty would drop"
        );
    }

    #[test]
    fn bypass_returns_true_when_duty_above_last_even_if_temp_steady() {
        // Temp unchanged but target duty above last applied duty.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 50.0;
        metadata.last_applied_duty = Some(20);
        metadata.temp_hist_stack.push_back(50.0);
        // At 50C with profile (20,20)-(80,100), target duty = 60.
        // 60 >= 20-2 = 18, so duty condition fires.
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should bypass when target duty exceeds last applied even if temp steady"
        );
    }

    #[test]
    fn bypass_uses_asymmetric_step_decrease_min() {
        // Asymmetric steps: step_size_min_decreasing differs from step_size_min.
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 50.0;
        metadata.last_applied_duty = Some(65);
        metadata.temp_hist_stack.push_back(50.0);
        // At 50C with profile (20,20)-(80,100), target duty = 60.
        // step_size_min_decreasing=10, so threshold = 65-10 = 55.
        // 60 >= 55, so duty condition fires.
        let function = Function {
            step_size_min: 2,
            step_size_min_decreasing: 10,
            kind: FunctionKind::Standard {
                deviance: Some(2.0),
                only_downward: Some(true),
                response_delay: Some(5),
            },
            ..Default::default()
        };
        let ideal_stack_size = calc_ideal_stack_size(&function, 1.0);
        let mut data = SpeedProfileData {
            profile: std::rc::Rc::new(NormalizedGraphProfile {
                profile_uid: "test-profile".to_string(),
                profile_name: "Test Profile".to_string(),
                speed_profile: vec![(20.0, 20), (80.0, 100)],
                temp_source: TempSource {
                    device_uid: "test-device".to_string(),
                    temp_name: "test-temp".to_string(),
                },
                function,
                poll_rate: 1.0,
                ideal_stack_size,
            }),
            temp: None,
            duty: None,
            processing_started: true,
            safety_latch_triggered: false,
        };

        assert!(
            FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should use asymmetric step_size_min_decreasing"
        );
    }

    #[test]
    fn bypass_returns_false_when_no_duty_history() {
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 30.0;
        // last_applied_duty is None
        metadata.temp_hist_stack.push_back(50.0);
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            !FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should not bypass when there is no duty history"
        );
    }

    #[test]
    fn bypass_returns_false_when_no_temp_history() {
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = 30.0;
        metadata.last_applied_duty = Some(20);
        // temp_hist_stack is empty
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], 2);

        assert!(
            !FunctionStandardPreProcessor::should_bypass_for_upward_temp(&mut metadata, &mut data),
            "should not bypass when there is no temp history"
        );
    }

    // ==================== should_continue_stepping tests ====================

    /// Builds the data for a continuation check. The curve is linear from
    /// 20C/20% to 80C/100%, so 50C interpolates to 60%.
    fn create_continuation_test_data(
        step_size_min: u8,
        step_size_max: u8,
        step_size_min_decreasing: u8,
        step_size_max_decreasing: u8,
    ) -> SpeedProfileData {
        let mut data = create_bypass_test_data(vec![(20.0, 20), (80.0, 100)], step_size_min);
        let profile = std::rc::Rc::get_mut(&mut data.profile).unwrap();
        profile.function.step_size_max = step_size_max;
        profile.function.step_size_min_decreasing = step_size_min_decreasing;
        profile.function.step_size_max_decreasing = step_size_max_decreasing;
        data
    }

    fn continuation_metadata(
        last_applied_temp: f64,
        last_applied_duty: Option<u8>,
    ) -> super::ChannelSettingMetadata {
        let mut metadata = super::ChannelSettingMetadata::new();
        metadata.last_applied_temp = last_applied_temp;
        metadata.last_applied_duty = last_applied_duty;
        metadata
    }

    #[test]
    fn continue_stepping_fires_on_a_clamped_decrease() {
        // Goal: the max decreasing step left the fan short of the target, so
        // the ramp must continue. 50C targets 60%, the fan sits at 80%.
        let data = create_continuation_test_data(1, 100, 1, 1);
        let metadata = continuation_metadata(50.0, Some(80));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data),
            "a 20% remainder above the 1% min step must continue"
        );
    }

    #[test]
    fn continue_stepping_fires_on_a_clamped_increase() {
        // Goal: the same in the increasing direction. 50C targets 60%, the
        // fan sits at 40%.
        let data = create_continuation_test_data(1, 1, 1, 1);
        let metadata = continuation_metadata(50.0, Some(40));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data),
            "a 20% remainder above the 1% min step must continue"
        );
    }

    #[test]
    fn continue_stepping_stops_on_target() {
        // Goal: the ramp must terminate once the fan is on the curve target.
        // Without this the chain would run and re-write the same duty forever.
        let data = create_continuation_test_data(1, 100, 1, 1);
        let metadata = continuation_metadata(50.0, Some(60));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "an exact target match must not continue"
        );
    }

    #[test]
    fn continue_stepping_stops_below_the_min_decreasing_step() {
        // Goal: a remainder under the min step is the limiter's own deadband.
        // Clearing it is the safety latch's job, not the continuation's.
        // 50C targets 60%, the fan sits at 65%, min decreasing step is 10.
        let data = create_continuation_test_data(2, 100, 10, 20);
        let metadata = continuation_metadata(50.0, Some(65));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "a 5% remainder under the 10% min decreasing step must not continue"
        );
    }

    #[test]
    fn continue_stepping_stops_below_the_min_increasing_step() {
        // Goal: the same guard on the increasing side. 50C targets 60%, the
        // fan sits at 55%, min increasing step is 10.
        let data = create_continuation_test_data(10, 20, 0, 0);
        let metadata = continuation_metadata(50.0, Some(55));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "a 5% remainder under the 10% min increasing step must not continue"
        );
    }

    #[test]
    fn continue_stepping_uses_the_symmetric_min_step() {
        // Goal: with step_size_min_decreasing=0 the decrease side must mirror
        // step_size_min, matching determine_step_sizes. Min step 10, so a 5%
        // remainder must not continue.
        let data = create_continuation_test_data(10, 100, 0, 0);
        let metadata = continuation_metadata(50.0, Some(65));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "the symmetric min step must apply to the decrease side"
        );
    }

    #[test]
    fn continue_stepping_skips_interpolation_without_a_max_clamp() {
        // Goal: the default function (step_size_max=100) can never be clamped,
        // so the fast path must reject before interpolating. Verified through
        // behaviour: a remainder that would otherwise continue must not.
        let data = create_continuation_test_data(2, 100, 0, 0);
        let metadata = continuation_metadata(50.0, Some(80));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "a max step of 100 never clamps, so there is nothing to continue"
        );
    }

    #[test]
    fn continue_stepping_fires_when_only_one_direction_is_clamped() {
        // Goal: the fast path needs BOTH directions unclamped. An asymmetric
        // function with a free increase but a clamped decrease must still
        // continue a downward ramp.
        let data = create_continuation_test_data(2, 100, 1, 1);
        let metadata = continuation_metadata(50.0, Some(80));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data),
            "a clamped decrease must continue even when the increase is free"
        );
    }

    #[test]
    fn continue_stepping_fires_below_the_min_step_at_an_extreme_target() {
        // Goal: bypass_min_at_extremes lets the post-processor apply a sub-min
        // step at 0% and 100%, so the continuation must keep feeding it there.
        // Otherwise the last remainder stalls on the 30 second safety latch,
        // which is the exact symptom this fix exists to remove.
        // 80C targets 100%, the fan sits at 95%, min increasing step is 10.
        let mut data = create_continuation_test_data(10, 20, 0, 0);
        std::rc::Rc::get_mut(&mut data.profile)
            .unwrap()
            .function
            .bypass_min_at_extremes = true;
        let metadata = continuation_metadata(80.0, Some(95));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data),
            "a sub-min remainder at an extreme target must continue"
        );
    }

    #[test]
    fn continue_stepping_stops_below_the_min_step_without_extreme_bypass() {
        // Goal: the same setup with the option off must keep the min deadband.
        let data = create_continuation_test_data(10, 20, 0, 0);
        let metadata = continuation_metadata(80.0, Some(95));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "the extreme bypass must not apply when the option is disabled"
        );
    }

    #[test]
    fn continue_stepping_ignores_extreme_bypass_off_the_extremes() {
        // Goal: the bypass is scoped to 0% and 100% only. 50C targets 60%,
        // the fan sits at 65%, min decreasing step is 10, so the 5% remainder
        // stays the limiter's deadband even with the option on.
        let mut data = create_continuation_test_data(2, 100, 10, 20);
        std::rc::Rc::get_mut(&mut data.profile)
            .unwrap()
            .function
            .bypass_min_at_extremes = true;
        let metadata = continuation_metadata(50.0, Some(65));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "a mid-curve target must not use the extreme bypass"
        );
    }

    #[test]
    fn continue_stepping_stops_without_duty_history() {
        // Goal: before the first application there is nothing to continue.
        let data = create_continuation_test_data(1, 100, 1, 1);
        let metadata = continuation_metadata(50.0, None);

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "no duty history must not continue"
        );
    }

    #[test]
    fn continue_stepping_stops_on_an_empty_speed_profile() {
        // Goal: an empty curve cannot be interpolated, so refuse rather than
        // ramp toward the 0% that interpolate_profile returns for one.
        let mut data = create_continuation_test_data(1, 100, 1, 1);
        std::rc::Rc::get_mut(&mut data.profile)
            .unwrap()
            .speed_profile
            .clear();
        let metadata = continuation_metadata(50.0, Some(80));

        assert!(
            FunctionStandardPreProcessor::should_continue_stepping(&metadata, &data).not(),
            "an empty speed profile must not continue"
        );
    }

    // ==================== calc_ideal_stack_size tests ====================

    #[test]
    fn test_calc_ideal_stack_size_zero_delay_clamps_to_min() {
        // Goal: verify that a zero response_delay still leaves room for one temp,
        // since the stack is what the hysteresis reads from.
        let function = Function {
            kind: FunctionKind::Standard {
                deviance: None,
                only_downward: None,
                response_delay: Some(0),
            },
            ..Default::default()
        };
        let size = calc_ideal_stack_size(&function, 1.0);
        assert_eq!(size, MIN_TEMP_HIST_STACK_SIZE as usize);
    }

    #[test]
    fn test_calc_ideal_stack_size_exact() {
        // Goal: verify that response_delay=5 with poll_rate=1.0 gives exactly 5,
        // not 6 (the old off-by-one behavior).
        let function = Function {
            kind: FunctionKind::Standard {
                deviance: None,
                only_downward: None,
                response_delay: Some(5),
            },
            ..Default::default()
        };
        let size = calc_ideal_stack_size(&function, 1.0);
        assert_eq!(size, 5);
    }

    #[test]
    fn test_calc_ideal_stack_size_fractional_poll() {
        // Goal: verify ceiling division with a sub-second poll rate.
        // response_delay=3 / poll_rate=0.5 = 6.0 (exact, no rounding needed).
        let function = Function {
            kind: FunctionKind::Standard {
                deviance: None,
                only_downward: None,
                response_delay: Some(3),
            },
            ..Default::default()
        };
        let size = calc_ideal_stack_size(&function, 0.5);
        assert_eq!(size, 6);
    }
}
