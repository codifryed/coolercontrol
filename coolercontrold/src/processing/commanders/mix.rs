/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2024  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::ops::{Div, Not};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::config::Config;
use crate::device::{ChannelName, DeviceUID, Duty, UID};
use crate::processing::commanders::graph::GraphProfileCommander;
use crate::processing::settings::ReposByType;
use crate::setting::{Profile, ProfileMixFunctionType, ProfileType, ProfileUID};
use crate::AllDevices;

type MixProfile = Profile;

/// A Commander for Mix Profile Processing
/// This has its own GraphProfile Commander for processing each member profile. It handles
/// scheduling, caching, as well as processing of the MixProfileFunction.
pub struct MixProfileCommander {
    graph_commander: GraphProfileCommander,
    scheduled_settings:
        RwLock<HashMap<Arc<NormalizedMixProfile>, HashSet<(DeviceUID, ChannelName)>>>,
    all_member_profiles_last_applied_duties: RwLock<HashMap<ProfileUID, Duty>>,
    all_member_profiles_requested_duties: RwLock<HashMap<ProfileUID, Option<Duty>>>,
}

impl MixProfileCommander {
    pub fn new(all_devices: AllDevices, repos: ReposByType, config: Arc<Config>) -> Self {
        Self {
            graph_commander: GraphProfileCommander::new(all_devices, repos, config),
            scheduled_settings: RwLock::new(HashMap::new()),
            all_member_profiles_last_applied_duties: RwLock::new(HashMap::new()),
            all_member_profiles_requested_duties: RwLock::new(HashMap::new()),
        }
    }

    pub async fn schedule_setting(
        &self,
        device_uid: &UID,
        channel_name: &str,
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
        let normalized_mix_setting = self
            .normalize_mix_setting(mix_profile, &member_profiles)
            .await?;
        self.prepare_member_profiles(device_uid, channel_name, member_profiles)
            .await?;
        let device_channel = (device_uid.clone(), channel_name.to_string());
        let mut settings_lock = self.scheduled_settings.write().await;
        if let Some(mut existing_device_channels) = settings_lock.remove(&normalized_mix_setting) {
            // We replace the existing NormalizedMixProfile if it exists to make sure it's
            // internal settings are up-to-date
            existing_device_channels.insert(device_channel);
            settings_lock.insert(Arc::new(normalized_mix_setting), existing_device_channels);
        } else {
            let mut new_device_channels = HashSet::new();
            new_device_channels.insert(device_channel);
            settings_lock.insert(Arc::new(normalized_mix_setting), new_device_channels);
        }
        Ok(())
    }

    async fn prepare_member_profiles(
        &self,
        device_uid: &UID,
        channel_name: &str,
        member_profiles: Vec<Profile>,
    ) -> Result<()> {
        let mut member_profile_last_applied_duty_lock =
            self.all_member_profiles_last_applied_duties.write().await;
        let mut member_profile_requested_duties_lock =
            self.all_member_profiles_requested_duties.write().await;
        for member_profile in member_profiles {
            self.graph_commander
                .clear_channel_setting(device_uid, channel_name)
                .await;
            self.graph_commander
                .schedule_setting(device_uid, channel_name, &member_profile)
                .await?;
            // we don't want to overwrite the last applied duty if it's already present:
            if member_profile_last_applied_duty_lock
                .contains_key(&member_profile.uid)
                .not()
            {
                member_profile_last_applied_duty_lock.insert(member_profile.uid.clone(), 0);
            }
            member_profile_requested_duties_lock.insert(member_profile.uid, None);
        }
        Ok(())
    }

    pub async fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        let mut mix_profile_to_remove: Option<Arc<NormalizedMixProfile>> = None;
        let device_channel = (device_uid.clone(), channel_name.to_string());
        let mut scheduled_settings_lock = self.scheduled_settings.write().await;
        for (mix_profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel);
            if device_channels.is_empty() {
                mix_profile_to_remove.replace(Arc::clone(mix_profile));
            }
        }
        if let Some(mix_profile) = mix_profile_to_remove {
            scheduled_settings_lock.remove(&mix_profile);
        }
        self.graph_commander
            .clear_channel_setting(device_uid, channel_name)
            .await;
    }

    /// Processes all the member Profiles and applies the appropriate output per Mix Profile.
    /// This processes the member profiles for all mix profiles first, then applies the
    /// MixProfileFunction appropriately.
    /// Normally triggered by a loop/timer.
    pub async fn update_speeds(&self) {
        if self.scheduled_settings.read().await.is_empty() {
            return;
        }
        self.process_member_profiles().await;
        let requested_duties = self.all_member_profiles_requested_duties.read().await;
        let last_applied_duties = self.all_member_profiles_last_applied_duties.read().await;
        for (mix_profile, device_channels) in self.scheduled_settings.read().await.iter() {
            let mut member_values = Vec::with_capacity(requested_duties.len());
            let mut members_have_no_output = true;
            for member_profile_uid in &mix_profile.member_profile_uids {
                let output = &requested_duties[member_profile_uid];
                if output.is_some() {
                    members_have_no_output = false;
                }
                // We need the last applied values when ANY profile produces output, so we can
                // properly compare the results and apply the correct Duty. None output from
                // a profile means 'No Change' and device communication can be costly.
                let value_for_calculation = output
                    .as_ref()
                    .unwrap_or_else(|| &last_applied_duties[member_profile_uid]);
                member_values.push(value_for_calculation);
            }
            if members_have_no_output {
                continue; // Nothing to do if all member Profile Outputs are None
            }
            let duty_to_apply = Self::apply_mix_function(&member_values, &mix_profile.mix_function);
            for (device_uid, channel_name) in device_channels {
                self.graph_commander
                    .set_speed(device_uid, channel_name, duty_to_apply)
                    .await;
            }
        }
    }

    /// Processes all the member Profiles and collects their outputs.
    /// This allows us to calculate all member profiles only once, and use them for any number of
    /// Mix Profiles.
    async fn process_member_profiles(&self) {
        let mut requested_duties = self.all_member_profiles_requested_duties.write().await;
        let mut last_applied_duties = self.all_member_profiles_last_applied_duties.write().await;
        for member_profile in self.graph_commander.scheduled_settings.read().await.keys() {
            let optional_duty_to_set = self
                .graph_commander
                .process_speed_setting(member_profile)
                .await;
            requested_duties
                .get_mut(&member_profile.profile_uid)
                .map(|d| *d = optional_duty_to_set);
            if let Some(duty_to_set) = optional_duty_to_set {
                last_applied_duties
                    .get_mut(&member_profile.profile_uid)
                    .map(|d| *d = duty_to_set);
            }
        }
    }

    fn apply_mix_function(
        member_values: &Vec<&Duty>,
        mix_function: &ProfileMixFunctionType,
    ) -> Duty {
        // Since the member functions manage their own thresholds and the safety latch should
        //  kick off about the same time for all of them, we don't check thresholds here.
        match mix_function {
            ProfileMixFunctionType::Min => **member_values.iter().min().unwrap_or(&&0),
            ProfileMixFunctionType::Max => **member_values.iter().max().unwrap_or(&&0),
            ProfileMixFunctionType::Avg => member_values
                .iter()
                .map(|d| **d as usize)
                .sum::<usize>()
                .div(member_values.len()) as Duty,
        }
    }

    pub async fn normalize_mix_setting(
        &self,
        profile: &Profile,
        member_profiles: &Vec<Profile>,
    ) -> Result<NormalizedMixProfile> {
        Ok(NormalizedMixProfile {
            profile_uid: profile.uid.clone(),
            mix_function: profile.mix_function_type.unwrap(),
            member_profile_uids: member_profiles.iter().map(|p| p.uid.clone()).collect(),
        })
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
    /// Only compare ProfileUID
    /// This allows us to update the Profile settings easily, and the UID is what matters anyway.
    fn eq(&self, other: &Self) -> bool {
        self.profile_uid == other.profile_uid
    }
}

impl Eq for NormalizedMixProfile {}

impl Hash for NormalizedMixProfile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.profile_uid.hash(state)
    }
}

#[cfg(test)]
mod tests {
    use crate::processing::commanders::mix::MixProfileCommander;
    use crate::setting::ProfileMixFunctionType;

    #[test]
    fn apply_mix_function_test_min() {
        let member_values = vec![&20, &21, &22, &23, &24];
        let mix_function = ProfileMixFunctionType::Min;
        let result = MixProfileCommander::apply_mix_function(&member_values, &mix_function);
        assert_eq!(result, 20);
    }

    #[test]
    fn apply_mix_function_test_max() {
        let member_values = vec![&0, &1, &2, &3, &4];
        let mix_function = ProfileMixFunctionType::Max;
        let result = MixProfileCommander::apply_mix_function(&member_values, &mix_function);
        assert_eq!(result, 4);
    }

    #[test]
    fn apply_mix_function_test_avg() {
        let member_values = vec![&0, &1, &2, &3, &4];
        let mix_function = ProfileMixFunctionType::Avg;
        let result = MixProfileCommander::apply_mix_function(&member_values, &mix_function);
        assert_eq!(result, 2);
    }

    #[test]
    fn apply_mix_function_test_avg_large() {
        let member_values = vec![&120, &121, &122, &123, &124];
        let mix_function = ProfileMixFunctionType::Avg;
        let result = MixProfileCommander::apply_mix_function(&member_values, &mix_function);
        assert_eq!(result, 122);
    }
}
