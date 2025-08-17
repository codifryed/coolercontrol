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

use crate::device::{ChannelName, DeviceUID, Duty, UID};
use crate::engine::commanders::graph::GraphProfileCommander;
use crate::engine::commanders::mix::MixProfileCommander;
use crate::engine::{utils, DeviceChannelProfileSetting};
use crate::setting::{Offset, Profile, ProfileType, ProfileUID};
use anyhow::anyhow;
use log::error;
use moro_local::Scope;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::rc::Rc;

type OverlayProfile = Profile;

pub struct OverlayProfileCommander {
    graph_commander: Rc<GraphProfileCommander>,
    mix_commander: Rc<MixProfileCommander>,
    scheduled_settings:
        RefCell<HashMap<Rc<NormalizedOverlayProfile>, HashSet<DeviceChannelProfileSetting>>>,
    /// The last calculated Option<Duty> for each Overlay Profile.
    pub process_output_cache: RefCell<HashMap<ProfileUID, Option<Duty>>>,
}

impl OverlayProfileCommander {
    pub fn new(
        graph_commander: Rc<GraphProfileCommander>,
        mix_commander: Rc<MixProfileCommander>,
    ) -> Self {
        Self {
            graph_commander,
            mix_commander,
            scheduled_settings: RefCell::new(HashMap::new()),
            process_output_cache: RefCell::new(HashMap::new()),
        }
    }

    pub fn schedule_setting(
        &self,
        device_channel: DeviceChannelProfileSetting,
        overlay_profile: &OverlayProfile,
        member_profile: &Profile,
        member_profile_members: Vec<Profile>,
    ) -> anyhow::Result<()> {
        if overlay_profile.p_type != ProfileType::Overlay {
            return Err(anyhow!(
                "Only Overlay Profiles are supported for scheduling in the MixProfileCommander"
            ));
        }
        if overlay_profile.offset_profile.is_none() {
            return Err(anyhow!(
                "Offset Profile must be present for an Overlay Profile"
            ));
        }
        if overlay_profile.offset_profile.as_ref().unwrap().is_empty() {
            return Err(anyhow!(
                "Overlay Profile offset profiles should have at least one duty/offset pair"
            ));
        }
        // Clear the channel setting in case another overlay profile is already scheduled:
        self.clear_channel_setting(device_channel.device_uid(), device_channel.channel_name());
        let normalized_overlay_setting =
            Self::normalize_overlay_setting(overlay_profile, member_profile);
        self.schedule_member_profiles(&device_channel, member_profile, member_profile_members)?;
        let mut settings_lock = self.scheduled_settings.borrow_mut();
        if let Some(mut existing_device_channels) =
            settings_lock.remove(&normalized_overlay_setting)
        {
            // We replace the existing NormalizedOverlayProfile if it exists to make sure it's
            // internal settings are up to date
            existing_device_channels.insert(device_channel);
            settings_lock.insert(
                Rc::new(normalized_overlay_setting),
                existing_device_channels,
            );
        } else {
            let mut new_device_channels = HashSet::new();
            new_device_channels.insert(device_channel);
            settings_lock.insert(Rc::new(normalized_overlay_setting), new_device_channels);
            self.process_output_cache
                .borrow_mut()
                .insert(overlay_profile.uid.clone(), None);
        }
        Ok(())
    }

    fn schedule_member_profiles(
        &self,
        device_channel: &DeviceChannelProfileSetting,
        member_profile: &Profile,
        member_profile_members: Vec<Profile>,
    ) -> anyhow::Result<()> {
        // all graph profiles for this DeviceChannelProfileSetting are already cleared
        // Add the Overlay settings for the member profile to be processed
        match member_profile.p_type {
            ProfileType::Graph => {
                self.graph_commander
                    .schedule_setting(device_channel.clone(), member_profile)?;
            }
            ProfileType::Mix => {
                self.mix_commander.schedule_setting(
                    device_channel.clone(),
                    member_profile,
                    member_profile_members,
                )?;
            }
            _ => return Err(anyhow!("Only Graph and Mix Profiles are supported")),
        }
        Ok(())
    }

    pub fn clear_channel_setting(&self, device_uid: &UID, channel_name: &str) {
        let mut overlay_profile_to_remove: Option<Rc<NormalizedOverlayProfile>> = None;
        let device_channel = DeviceChannelProfileSetting::Overlay {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_string(),
        };
        let mut scheduled_settings_lock = self.scheduled_settings.borrow_mut();
        for (overlay_profile, device_channels) in scheduled_settings_lock.iter_mut() {
            device_channels.remove(&device_channel);
            if device_channels.is_empty() {
                overlay_profile_to_remove.replace(Rc::clone(overlay_profile));
                self.process_output_cache
                    .borrow_mut()
                    .remove(&overlay_profile.profile_uid);
            }
        }
        if let Some(overlay_profile) = overlay_profile_to_remove {
            scheduled_settings_lock.remove(&overlay_profile);
        }
        self.mix_commander
            .clear_channel_setting(device_uid, channel_name);
        self.graph_commander
            .clear_channel_setting(device_uid, channel_name);
    }

    /// This method processes all scheduled profiles and updates the output cache.
    /// This should be called very early, right after the `GraphProfileCommander`
    /// and `MixProfileCommander` processes, and only once per update cycle.
    pub fn process_all_profiles(&self) {
        let mut output_cache_lock = self.process_output_cache.borrow_mut();
        // All the member profiles have been processed already by the graph and mix commanders:
        let requested_graph_duties = self.graph_commander.process_output_cache.borrow();
        let requested_mix_duties = self.mix_commander.process_output_cache.borrow();
        for overlay_profile in self.scheduled_settings.borrow().keys() {
            let member_output_option = match overlay_profile.member_profile_type {
                ProfileType::Graph => {
                    requested_graph_duties.get(&overlay_profile.member_profile_uid)
                }
                ProfileType::Mix => requested_mix_duties.get(&overlay_profile.member_profile_uid),
                _ => {
                    error!("Only Graph and Mix Profiles are supported for Overlay Profiles");
                    continue;
                }
            };
            let Some(member_output) = member_output_option else {
                error!(
                        "Overlay Profile calculation for {} skipped because of missing member output duty ",
                        overlay_profile.profile_uid
                    );
                // In very rare cases in the past, this was possible due to a race condition.
                // This should no longer happen, but we avoid the panic anyway.
                if let Some(cache) = output_cache_lock.get_mut(&overlay_profile.profile_uid) {
                    *cache = None;
                }
                continue;
            };
            let Some(member_duty) = member_output else {
                // Nothing to apply
                if let Some(cache) = output_cache_lock.get_mut(&overlay_profile.profile_uid) {
                    *cache = None;
                }
                continue;
            };
            let duty_to_apply = Self::apply_offset(*member_duty, &overlay_profile.offset_profile);
            if let Some(cache) = output_cache_lock.get_mut(&overlay_profile.profile_uid) {
                *cache = Some(duty_to_apply);
            }
        }
    }

    /// Processes all the member Profiles and applies the appropriate output per Overlay Profile.
    /// Normally triggered by a loop/timer.
    pub fn update_speeds<'s>(&'s self, scope: &'s Scope<'s, 's, anyhow::Result<()>>) {
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

    /// Collects the duties to apply for all scheduled Overlay Profiles from the output cache.
    fn collect_duties_to_apply(&self) -> HashMap<DeviceUID, Vec<(ChannelName, Duty)>> {
        let mut output_to_apply = HashMap::new();
        let output_cache_lock = self.process_output_cache.borrow();
        for (overlay_profile, device_channels) in self.scheduled_settings.borrow().iter() {
            let optional_duty_to_set = output_cache_lock[&overlay_profile.profile_uid]
                .as_ref()
                .copied();
            let Some(duty_to_set) = optional_duty_to_set else {
                continue;
            };
            for device_channel in device_channels {
                // We only apply Overlay Profiles directly applied to fan channels
                if let DeviceChannelProfileSetting::Overlay {
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
    #[allow(clippy::cast_sign_loss)]
    fn apply_offset(member_duty: Duty, offset_profile: &[(Duty, Offset)]) -> Duty {
        let calculated_offset = utils::interpolate_offset_profile(offset_profile, member_duty);
        (i32::from(member_duty) + i32::from(calculated_offset)).clamp(0, 100) as Duty
    }

    fn normalize_overlay_setting(
        profile: &Profile,
        member_profile: &Profile,
    ) -> NormalizedOverlayProfile {
        let normalized_offset_profile =
            utils::normalize_offset_profile(profile.offset_profile.as_ref().unwrap());
        NormalizedOverlayProfile {
            profile_uid: profile.uid.clone(),
            offset_profile: normalized_offset_profile,
            member_profile_uid: member_profile.uid.clone(),
            member_profile_type: member_profile.p_type.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedOverlayProfile {
    profile_uid: ProfileUID,
    offset_profile: Vec<(Duty, Offset)>,
    member_profile_uid: ProfileUID,
    member_profile_type: ProfileType,
}

impl Default for NormalizedOverlayProfile {
    fn default() -> Self {
        Self {
            profile_uid: String::default(),
            offset_profile: vec![],
            member_profile_uid: String::default(),
            member_profile_type: ProfileType::Graph,
        }
    }
}

impl PartialEq for NormalizedOverlayProfile {
    /// Only compare `ProfileUID`
    /// This allows us to update the Profile settings easily, and the UID is what matters anyway.
    fn eq(&self, other: &Self) -> bool {
        self.profile_uid == other.profile_uid
    }
}

impl Eq for NormalizedOverlayProfile {}

impl Hash for NormalizedOverlayProfile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.profile_uid.hash(state);
    }
}
