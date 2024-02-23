/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
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

use std::collections::HashMap;

use async_trait::async_trait;
use log::{error, info, warn};
use tokio::sync::RwLock;

use crate::device::UID;
use crate::processors::{utils, Processor, SpeedProfileData};
use crate::setting::{ProfileMixFunctionType, ProfileType};
use crate::AllDevices;

/// The standard Graph Profile processor that calculates duty from interpolating the speed profile.
pub struct GraphProfileProcessor {}

impl GraphProfileProcessor {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct MixProfileProcessor {
    all_devices: AllDevices,
    cache: RwLock<HashMap<UID, u8>>,
}

impl MixProfileProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices,
            cache: RwLock::new(HashMap::new()),
        }
    }
}

#[async_trait]
impl Processor for GraphProfileProcessor {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.p_type == ProfileType::Graph && data.temp.is_some()
    }

    async fn init_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn clear_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        data.duty = Some(utils::interpolate_profile(
            &data.profile.speed_profile,
            data.temp.unwrap(),
        ));
        data
    }
}

#[async_trait]
impl Processor for MixProfileProcessor {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.p_type == ProfileType::Mix
    }

    async fn init_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn clear_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let mut member_requested_duties = Vec::new();

        for member_profile in data.profile.member_profiles.iter() {
            let Some(temp_source_device) = self
                .all_devices
                .get(member_profile.temp_source.device_uid.as_str())
            else {
                error!(
                    "Member Temperature Source Device is currently not present: {}",
                    member_profile.temp_source.device_uid
                );
                if let Some(cached_duty) = self
                    .cache
                    .read()
                    .await
                    .get(member_profile.temp_source.device_uid.as_str())
                {
                    member_requested_duties.push(*cached_duty);
                    warn!("Using duty from cache: {:?}", *cached_duty);
                } else {
                    error!("The temperature device is not present AND there is no cached duty.");
                }
                continue;
            };
            let device_lock = temp_source_device.read().await;

            let Some(temp) = device_lock
                .status_history
                .iter()
                .last() // last = latest temp
                .and_then(|status| {
                    status
                        .temps
                        .iter()
                        .filter(|temp_status| {
                            temp_status.name == member_profile.temp_source.temp_name
                        })
                        .map(|temp_status| temp_status.temp)
                        .last()
                })
            else {
                if let Some(cached_duty) = self.cache.read().await.get(&device_lock.uid) {
                    member_requested_duties.push(*cached_duty);
                    warn!("Using duty from cache: {:?}", *cached_duty);
                }
                continue;
            };

            let duty = utils::interpolate_profile(&member_profile.speed_profile, temp);

            self.cache
                .write()
                .await
                .entry(device_lock.uid.clone())
                .and_modify(|cache| *cache = duty)
                .or_insert(duty);
            member_requested_duties.push(duty);
        }

        if member_requested_duties.is_empty() {
            warn!("No member requested a duty!");
            return data;
        }

        match data.profile.mix_function.unwrap() {
            ProfileMixFunctionType::Min => data.duty = member_requested_duties.iter().min().copied(),
            ProfileMixFunctionType::Max => data.duty = member_requested_duties.iter().max().copied(),
            ProfileMixFunctionType::Avg => {
                let sum: u32 = member_requested_duties.iter().map(|x| *x as u32).sum();
                let len = member_requested_duties.iter().len() as u32;
                let avg = (sum / len) as u8;
                data.duty = Some(avg);
            }
        }

        info!(
            "Duty set to {:?} from requested duties {:?}",
            data.duty, member_requested_duties
        );
        data
    }
}

//#[cfg(test)]
//mod tests {
//    fn mix_profile_max
//}
