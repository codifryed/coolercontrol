// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::rc::Rc;

use crate::calibration::{self, effective_speed_options, CalibrationStore, FanStateMap};
use crate::config::Config;
use crate::device::{ChannelName, DeviceUID, Duty, UID};
use crate::engine::main::DutyWritersByType;
use crate::engine::processors::functions::{
    calc_ideal_stack_size, FunctionDutyThresholdPostProcessor, FunctionIdentityPreProcessor,
    FunctionSafetyLatchProcessor, FunctionStandardPostProcessor, FunctionStandardPreProcessor,
};
use crate::engine::processors::profiles::GraphProcessor;
use crate::engine::{
    utils, DeviceChannelProfileSetting, NormalizedGraphProfile, Processor, SpeedProfileData,
};
use crate::setting::{Function, FunctionUID, Profile, ProfileType, ProfileUID};
use crate::AllDevices;
use anyhow::{anyhow, Context, Result};
use log::{debug, info, warn};
use moro_local::Scope;

/// Tracks which device channels are in a run of failing duty writes.
///
/// A device that stops answering fails every write on every tick. Ungated, that produced ~9,000
/// identical lines in one user's journal, enough that pasting it into an issue truncated. Only the
/// start and the end of a run are worth a line; the repository owns the escalation and the
/// `error!` that goes with giving up. Bounded by the number of scheduled device channels.
#[derive(Debug, Default)]
struct WriteFailureLog {
    failing_channels_by_device: HashMap<DeviceUID, HashSet<ChannelName>>,
}

impl WriteFailureLog {
    /// Records a failed write. Returns `true` when this failure starts a new run and should be
    /// logged, `false` while a run for this channel is already in progress.
    fn record_failure(&mut self, device_uid: &UID, channel_name: &str) -> bool {
        self.failing_channels_by_device
            .entry(device_uid.clone())
            .or_default()
            .insert(channel_name.to_string())
    }

    /// Records a successful write. Returns `true` when this ends a run and recovery should be
    /// logged, `false` when the channel was not failing.
    ///
    /// Borrowed lookups throughout: this runs on every successful write on every tick, so it must
    /// not allocate a key to discover that nothing is wrong.
    fn record_success(&mut self, device_uid: &UID, channel_name: &str) -> bool {
        let Some(failing_channels) = self.failing_channels_by_device.get_mut(device_uid) else {
            return false;
        };
        let was_failing = failing_channels.remove(channel_name);
        if failing_channels.is_empty() {
            self.failing_channels_by_device.remove(device_uid);
        }
        was_failing
    }
}

struct ProcessorCollection {
    fun_safety_latch: FunctionSafetyLatchProcessor,
    fun_identity_pre: FunctionIdentityPreProcessor,
    fun_std_pre: FunctionStandardPreProcessor,
    graph_proc: GraphProcessor,
    fun_duty_thresh_post: FunctionDutyThresholdPostProcessor,
    fun_std_post: FunctionStandardPostProcessor,
}

/// This is the commander for Graph Profile Processing.
/// This enables the use of a scheduler to automatically set the speed on devices in relation to
/// temperature sources that are not supported on the device itself.
/// For ex. Fan and Pump controls based on CPU Temp,
/// or profile speed settings for devices that only support fixed speeds.
pub struct GraphProfileCommander {
    all_devices: AllDevices,
    duty_writers_by_type: DutyWritersByType,
    scheduled_settings:
        RefCell<HashMap<Rc<NormalizedGraphProfile>, HashSet<DeviceChannelProfileSetting>>>,
    config: Rc<Config>,
    processors: ProcessorCollection,
    /// The last calculated Option<Duty> for each Graph Profile.
    /// This allows other Profiles to use the output of a Graph Profile.
    pub process_output_cache: RefCell<HashMap<ProfileUID, Option<Duty>>>,
    calibration_store: Rc<CalibrationStore>,
    fan_state_map: Rc<FanStateMap>,
    write_failure_log: RefCell<WriteFailureLog>,
}

impl GraphProfileCommander {
    pub fn new(
        all_devices: AllDevices,
        duty_writers_by_type: DutyWritersByType,
        config: Rc<Config>,
        calibration_store: Rc<CalibrationStore>,
        fan_state_map: Rc<FanStateMap>,
    ) -> Self {
        Self {
            duty_writers_by_type,
            scheduled_settings: RefCell::new(HashMap::new()),
            config,
            processors: {
                let fun_std_pre = FunctionStandardPreProcessor::new(all_devices.clone());
                let fun_std_post = fun_std_pre.create_post_processor();
                ProcessorCollection {
                    fun_safety_latch: FunctionSafetyLatchProcessor::new(),
                    fun_identity_pre: FunctionIdentityPreProcessor::new(all_devices.clone()),
                    fun_std_pre,
                    graph_proc: GraphProcessor::new(),
                    fun_duty_thresh_post: FunctionDutyThresholdPostProcessor::new(),
                    fun_std_post,
                }
            },
            all_devices,
            process_output_cache: RefCell::new(HashMap::new()),
            calibration_store,
            fan_state_map,
            write_failure_log: RefCell::new(WriteFailureLog::default()),
        }
    }

    /// This is called on both the initial setting of Settings and when a Profile is updated
    pub fn schedule_setting(
        &self,
        device_channel: DeviceChannelProfileSetting,
        profile: &Profile,
    ) -> Result<()> {
        if profile.p_type() != ProfileType::Graph {
            return Err(anyhow!(
                "Only Graph Profiles are supported for scheduling in the GraphProfileCommander"
            ));
        }
        let normalized_profile_setting = self.normalize_profile_setting(
            device_channel.device_uid(),
            device_channel.channel_name(),
            profile,
        )?;
        let mut settings_lock = self.scheduled_settings.borrow_mut();
        if let Some(mut existing_device_channels) =
            settings_lock.remove(&normalized_profile_setting)
        {
            // We replace the existing NormalizedGraphProfile if it exists to make sure it's
            // internal settings are up-to-date
            existing_device_channels.insert(device_channel);
            settings_lock.insert(
                Rc::new(normalized_profile_setting),
                existing_device_channels,
            );
            // When applying a profile to an additional device_channel, we re-init the safety
            // latch so that the setting is applied right away.
            self.processors.fun_safety_latch.init_state(&profile.uid);
        } else {
            let mut new_device_channels = HashSet::new();
            new_device_channels.insert(device_channel);
            settings_lock.insert(Rc::new(normalized_profile_setting), new_device_channels);
            self.processors.fun_safety_latch.init_state(&profile.uid);
            self.processors
                .fun_duty_thresh_post
                .init_state(&profile.uid);
            self.processors.fun_std_pre.init_state(&profile.uid);
            self.process_output_cache
                .borrow_mut()
                .insert(profile.uid.clone(), None);
        }
        Ok(())
    }

    pub fn clear_channel_setting(&self, device_uid: &DeviceUID, channel_name: &str) {
        // the mix commander will have multiple profiles for the same channel, so we need a Vec:
        let mut profiles_to_remove = HashSet::new();
        let device_channel_setting = DeviceChannelProfileSetting::Graph {
            // device_uid and channel_name are used to identify the setting, the
            // DeviceChannelProfileSetting variant is irrelevant for the hash.
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_string(),
        };
        let mut scheduled_settings_lock = self.scheduled_settings.borrow_mut();
        for (profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel_setting);
            if device_channels.is_empty() {
                self.processors
                    .fun_safety_latch
                    .clear_state(&profile.profile_uid);
                self.processors
                    .fun_duty_thresh_post
                    .clear_state(&profile.profile_uid);
                self.processors
                    .fun_std_pre
                    .clear_state(&profile.profile_uid);
                self.process_output_cache
                    .borrow_mut()
                    .remove(&profile.profile_uid);
                profiles_to_remove.insert(profile.profile_uid.clone());
            }
        }
        scheduled_settings_lock
            .retain(|profile, _| profiles_to_remove.contains(&profile.profile_uid).not());
    }

    /// This method processes all scheduled profiles and updates the output cache.
    /// This should be called first and only once per update cycle.
    pub fn process_all_profiles(&self) {
        let mut output_cache_lock = self.process_output_cache.borrow_mut();
        for normalized_profile in self.scheduled_settings.borrow().keys() {
            let optional_duty_to_set = self.process_speed_setting(normalized_profile);
            if let Some(cache) = output_cache_lock.get_mut(&normalized_profile.profile_uid) {
                *cache = optional_duty_to_set;
            }
        }
    }

    /// Applies the speed of all devices that have a scheduled Graph Profile setting.
    /// Normally triggered by a loop/timer.
    pub fn update_speeds<'s>(&'s self, scope: &'s Scope<'s, 's, Result<()>>) {
        for (device_uid, channel_duties_to_set) in self.collect_processed_outputs() {
            scope.spawn(async move {
                for (channel_name, duty_to_set) in channel_duties_to_set {
                    self.set_device_speed(&device_uid, &channel_name, duty_to_set)
                        .await;
                }
            });
        }
    }

    /// Collects all the processed outputs for all scheduled Graph Profiles.
    fn collect_processed_outputs(&self) -> HashMap<DeviceUID, Vec<(ChannelName, Duty)>> {
        let settings = self.scheduled_settings.borrow();
        let mut output_to_apply = HashMap::with_capacity(settings.len());
        for (normalized_profile, device_channels) in settings.iter() {
            let optional_duty_to_set = self.process_output_cache.borrow()
                [&normalized_profile.profile_uid]
                .as_ref()
                .copied();
            let Some(duty_to_set) = optional_duty_to_set else {
                continue;
            };
            for device_channel in device_channels {
                // We only apply Graph Profiles directly applied to fan channels, as we
                // can also schedule Overlay Member Profiles and Mix Member Profiles,
                // which need to be handled properly upstream.
                if let DeviceChannelProfileSetting::Graph {
                    device_uid,
                    channel_name,
                } = device_channel
                {
                    output_to_apply
                        .entry(device_uid.clone())
                        .or_insert_with(Vec::new)
                        .push((channel_name.clone(), duty_to_set));
                }
            }
        }
        output_to_apply
    }

    fn process_speed_setting(
        &self,
        normalized_profile: &Rc<NormalizedGraphProfile>,
    ) -> Option<Duty> {
        SpeedProfileData {
            temp: None,
            duty: None,
            profile: Rc::clone(normalized_profile),
            processing_started: false,
            safety_latch_triggered: false,
        }
        .apply(&self.processors.fun_safety_latch)
        .apply(&self.processors.fun_identity_pre)
        .apply(&self.processors.fun_std_pre)
        .apply(&self.processors.graph_proc)
        .apply(&self.processors.fun_duty_thresh_post)
        .apply(&self.processors.fun_std_post)
        .apply(&self.processors.fun_safety_latch)
        .return_processed_duty()
    }

    /// Sets the speed of a device. This is normally called by the `update_speeds` method
    /// from various Commanders. This keeps this logic in one place.
    ///
    /// The user-facing `duty_to_set` is routed through `calibration::dispatch`,
    /// which applies the per-channel true-duty mapping (and kick-then-settle
    /// orchestration) for calibrated smooth channels and passes through
    /// uncalibrated channels unchanged. The writer is looked up from the
    /// pre-built `duty_writers_by_type` cache so the hot path has no clones or
    /// allocations.
    pub async fn set_device_speed(&self, device_uid: &UID, channel_name: &str, duty_to_set: u8) {
        let (device_type, device_name) = {
            // this will block if reference is held, thus clone()
            let device_lock = self.all_devices[device_uid].borrow();
            (device_lock.d_type, device_lock.name.clone())
        };
        debug!(
            "Applying scheduled Speed Profile for device: {device_name}:{device_uid} \
            channel: {channel_name}; DUTY: {duty_to_set}"
        );
        let Some(writer) = self.duty_writers_by_type.get(&device_type) else {
            return;
        };
        let write_result = calibration::dispatch(
            &self.fan_state_map,
            &self.calibration_store,
            writer,
            device_uid.clone(),
            channel_name.to_string(),
            duty_to_set,
        )
        .await;
        self.log_write_outcome(device_uid, &device_name, channel_name, write_result.err());
    }

    /// Logs the start and the end of a run of failing duty writes for one device channel, and
    /// nothing in between. The device and channel are named here because the error alone does not
    /// carry them, which made the untargeted spam hard to attribute to a specific cooler.
    fn log_write_outcome(
        &self,
        device_uid: &UID,
        device_name: &str,
        channel_name: &str,
        write_error: Option<anyhow::Error>,
    ) {
        let mut write_failure_log = self.write_failure_log.borrow_mut();
        let Some(err) = write_error else {
            if write_failure_log.record_success(device_uid, channel_name) {
                info!(
                    "Applying duties to {device_name}:{device_uid} channel {channel_name} \
                    is working again."
                );
            }
            return;
        };
        if write_failure_log.record_failure(device_uid, channel_name) {
            warn!(
                "Error applying Graph/Mix Profile calculated duty to \
                {device_name}:{device_uid} channel {channel_name} - {err}. \
                Further failures for this channel are suppressed until it recovers."
            );
        }
    }

    fn normalize_profile_setting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        profile: &Profile,
    ) -> Result<NormalizedGraphProfile> {
        let (Some(temp_source), Some(speed_profile)) =
            (profile.temp_source(), profile.speed_profile())
        else {
            return Err(anyhow!(
                "Not enough info to schedule a manual speed profile"
            ));
        };
        let temp_source_device = self
            .all_devices
            .get(temp_source.device_uid.as_str())
            .with_context(|| {
                format!(
                    "temp_source Device must currently be present to schedule speed: {}",
                    temp_source.device_uid
                )
            })?;
        let max_temp = f64::from(temp_source_device.borrow().info.temp_max);
        let max_duty = self.get_max_device_duty(device_uid, channel_name)?;
        let function = self.get_profiles_function(&profile.function_uid)?;
        let normalized_speed_profile = utils::normalize_profile(speed_profile, max_temp, max_duty);
        let poll_rate = self.config.get_settings()?.poll_rate;
        let ideal_stack_size = calc_ideal_stack_size(&function, poll_rate);
        Ok(NormalizedGraphProfile {
            profile_uid: profile.uid.clone(),
            profile_name: profile.name.clone(),
            speed_profile: normalized_speed_profile,
            temp_source: temp_source.clone(),
            function,
            poll_rate,
            ideal_stack_size,
        })
    }

    fn get_max_device_duty(&self, device_uid: &UID, channel_name: &str) -> Result<Duty> {
        let device_to_schedule = self.all_devices.get(device_uid).with_context(|| {
            format!("Target Device to schedule speed must be present: {device_uid}")
        })?;
        let device_lock = device_to_schedule.borrow();
        let channel_info = device_lock.info.channels.get(channel_name).with_context(|| {
            format!(
                "Channel Info for channel: {channel_name} in setting must be present for target device: {device_uid}"
            )
        })?;
        let raw_speed_options = channel_info.speed_options().with_context(|| {
            format!("Speed Options must be present for target device: {device_uid}")
        })?;
        // Consult the calibration store so a Smooth calibration's
        // RPM-normalised range (0-100 true-duty) is respected by the
        // profile normaliser. Without this, `normalize_profile` would
        // silently cap a calibrated Graph profile's high points at the
        // device's raw `max_duty` (e.g. 80 on some AMDGPU configs),
        // discarding the user's intent above that threshold.
        let key = (device_uid.clone(), channel_name.to_string());
        let calibration = self.calibration_store.get(&key);
        let effective = effective_speed_options(raw_speed_options, calibration.as_ref());
        Ok(effective.max_duty)
    }

    fn get_profiles_function(&self, function_uid: &FunctionUID) -> Result<Function> {
        self.config.get_function(function_uid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVICE: &str = "device-uid";
    const OTHER_DEVICE: &str = "other-device-uid";
    const FAN: &str = "fan1";
    const PUMP: &str = "pump";

    #[test]
    fn a_run_of_failures_logs_once_and_recovery_logs_once() {
        // Goal: the whole point of the gate. A device that stops answering fails every write on
        // every tick; only the first failure and the recovery are worth a line.
        // Method: many failures in a row, then a success, then more failures.
        let mut write_failure_log = WriteFailureLog::default();
        assert!(write_failure_log.record_failure(&DEVICE.to_string(), FAN));
        for _ in 0..1_000 {
            assert!(write_failure_log
                .record_failure(&DEVICE.to_string(), FAN)
                .not());
        }
        assert!(write_failure_log.record_success(&DEVICE.to_string(), FAN));
        // A new run after a recovery is a new episode and logs again.
        assert!(write_failure_log.record_failure(&DEVICE.to_string(), FAN));
    }

    #[test]
    fn each_channel_gets_its_own_run() {
        // Goal: one channel failing must not silence another. A single channel can fail on its
        // own while the rest of the device is fine, and per-device gating would hide that.
        // Method: two channels on one device, failing and recovering independently.
        let mut write_failure_log = WriteFailureLog::default();
        assert!(write_failure_log.record_failure(&DEVICE.to_string(), FAN));
        assert!(write_failure_log.record_failure(&DEVICE.to_string(), PUMP));
        assert!(write_failure_log.record_success(&DEVICE.to_string(), FAN));
        assert!(write_failure_log.record_failure(&DEVICE.to_string(), FAN));
        assert!(write_failure_log.record_success(&DEVICE.to_string(), PUMP));
    }

    #[test]
    fn devices_do_not_share_channel_state() {
        // Goal: two devices with identically named channels are tracked separately, which matters
        // for the two identically named AIOs this gate was written for.
        // Method: the same channel name failing on two different device UIDs.
        let mut write_failure_log = WriteFailureLog::default();
        assert!(write_failure_log.record_failure(&DEVICE.to_string(), FAN));
        assert!(write_failure_log.record_failure(&OTHER_DEVICE.to_string(), FAN));
        assert!(write_failure_log.record_success(&DEVICE.to_string(), FAN));
        // The other device is still failing, so its recovery is still pending.
        assert!(write_failure_log.record_success(&OTHER_DEVICE.to_string(), FAN));
    }

    #[test]
    fn a_success_with_nothing_failing_logs_nothing() {
        // Goal: the negative space and the hot path. Nearly every write succeeds with nothing
        // failing, and that must report no recovery.
        // Method: successes against an empty log, and against a device that never failed.
        let mut write_failure_log = WriteFailureLog::default();
        assert!(write_failure_log
            .record_success(&DEVICE.to_string(), FAN)
            .not());
        write_failure_log.record_failure(&OTHER_DEVICE.to_string(), PUMP);
        assert!(write_failure_log
            .record_success(&DEVICE.to_string(), FAN)
            .not());
        assert!(write_failure_log
            .record_success(&OTHER_DEVICE.to_string(), FAN)
            .not());
    }

    #[test]
    fn a_recovered_device_is_dropped_from_the_map() {
        // Goal: the tracking map must not grow without bound over a long uptime; a device with no
        // failing channels left carries no state. Method: fail then recover every channel.
        let mut write_failure_log = WriteFailureLog::default();
        write_failure_log.record_failure(&DEVICE.to_string(), FAN);
        write_failure_log.record_failure(&DEVICE.to_string(), PUMP);
        write_failure_log.record_success(&DEVICE.to_string(), FAN);
        assert_eq!(write_failure_log.failing_channels_by_device.len(), 1);
        write_failure_log.record_success(&DEVICE.to_string(), PUMP);
        assert!(write_failure_log.failing_channels_by_device.is_empty());
    }
}
