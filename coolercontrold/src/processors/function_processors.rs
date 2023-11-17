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
use yata::methods::TMA;
use yata::prelude::Method;

use crate::AllDevices;
use crate::device::UID;
use crate::processors::{Processor, SpeedProfileData};
use crate::setting::{FunctionType, ProfileType};

pub const TMA_DEFAULT_WINDOW_SIZE: u8 = 8;
const SAMPLE_SIZE: isize = 16;

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
        let temp_source_device = temp_source_device_option.unwrap().read().await;
        let mut temps = temp_source_device.status_history.iter()
            .rev() // reverse so that take() takes the end part
            // we only need the last (sample_size ) temps for EMA:
            .take(SAMPLE_SIZE as usize)
            .flat_map(|status| status.temps.as_slice())
            .filter(|temp_status| temp_status.name == data.profile.temp_source.temp_name)
            .map(|temp_status| temp_status.temp)
            .collect::<Vec<f64>>();
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
