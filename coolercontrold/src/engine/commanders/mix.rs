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
use std::ops::{Div, Not};
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
        // Clear the channel setting in case another mix profile is already scheduled:
        self.clear_channel_setting(device_channel.device_uid(), device_channel.channel_name());
        let normalized_mix_setting = Self::normalize_mix_setting(mix_profile, &member_profiles);
        self.schedule_member_profiles(&device_channel, member_profiles)?;
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
    ) -> Result<()> {
        // all graph profiles for this DeviceChannelProfileSetting are already cleared
        for member_profile in member_profiles {
            // Add the Mix setting for each member profile to be processed
            self.graph_commander
                .schedule_setting(device_channel.clone(), &member_profile)?;
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
        let mut mix_profile_to_remove: Option<Rc<NormalizedMixProfile>> = None;
        let device_channel = DeviceChannelProfileSetting::Mix {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_string(),
        };
        let mut scheduled_settings_lock = self.scheduled_settings.borrow_mut();
        for (mix_profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel);
            if device_channels.is_empty() {
                mix_profile_to_remove.replace(Rc::clone(mix_profile));
                self.process_output_cache
                    .borrow_mut()
                    .remove(&mix_profile.profile_uid);
            }
        }
        if let Some(mix_profile) = mix_profile_to_remove {
            scheduled_settings_lock.remove(&mix_profile);
        }
        self.graph_commander
            .clear_channel_setting(device_uid, channel_name);
    }

    /// This method processes all scheduled profiles and updates the output cache.
    /// This should be called very early, right after the `GraphProfileCommander` processes,
    /// and only once per update cycle.
    pub fn process_all_profiles(&self) {
        self.update_last_applied_duties();
        let mut output_cache_lock = self.process_output_cache.borrow_mut();
        // All the member profiles have been processed already by the graph_commander:
        let requested_duties = self.graph_commander.process_output_cache.borrow();
        let last_applied_duties = self.all_last_applied_duties.borrow();
        'mix: for mix_profile in self.scheduled_settings.borrow().keys() {
            let mut member_values = Vec::with_capacity(last_applied_duties.len());
            let mut members_have_no_output = true;
            for member_profile_uid in &mix_profile.member_profile_uids {
                let Some(output) = requested_duties.get(member_profile_uid) else {
                    error!(
                        "Mix Profile calculation for {} skipped because of missing member output duty ",
                        mix_profile.profile_uid
                    );
                    // In very rare cases in the past, this was possible due to a race condition.
                    // This should no longer happen, but we avoid the panic anyway.
                    if let Some(cache) = output_cache_lock.get_mut(&mix_profile.profile_uid) {
                        *cache = None;
                    }
                    continue 'mix;
                };
                let duty_value_for_calculation = if let Some(duty) = output {
                    members_have_no_output = false;
                    duty
                } else {
                    // We need the last applied values as a backup from all member profiles when ANY
                    // profile produces output, so we can properly compare the results and apply the
                    // correct Duty.
                    &last_applied_duties[member_profile_uid]
                };
                member_values.push(duty_value_for_calculation);
            }
            if members_have_no_output {
                // Nothing to set if all member Profile Outputs are None
                if let Some(cache) = output_cache_lock.get_mut(&mix_profile.profile_uid) {
                    *cache = None;
                }
                continue;
            }
            let duty_to_apply = Self::apply_mix_function(&member_values, mix_profile.mix_function);
            if let Some(cache) = output_cache_lock.get_mut(&mix_profile.profile_uid) {
                *cache = Some(duty_to_apply);
            }
        }
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
        let requested_duties = self.graph_commander.process_output_cache.borrow();
        for (profile_uid, output) in requested_duties.iter() {
            let Some(duty) = output else {
                continue;
            };
            if last_applied_duties.contains_key(profile_uid) {
                if let Some(d) = last_applied_duties.get_mut(profile_uid) {
                    *d = *duty;
                }
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
    #[allow(clippy::cast_possible_truncation)]
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
        }
    }

    fn normalize_mix_setting(
        profile: &Profile,
        member_profiles: &[Profile],
    ) -> NormalizedMixProfile {
        NormalizedMixProfile {
            profile_uid: profile.uid.clone(),
            mix_function: profile.mix_function_type.unwrap(),
            member_profile_uids: member_profiles.iter().map(|p| p.uid.clone()).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedMixProfile {
    profile_uid: ProfileUID,
    mix_function: ProfileMixFunctionType,
    member_profile_uids: Vec<ProfileUID>,
}

impl Default for NormalizedMixProfile {
    fn default() -> Self {
        Self {
            profile_uid: String::default(),
            mix_function: ProfileMixFunctionType::Max,
            member_profile_uids: Vec::new(),
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
    use crate::engine::commanders::mix::MixProfileCommander;
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
}
