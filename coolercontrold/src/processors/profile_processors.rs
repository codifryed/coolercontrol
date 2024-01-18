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
use std::sync::Arc;

use async_trait::async_trait;
use log::error;
use tokio::sync::RwLock;

use crate::device::{Device, UID};
use crate::processors::{utils, Processor, SpeedProfileData};
use crate::setting::{FunctionType, ProfileType};
use crate::AllDevices;

use super::NormalizedProfile;

/// The standard Graph Profile processor that calculates duty from interpolating the speed profile.
pub struct GraphProfileProcessor {}

impl GraphProfileProcessor {
    pub fn new() -> Self {
        Self {}
    }
}

pub struct MixProfileProcessor {
    all_devices: AllDevices,
}

impl MixProfileProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self { all_devices }
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
        data.profile.p_type == ProfileType::Mix && data.temp.is_some()
    }

    async fn init_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn clear_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_options = self
            .all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if temp_source_device_options.is_none() {}

        let temp_source_devices = data
            .profile
            .member_profiles
            .iter()
            .filter_map(|profile| {
                let Some(temp_source_device) = self
                    .all_devices
                    .get(profile.temp_source.device_uid.as_str())
                else {
                    error!(
                        "Temperature Source Device is currently not present: {}",
                        data.profile.temp_source.device_uid
                    );
                    return None;
                };

                Some((profile.speed_profile, temp_source_device))
            })
            .collect::<Vec<(Vec<(f64, u8)>, &Arc<RwLock<Device>>)>>();

        let mut member_duties = Vec::new();
        for (speed_profile, device) in temp_source_devices {
            let Some(temp) = device
                .read()
                .await
                .status_history
                .iter()
                .last() // last = latest temp
                .and_then(|status| {
                    status
                        .temps
                        .iter()
                        .filter(|temp_status| {
                            temp_status.name == data.profile.temp_source.temp_name
                        })
                        .map(|temp_status| temp_status.temp)
                        .last()
                })
            else {
                todo!()
            };

            let duty = utils::interpolate_profile(&speed_profile, temp);
            member_duties.push(duty);
        }

        match data.profile.function.f_type {
            FunctionType::Min => data.duty = member_duties.iter().min().copied(),
            FunctionType::Max => data.duty = member_duties.iter().max().copied(),
            _ => todo!(), // will other function types be allowed?
        }

        data
    }
}

//#[cfg(test)]
//mod tests {
//    fn mix_profile_max
//}
