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
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{Div, Not, Sub};
use std::rc::Rc;

use anyhow::{anyhow, Result};
use log::error;
use moro_local::Scope;
use serde::{Deserialize, Serialize};

use crate::device::{ChannelName, DeviceUID, Duty, UID};
use crate::engine::commanders::graph::GraphProfileCommander;
use crate::engine::DeviceChannelProfileSetting;
use crate::setting::{Profile, ProfileMixFunctionType, ProfileType, ProfileUID};

type MixProfile = Profile;

/// A Commander for Mix Profile Processing
/// This has its own `GraphProfile` Commander for processing each member profile. It handles
/// scheduling, caching, as well as processing of the `MixProfileFunction`.
pub struct MixProfileCommander {
    graph_commander: Rc<GraphProfileCommander>,
    scheduled_settings:
        RefCell<HashMap<Rc<NormalizedMixProfile>, HashSet<DeviceChannelProfileSetting>>>,
    all_last_applied_duties: RefCell<HashMap<ProfileUID, Duty>>,
    /// The last calculated Option<Duty> for each Mix Profile.
    /// This allows other Profiles to use the output of a Mix Profile.
    pub process_output_cache: RefCell<HashMap<ProfileUID, Option<Duty>>>,
}

impl MixProfileCommander {
    pub fn new(graph_commander: Rc<GraphProfileCommander>) -> Self {
        Self {
            graph_commander,
            scheduled_settings: RefCell::new(HashMap::new()),
            all_last_applied_duties: RefCell::new(HashMap::new()),
            process_output_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn schedule_setting(
        &self,
        device_channel: DeviceChannelProfileSetting,
        mix_profile: &MixProfile,
        member_profiles: Vec<Profile>,
        member_sub_profiles: HashMap<ProfileUID, Vec<Profile>>,
    ) -> Result<()> {
        if mix_profile.p_type != ProfileType::Mix {
            return Err(anyhow!(
                "Only Mix Profiles are supported for scheduling in the MixProfileCommander"
            ));
        }
        if mix_profile.mix_function_type.is_none() {
            return Err(anyhow!(
                "Mix Function Type must be present for a Mix Profile"
            ));
        }
        if member_profiles.is_empty() {
            return Err(anyhow!("Member profiles must be present for a Mix Profile"));
        }
        let normalized_mix_setting = Self::normalize_mix_setting(mix_profile, &member_profiles);
        self.schedule_member_profiles(&device_channel, member_profiles, &member_sub_profiles)?;
        let mut settings_lock = self.scheduled_settings.borrow_mut();
        if let Some(mut existing_device_channels) = settings_lock.remove(&normalized_mix_setting) {
            // We replace the existing NormalizedMixProfile if it exists to make sure it's
            // internal settings are up to date
            existing_device_channels.insert(device_channel);
            settings_lock.insert(Rc::new(normalized_mix_setting), existing_device_channels);
        } else {
            let mut new_device_channels = HashSet::new();
            new_device_channels.insert(device_channel);
            settings_lock.insert(Rc::new(normalized_mix_setting), new_device_channels);
            self.process_output_cache
                .borrow_mut()
                .insert(mix_profile.uid.clone(), None);
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
            match member_profile.p_type {
                ProfileType::Graph => {
                    self.graph_commander
                        .schedule_setting(device_channel.clone(), &member_profile)?;
                }
                ProfileType::Mix => {
                    // Schedule the child Mix's own Graph sub-members via graph_commander
                    if let Some(sub_profiles) = member_sub_profiles.get(&member_profile.uid) {
                        for sub_profile in sub_profiles {
                            self.graph_commander
                                .schedule_setting(device_channel.clone(), sub_profile)?;
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
                        existing_device_channels.insert(device_channel.clone());
                        settings_lock.insert(Rc::new(normalized_child), existing_device_channels);
                    } else {
                        let mut new_device_channels = HashSet::new();
                        new_device_channels.insert(device_channel.clone());
                        settings_lock.insert(Rc::new(normalized_child), new_device_channels);
                        self.process_output_cache
                            .borrow_mut()
                            .insert(member_profile.uid.clone(), None);
                    }
                }
                _ => {
                    return Err(anyhow!(
                        "Only Graph and Mix Profiles are supported as Mix members"
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
    /// Two-pass processing (mirrors custom_sensors_repo::update_statuses()):
    /// - Pass 1: Process child Mix profiles (those with no Mix sub-members)
    /// - Pass 2: Process parent Mix profiles (those with Mix sub-members),
    ///   reading child Mix output from the process_output_cache populated in pass 1
    pub fn process_all_profiles(&self) {
        self.update_last_applied_duties();
        let graph_duties = self.graph_commander.process_output_cache.borrow();
        let last_applied_duties = self.all_last_applied_duties.borrow();
        let scheduled = self.scheduled_settings.borrow();

        // Pass 1: Process children (Mix profiles with no Mix sub-members)
        let mut pass1_results: HashMap<ProfileUID, Option<Duty>> = HashMap::new();
        for mix_profile in scheduled.keys() {
            if mix_profile.member_mix_profile_uids.is_empty().not() {
                continue; // Skip parents in pass 1
            }
            let result = Self::process_single_mix_profile(
                mix_profile,
                &graph_duties,
                &HashMap::new(), // No mix outputs needed for children
                &last_applied_duties,
            );
            pass1_results.insert(mix_profile.profile_uid.clone(), result);
        }

        // Write pass 1 results into the output cache
        {
            let mut output_cache_lock = self.process_output_cache.borrow_mut();
            for (uid, result) in &pass1_results {
                if let Some(cache) = output_cache_lock.get_mut(uid) {
                    *cache = *result;
                }
            }
        }

        // Pass 2: Process parents (Mix profiles with Mix sub-members)
        let mix_duties = self.process_output_cache.borrow();
        let mut pass2_results: HashMap<ProfileUID, Option<Duty>> = HashMap::new();
        for mix_profile in scheduled.keys() {
            if mix_profile.member_mix_profile_uids.is_empty() {
                continue; // Skip children in pass 2
            }
            let result = Self::process_single_mix_profile(
                mix_profile,
                &graph_duties,
                &mix_duties,
                &last_applied_duties,
            );
            pass2_results.insert(mix_profile.profile_uid.clone(), result);
        }
        drop(mix_duties);

        // Write pass 2 results into the output cache
        let mut output_cache_lock = self.process_output_cache.borrow_mut();
        for (uid, result) in pass2_results {
            if let Some(cache) = output_cache_lock.get_mut(&uid) {
                *cache = result;
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
        let mut member_values = Vec::with_capacity(mix_profile.member_profile_uids.len());
        let mut members_have_no_output = true;
        for member_profile_uid in &mix_profile.member_profile_uids {
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
                last_applied_duties.get(member_profile_uid)?
            };
            member_values.push(duty_value_for_calculation);
        }
        if members_have_no_output {
            return None;
        }
        Some(Self::apply_mix_function(
            &member_values,
            mix_profile.mix_function,
        ))
    }

    /// Processes all the member Profiles and applies the appropriate output per Mix Profile.
    /// Normally triggered by a loop/timer.
    pub fn update_speeds<'s>(&'s self, scope: &'s Scope<'s, 's, Result<()>>) {
        for (device_uid, channel_duties_to_apply) in self.collect_duties_to_apply() {
            scope.spawn(async move {
                for (channel_name, duty) in channel_duties_to_apply {
                    self.graph_commander
                        .set_device_speed(&device_uid, &channel_name, duty)
                        .await;
                }
            });
        }
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

    /// Collects the duties to apply for all scheduled Mix Profiles from the output cache.
    fn collect_duties_to_apply(&self) -> HashMap<DeviceUID, Vec<(ChannelName, Duty)>> {
        let mut output_to_apply = HashMap::new();
        let output_cache_lock = self.process_output_cache.borrow();
        for (mix_profile, device_channels) in self.scheduled_settings.borrow().iter() {
            let optional_duty_to_set = output_cache_lock[&mix_profile.profile_uid]
                .as_ref()
                .copied();
            let Some(duty_to_set) = optional_duty_to_set else {
                continue;
            };
            for device_channel in device_channels {
                // We only apply Mix Profiles directly applied to fan channels, as we
                // can also schedule Overlay Member Profiles,
                // which need to be handled properly upstream.
                if let DeviceChannelProfileSetting::Mix {
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

    /// This function expects a non-empty `member_values` vector
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    fn apply_mix_function(member_values: &[&Duty], mix_function: ProfileMixFunctionType) -> Duty {
        // Since the member functions manage their own thresholds and the safety latch should
        //  kick off about the same time for all of them, we don't check thresholds here.
        match mix_function {
            ProfileMixFunctionType::Min => **member_values.iter().min().unwrap(),
            ProfileMixFunctionType::Max => **member_values.iter().max().unwrap(),
            ProfileMixFunctionType::Avg => member_values
                .iter()
                .map(|d| **d as usize)
                .sum::<usize>()
                .div(member_values.len()) as Duty,
            ProfileMixFunctionType::Diff => member_values
                .iter()
                .map(|d| **d as isize)
                .reduce(Sub::sub)
                .unwrap_or_default()
                .clamp(0, 100) as Duty,
        }
    }

    fn normalize_mix_setting(
        profile: &Profile,
        member_profiles: &[Profile],
    ) -> NormalizedMixProfile {
        NormalizedMixProfile {
            profile_uid: profile.uid.clone(),
            mix_function: profile.mix_function_type.unwrap(),
            member_mix_profile_uids: member_profiles
                .iter()
                .filter(|p| p.p_type == ProfileType::Mix)
                .map(|p| p.uid.clone())
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
    /// Subset of member_profile_uids that are Mix-type profiles (children).
    member_mix_profile_uids: Vec<ProfileUID>,
}

impl Default for NormalizedMixProfile {
    fn default() -> Self {
        Self {
            profile_uid: String::default(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: Vec::new(),
            member_mix_profile_uids: Vec::new(),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::engine::commanders::mix::{MixProfileCommander, NormalizedMixProfile};
    use crate::setting::ProfileMixFunctionType;

    #[test]
    fn apply_mix_function_test_min() {
        let member_values = vec![&20, &21, &22, &23, &24];
        let mix_function = ProfileMixFunctionType::Min;
        let result = MixProfileCommander::apply_mix_function(&member_values, mix_function);
        assert_eq!(result, 20);
    }

    #[test]
    fn apply_mix_function_test_max() {
        let member_values = vec![&0, &1, &2, &3, &4];
        let mix_function = ProfileMixFunctionType::Max;
        let result = MixProfileCommander::apply_mix_function(&member_values, mix_function);
        assert_eq!(result, 4);
    }

    #[test]
    fn apply_mix_function_test_avg() {
        let member_values = vec![&0, &1, &2, &3, &4];
        let mix_function = ProfileMixFunctionType::Avg;
        let result = MixProfileCommander::apply_mix_function(&member_values, mix_function);
        assert_eq!(result, 2);
    }

    #[test]
    fn apply_mix_function_test_avg_large() {
        let member_values = vec![&120, &121, &122, &123, &124];
        let mix_function = ProfileMixFunctionType::Avg;
        let result = MixProfileCommander::apply_mix_function(&member_values, mix_function);
        assert_eq!(result, 122);
    }

    #[test]
    fn apply_mix_function_test_diff() {
        let member_values = vec![&50, &20];
        let mix_function = ProfileMixFunctionType::Diff;
        let result = MixProfileCommander::apply_mix_function(&member_values, mix_function);
        assert_eq!(result, 30);
    }

    #[test]
    fn apply_mix_function_test_diff_neg() {
        let member_values = vec![&20, &50];
        let mix_function = ProfileMixFunctionType::Diff;
        let result = MixProfileCommander::apply_mix_function(&member_values, mix_function);
        assert_eq!(result, 0);
    }

    /// Verify child Mix profiles (no Mix sub-members) process from graph cache only.
    #[test]
    fn process_child_before_parent() {
        let child = NormalizedMixProfile {
            profile_uid: "child_mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_b".to_string()],
            member_mix_profile_uids: vec![],
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

    /// Verify parent Mix reads Mix member output from mix_duties, not graph_duties.
    #[test]
    fn parent_reads_mix_member_output() {
        let parent = NormalizedMixProfile {
            profile_uid: "parent_mix".to_string(),
            mix_function: ProfileMixFunctionType::Avg,
            member_profile_uids: vec!["graph_c".to_string(), "child_mix".to_string()],
            member_mix_profile_uids: vec!["child_mix".to_string()],
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

    /// Verify last_applied_duties fallback when one member has output and another doesn't.
    #[test]
    fn uses_last_applied_duties_as_fallback() {
        let mix_profile = NormalizedMixProfile {
            profile_uid: "mix".to_string(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: vec!["graph_a".to_string(), "graph_b".to_string()],
            member_mix_profile_uids: vec![],
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
}
