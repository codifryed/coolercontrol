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

use async_trait::async_trait;
use log::error;

use crate::{AllDevices, utils};
use crate::device::UID;
use crate::processors::{Processor, SpeedProfileData};
use crate::setting::{FunctionType, ProfileType};

/// The default function returns the source temp as-is.
pub struct FunctionIdentityPreProcessor {
    all_devices: AllDevices,
}

impl FunctionIdentityPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices
        }
    }
}

#[async_trait]
impl Processor for FunctionIdentityPreProcessor {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.p_type == ProfileType::Graph
            && data.profile.function.f_type == FunctionType::Identity
            && data.temp.is_none() // preprocessor only
    }

    async fn init_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn clear_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_option = self.all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if temp_source_device_option.is_none() {
            error!("Temperature Source Device is currently not present: {}", data.profile.temp_source.device_uid);
            return data;
        }
        data.temp = temp_source_device_option.unwrap().read().await
            .status_history.iter().last() // last = latest temp
            .and_then(|status| status.temps.iter()
                .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                .map(|temp_status| temp_status.temp)
                .last()
            );
        data
    }
}

/// The EMA function calculates an Exponential Moving Average over recent temperatures and
/// returns the most recent value. (Dynamically affected by temp history)
pub struct FunctionEMAPreProcessor {
    all_devices: AllDevices,
}

impl FunctionEMAPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices
        }
    }
}

#[async_trait]
impl Processor for FunctionEMAPreProcessor {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.p_type == ProfileType::Graph
            && data.profile.function.f_type == FunctionType::ExponentialMovingAvg
            && data.temp.is_none() // preprocessor only
    }

    async fn init_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn clear_state(&self, _device_uid: &UID, _channel_name: &str) {}

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_option = self.all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if temp_source_device_option.is_none() {
            error!("Temperature Source Device is currently not present: {}", data.profile.temp_source.device_uid);
            return data;
        }
        let temp_source_device = temp_source_device_option.unwrap().read().await;
        let mut temps = temp_source_device.status_history.iter()
            .rev() // reverse so that take() takes the end part
            // we only need the last (sample_size ) temps for EMA:
            .take(utils::SAMPLE_SIZE as usize)
            .flat_map(|status| status.temps.as_slice())
            .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
            .map(|temp_status| temp_status.temp)
            .collect::<Vec<f64>>();
        temps.reverse(); // re-order temps so last is last
        data.temp = if temps.is_empty() { None } else {
            Some(utils::current_temp_from_exponential_moving_average(&temps))
        };
        data
    }
}
