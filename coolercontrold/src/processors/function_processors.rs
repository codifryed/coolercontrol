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

use std::collections::{HashMap, VecDeque};
use std::ops::Not;

use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use log::error;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use yata::methods::TMA;
use yata::prelude::Method;

use crate::AllDevices;
use crate::device::UID;
use crate::processors::{Processor, SpeedProfileData};
use crate::repositories::repository::DeviceLock;
use crate::setting::{FunctionType, ProfileType};

pub const TMA_DEFAULT_WINDOW_SIZE: u8 = 8;
const SAMPLE_SIZE: isize = 16;
const MIN_TEMP_HIST_STACK_SIZE: u8 = 2;

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

/// The standard Function with Hysteresis control
pub struct FunctionStandardPreProcessor {
    all_devices: AllDevices,
    channel_settings_metadata: RwLock<HashMap<UID, HashMap<String, ChannelSettingMetadata>>>,
}

impl FunctionStandardPreProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices,
            channel_settings_metadata: RwLock::new(HashMap::new()),
        }
    }

    fn data_is_sane(&self, data: &SpeedProfileData, temp_source_device_option: Option<&DeviceLock>) -> bool {
        if data.profile.function.response_delay.is_none()
            || data.profile.function.deviance.is_none()
            || data.profile.function.only_downward.is_none() {
            error!(
                "All required fields must be set for the standard Function: {:?}, {:?}, {:?}",
                data.profile.function.response_delay,
                data.profile.function.deviance,
                data.profile.function.only_downward,
            );
            return false;
        }
        if temp_source_device_option.is_none() {
            error!("Temperature Source Device is currently not present: {}", data.profile.temp_source.device_uid);
            return false;
        }
        return true;
    }

    async fn fill_temp_stack(
        metadata: &mut ChannelSettingMetadata,
        data: &mut SpeedProfileData,
        temp_source_device_option: Option<&DeviceLock>,
    ) -> Result<()> {
        let temp_source_device = temp_source_device_option.unwrap().read().await;
        if metadata.last_applied_temp == 0. {
            // this is needed for the first application
            let mut latest_temps = temp_source_device.status_history.iter()
                .rev() // reverse so that take() takes the latest
                .take(metadata.ideal_stack_size)
                .flat_map(|status| status.temps.as_slice())
                .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                .map(|temp_status| temp_status.temp)
                .collect::<Vec<f64>>();
            latest_temps.reverse(); // re-order temps to proper Vec order
            if latest_temps.is_empty() {
                return Err(anyhow!("There is no associated temperature with the Profile's Temp Source"));
            }
            metadata.temp_hist_stack.clear();
            metadata.temp_hist_stack.extend(latest_temps);
        } else {
            // the normal operation
            let current_temp: Option<f64> = temp_source_device.status_history
                .last()
                .and_then(|status| status.temps.as_slice().iter()
                    .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                    .map(|temp_status| temp_status.temp)
                    .last()
                );
            if current_temp.is_none() {
                return Err(anyhow!("There is no associated temperature with the Profile's Temp Source"));
            }
            metadata.temp_hist_stack.push_back(current_temp.unwrap());
        }
        Ok(())
    }

    fn temp_within_tolerance(
        temp_to_verify: &f64,
        last_applied_temp: &f64,
        deviance: &f64,
    ) -> bool {
        temp_to_verify <= &(last_applied_temp + deviance)
            && temp_to_verify >= &(last_applied_temp - deviance)
    }
}

#[async_trait]
impl Processor for FunctionStandardPreProcessor {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.p_type == ProfileType::Graph
            && data.profile.function.f_type == FunctionType::Standard
            && data.temp.is_none() // preprocessor only
    }

    async fn init_state(&self, device_uid: &UID, channel_name: &str) {
        self.channel_settings_metadata.write().await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(channel_name.to_string(), ChannelSettingMetadata::new());
    }

    async fn clear_state(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) =
            self.channel_settings_metadata.write().await
                .get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
    }

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        let temp_source_device_option = self.all_devices
            .get(data.profile.temp_source.device_uid.as_str());
        if self.data_is_sane(data, temp_source_device_option).not() {
            return data;
        }

        // setup metadata:
        let mut metadata_lock = self.channel_settings_metadata.write().await;
        let metadata = metadata_lock
            .get_mut(&data.device_uid).unwrap()
            .get_mut(&data.channel_name).unwrap();
        if metadata.ideal_stack_size == 0 {
            // set ideal size on initial run:
            metadata.ideal_stack_size = MIN_TEMP_HIST_STACK_SIZE.max(data.profile.function.response_delay.unwrap() + 1) as usize;
        }
        if let Err(err) = Self::fill_temp_stack(metadata, data, temp_source_device_option).await {
            error!("{err}");
            return data;
        }
        if metadata.temp_hist_stack.len() > metadata.ideal_stack_size {
            metadata.temp_hist_stack.pop_front();
        } else if metadata.last_applied_temp == 0. && metadata.temp_hist_stack.len() < metadata.ideal_stack_size {
            // Very first run after boot/wakeup, let's apply something right away
            let temp_to_apply = metadata.temp_hist_stack.front().cloned().unwrap();
            data.temp = Some(temp_to_apply);
            metadata.last_applied_temp = temp_to_apply;
            return data;
        }

        // main processor logic:
        if data.profile.function.only_downward.unwrap() {
            let newest_temp = metadata.temp_hist_stack.back().unwrap().clone();
            if newest_temp > metadata.last_applied_temp {
                metadata.temp_hist_stack.clear();
                metadata.temp_hist_stack.push_back(newest_temp);
                data.temp = Some(newest_temp);
                metadata.last_applied_temp = newest_temp;
                return data;
            }
        }
        let oldest_temp = metadata.temp_hist_stack.front().cloned().unwrap();
        let oldest_temp_within_tolerance = Self::temp_within_tolerance(
            &oldest_temp,
            &metadata.last_applied_temp,
            data.profile.function.deviance.as_ref().unwrap(),
        );
        if metadata.temp_hist_stack.len() > MIN_TEMP_HIST_STACK_SIZE as usize {
            let newest_temp_within_tolerance = Self::temp_within_tolerance(
                &metadata.temp_hist_stack.back().unwrap(),
                &metadata.last_applied_temp,
                data.profile.function.deviance.as_ref().unwrap(),
            );
            if oldest_temp_within_tolerance && newest_temp_within_tolerance {
                // collapse the stack, as we want to skip any spikes that happened within the delay period
                let newest_temp = metadata.temp_hist_stack.pop_back().unwrap();
                metadata.temp_hist_stack.truncate(1);
                metadata.temp_hist_stack.push_back(newest_temp);
            }
        }
        if oldest_temp_within_tolerance {
            return data; // nothing to apply
        }
        data.temp = Some(oldest_temp);
        metadata.last_applied_temp = oldest_temp;
        data
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelSettingMetadata {
    pub temp_hist_stack: VecDeque<f64>,
    pub ideal_stack_size: usize,
    pub last_applied_temp: f64,
}

impl ChannelSettingMetadata {
    pub fn new() -> Self {
        Self {
            temp_hist_stack: VecDeque::new(),
            ideal_stack_size: 0,
            last_applied_temp: 0.,
        }
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

    /// Computes an exponential moving average from give temps and returns the final/current value from that average.
    /// Exponential moving average gives the most recent values more weight. This is particularly helpful
    /// for setting duty for dynamic temperature sources like CPU. (Good reaction but also averaging)
    /// Will panic if sample_size is 0.
    /// Rounded to the nearest 100th decimal place
    fn current_temp_from_exponential_moving_average(all_temps: &[f64], window_size: Option<u8>) -> f64 {
        (TMA::new_over(
            window_size.unwrap_or(TMA_DEFAULT_WINDOW_SIZE),
            Self::get_temps_slice(all_temps),
        ).unwrap()
            .last().unwrap() * 100.
        ).round() / 100.
    }

    fn get_temps_slice(all_temps: &[f64]) -> &[f64] {
        // keeping the sample size low allows the average to be more forward-aggressive,
        // otherwise the actual reading and the EMA take quite a while before they are the same value
        // todo: we could auto-size the sample size, if the window is larger than the default sample size,
        //  but should test what the actual outcome with be and if that's a realistic value for users.
        let sample_delta = all_temps.len() as isize - SAMPLE_SIZE;
        if sample_delta > 0 {
            all_temps.split_at(sample_delta as usize).1
        } else {
            all_temps
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
        let mut temps = { // scoped for the device read lock
            let temp_source_device = temp_source_device_option.unwrap().read().await;
            temp_source_device.status_history.iter()
                .rev() // reverse so that take() takes the end part
                // we only need the last (sample_size ) temps for EMA:
                .take(SAMPLE_SIZE as usize)
                .flat_map(|status| status.temps.as_slice())
                .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
                .map(|temp_status| temp_status.temp)
                .collect::<Vec<f64>>()
        };
        temps.reverse(); // re-order temps so last is last
        data.temp = if temps.is_empty() { None } else {
            Some(Self::current_temp_from_exponential_moving_average(
                &temps,
                data.profile.function.sample_window,
            ))
        };
        data
    }
}

#[cfg(test)]
mod tests {
    use crate::processors::function_processors::FunctionEMAPreProcessor;

    #[test]
    fn current_temp_from_exponential_moving_average_test() {
        let given_expected: Vec<(&[f64], f64)> = vec![
            // these are just samples. Tested with real hardware for expected results,
            // which are not so clear in numbers here.
            (
                &[20., 25.],
                20.05
            ),
            (
                &[20., 25., 30., 90., 90., 90., 30., 30., 30., 30.],
                35.86
            ),
            (
                &[30., 30., 30., 30.],
                30.
            ),
        ];
        for (given, expected) in given_expected {
            assert_eq!(
                FunctionEMAPreProcessor::current_temp_from_exponential_moving_average(given, None),
                expected
            )
        }
    }
}
