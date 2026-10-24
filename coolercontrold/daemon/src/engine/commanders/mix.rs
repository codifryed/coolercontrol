// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{Div, Not};
use std::rc::Rc;

use anyhow::{anyhow, Result};
use log::error;
use moro_local::Scope;
use serde::{Deserialize, Serialize};

use crate::device::{Duty, UID};
use crate::engine::commanders::graph::GraphProfileCommander;
use crate::engine::commanders::{spawn_duty_writes, DutyBuffer, OutputDedupState};
use crate::engine::{ChannelDuty, DeviceChannelProfileSetting, ScheduledChannel};
use crate::setting::{Profile, ProfileMixFunctionType, ProfileType, ProfileUID};

type MixProfile = Profile;

/// A Commander for Mix Profile Processing.
/// This has its own `GraphProfile` Commander for processing each member profile. It handles
/// scheduling, caching, as well as processing of the `MixProfileFunction`.
pub struct MixProfileCommander {
    graph_commander: Rc<GraphProfileCommander>,
    scheduled_settings: RefCell<HashMap<Rc<NormalizedMixProfile>, HashSet<ScheduledChannel>>>,
    all_last_applied_duties: RefCell<HashMap<ProfileUID, Duty>>,
    /// The last calculated Option<Duty> for each Mix Profile.
    /// This allows other Profiles to use the output of a Mix Profile.
    pub process_output_cache: RefCell<HashMap<ProfileUID, Option<Duty>>>,
    output_dedup: RefCell<HashMap<ProfileUID, OutputDedupState>>,
    duty_buffer: DutyBuffer,
}

impl MixProfileCommander {
    pub fn new(graph_commander: Rc<GraphProfileCommander>) -> Self {
        Self {
            graph_commander,
            scheduled_settings: RefCell::new(HashMap::new()),
            all_last_applied_duties: RefCell::new(HashMap::new()),
            process_output_cache: RefCell::new(HashMap::new()),
            output_dedup: RefCell::new(HashMap::new()),
            duty_buffer: DutyBuffer::default(),
        }
    }

    pub fn schedule_setting(
        &self,
        device_channel: DeviceChannelProfileSetting,
        mix_profile: &MixProfile,
        member_profiles: Vec<Profile>,
        member_sub_profiles: &HashMap<ProfileUID, Vec<Profile>>,
    ) -> Result<()> {
        if mix_profile.p_type() != ProfileType::Mix {
            return Err(anyhow!(
                "Only Mix Profiles are supported for scheduling in the MixProfileCommander"
            ));
        }
        if mix_profile.mix_function_type().is_none() {
            return Err(anyhow!(
                "Mix Function Type must be present for a Mix Profile"
            ));
        }
        if member_profiles.is_empty() {
            return Err(anyhow!("Member profiles must be present for a Mix Profile"));
        }
        let normalized_mix_setting = Self::normalize_mix_setting(mix_profile, &member_profiles);
        self.schedule_member_profiles(&device_channel, member_profiles, member_sub_profiles)?;
        let mut settings_lock = self.scheduled_settings.borrow_mut();
        if let Some(mut existing_device_channels) = settings_lock.remove(&normalized_mix_setting) {
            // We replace the existing NormalizedMixProfile if it exists to make sure it's
            // internal settings are up to date
            existing_device_channels.insert(Rc::new(device_channel));
            settings_lock.insert(Rc::new(normalized_mix_setting), existing_device_channels);
            // Reset the dedup state so the newly-added device_channel
            // gets the current duty applied on the next tick. Without
            // this, the existing entry's `last_applied_duty` matches
            // and `should_apply` suppresses the write for every
            // channel under this profile until either the duty
            // changes or the safety latch fires.
            self.output_dedup
                .borrow_mut()
                .insert(mix_profile.uid.clone(), OutputDedupState::new());
        } else {
            let mut new_device_channels = HashSet::new();
            new_device_channels.insert(Rc::new(device_channel));
            settings_lock.insert(Rc::new(normalized_mix_setting), new_device_channels);
            self.process_output_cache
                .borrow_mut()
                .insert(mix_profile.uid.clone(), None);
            self.output_dedup
                .borrow_mut()
                .insert(mix_profile.uid.clone(), OutputDedupState::new());
        }
        Ok(())
    }

    fn schedule_member_profiles(
        &self,
        device_channel: &DeviceChannelProfileSetting,
        member_profiles: Vec<Profile>,
        member_sub_profiles: &HashMap<ProfileUID, Vec<Profile>>,
    ) -> Result<()> {
        // all graph profiles for this DeviceChannelProfileSetting are already cleared
        for member_profile in member_profiles {
            match member_profile.p_type() {
                ProfileType::Graph => {
                    self.graph_commander
                        .schedule_setting(device_channel.clone(), &member_profile)?;
                }
                ProfileType::Mix => {
                    // Schedule the child Mix's Graph sub-members via graph_commander.
                    // Fixed sub-members are skipped here; their constant duties are
                    // stored directly in the child's NormalizedMixProfile.
                    if let Some(sub_profiles) = member_sub_profiles.get(&member_profile.uid) {
                        for sub_profile in sub_profiles {
                            if sub_profile.p_type() == ProfileType::Graph {
                                self.graph_commander
                                    .schedule_setting(device_channel.clone(), sub_profile)?;
                            }
                        }
                    }
                    // Add the child Mix itself to scheduled_settings with its own
                    // NormalizedMixProfile entry
                    let sub_members = member_sub_profiles
                        .get(&member_profile.uid)
                        .cloned()
                        .unwrap_or_default();
                    let normalized_child =
                        Self::normalize_mix_setting(&member_profile, &sub_members);
                    let mut settings_lock = self.scheduled_settings.borrow_mut();
                    if let Some(mut existing_device_channels) =
                        settings_lock.remove(&normalized_child)
                    {
                        existing_device_channels.insert(Rc::new(device_channel.clone()));
                        settings_lock.insert(Rc::new(normalized_child), existing_device_channels);
                    } else {
                        let mut new_device_channels = HashSet::new();
                        new_device_channels.insert(Rc::new(device_channel.clone()));
                        settings_lock.insert(Rc::new(normalized_child), new_device_channels);
                        self.process_output_cache
                            .borrow_mut()
                            .insert(member_profile.uid.clone(), None);
                    }
                }
                ProfileType::Fixed => {
                    // Fixed profiles produce a constant duty stored in NormalizedMixProfile.
                    // No graph commander scheduling needed.
                }
                _ => {
                    return Err(anyhow!(
                        "Only Graph, Fixed, and Mix Profiles are supported as Mix members"
                    ));
                }
            }
            if self
                .all_last_applied_duties
                .borrow()
                .contains_key(&member_profile.uid)
                .not()
            {
                self.all_last_applied_duties
                    .borrow_mut()
                    .insert(member_profile.uid.clone(), 0);
            }
        }
        Ok(())
    }

    pub fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        let mut mix_profiles_to_remove: Vec<Rc<NormalizedMixProfile>> = Vec::new();
        let device_channel = DeviceChannelProfileSetting::Mix {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_string(),
        };
        let mut scheduled_settings_lock = self.scheduled_settings.borrow_mut();
        for (mix_profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel);
            if device_channels.is_empty() {
                mix_profiles_to_remove.push(Rc::clone(mix_profile));
                self.process_output_cache
                    .borrow_mut()
                    .remove(&mix_profile.profile_uid);
                self.output_dedup
                    .borrow_mut()
                    .remove(&mix_profile.profile_uid);
            }
        }
        for mix_profile in mix_profiles_to_remove {
            scheduled_settings_lock.remove(&mix_profile);
        }
        self.graph_commander
            .clear_channel_setting(device_uid, channel_name);
    }

    /// This method processes all scheduled profiles and updates the output cache.
    /// This should be called very early, right after the `GraphProfileCommander` processes,
    /// and only once per update cycle.
    ///
    /// Two-pass processing (mirrors `custom_sensors_repo::update_statuses()`):
    /// - Pass 1: Process child Mix profiles (those with no Mix sub-members)
    /// - Pass 2: Process parent Mix profiles (those with Mix sub-members),
    ///   reading child Mix output from the `process_output_cache` populated in pass 1
    ///
    /// Each result is written in place. A child reads no Mix output, and a parent reads only
    /// its children's (nesting is single-level), so no write is read later in its own pass.
    pub fn process_all_profiles(&self) {
        self.update_last_applied_duties();
        let graph_duties = self.graph_commander.process_output_cache.borrow();
        let last_applied_duties = self.all_last_applied_duties.borrow();
        let scheduled = self.scheduled_settings.borrow();
        let mut output_cache = self.process_output_cache.borrow_mut();
        for parents_pass in [false, true] {
            for mix_profile in scheduled.keys() {
                let is_parent = mix_profile.member_mix_profile_uids.is_empty().not();
                if is_parent != parents_pass {
                    continue;
                }
                debug_assert!(
                    mix_profile.member_mix_profile_uids.iter().all(|child_uid| {
                        scheduled
                            .keys()
                            .filter(|child| &child.profile_uid == child_uid)
                            .all(|child| child.member_mix_profile_uids.is_empty())
                    }),
                    "Mix nesting must be single-level"
                );
                let result = Self::process_single_mix_profile(
                    mix_profile,
                    &graph_duties,
                    &output_cache,
                    &last_applied_duties,
                );
                if let Some(cache) = output_cache.get_mut(&mix_profile.profile_uid) {
                    *cache = result;
                }
            }
        }
    }

    /// Processes a single Mix profile and returns the calculated duty.
    fn process_single_mix_profile(
        mix_profile: &NormalizedMixProfile,
        graph_duties: &HashMap<ProfileUID, Option<Duty>>,
        mix_duties: &HashMap<ProfileUID, Option<Duty>>,
        last_applied_duties: &HashMap<ProfileUID, Duty>,
    ) -> Option<Duty> {
        let mut fold = MixFold::default();
        let mut members_have_no_output = true;
        for member_profile_uid in &mix_profile.member_profile_uids {
            // Fixed members have a constant duty - no cache lookup needed.
            if let Some(&fixed_duty) = mix_profile
                .member_fixed_profile_duties
                .get(member_profile_uid)
            {
                members_have_no_output = false;
                fold.push(fixed_duty);
                continue;
            }
            // Look up the member's output from the appropriate cache
            let output = if mix_profile
                .member_mix_profile_uids
                .contains(member_profile_uid)
            {
                mix_duties.get(member_profile_uid)
            } else {
                graph_duties.get(member_profile_uid)
            };
            let Some(output) = output else {
                error!(
                    "Mix Profile calculation for {} skipped because of missing member output duty ",
                    mix_profile.profile_uid
                );
                return None;
            };
            let duty_value_for_calculation = if let Some(duty) = output {
                members_have_no_output = false;
                duty
            } else {
                // We need the last applied values as a backup from all member profiles when ANY
                // profile produces output, so we can properly compare the results and apply the
                // correct Duty.
                let Some(last_duty) = last_applied_duties.get(member_profile_uid) else {
                    error!(
                        "Mix Profile {} skipped: member {} has no output \
                         and no last applied duty fallback",
                        mix_profile.profile_uid, member_profile_uid
                    );
                    return None;
                };
                last_duty
            };
            fold.push(*duty_value_for_calculation);
        }
        if members_have_no_output {
            return None;
        }
        Some(fold.finish(mix_profile.mix_function))
    }

    /// Processes all the member Profiles and applies the appropriate output per Mix Profile.
    /// Normally triggered by a loop/timer.
    pub fn update_speeds<'s>(&'s self, scope: &'s Scope<'s, 's, Result<()>>) {
        let mut duties = self.duty_buffer.take();
        self.collect_duties_to_apply(&mut duties);
        spawn_duty_writes(scope, &self.graph_commander, &self.duty_buffer, duties);
    }

    /// Updates the last applied duties for all profiles. This is done somewhat proactively so
    /// that when a member profile is first used, it has a proper last applied duty to compare to.
    fn update_last_applied_duties(&self) {
        let mut last_applied_duties = self.all_last_applied_duties.borrow_mut();
        // Update from graph commander output cache
        let graph_duties = self.graph_commander.process_output_cache.borrow();
        for (profile_uid, output) in graph_duties.iter() {
            let Some(duty) = output else {
                continue;
            };
            if let Some(d) = last_applied_duties.get_mut(profile_uid) {
                *d = *duty;
            } else {
                last_applied_duties.insert(profile_uid.clone(), *duty);
            }
        }
        // Also update from own process output cache (for Mix members used as children)
        let mix_duties = self.process_output_cache.borrow();
        for (profile_uid, output) in mix_duties.iter() {
            let Some(duty) = output else {
                continue;
            };
            if let Some(d) = last_applied_duties.get_mut(profile_uid) {
                *d = *duty;
            } else {
                last_applied_duties.insert(profile_uid.clone(), *duty);
            }
        }
    }

    /// Collects the duties to apply for all scheduled Mix Profiles from the output cache into
    /// `duties`. Duties that haven't changed since the last application are suppressed unless
    /// the safety latch counter has been reached.
    fn collect_duties_to_apply(&self, duties: &mut Vec<ChannelDuty>) {
        let output_cache = self.process_output_cache.borrow();
        let mut dedup_lock = self.output_dedup.borrow_mut();
        // Each scheduled profile is asked once per tick, so each dedup state counts one tick.
        let duty_to_apply = |uid: &ProfileUID| {
            let duty = output_cache[uid]?;
            dedup_lock
                .get_mut(uid)
                .is_none_or(|state| state.should_apply(duty))
                .then_some(duty)
        };
        Self::collect_duties_from_scheduled(
            &self.scheduled_settings.borrow(),
            duty_to_apply,
            duties,
        );
    }

    /// For child Mix profiles (those referenced by a parent's `member_mix_profile_uids`),
    /// only device channels owned by the parent are skipped. The child's own directly
    /// assigned channels are still applied. This allows a Mix Profile to be both directly
    /// applied to a channel AND used as a member of another Mix Profile.
    fn collect_duties_from_scheduled(
        scheduled: &HashMap<Rc<NormalizedMixProfile>, HashSet<ScheduledChannel>>,
        mut duty_to_apply: impl FnMut(&ProfileUID) -> Option<Duty>,
        duties: &mut Vec<ChannelDuty>,
    ) {
        for (mix_profile, device_channels) in scheduled {
            let Some(duty_to_set) = duty_to_apply(&mix_profile.profile_uid) else {
                continue;
            };
            for device_channel in device_channels {
                // Skip channels owned by a parent Mix - the parent applies its own
                // calculated duty to those channels.
                if Self::is_owned_by_a_parent(scheduled, &mix_profile.profile_uid, device_channel) {
                    continue;
                }
                // We only apply Mix Profiles directly applied to fan channels, as we
                // can also schedule Overlay Member Profiles,
                // which need to be handled properly upstream.
                if let DeviceChannelProfileSetting::Mix { .. } = **device_channel {
                    duties.push((Rc::clone(device_channel), duty_to_set));
                }
            }
        }
    }

    /// Whether a parent of the Mix `child_uid` is applied to `device_channel`, which the child
    /// then leaves to it. A scan rather than a lookup map: a child's duty changes on most ticks,
    /// so a map built per tick allocated on most ticks, and there are only a few Mix profiles.
    fn is_owned_by_a_parent(
        scheduled: &HashMap<Rc<NormalizedMixProfile>, HashSet<ScheduledChannel>>,
        child_uid: &ProfileUID,
        device_channel: &ScheduledChannel,
    ) -> bool {
        scheduled.iter().any(|(parent, parent_channels)| {
            parent.member_mix_profile_uids.contains(child_uid)
                && parent_channels.contains(&**device_channel)
        })
    }

    /// Applies the mix function to the member duties.
    #[cfg(test)]
    fn apply_mix_function(
        member_duties: impl IntoIterator<Item = Duty>,
        mix_function: ProfileMixFunctionType,
    ) -> Duty {
        let mut fold = MixFold::default();
        for duty in member_duties {
            fold.push(duty);
        }
        fold.finish(mix_function)
    }

    fn normalize_mix_setting(
        profile: &Profile,
        member_profiles: &[Profile],
    ) -> NormalizedMixProfile {
        debug_assert!(
            profile.mix_function_type().is_some(),
            "mix_function_type must be validated before normalization"
        );
        debug_assert!(
            member_profiles.is_empty().not(),
            "member_profiles must be validated before normalization"
        );
        NormalizedMixProfile {
            profile_uid: profile.uid.clone(),
            // Validated in schedule_setting before reaching here.
            mix_function: profile.mix_function_type().unwrap(),
            member_mix_profile_uids: member_profiles
                .iter()
                .filter(|p| p.p_type() == ProfileType::Mix)
                .map(|p| p.uid.clone())
                .collect(),
            member_fixed_profile_duties: member_profiles
                .iter()
                .filter(|p| p.p_type() == ProfileType::Fixed)
                .filter_map(|p| p.speed_fixed().map(|duty| (p.uid.clone(), duty)))
                .collect(),
            member_profile_uids: member_profiles.iter().map(|p| p.uid.clone()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMixProfile {
    profile_uid: ProfileUID,
    mix_function: ProfileMixFunctionType,
    member_profile_uids: Vec<ProfileUID>,
    /// Subset of `member_profile_uids` that are Mix-type profiles (children).
    member_mix_profile_uids: Vec<ProfileUID>,
    /// Fixed-duty members: constant duty values keyed by profile UID.
    member_fixed_profile_duties: HashMap<ProfileUID, Duty>,
}

impl Default for NormalizedMixProfile {
    fn default() -> Self {
        Self {
            profile_uid: String::default(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: Vec::new(),
            member_mix_profile_uids: Vec::new(),
            member_fixed_profile_duties: HashMap::new(),
        }
    }
}

impl PartialEq for NormalizedMixProfile {
    /// Only compare `ProfileUID`
    /// This allows us to update the Profile settings easily, and the UID is what matters anyway.
    fn eq(&self, other: &Self) -> bool {
        self.profile_uid == other.profile_uid
    }
}

impl Eq for NormalizedMixProfile {}

impl Hash for NormalizedMixProfile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.profile_uid.hash(state);
    }
}

/// Every mix function's running result over the member duties, taken one at a time.
#[derive(Default)]
struct MixFold {
    count: usize,
    min: Duty,
    max: Duty,
    sum: usize,
    /// The first duty minus each later one.
    diff: isize,
}

impl MixFold {
    fn push(&mut self, duty: Duty) {
        if self.count == 0 {
            self.min = duty;
            self.max = duty;
            self.diff = isize::from(duty);
        } else {
            self.min = self.min.min(duty);
            self.max = self.max.max(duty);
            self.diff -= isize::from(duty);
        }
        self.sum += usize::from(duty);
        self.count += 1;
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn finish(&self, mix_function: ProfileMixFunctionType) -> Duty {
        assert!(self.count > 0, "a mix needs at least one member duty");
        match mix_function {
            ProfileMixFunctionType::Min => self.min,
            ProfileMixFunctionType::Max => self.max,
            ProfileMixFunctionType::Avg => self.sum.div(self.count) as Duty,
            ProfileMixFunctionType::Diff => self.diff.clamp(0, 100) as Duty,
            ProfileMixFunctionType::Sum => self.sum.clamp(0, 100) as Duty,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::calibration::{CalibrationStore, FanStateMap};
    use crate::config::Config;
    use crate::device::{ChannelName, DeviceUID, Duty};
    use crate::engine::commanders::graph::GraphProfileCommander;
    use crate::engine::commanders::mix::{
        MixProfileCommander, NormalizedMixProfile, OutputDedupState,
    };
    use crate::engine::commanders::{by_device, DEFAULT_SAFETY_LATCH_COUNT};
    use crate::engine::{DeviceChannelProfileSetting, ScheduledChannel};
    use crate::setting::ProfileMixFunctionType;
    use crate::setting::ProfileUID;
    use std::collections::{HashMap, HashSet};
    use std::ops::Not;
    use std::rc::Rc;

    #[test]
    fn apply_mix_function_test_min() {
        let member_duties = vec![&20, &21, &22, &23, &24];
        let mix_function = ProfileMixFunctionType::Min;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 20);
    }

    #[test]
    fn apply_mix_function_test_max() {
        let member_duties = vec![&0, &1, &2, &3, &4];
        let mix_function = ProfileMixFunctionType::Max;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 4);
    }

    #[test]
    fn apply_mix_function_test_avg() {
        let member_duties = vec![&0, &1, &2, &3, &4];
        let mix_function = ProfileMixFunctionType::Avg;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 2);
    }

    #[test]
    fn apply_mix_function_test_avg_large() {
        let member_duties = vec![&120, &121, &122, &123, &124];
        let mix_function = ProfileMixFunctionType::Avg;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 122);
    }

    #[test]
    fn apply_mix_function_test_diff() {
        let member_duties = vec![&50, &20];
        let mix_function = ProfileMixFunctionType::Diff;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 30);
    }

    #[test]
    fn apply_mix_function_test_diff_neg() {
        let member_duties = vec![&20, &50];
        let mix_function = ProfileMixFunctionType::Diff;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 0);
    }

    #[test]
    fn apply_mix_function_test_sum() {
        // Sum of two values within range.
        let member_duties = vec![&30, &40];
        let mix_function = ProfileMixFunctionType::Sum;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 70);
    }

    #[test]
    fn apply_mix_function_test_sum_clamped_at_max() {
        // Sum exceeds 100% — must clamp to 100.
        let member_duties = vec![&70, &60];
        let mix_function = ProfileMixFunctionType::Sum;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 100);
    }

    #[test]
    fn apply_mix_function_test_sum_three_members() {
        // Sum of three values, exact total in range.
        let member_duties = vec![&20, &30, &25];
        let mix_function = ProfileMixFunctionType::Sum;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 75);
    }

    #[test]
    fn apply_mix_function_test_sum_at_boundary() {
        // Sum exactly at 100% — no clamping needed.
        let member_duties = vec![&50, &50];
        let mix_function = ProfileMixFunctionType::Sum;
        let result = MixProfileCommander::apply_mix_function(
            member_duties.into_iter().copied(),
            mix_function,
        );
        assert_eq!(result, 100);
    }

    /// Verify child Mix profiles (no Mix sub-members) process from graph cache only.
    #[test]
    fn process_child_before_parent() {
        let child = NormalizedMixProfile {
            profile_uid: "child_mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_b".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([
            ("graph_a".to_string(), Some(40u8)),
            ("graph_b".to_string(), Some(60u8)),
        ]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &child,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, Some(60)); // Max of 40, 60
    }

    /// Verify parent Mix reads Mix member output from `mix_duties`, not `graph_duties`.
    #[test]
    fn parent_reads_mix_member_output() {
        let parent = NormalizedMixProfile {
            profile_uid: "parent_mix".to_string(),
            mix_function: ProfileMixFunctionType::Avg,
            member_profile_uids: vec!["graph_c".to_string(), "child_mix".to_string()],
            member_mix_profile_uids: vec!["child_mix".to_string()],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([("graph_c".to_string(), Some(80u8))]);
        let mix_duties = HashMap::from([("child_mix".to_string(), Some(60u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &parent,
            &graph_duties,
            &mix_duties,
            &last_applied,
        );
        assert_eq!(result, Some(70)); // Avg of 80, 60
    }

    /// Verify a parent with both Graph and Mix members combines values correctly.
    #[test]
    fn mixed_graph_and_mix_members() {
        let parent = NormalizedMixProfile {
            profile_uid: "parent".to_string(),
            mix_function: ProfileMixFunctionType::Min,
            member_profile_uids: vec![
                "graph_1".to_string(),
                "graph_2".to_string(),
                "child_mix".to_string(),
            ],
            member_mix_profile_uids: vec!["child_mix".to_string()],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([
            ("graph_1".to_string(), Some(50u8)),
            ("graph_2".to_string(), Some(30u8)),
        ]);
        let mix_duties = HashMap::from([("child_mix".to_string(), Some(45u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &parent,
            &graph_duties,
            &mix_duties,
            &last_applied,
        );
        assert_eq!(result, Some(30)); // Min of 50, 30, 45
    }

    /// Verify mix functions work correctly with values from different processing stages.
    #[test]
    fn apply_mix_function_nested_values_max() {
        let parent = NormalizedMixProfile {
            profile_uid: "parent".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "child_mix".to_string()],
            member_mix_profile_uids: vec!["child_mix".to_string()],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(25u8))]);
        let mix_duties = HashMap::from([("child_mix".to_string(), Some(75u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &parent,
            &graph_duties,
            &mix_duties,
            &last_applied,
        );
        assert_eq!(result, Some(75)); // Max of 25, 75
    }

    /// Verify Diff function with nested values.
    #[test]
    fn apply_mix_function_nested_values_diff() {
        let parent = NormalizedMixProfile {
            profile_uid: "parent".to_string(),
            mix_function: ProfileMixFunctionType::Diff,
            member_profile_uids: vec!["child_mix".to_string(), "graph_a".to_string()],
            member_mix_profile_uids: vec!["child_mix".to_string()],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(30u8))]);
        let mix_duties = HashMap::from([("child_mix".to_string(), Some(80u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &parent,
            &graph_duties,
            &mix_duties,
            &last_applied,
        );
        assert_eq!(result, Some(50)); // Diff: 80 - 30 = 50
    }

    /// Verify None output when all members have no output.
    #[test]
    fn all_members_no_output_returns_none() {
        let child = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_b".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties =
            HashMap::from([("graph_a".to_string(), None), ("graph_b".to_string(), None)]);
        let last_applied =
            HashMap::from([("graph_a".to_string(), 50u8), ("graph_b".to_string(), 60u8)]);
        let result = MixProfileCommander::process_single_mix_profile(
            &child,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, None);
    }

    /// Verify missing member output returns None (skips processing).
    #[test]
    fn missing_member_output_returns_none() {
        let mix_profile = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_missing".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(50u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &mix_profile,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, None);
    }

    /// Verify `last_applied_duties` fallback when one member has output and another doesn't.
    #[test]
    fn uses_last_applied_duties_as_fallback() {
        let mix_profile = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_b".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([
            ("graph_a".to_string(), Some(70u8)),
            ("graph_b".to_string(), None),
        ]);
        let last_applied = HashMap::from([("graph_b".to_string(), 40u8)]);
        let result = MixProfileCommander::process_single_mix_profile(
            &mix_profile,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, Some(70)); // Max of 70, 40
    }

    /// Verify that missing `last_applied_duties` for a member returns None (not silent).
    #[test]
    fn missing_last_applied_duty_returns_none() {
        // Member graph_b has no output and no last_applied fallback.
        let mix_profile = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_b".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::new(),
        };
        let graph_duties = HashMap::from([
            ("graph_a".to_string(), Some(70u8)),
            ("graph_b".to_string(), None),
        ]);
        let last_applied = HashMap::new(); // graph_b missing
        let result = MixProfileCommander::process_single_mix_profile(
            &mix_profile,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, None);
    }

    // -- collect_duties_from_scheduled tests --

    /// Helper to build a scheduled settings map entry.
    fn make_scheduled(
        entries: Vec<(NormalizedMixProfile, Vec<DeviceChannelProfileSetting>)>,
    ) -> HashMap<Rc<NormalizedMixProfile>, HashSet<ScheduledChannel>> {
        entries
            .into_iter()
            .map(|(profile, channels)| {
                let channels = channels.into_iter().map(Rc::new).collect();
                (Rc::new(profile), channels)
            })
            .collect()
    }

    /// `collect_duties_from_scheduled` into a new buffer, by device.
    fn collect_from_scheduled(
        scheduled: &HashMap<Rc<NormalizedMixProfile>, HashSet<ScheduledChannel>>,
        duty_to_apply: impl FnMut(&ProfileUID) -> Option<Duty>,
    ) -> HashMap<DeviceUID, Vec<(ChannelName, Duty)>> {
        let mut duties = Vec::new();
        MixProfileCommander::collect_duties_from_scheduled(scheduled, duty_to_apply, &mut duties);
        by_device(&duties)
    }

    /// `collect_duties_to_apply` into a new buffer, by device.
    fn collected(commander: &MixProfileCommander) -> HashMap<DeviceUID, Vec<(ChannelName, Duty)>> {
        let mut duties = Vec::new();
        commander.collect_duties_to_apply(&mut duties);
        by_device(&duties)
    }

    fn mix_channel(device_uid: &str, channel_name: &str) -> DeviceChannelProfileSetting {
        DeviceChannelProfileSetting::Mix {
            device_uid: device_uid.to_string(),
            channel_name: channel_name.to_string(),
        }
    }

    /// A simple top-level Mix profile applies to its channel.
    #[test]
    fn collect_duties_simple_mix() {
        let scheduled = make_scheduled(vec![(
            NormalizedMixProfile {
                profile_uid: "mix_a".to_string(),
                mix_function: ProfileMixFunctionType::Max,
                member_profile_uids: vec!["g1".to_string()],
                member_mix_profile_uids: vec![],
                member_fixed_profile_duties: HashMap::new(),
            },
            vec![mix_channel("dev1", "fan1")],
        )]);
        let output_cache = HashMap::from([("mix_a".to_string(), Some(55u8))]);
        let result = collect_from_scheduled(&scheduled, |uid| output_cache[uid]);
        assert_eq!(result.len(), 1);
        assert_eq!(result["dev1"], vec![("fan1".to_string(), 55)]);
    }

    /// A child Mix profile used by a parent should NOT apply to the parent's channel,
    /// but SHOULD still apply to its own directly assigned channel.
    #[test]
    fn collect_duties_child_mix_keeps_own_channel() {
        // child_mix is directly applied to dev1/fan1
        // parent_mix uses child_mix as a member, applied to dev2/fan2
        // child_mix's device_channels include BOTH (from scheduling).
        let scheduled = make_scheduled(vec![
            (
                NormalizedMixProfile {
                    profile_uid: "child_mix".to_string(),
                    mix_function: ProfileMixFunctionType::Max,
                    member_profile_uids: vec!["g1".to_string(), "g2".to_string()],
                    member_mix_profile_uids: vec![],
                    member_fixed_profile_duties: HashMap::new(),
                },
                // child_mix has both its own channel and the parent's channel
                vec![mix_channel("dev1", "fan1"), mix_channel("dev2", "fan2")],
            ),
            (
                NormalizedMixProfile {
                    profile_uid: "parent_mix".to_string(),
                    mix_function: ProfileMixFunctionType::Avg,
                    member_profile_uids: vec!["child_mix".to_string(), "g3".to_string()],
                    member_mix_profile_uids: vec!["child_mix".to_string()],
                    member_fixed_profile_duties: HashMap::new(),
                },
                vec![mix_channel("dev2", "fan2")],
            ),
        ]);
        let output_cache = HashMap::from([
            ("child_mix".to_string(), Some(60u8)),
            ("parent_mix".to_string(), Some(70u8)),
        ]);
        let result = collect_from_scheduled(&scheduled, |uid| output_cache[uid]);
        // child_mix should apply to dev1/fan1 (its own) but NOT dev2/fan2 (parent's)
        assert!(result.contains_key("dev1"));
        let dev1_duties = &result["dev1"];
        assert_eq!(dev1_duties.len(), 1);
        assert_eq!(dev1_duties[0], ("fan1".to_string(), 60));
        // parent_mix should apply to dev2/fan2
        assert!(result.contains_key("dev2"));
        let dev2_duties = &result["dev2"];
        assert_eq!(dev2_duties.len(), 1);
        assert_eq!(dev2_duties[0], ("fan2".to_string(), 70));
    }

    /// A child Mix with only parent-owned channels should not produce any output.
    #[test]
    fn collect_duties_child_only_channels_skipped() {
        // child_mix only has channels from the parent, no direct assignment.
        let scheduled = make_scheduled(vec![
            (
                NormalizedMixProfile {
                    profile_uid: "child_mix".to_string(),
                    mix_function: ProfileMixFunctionType::Max,
                    member_profile_uids: vec!["g1".to_string()],
                    member_mix_profile_uids: vec![],
                    member_fixed_profile_duties: HashMap::new(),
                },
                vec![mix_channel("dev2", "fan2")],
            ),
            (
                NormalizedMixProfile {
                    profile_uid: "parent_mix".to_string(),
                    mix_function: ProfileMixFunctionType::Avg,
                    member_profile_uids: vec!["child_mix".to_string(), "g2".to_string()],
                    member_mix_profile_uids: vec!["child_mix".to_string()],
                    member_fixed_profile_duties: HashMap::new(),
                },
                vec![mix_channel("dev2", "fan2")],
            ),
        ]);
        let output_cache = HashMap::from([
            ("child_mix".to_string(), Some(40u8)),
            ("parent_mix".to_string(), Some(50u8)),
        ]);
        let result = collect_from_scheduled(&scheduled, |uid| output_cache[uid]);
        // Only parent_mix applies to dev2/fan2. child_mix is fully skipped.
        assert_eq!(result.len(), 1);
        let dev2_duties = &result["dev2"];
        assert_eq!(dev2_duties.len(), 1);
        assert_eq!(dev2_duties[0], ("fan2".to_string(), 50));
    }

    /// A Mix profile with None output should not produce any duties.
    #[test]
    fn collect_duties_none_output_skipped() {
        let scheduled = make_scheduled(vec![(
            NormalizedMixProfile {
                profile_uid: "mix_a".to_string(),
                mix_function: ProfileMixFunctionType::Max,
                member_profile_uids: vec!["g1".to_string()],
                member_mix_profile_uids: vec![],
                member_fixed_profile_duties: HashMap::new(),
            },
            vec![mix_channel("dev1", "fan1")],
        )]);
        let output_cache = HashMap::from([("mix_a".to_string(), None)]);
        let result = collect_from_scheduled(&scheduled, |uid| output_cache[uid]);
        assert!(result.is_empty());
    }

    /// Overlay device channels (non-Mix variant) should not be collected.
    #[test]
    fn collect_duties_overlay_channels_ignored() {
        let scheduled = make_scheduled(vec![(
            NormalizedMixProfile {
                profile_uid: "mix_a".to_string(),
                mix_function: ProfileMixFunctionType::Max,
                member_profile_uids: vec!["g1".to_string()],
                member_mix_profile_uids: vec![],
                member_fixed_profile_duties: HashMap::new(),
            },
            vec![DeviceChannelProfileSetting::Overlay {
                device_uid: "dev1".to_string(),
                channel_name: "fan1".to_string(),
            }],
        )]);
        let output_cache = HashMap::from([("mix_a".to_string(), Some(80u8))]);
        let result = collect_from_scheduled(&scheduled, |uid| output_cache[uid]);
        assert!(result.is_empty());
    }

    // -- Fixed member profile tests --

    /// Verify Fixed member duties are used directly without cache lookup.
    #[test]
    fn fixed_member_produces_constant_duty() {
        let mix = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["fixed_a".to_string(), "graph_a".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::from([("fixed_a".to_string(), 45u8)]),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(70u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &mix,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, Some(70)); // Max of 45, 70
    }

    /// Verify Mix with only Fixed members always produces output.
    #[test]
    fn all_fixed_members_always_produce_output() {
        let mix = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Avg,
            member_profile_uids: vec!["fixed_a".to_string(), "fixed_b".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::from([
                ("fixed_a".to_string(), 30u8),
                ("fixed_b".to_string(), 50u8),
            ]),
        };
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &mix,
            &HashMap::new(),
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, Some(40)); // Avg of 30, 50
    }

    /// Verify Fixed members work with Diff function (subtraction order matters).
    #[test]
    fn fixed_member_diff_function() {
        let mix = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Diff,
            member_profile_uids: vec!["graph_a".to_string(), "fixed_a".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::from([("fixed_a".to_string(), 20u8)]),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(80u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &mix,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, Some(60)); // Diff: 80 - 20 = 60
    }

    /// Verify Fixed members combined with Graph and Mix members.
    #[test]
    fn fixed_graph_and_mix_members_combined() {
        let parent = NormalizedMixProfile {
            profile_uid: "parent".to_string(),
            mix_function: ProfileMixFunctionType::Min,
            member_profile_uids: vec![
                "fixed_a".to_string(),
                "graph_a".to_string(),
                "child_mix".to_string(),
            ],
            member_mix_profile_uids: vec!["child_mix".to_string()],
            member_fixed_profile_duties: HashMap::from([("fixed_a".to_string(), 25u8)]),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(50u8))]);
        let mix_duties = HashMap::from([("child_mix".to_string(), Some(40u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &parent,
            &graph_duties,
            &mix_duties,
            &last_applied,
        );
        assert_eq!(result, Some(25)); // Min of 25, 50, 40
    }

    /// Verify a child Mix with Fixed sub-members processes correctly.
    /// Reproduces the scenario where a Fixed profile is a member of a child Mix
    /// (nested Mix). The child's Fixed member duty should come from its
    /// `NormalizedMixProfile`, not from a graph cache lookup.
    #[test]
    fn child_mix_with_fixed_sub_member() {
        let child = NormalizedMixProfile {
            profile_uid: "child_mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "fixed_a".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::from([("fixed_a".to_string(), 55u8)]),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), Some(40u8))]);
        let last_applied = HashMap::new();
        let result = MixProfileCommander::process_single_mix_profile(
            &child,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        assert_eq!(result, Some(55)); // Max of 40, 55
    }

    /// Verify Fixed members keep Mix producing output even when Graph members have no output.
    #[test]
    fn fixed_member_provides_output_when_graph_has_none() {
        let mix = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["fixed_a".to_string(), "graph_a".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::from([("fixed_a".to_string(), 60u8)]),
        };
        let graph_duties = HashMap::from([("graph_a".to_string(), None)]);
        let last_applied = HashMap::from([("graph_a".to_string(), 30u8)]);
        let result = MixProfileCommander::process_single_mix_profile(
            &mix,
            &graph_duties,
            &HashMap::new(),
            &last_applied,
        );
        // Fixed=60 has output, so members_have_no_output=false.
        // Graph has None, so it uses last_applied=30 as fallback.
        assert_eq!(result, Some(60)); // Max of 60, 30
    }

    // -- OutputDedupState tests --

    /// Verify first call always applies (counter starts at latch count).
    #[test]
    fn dedup_first_call_always_applies() {
        let mut state = OutputDedupState::new();
        assert!(state.should_apply(50));
    }

    /// Verify identical duty is suppressed on the next tick.
    #[test]
    fn dedup_suppresses_identical_duty() {
        let mut state = OutputDedupState::new();
        assert!(state.should_apply(50)); // first: apply
        assert!(state.should_apply(50).not()); // same: suppress
        assert!(state.should_apply(50).not()); // same: suppress
    }

    /// Verify a changed duty is applied immediately and resets the counter.
    #[test]
    fn dedup_allows_changed_duty() {
        let mut state = OutputDedupState::new();
        assert!(state.should_apply(50));
        assert!(state.should_apply(50).not());
        assert!(state.should_apply(60)); // changed: apply immediately
        assert!(state.should_apply(60).not()); // same again: suppress
    }

    /// Verify the safety latch fires after `DEFAULT_SAFETY_LATCH_COUNT` suppressed ticks.
    #[test]
    fn dedup_safety_latch_reapplies() {
        let mut state = OutputDedupState::new();
        assert!(state.should_apply(50)); // tick 0: first apply, counter resets to 0
                                         // Ticks 1..=30: all suppressed (counter goes 0->1, 1->2, ..., 29->30)
        for _ in 0..DEFAULT_SAFETY_LATCH_COUNT {
            assert!(state.should_apply(50).not());
        }
        // Tick 31: counter is 30 == DEFAULT_SAFETY_LATCH_COUNT, safety latch fires
        assert!(state.should_apply(50));
        // Tick 32: suppressed again
        assert!(state.should_apply(50).not());
    }

    /// Verify counter resets when duty changes mid-suppression.
    #[test]
    fn dedup_resets_on_duty_change() {
        let mut state = OutputDedupState::new();
        assert!(state.should_apply(50));
        for _ in 0..10 {
            assert!(state.should_apply(50).not());
        }
        // Change duty: apply immediately, counter resets to 0.
        assert!(state.should_apply(70));
        // Now need another full DEFAULT_SAFETY_LATCH_COUNT suppressed ticks before latch fires.
        for _ in 0..DEFAULT_SAFETY_LATCH_COUNT {
            assert!(state.should_apply(70).not());
        }
        assert!(state.should_apply(70)); // safety latch
    }

    /// A Mix commander over no devices, with its graph commander's outputs set by hand.
    fn mix_commander() -> MixProfileCommander {
        let graph_commander = GraphProfileCommander::new(
            Rc::new(HashMap::new()),
            HashMap::new(),
            Rc::new(Config::init_default_config().unwrap()),
            Rc::new(CalibrationStore::empty()),
            Rc::new(FanStateMap::new()),
        );
        MixProfileCommander::new(Rc::new(graph_commander))
    }

    /// Schedules a child Max of g1 (30) and g2 (40) on fan1, and a parent Sum of the child and
    /// g3 (20) on fan2.
    fn schedule_child_and_parent(commander: &MixProfileCommander) {
        let child = NormalizedMixProfile {
            profile_uid: "child".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["g1".to_string(), "g2".to_string()],
            member_mix_profile_uids: vec![],
            member_fixed_profile_duties: HashMap::new(),
        };
        let parent = NormalizedMixProfile {
            profile_uid: "parent".to_string(),
            mix_function: ProfileMixFunctionType::Sum,
            member_profile_uids: vec!["child".to_string(), "g3".to_string()],
            member_mix_profile_uids: vec!["child".to_string()],
            member_fixed_profile_duties: HashMap::new(),
        };
        commander
            .scheduled_settings
            .borrow_mut()
            .extend(make_scheduled(vec![
                (child, vec![mix_channel("dev1", "fan1")]),
                (parent, vec![mix_channel("dev1", "fan2")]),
            ]));
        for uid in ["child", "parent"] {
            commander
                .process_output_cache
                .borrow_mut()
                .insert(uid.to_string(), None);
            commander
                .output_dedup
                .borrow_mut()
                .insert(uid.to_string(), OutputDedupState::new());
        }
        commander
            .graph_commander
            .process_output_cache
            .borrow_mut()
            .extend([
                ("g1".to_string(), Some(30)),
                ("g2".to_string(), Some(40)),
                ("g3".to_string(), Some(20)),
            ]);
    }

    #[test]
    fn a_parent_reads_its_childs_output_from_the_same_tick() {
        // Goal: the in-place write gives a parent its child's fresh output. Method: process a
        // child and parent once from empty caches and check both results.
        let commander = mix_commander();
        schedule_child_and_parent(&commander);

        commander.process_all_profiles();

        let cache = commander.process_output_cache.borrow();
        assert_eq!(cache["child"], Some(40));
        assert_eq!(cache["parent"], Some(60));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "Mix nesting must be single-level")]
    fn a_parent_of_a_parent_is_refused() {
        // Goal: the in-place write must never see nesting deeper than one level. Method:
        // schedule a Mix whose Mix member is itself a parent, then process.
        let commander = mix_commander();
        schedule_child_and_parent(&commander);
        let grandparent = NormalizedMixProfile {
            profile_uid: "grandparent".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["parent".to_string()],
            member_mix_profile_uids: vec!["parent".to_string()],
            member_fixed_profile_duties: HashMap::new(),
        };
        commander
            .scheduled_settings
            .borrow_mut()
            .extend(make_scheduled(vec![(
                grandparent,
                vec![mix_channel("dev1", "fan3")],
            )]));

        commander.process_all_profiles();
    }

    #[test]
    fn each_mix_applies_to_its_own_channel() {
        // Goal: a child and its parent each apply only to their own channel. Method: process
        // once and collect the duties to apply.
        let commander = mix_commander();
        schedule_child_and_parent(&commander);
        commander.process_all_profiles();

        let mut applied = collected(&commander).remove("dev1").unwrap();

        applied.sort();
        assert_eq!(
            applied,
            vec![("fan1".to_string(), 40), ("fan2".to_string(), 60)]
        );
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_child_left_to_its_parent_allocates_nothing() {
        // Goal: a child Mix whose duty changes every tick, on a channel its parent owns, costs
        // a collect nothing. Method: count a collect where only the child has a new duty.
        let scheduled = make_scheduled(vec![
            (
                NormalizedMixProfile {
                    profile_uid: "child_mix".to_string(),
                    mix_function: ProfileMixFunctionType::Max,
                    member_profile_uids: vec!["g1".to_string()],
                    member_mix_profile_uids: vec![],
                    member_fixed_profile_duties: HashMap::new(),
                },
                vec![mix_channel("dev2", "fan2")],
            ),
            (
                NormalizedMixProfile {
                    profile_uid: "parent_mix".to_string(),
                    mix_function: ProfileMixFunctionType::Avg,
                    member_profile_uids: vec!["child_mix".to_string(), "g2".to_string()],
                    member_mix_profile_uids: vec!["child_mix".to_string()],
                    member_fixed_profile_duties: HashMap::new(),
                },
                vec![mix_channel("dev2", "fan2")],
            ),
        ]);
        let output_cache = HashMap::from([
            ("child_mix".to_string(), Some(40u8)),
            ("parent_mix".to_string(), None),
        ]);
        let mut duties = Vec::new();

        let ((), allocations) = crate::alloc_count::count_allocations(|| {
            MixProfileCommander::collect_duties_from_scheduled(
                &scheduled,
                |uid| output_cache[uid],
                &mut duties,
            );
        });

        assert!(
            duties.is_empty(),
            "the parent owns the child's only channel"
        );
        assert_eq!(allocations, 0);
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_tick_with_unchanged_duties_allocates_nothing() {
        // Goal: steady state costs no allocation. Method: one tick applies both mixes, and the
        // next records their outputs as last applied, a first sight. Then count the third, whose
        // duties match and are suppressed.
        let commander = mix_commander();
        schedule_child_and_parent(&commander);
        commander.process_all_profiles();
        assert_eq!(collected(&commander)["dev1"].len(), 2);
        commander.process_all_profiles();
        assert!(collected(&commander).is_empty());
        let mut duties = Vec::new();

        let ((), allocations) = crate::alloc_count::count_allocations(|| {
            commander.process_all_profiles();
            commander.collect_duties_to_apply(&mut duties);
        });

        assert!(duties.is_empty());
        assert_eq!(allocations, 0);
        assert_eq!(commander.process_output_cache.borrow()["parent"], Some(60));
    }
}
