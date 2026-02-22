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

use heck::ToTitleCase;
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::LazyLock;

use crate::device::{
    ChannelStatus, DeviceInfo, LightingMode, LightingModeType, Status, TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;

pub type StatusMap = HashMap<String, String>;

pub fn get_firmware_ver(status_map: &StatusMap) -> Option<String> {
    status_map.get("firmware version").cloned()
}

#[allow(clippy::ptr_arg)]
pub fn parse_float(value: &String) -> Option<f64> {
    parse_float_inner(value)
}
fn parse_float_inner(value: &str) -> Option<f64> {
    value.parse::<f64>().ok()
}
#[allow(clippy::ptr_arg)]
pub fn parse_u32(value: &String) -> Option<u32> {
    parse_u32_inner(value)
}
fn parse_u32_inner(value: &str) -> Option<u32> {
    value.parse::<u32>().ok()
}

/// Clamps the temperature to the range of -40.0 to 200.0 to filter out obviously invalid readings
fn valid_temp(temp: f64) -> f64 {
    temp.clamp(-40.0, 200.0)
}

/// Clamps the duty to the range of 0.0 to 100.0 to filter out obviously invalid readings
fn valid_duty(duty: f64) -> f64 {
    duty.clamp(0.0, 100.0)
}

/// Clamps the RPM to the range of 0 to `10_000` to filter out obviously invalid readings
fn valid_rpm(rpm: u32) -> u32 {
    rpm.clamp(0, 10_000)
}

pub struct ColorMode {
    pub name: String,
    pub min_colors: u8,
    pub max_colors: u8,
    pub speed_enabled: bool,
    pub backward_enabled: bool,
}

impl ColorMode {
    pub fn new(
        name: &str,
        min_colors: u8,
        max_colors: u8,
        speed_enabled: bool,
        backward_enabled: bool,
    ) -> Self {
        Self {
            name: name.to_string(),
            min_colors,
            max_colors,
            speed_enabled,
            backward_enabled,
        }
    }
}

/// It is a general purpose trait and each supported device struct must implement this trait.
/// Many of the default methods will cover all use cases, but it is advisable to override them
/// for increased efficiency and performance.
pub trait DeviceSupport: Debug {
    fn supported_driver(&self) -> BaseDriver;

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo;

    fn get_color_channel_modes(&self, channel_name: Option<&str>) -> Vec<LightingMode>;

    fn collect_driver_locations(&self, device_response: &DeviceResponse) -> Vec<String> {
        let mut locations = Vec::with_capacity(2);
        if let Some(hid_address) = &device_response.hid_address {
            locations.push(hid_address.clone());
        }
        if let Some(hwmon_address) = &device_response.hwmon_address {
            locations.push(hwmon_address.clone());
        }
        locations
    }

    fn extract_status(&self, status_map: &StatusMap, device_index: u8) -> Status {
        Status {
            temps: self.get_temperatures(status_map),
            channels: self.get_channel_statuses(status_map, device_index),
            ..Default::default()
        }
    }

    /// It's possible to override this method and use only the needed sub-functions per device
    fn get_temperatures(&self, status_map: &StatusMap) -> Vec<TempStatus> {
        let mut temps = Vec::with_capacity(status_map.len());
        self.add_liquid_temp(status_map, &mut temps);
        self.add_water_temp(status_map, &mut temps);
        self.add_temp(status_map, &mut temps);
        self.add_temp_probes(status_map, &mut temps);
        self.add_vrm_temp(status_map, &mut temps);
        self.add_case_temp(status_map, &mut temps);
        self.add_temp_sensors(status_map, &mut temps);
        // self.add_software_temp_sensors(status_map, &mut temps);
        // ...for a future feature (needs testing and is in dB)
        // self.add_noise_level(status_map, &mut temps, device_index);
        temps.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        temps
    }

    fn add_liquid_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let liquid_temp = status_map
            .get("liquid temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = liquid_temp {
            temps.push(TempStatus {
                name: "liquid".to_string(),
                temp,
            });
        }
    }

    fn add_water_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let water_temp = status_map
            .get("water temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = water_temp {
            temps.push(TempStatus {
                name: "water".to_string(),
                temp,
            });
        }
    }

    fn add_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let plain_temp = status_map
            .get("temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = plain_temp {
            temps.push(TempStatus {
                name: "temp".to_string(),
                temp,
            });
        }
    }

    fn add_temp_probes(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        static TEMP_PROBE_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"temperature \d+").unwrap());
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        for (probe_name, value) in status_map {
            if TEMP_PROBE_PATTERN.is_match(probe_name) {
                if let Some(temp) = parse_float_inner(value).map(valid_temp) {
                    if let Some(probe_number) =
                        NUMBER_PATTERN.find_at(probe_name, probe_name.len() - 2)
                    {
                        let name = format!("temp{}", probe_number.as_str());
                        temps.push(TempStatus { name, temp });
                    }
                }
            }
        }
    }

    /// Voltage Regulator temp for PSUs
    fn add_vrm_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let vrm_temp = status_map
            .get("vrm temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = vrm_temp {
            temps.push(TempStatus {
                name: "vrm".to_string(),
                temp,
            });
        }
    }

    fn add_case_temp(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let case_temp = status_map
            .get("case temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = case_temp {
            temps.push(TempStatus {
                name: "case".to_string(),
                temp,
            });
        }
    }

    fn add_temp_sensors(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        static TEMP_SENSOR_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"sensor \d+").unwrap());
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        for (sensor_name, value) in status_map {
            if TEMP_SENSOR_PATTERN.is_match(sensor_name) {
                if let Some(temp) = parse_float_inner(value).map(valid_temp) {
                    if let Some(sensor_number) =
                        NUMBER_PATTERN.find_at(sensor_name, sensor_name.len() - 2)
                    {
                        let name = format!("sensor{}", sensor_number.as_str());
                        temps.push(TempStatus { name, temp });
                    }
                }
            }
        }
    }

    fn add_software_temp_sensors(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        static SOFT_TEMP_SENSOR_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"soft. sensor \d+").unwrap());
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        for (sensor_name, value) in status_map {
            if SOFT_TEMP_SENSOR_PATTERN.is_match(sensor_name) {
                if let Some(temp) = parse_float_inner(value).map(valid_temp) {
                    if let Some(sensor_number) =
                        NUMBER_PATTERN.find_at(sensor_name, sensor_name.len() - 2)
                    {
                        let name = format!("soft-sensor{}", sensor_number.as_str());
                        temps.push(TempStatus { name, temp });
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn add_noise_level(&self, status_map: &StatusMap, temps: &mut Vec<TempStatus>) {
        let noise_lvl = status_map.get("noise level").and_then(parse_float);
        if let Some(noise) = noise_lvl {
            temps.push(TempStatus {
                name: "noise".to_string(),
                temp: noise,
            });
        }
    }

    /// It's possible to override this method and use only the needed sub-functions per device
    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = Vec::with_capacity(status_map.len());
        self.add_single_fan_status(status_map, &mut channel_statuses);
        self.add_single_external_fans_status(status_map, &mut channel_statuses);
        self.add_single_pump_status(status_map, &mut channel_statuses);
        self.add_single_pump_fan_status(status_map, &mut channel_statuses);
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
        self.add_multiple_external_fans_status(status_map, &mut channel_statuses);
        self.add_single_water_block_status(status_map, &mut channel_statuses);
        // only applicable for a single device so far:
        // self.add_psu_power_statuses(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        channel_statuses
    }

    fn add_single_fan_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        let fan_rpm = status_map
            .get("fan speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let fan_duty = status_map
            .get("fan duty")
            .and_then(parse_float)
            .map(valid_duty);
        if fan_rpm.is_some() || fan_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "fan".to_string(),
                rpm: fan_rpm,
                duty: fan_duty,
                ..Default::default()
            });
        }
    }

    fn add_single_external_fans_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        // asus_ryujin: where the status comes in singular, but the control channel is plural
        let fan_duty = status_map
            .get("external fan duty")
            .and_then(parse_float)
            .map(valid_duty);
        if fan_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "external-fans".to_string(),
                duty: fan_duty,
                ..Default::default()
            });
        }
    }

    fn add_single_pump_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        let pump_rpm = status_map
            .get("pump speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let pump_duty = status_map
            .get("pump duty")
            .and_then(parse_float)
            .map(valid_duty);
        if pump_rpm.is_some() || pump_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "pump".to_string(),
                rpm: pump_rpm,
                duty: pump_duty,
                ..Default::default()
            });
        }
    }

    fn add_single_pump_fan_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        let pump_rpm = status_map
            .get("pump fan speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let pump_duty = status_map
            .get("pump fan duty")
            .and_then(parse_float)
            .map(valid_duty);
        if pump_rpm.is_some() || pump_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "pump-fan".to_string(),
                rpm: pump_rpm,
                duty: pump_duty,
                ..Default::default()
            });
        }
    }

    fn add_single_water_block_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        let water_block_rpm = status_map
            .get("water block speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let water_block_duty = status_map
            .get("water block duty")
            .and_then(parse_float)
            .map(valid_duty);
        if water_block_rpm.is_some() || water_block_duty.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "waterblock-fan".to_string(),
                rpm: water_block_rpm,
                duty: water_block_duty,
                ..Default::default()
            });
        }
    }

    fn add_psu_power_statuses(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        if let Some(watts) = status_map.get("total power output").and_then(parse_float) {
            channel_statuses.push(ChannelStatus {
                name: "total-power".to_string(),
                watts: Some(watts),
                ..Default::default()
            });
        }
        if let Some(watts) = status_map
            .get("estimated input power")
            .and_then(parse_float)
        {
            channel_statuses.push(ChannelStatus {
                name: "estimated-input-power".to_string(),
                watts: Some(watts),
                ..Default::default()
            });
        }
        if let Some(watts) = status_map.get("+12v output power").and_then(parse_float) {
            channel_statuses.push(ChannelStatus {
                name: "12v-power".to_string(),
                watts: Some(watts),
                ..Default::default()
            });
        }
        if let Some(watts) = status_map.get("+5v output power").and_then(parse_float) {
            channel_statuses.push(ChannelStatus {
                name: "5v-power".to_string(),
                watts: Some(watts),
                ..Default::default()
            });
        }
        if let Some(watts) = status_map.get("+3.3v output power").and_then(parse_float) {
            channel_statuses.push(ChannelStatus {
                name: "3.3v-power".to_string(),
                watts: Some(watts),
                ..Default::default()
            });
        }
    }

    #[allow(dead_code)]
    /// This is used for special devices with limited pump speeds that are named (str)
    fn get_pump_mode(&self, status_map: &StatusMap) -> Option<String> {
        status_map.get("pump mode").cloned()
    }

    fn add_multiple_fans_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        static MULTIPLE_FAN_SPEED: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^fan \d+ speed").unwrap());
        static MULTIPLE_FAN_SPEED_CORSAIR: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^fan speed \d+").unwrap());
        static MULTIPLE_FAN_DUTY: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^fan \d+ duty").unwrap());
        let mut fans_map: HashMap<String, (Option<u32>, Option<f64>)> =
            HashMap::with_capacity(status_map.len());
        for (name, value) in status_map {
            if let Some(fan_number) = NUMBER_PATTERN
                .find_at(name, 3)
                .and_then(|number| parse_u32_inner(number.as_str()))
            {
                let fan_name = format!("fan{fan_number}");
                if MULTIPLE_FAN_SPEED.is_match(name) || MULTIPLE_FAN_SPEED_CORSAIR.is_match(name) {
                    let (rpm, _) = fans_map.entry(fan_name).or_insert((None, None));
                    *rpm = parse_u32_inner(value).map(valid_rpm);
                } else if MULTIPLE_FAN_DUTY.is_match(name) {
                    let (_, duty) = fans_map.entry(fan_name).or_insert((None, None));
                    *duty = parse_float_inner(value).map(valid_duty);
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

    fn add_multiple_external_fans_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        static MULTIPLE_FAN_SPEED: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"external fan \d+ speed").unwrap());
        let mut fans_map: HashMap<String, (Option<u32>, Option<f64>)> =
            HashMap::with_capacity(status_map.len());
        for (name, value) in status_map {
            if MULTIPLE_FAN_SPEED.is_match(name) {
                if let Some(fan_number) = NUMBER_PATTERN
                    .find_at(name, 12)
                    .and_then(|number| parse_u32_inner(number.as_str()))
                {
                    let fan_name = format!("external-fan{fan_number}");
                    let (rpm, _) = fans_map.entry(fan_name).or_insert((None, None));
                    *rpm = parse_u32_inner(value).map(valid_rpm);
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

    fn add_flow_sensor_status(
        &self,
        status_map: &StatusMap,
        channel_statuses: &mut Vec<ChannelStatus>,
    ) {
        // We use rpm speed as a proxy for flow speed
        let flow_speed = status_map
            .get("flow sensor")
            .and_then(parse_u32)
            .map(valid_rpm);
        if flow_speed.is_some() {
            channel_statuses.push(ChannelStatus {
                name: "flow".to_string(),
                rpm: flow_speed,
                ..Default::default()
            });
        }
    }

    fn channel_to_frontend_name(&self, lighting_channel: &str) -> String {
        lighting_channel.replace(['-', '_'], " ").to_title_case()
    }

    fn convert_to_channel_lighting_modes(&self, color_modes: Vec<ColorMode>) -> Vec<LightingMode> {
        let mut channel_lighting_modes = Vec::with_capacity(color_modes.len());
        for color_mode in color_modes {
            channel_lighting_modes.push(LightingMode {
                frontend_name: self.channel_to_frontend_name(&color_mode.name),
                name: color_mode.name,
                min_colors: color_mode.min_colors,
                max_colors: color_mode.max_colors,
                speed_enabled: color_mode.speed_enabled,
                backward_enabled: color_mode.backward_enabled,
                type_: LightingModeType::Liquidctl,
            });
        }
        channel_lighting_modes
    }
}

/// Tests
#[cfg(test)]
mod tests {
    use crate::repositories::liquidctl::supported_devices::kraken_x3::KrakenX3Support;

    use super::*;

    fn assert_temp_status_vector_contents_eq(
        device_support: &KrakenX3Support,
        given_expected: Vec<(HashMap<String, String>, Vec<TempStatus>)>,
    ) {
        for (given, expected) in given_expected {
            let result = device_support.get_temperatures(&given);
            assert!(expected
                .iter()
                .all(|temp_status| result.contains(temp_status)));
            assert!(result
                .iter()
                .all(|temp_status| expected.contains(temp_status)));
        }
    }

    /// Using `KrakenX3Support` to test the Trait functions
    #[test]
    fn get_firmware() {
        let given_expected = vec![
            (
                HashMap::from([("firmware version".to_string(), "1.0.0".to_string())]),
                Some("1.0.0".to_string()),
            ),
            (
                HashMap::from([("firmware".to_string(), "1.0.0".to_string())]),
                None,
            ),
            (
                HashMap::from([("firmware version".to_string(), "whatever".to_string())]),
                Some("whatever".to_string()),
            ),
        ];
        for (given, expected) in given_expected {
            assert_eq!(get_firmware_ver(&given), expected);
        }
    }

    #[test]
    fn get_temperatures_fail() {
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![
            (
                HashMap::from([("liquid temperature".to_string(), "whatever".to_string())]),
                vec![],
            ),
            (
                HashMap::from([("some other temperature".to_string(), temp.clone())]),
                vec![],
            ),
        ];
        for (given, expected) in given_expected {
            let result = device_support.get_temperatures(&given);
            assert!(expected
                .iter()
                .all(|temp_status| !result.contains(temp_status)));
            assert!(result
                .iter()
                .all(|temp_status| !expected.contains(temp_status)));
        }
    }

    #[test]
    fn add_liquid_temp() {
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([("liquid temperature".to_string(), temp.clone())]),
            vec![TempStatus {
                name: "liquid".to_string(),
                temp: temp.parse().unwrap(),
            }],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_water_temp() {
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([("water temperature".to_string(), temp.clone())]),
            vec![TempStatus {
                name: "water".to_string(),
                temp: temp.parse().unwrap(),
            }],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_plain_temp() {
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([("temperature".to_string(), temp.clone())]),
            vec![TempStatus {
                name: "temp".to_string(),
                temp: temp.parse().unwrap(),
            }],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_temp_probes() {
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([
                ("temperature 1".to_string(), temp.clone()),
                ("temperature 2".to_string(), temp.clone()),
                ("temperature 3".to_string(), temp.clone()),
            ]),
            vec![
                TempStatus {
                    name: "temp1".to_string(),
                    temp: temp.parse().unwrap(),
                },
                TempStatus {
                    name: "temp2".to_string(),
                    temp: temp.parse().unwrap(),
                },
                TempStatus {
                    name: "temp3".to_string(),
                    temp: temp.parse().unwrap(),
                },
            ],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_vrm_temp() {
        let device_support = KrakenX3Support::new();
        let vrm_temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([("vrm temperature".to_string(), vrm_temp.clone())]),
            vec![TempStatus {
                name: "vrm".to_string(),
                temp: vrm_temp.parse().unwrap(),
            }],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_case_temp() {
        let device_support = KrakenX3Support::new();
        let case_temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([("case temperature".to_string(), case_temp.clone())]),
            vec![TempStatus {
                name: "case".to_string(),
                temp: case_temp.parse().unwrap(),
            }],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_temp_sensors() {
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([
                ("sensor 1".to_string(), temp.clone()),
                ("sensor 2".to_string(), temp.clone()),
                ("sensor 3".to_string(), temp.clone()),
            ]),
            vec![
                TempStatus {
                    name: "sensor1".to_string(),
                    temp: temp.parse().unwrap(),
                },
                TempStatus {
                    name: "sensor2".to_string(),
                    temp: temp.parse().unwrap(),
                },
                TempStatus {
                    name: "sensor3".to_string(),
                    temp: temp.parse().unwrap(),
                },
            ],
        )];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_noise_level() {
        let device_support = KrakenX3Support::new();
        let noise_lvl = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([("noise level".to_string(), noise_lvl.clone())]),
            vec![TempStatus {
                name: "noise".to_string(),
                temp: noise_lvl.parse().unwrap(),
            }],
        )];
        for (given, expected) in given_expected {
            let mut result_temps = vec![];
            device_support.add_noise_level(&given, &mut result_temps);
            assert!(expected
                .iter()
                .all(|temp_status| result_temps.contains(temp_status)));
            assert!(result_temps
                .iter()
                .all(|temp_status| expected.contains(temp_status)));
        }
    }

    fn assert_channel_statuses_eq(
        device_support: &KrakenX3Support,
        device_id: u8,
        given_expected: Vec<(HashMap<String, String>, Vec<ChannelStatus>)>,
    ) {
        for (given, expected) in given_expected {
            let result = device_support.get_channel_statuses(&given, device_id);
            assert!(
                expected
                    .iter()
                    .all(|channel_status| result.contains(channel_status)),
                "Resulting channel status doesn't contain all Expected statuses\nResults:{result:?}\nExpected:{expected:?}",
            );
            assert!(
                result
                    .iter()
                    .all(|channel_status| expected.contains(channel_status)),
                "Expected channel statuses don't contain all Resulting statuses\nExpected:{expected:?}\nResults:{result:?}"
            );
        }
    }

    #[test]
    fn add_single_fan_status() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([
                ("fan speed".to_string(), rpm.to_string()),
                ("fan duty".to_string(), duty.to_string()),
            ]),
            vec![ChannelStatus {
                name: "fan".to_string(),
                rpm: Some(rpm),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_fan_status_rpm() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let given_expected = vec![(
            HashMap::from([("fan speed".to_string(), rpm.to_string())]),
            vec![ChannelStatus {
                name: "fan".to_string(),
                rpm: Some(rpm),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_fan_status_duty() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([("fan duty".to_string(), duty.to_string())]),
            vec![ChannelStatus {
                name: "fan".to_string(),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_external_fan_status() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([("external fan duty".to_string(), duty.to_string())]),
            vec![ChannelStatus {
                name: "external-fans".to_string(),
                rpm: None,
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_status() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([
                ("pump speed".to_string(), rpm.to_string()),
                ("pump duty".to_string(), duty.to_string()),
            ]),
            vec![ChannelStatus {
                name: "pump".to_string(),
                rpm: Some(rpm),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_status_rpm() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let given_expected = vec![(
            HashMap::from([("pump speed".to_string(), rpm.to_string())]),
            vec![ChannelStatus {
                name: "pump".to_string(),
                rpm: Some(rpm),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_status_duty() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([("pump duty".to_string(), duty.to_string())]),
            vec![ChannelStatus {
                name: "pump".to_string(),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_fan_status() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([
                ("pump fan speed".to_string(), rpm.to_string()),
                ("pump fan duty".to_string(), duty.to_string()),
            ]),
            vec![ChannelStatus {
                name: "pump-fan".to_string(),
                rpm: Some(rpm),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_fan_status_rpm() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let given_expected = vec![(
            HashMap::from([("pump fan speed".to_string(), rpm.to_string())]),
            vec![ChannelStatus {
                name: "pump-fan".to_string(),
                rpm: Some(rpm),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_fan_status_duty() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([("pump fan duty".to_string(), duty.to_string())]),
            vec![ChannelStatus {
                name: "pump-fan".to_string(),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_water_block_status() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([
                ("water block speed".to_string(), rpm.to_string()),
                ("water block duty".to_string(), duty.to_string()),
            ]),
            vec![ChannelStatus {
                name: "waterblock-fan".to_string(),
                rpm: Some(rpm),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_water_block_status_rpm() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let given_expected = vec![(
            HashMap::from([("water block speed".to_string(), rpm.to_string())]),
            vec![ChannelStatus {
                name: "waterblock-fan".to_string(),
                rpm: Some(rpm),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_water_block_status_duty() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([("water block duty".to_string(), duty.to_string())]),
            vec![ChannelStatus {
                name: "waterblock-fan".to_string(),
                duty: Some(duty),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn get_pump_mode() {
        let device_support = KrakenX3Support::new();
        let status_map = HashMap::from([("pump mode".to_string(), "balanced".to_string())]);
        let result = device_support.get_pump_mode(&status_map);
        assert_eq!(result, Some("balanced".to_string()));
    }

    #[test]
    fn channel_to_frontend_name() {
        let device_support = KrakenX3Support::new();
        let channel_name = "here_is-the_channel-name".to_string();
        let result = device_support.channel_to_frontend_name(&channel_name);
        assert_eq!(result, "Here Is The Channel Name".to_string());
    }

    #[test]
    fn add_multiple_fans() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let duty: f64 = 33.3;
        let given_expected = vec![(
            HashMap::from([
                ("fan 1 speed".to_string(), rpm.to_string()),
                ("fan 1 duty".to_string(), duty.to_string()),
                ("fan 2 speed".to_string(), rpm.to_string()),
                ("fan 3 duty".to_string(), duty.to_string()),
                ("fan speed 4".to_string(), rpm.to_string()),
            ]),
            vec![
                ChannelStatus {
                    name: "fan1".to_string(),
                    rpm: Some(rpm),
                    duty: Some(duty),
                    ..Default::default()
                },
                ChannelStatus {
                    name: "fan2".to_string(),
                    rpm: Some(rpm),
                    ..Default::default()
                },
                ChannelStatus {
                    name: "fan3".to_string(),
                    duty: Some(duty),
                    ..Default::default()
                },
                ChannelStatus {
                    name: "fan4".to_string(),
                    rpm: Some(rpm),
                    ..Default::default()
                },
            ],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_multiple_external_fans() {
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 33;
        let given_expected = vec![(
            HashMap::from([
                ("external fan 1 speed".to_string(), rpm.to_string()),
                ("external fan 2 speed".to_string(), rpm.to_string()),
                ("external fan 3 speed".to_string(), rpm.to_string()),
            ]),
            vec![
                ChannelStatus {
                    name: "external-fan1".to_string(),
                    rpm: Some(rpm),
                    ..Default::default()
                },
                ChannelStatus {
                    name: "external-fan2".to_string(),
                    rpm: Some(rpm),
                    ..Default::default()
                },
                ChannelStatus {
                    name: "external-fan3".to_string(),
                    rpm: Some(rpm),
                    ..Default::default()
                },
            ],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn valid_temp_accepts_boundary_and_typical_values() {
        // Confirms that values within [-40, 200]°C pass through unchanged.
        assert_eq!(valid_temp(-40.0), -40.0);
        assert_eq!(valid_temp(0.0), 0.0);
        assert_eq!(valid_temp(25.0), 25.0);
        assert_eq!(valid_temp(100.0), 100.0);
        assert_eq!(valid_temp(200.0), 200.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn valid_temp_rejects_out_of_range() {
        // Confirms that hardware values outside the sane range are filtered to None.
        assert_eq!(valid_temp(-40.1), -40.);
        assert_eq!(valid_temp(200.1), 200.);
        assert_eq!(valid_temp(-273.15), -40.);
        assert_eq!(valid_temp(999.0), 200.);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn valid_duty_accepts_boundary_and_typical_values() {
        // Confirms that values within [0, 100]% pass through unchanged.
        assert_eq!(valid_duty(0.0), 0.0);
        assert_eq!(valid_duty(50.0), 50.0);
        assert_eq!(valid_duty(100.0), 100.0);
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn valid_duty_rejects_out_of_range() {
        // Confirms that duty values outside 0–100% are filtered to None.
        assert_eq!(valid_duty(-0.1), 0.);
        assert_eq!(valid_duty(100.1), 100.);
        assert_eq!(valid_duty(-50.0), 0.);
        assert_eq!(valid_duty(150.0), 100.);
    }

    #[test]
    fn valid_rpm_accepts_boundary_and_typical_values() {
        // Confirms that values within [0, 10_000] RPM pass through unchanged.
        assert_eq!(valid_rpm(0), 0);
        assert_eq!(valid_rpm(1200), 1200);
        assert_eq!(valid_rpm(10_000), 10_000);
    }

    #[test]
    fn valid_rpm_clamps_out_of_range() {
        // Confirms that RPM values outside [0, 10_000] are clamped to the boundary,
        // preventing runaway hardware readings from propagating.
        assert_eq!(valid_rpm(10_001), 10_000);
        assert_eq!(valid_rpm(u32::MAX), 10_000);
    }

    #[test]
    fn add_liquid_temp_filters_out_of_range() {
        // Confirms that hardware temperature values outside [-40, 200]°C are clamped.
        let device_support = KrakenX3Support::new();
        let given_expected = vec![
            (
                HashMap::from([("liquid temperature".to_string(), "999.0".to_string())]),
                vec![TempStatus {
                    name: "liquid".to_string(),
                    temp: 200.0,
                }],
            ),
            (
                HashMap::from([("liquid temperature".to_string(), "-100.0".to_string())]),
                vec![TempStatus {
                    name: "liquid".to_string(),
                    temp: -40.0,
                }],
            ),
        ];
        assert_temp_status_vector_contents_eq(&device_support, given_expected);
    }

    #[test]
    fn add_single_fan_status_filters_invalid_duty() {
        // Confirms that a duty value outside 0–100% is clamped while a valid RPM
        // value in the same reading is still included.
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 1000;
        let given_expected = vec![(
            HashMap::from([
                ("fan speed".to_string(), rpm.to_string()),
                ("fan duty".to_string(), "150.0".to_string()),
            ]),
            vec![ChannelStatus {
                name: "fan".to_string(),
                rpm: Some(rpm),
                duty: Some(100.0),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_single_pump_status_filters_invalid_duty() {
        // Confirms that an invalid duty value from hardware is clamped for pump channels.
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 2400;
        let given_expected = vec![(
            HashMap::from([
                ("pump speed".to_string(), rpm.to_string()),
                ("pump duty".to_string(), "-5.0".to_string()),
            ]),
            vec![ChannelStatus {
                name: "pump".to_string(),
                rpm: Some(rpm),
                duty: Some(0.0),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_multiple_fans_filters_invalid_duty() {
        // Confirms that an out-of-range duty in multi-fan status is clamped while
        // the corresponding RPM value is preserved.
        let device_support = KrakenX3Support::new();
        let device_id: u8 = 1;
        let rpm: u32 = 1200;
        let given_expected = vec![(
            HashMap::from([
                ("fan 1 speed".to_string(), rpm.to_string()),
                ("fan 1 duty".to_string(), "200.0".to_string()),
            ]),
            vec![ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(rpm),
                duty: Some(100.0),
                ..Default::default()
            }],
        )];
        assert_channel_statuses_eq(&device_support, device_id, given_expected);
    }

    #[test]
    fn add_software_temp_sensors() {
        // Confirms that "soft. sensor N" keys are parsed into "soft-sensorN" TempStatus
        // entries, extracting the trailing number as the sensor index.
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given_expected = vec![(
            HashMap::from([
                ("soft. sensor 1".to_string(), temp.clone()),
                ("soft. sensor 2".to_string(), temp.clone()),
                ("soft. sensor 3".to_string(), temp.clone()),
            ]),
            vec![
                TempStatus {
                    name: "soft-sensor1".to_string(),
                    temp: temp.parse().unwrap(),
                },
                TempStatus {
                    name: "soft-sensor2".to_string(),
                    temp: temp.parse().unwrap(),
                },
                TempStatus {
                    name: "soft-sensor3".to_string(),
                    temp: temp.parse().unwrap(),
                },
            ],
        )];
        for (given, expected) in given_expected {
            let mut result_temps = vec![];
            device_support.add_software_temp_sensors(&given, &mut result_temps);
            assert!(
                expected.iter().all(|ts| result_temps.contains(ts)),
                "result missing expected entries: {result_temps:?}"
            );
            assert!(
                result_temps.iter().all(|ts| expected.contains(ts)),
                "result contains unexpected entries: {result_temps:?}"
            );
        }
    }

    #[test]
    fn add_software_temp_sensors_ignores_non_matching_keys() {
        // Confirms that keys not matching "soft. sensor \d+" produce no output.
        let device_support = KrakenX3Support::new();
        let temp = "33.3".to_string();
        let given = HashMap::from([
            ("sensor 1".to_string(), temp.clone()),
            ("temperature 1".to_string(), temp.clone()),
            ("soft-sensor-1".to_string(), temp.clone()),
        ]);
        let mut result_temps = vec![];
        device_support.add_software_temp_sensors(&given, &mut result_temps);
        assert!(
            result_temps.is_empty(),
            "expected no results, got: {result_temps:?}"
        );
    }

    #[test]
    fn add_software_temp_sensors_filters_out_of_range() {
        // Confirms that temperature values outside [-40, 200]°C are clamped by valid_temp.
        let device_support = KrakenX3Support::new();
        let given = HashMap::from([
            ("soft. sensor 1".to_string(), "999.0".to_string()),
            ("soft. sensor 2".to_string(), "-100.0".to_string()),
        ]);
        let expected = [
            TempStatus {
                name: "soft-sensor1".to_string(),
                temp: 200.0,
            },
            TempStatus {
                name: "soft-sensor2".to_string(),
                temp: -40.0,
            },
        ];
        let mut result_temps = vec![];
        device_support.add_software_temp_sensors(&given, &mut result_temps);
        assert!(
            expected.iter().all(|ts| result_temps.contains(ts)),
            "result missing expected entries: {result_temps:?}"
        );
    }

    #[test]
    fn add_flow_sensor_status() {
        // Confirms that a "flow sensor" key is parsed into a "flow" ChannelStatus
        // with the value stored in rpm (used as a proxy for flow rate in L/h).
        let device_support = KrakenX3Support::new();
        let flow_speed: u32 = 250;
        let given = HashMap::from([("flow sensor".to_string(), flow_speed.to_string())]);
        let mut result_statuses = vec![];
        device_support.add_flow_sensor_status(&given, &mut result_statuses);
        assert_eq!(result_statuses.len(), 1);
        assert_eq!(result_statuses[0].name, "flow");
        assert_eq!(result_statuses[0].rpm, Some(flow_speed));
        assert_eq!(result_statuses[0].duty, None);
    }

    #[test]
    fn add_flow_sensor_status_absent_key_produces_no_output() {
        // Confirms that no ChannelStatus is pushed when "flow sensor" is absent.
        let device_support = KrakenX3Support::new();
        let given = HashMap::from([("fan speed".to_string(), "1200".to_string())]);
        let mut result_statuses = vec![];
        device_support.add_flow_sensor_status(&given, &mut result_statuses);
        assert!(
            result_statuses.is_empty(),
            "expected no results, got: {result_statuses:?}"
        );
    }

    #[test]
    fn add_flow_sensor_status_non_numeric_value_produces_no_output() {
        // Confirms that a non-parseable flow sensor value results in no status being pushed.
        let device_support = KrakenX3Support::new();
        let given = HashMap::from([("flow sensor".to_string(), "unknown".to_string())]);
        let mut result_statuses = vec![];
        device_support.add_flow_sensor_status(&given, &mut result_statuses);
        assert!(
            result_statuses.is_empty(),
            "expected no results, got: {result_statuses:?}"
        );
    }
}
