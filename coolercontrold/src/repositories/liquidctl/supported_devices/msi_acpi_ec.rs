/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use std::collections::HashMap;

use lazy_static::lazy_static;
use regex::Regex;

use crate::device::{ChannelStatus, ChannelInfo, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions, TempStatus};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

pub type StatusMap = HashMap<String, String>;

#[derive(Debug)]
pub struct MsiAcpiEcSupport;

impl MsiAcpiEcSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for MsiAcpiEcSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::MsiAcpiEc
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        let rt_fan_channel_names = vec![
            "cpu fan".to_string(),
            "gpu fan".to_string(),
        ];
        for rt_channel_name in rt_fan_channel_names {
            channels.insert(
                rt_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: true,
                    }),
                    ..Default::default()
                },
            );
        }

        let fan_channel_names = vec![
            "cpu fan step 1".to_string(),
            "cpu fan step 2".to_string(),
            "cpu fan step 3".to_string(),
            "cpu fan step 4".to_string(),
            "cpu fan step 5".to_string(),
            "cpu fan step 6".to_string(),
            "cpu fan step 7".to_string(),
            "gpu fan step 1".to_string(),
            "gpu fan step 2".to_string(),
            "gpu fan step 3".to_string(),
            "gpu fan step 4".to_string(),
            "gpu fan step 5".to_string(),
            "gpu fan step 6".to_string(),
            "gpu fan step 7".to_string(),
        ];
        for channel_name in fan_channel_names {
            channels.insert(
                channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: true,
                    }),
                    ..Default::default()
                },
            );
        }

        let color_channels = vec![
            "tail".to_string(),
            "mic".to_string(),
        ];
        for channel_name in color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name,
                ChannelInfo {
                    lighting_modes,
                    ..Default::default()
                },
            );
        }

        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 0,
            temp_max: 110,
            driver_info: DriverInfo {
                drv_type: DriverType::Liquidctl,
                name: Some(self.supported_driver().to_string()),
                version: device_response.liquidctl_version.clone(),
                locations: self.collect_driver_locations(device_response),
            },
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("on", 0, 0, false, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }

    fn add_temp_probes(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let cpu_temp = status_map
            .get("cpu temp")
            .and_then(|s| self.parse_float(s));
        if let Some(temp) = cpu_temp {
            temps.push(TempStatus {
                name: "cpu temp".to_string(),
                temp,
            });
        }
        let gpu_temp = status_map
            .get("gpu temp")
            .and_then(|s| self.parse_float(s));
        if let Some(temp) = gpu_temp {
            temps.push(TempStatus {
                name: "gpu temp".to_string(),
                temp,
            });
        }
    }

    fn add_single_fan_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        let cpu_fan_rpm = status_map.get("cpu fan speed").and_then(|s| self.parse_u32(s));
        let cpu_fan_duty = status_map.get("cpu fan duty").and_then(|s| self.parse_float(s));
        let gpu_fan_rpm = status_map.get("gpu fan speed").and_then(|s| self.parse_u32(s));
        let gpu_fan_duty = status_map.get("gpu fan duty").and_then(|s| self.parse_float(s));
        if cpu_fan_rpm.is_some() || cpu_fan_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "cpu fan".to_string(),
                rpm: cpu_fan_rpm,
                duty: cpu_fan_duty,
                ..Default::default()
            });
        }
        if gpu_fan_rpm.is_some() || gpu_fan_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "gpu fan".to_string(),
                rpm: gpu_fan_rpm,
                duty: gpu_fan_duty,
                ..Default::default()
            });
        }
    }

    fn add_multiple_fans_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        lazy_static! {
            static ref NUMBER_PATTERN: Regex = Regex::new(r"\d+").unwrap();
            static ref CPU_FAN_SPEED: Regex = Regex::new(r"cpu fan speed step \d+").unwrap();
            static ref GPU_FAN_SPEED: Regex = Regex::new(r"gpu fan speed step \d+").unwrap();
            static ref CPU_FAN_DUTY: Regex = Regex::new(r"cpu fan duty step \d+").unwrap();
            static ref GPU_FAN_DUTY: Regex = Regex::new(r"gpu fan duty step \d+").unwrap();
        }
        let mut fans_map: HashMap<String, (Option<u32>, Option<f64>)> = HashMap::new();
        for (name, value) in status_map {
            if let Some(fan_number) = NUMBER_PATTERN
                .find_at(name, 3)
                .and_then(|number| self.parse_u32(number.as_str()))
            {
                if CPU_FAN_SPEED.is_match(name) {
                    let fan_name = format!("cpu fan step {fan_number}");
                    let (rpm, _) = fans_map.entry(fan_name).or_insert((None, None));
                    *rpm = self.parse_u32(value);
                } else if GPU_FAN_SPEED.is_match(name) {
                    let fan_name = format!("gpu fan step {fan_number}");
                    let (rpm, _) = fans_map.entry(fan_name).or_insert((None, None));
                    *rpm = self.parse_u32(value);
                } else if CPU_FAN_DUTY.is_match(name) {
                    let fan_name = format!("cpu fan step {fan_number}");
                    let (_, duty) = fans_map.entry(fan_name).or_insert((None, None));
                    *duty = self.parse_float(value);
                } else if GPU_FAN_DUTY.is_match(name) {
                    let fan_name = format!("gpu fan step {fan_number}");
                    let (_, duty) = fans_map.entry(fan_name).or_insert((None, None));
                    *duty = self.parse_float(value);
                }
            }
        }
        for (name, (rpm, duty)) in fans_map {
            channel_statuses.push(ChannelStatus {
                name,
                rpm,
                duty,
                ..Default::default()
            });
        }
    }
}
