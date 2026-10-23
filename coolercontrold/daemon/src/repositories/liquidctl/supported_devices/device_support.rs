// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use heck::ToTitleCase;
use log::info;
use regex::Regex;
use std::collections::HashMap;
use std::fmt::Debug;
use std::ops::Not;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::LazyLock;

use crate::device::{
    ChannelInfo, ChannelKind, ChannelSample, ChannelStatus, DeviceInfo, LightingMode,
    LightingModeType, Status, StatusRefill, TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;

pub type StatusMap = HashMap<String, String>;

/// The channel a flow sensor's reading is reported under.
pub const FLOW_CHANNEL_NAME: &str = "flow";
/// liquidctl reports flow in dL/h, and the status carries it as rpm. The bracketed unit is
/// what tells the UI the value is not a speed.
pub const FLOW_CHANNEL_LABEL: &str = "Flow [dL/h]";

/// The highest fan number a status line may carry. liquidctl devices number a handful of fans;
/// a line past this is logged and left out rather than grown into.
const FAN_NUMBER_MAX: usize = 32;
static UNEXPECTED_FAN_NUMBER_LOGGED: AtomicBool = AtomicBool::new(false);

/// The slot for fan `fan_number` among a status's numbered fans, or `None`, logged once, when
/// the number is past `FAN_NUMBER_MAX`.
fn fan_slot<'a, T>(
    fans: &'a mut [Option<T>; FAN_NUMBER_MAX + 1],
    fan_number: u32,
    line: &str,
) -> Option<&'a mut Option<T>> {
    let slot = usize::try_from(fan_number)
        .ok()
        .and_then(|number| fans.get_mut(number));
    if slot.is_none() {
        if UNEXPECTED_FAN_NUMBER_LOGGED
            .swap(true, Ordering::Relaxed)
            .not()
        {
            info!("Ignoring liquidctl status line for an unexpected fan number: {line}");
        }
    }
    slot
}

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

    /// Completes the info with what a device only reveals in its status. Called once, with the
    /// device's first status. Override it for a driver with channels liquidctl does not
    /// announce up front.
    fn extend_info_from_status(&self, _status: &Status, _device_info: &mut DeviceInfo) {}

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
        let mut status = Status::default();
        self.refill_status(
            status_map,
            device_index,
            &mut status.channels,
            &mut status.temps,
        );
        status
    }

    /// Refills a device's status from its status map in place, leaving it as `extract_status`
    /// would build it.
    fn refill_status(
        &self,
        status_map: &StatusMap,
        device_index: u8,
        channels: &mut Vec<ChannelStatus>,
        temps: &mut Vec<TempStatus>,
    ) {
        let mut refill = StatusRefill::new(channels, temps);
        self.refill_temperatures(status_map, &mut refill);
        self.refill_channel_statuses(status_map, device_index, &mut refill);
    }

    #[cfg(test)]
    fn get_temperatures(&self, status_map: &StatusMap) -> Vec<TempStatus> {
        self.extract_status(status_map, 0).temps
    }

    #[cfg(test)]
    fn get_channel_statuses(&self, status_map: &StatusMap, device_index: u8) -> Vec<ChannelStatus> {
        let mut channels = Vec::new();
        let mut temps = Vec::new();
        let mut refill = StatusRefill::new(&mut channels, &mut temps);
        self.refill_channel_statuses(status_map, device_index, &mut refill);
        drop(refill);
        channels
    }

    /// It's possible to override this method and use only the needed sub-functions per device
    fn refill_temperatures(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        self.add_liquid_temp(status_map, refill);
        self.add_water_temp(status_map, refill);
        self.add_temp(status_map, refill);
        self.add_temp_probes(status_map, refill);
        self.add_vrm_temp(status_map, refill);
        self.add_case_temp(status_map, refill);
        self.add_temp_sensors(status_map, refill);
        // self.add_software_temp_sensors(status_map, refill);
        // ...for a future feature (needs testing and is in dB)
        // self.add_noise_level(status_map, &mut refill, device_index);
        refill
            .temps_mut()
            .sort_unstable_by(|a, b| a.name.cmp(&b.name));
    }

    fn add_liquid_temp(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let liquid_temp = status_map
            .get("liquid temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = liquid_temp {
            refill.temp("liquid", temp);
        }
    }

    fn add_water_temp(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let water_temp = status_map
            .get("water temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = water_temp {
            refill.temp("water", temp);
        }
    }

    fn add_temp(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let plain_temp = status_map
            .get("temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = plain_temp {
            refill.temp("temp", temp);
        }
    }

    fn add_temp_probes(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        static TEMP_PROBE_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"temperature \d+").unwrap());
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        for (probe_name, value) in status_map {
            if TEMP_PROBE_PATTERN.is_match(probe_name) {
                if let Some(temp) = parse_float_inner(value).map(valid_temp) {
                    if let Some(probe_number) =
                        NUMBER_PATTERN.find_at(probe_name, probe_name.len() - 2)
                    {
                        refill.temp_fmt(format_args!("temp{}", probe_number.as_str()), temp);
                    }
                }
            }
        }
    }

    /// Voltage Regulator temp for PSUs
    fn add_vrm_temp(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let vrm_temp = status_map
            .get("vrm temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = vrm_temp {
            refill.temp("vrm", temp);
        }
    }

    fn add_case_temp(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let case_temp = status_map
            .get("case temperature")
            .and_then(parse_float)
            .map(valid_temp);
        if let Some(temp) = case_temp {
            refill.temp("case", temp);
        }
    }

    fn add_temp_sensors(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        static TEMP_SENSOR_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"sensor \d+").unwrap());
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        for (sensor_name, value) in status_map {
            if TEMP_SENSOR_PATTERN.is_match(sensor_name) {
                if let Some(temp) = parse_float_inner(value).map(valid_temp) {
                    if let Some(sensor_number) =
                        NUMBER_PATTERN.find_at(sensor_name, sensor_name.len() - 2)
                    {
                        refill.temp_fmt(format_args!("sensor{}", sensor_number.as_str()), temp);
                    }
                }
            }
        }
    }

    fn add_software_temp_sensors(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        static SOFT_TEMP_SENSOR_PATTERN: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"soft. sensor \d+").unwrap());
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        for (sensor_name, value) in status_map {
            if SOFT_TEMP_SENSOR_PATTERN.is_match(sensor_name) {
                if let Some(temp) = parse_float_inner(value).map(valid_temp) {
                    if let Some(sensor_number) =
                        NUMBER_PATTERN.find_at(sensor_name, sensor_name.len() - 2)
                    {
                        let name = format_args!("soft-sensor{}", sensor_number.as_str());
                        refill.temp_fmt(name, temp);
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn add_noise_level(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let noise_lvl = status_map.get("noise level").and_then(parse_float);
        if let Some(noise) = noise_lvl {
            refill.temp("noise", noise);
        }
    }

    /// It's possible to override this method and use only the needed sub-functions per device
    fn refill_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
        refill: &mut StatusRefill<'_>,
    ) {
        self.add_single_fan_status(status_map, refill);
        self.add_single_external_fans_status(status_map, refill);
        self.add_single_pump_status(status_map, refill);
        self.add_single_pump_fan_status(status_map, refill);
        self.add_multiple_fans_status(status_map, refill);
        self.add_multiple_external_fans_status(status_map, refill);
        self.add_single_water_block_status(status_map, refill);
        // only applicable for a single device so far:
        // self.add_psu_power_statuses(status_map, refill);
        refill
            .channels_mut()
            .sort_unstable_by(|a, b| a.name.cmp(&b.name));
    }

    fn add_single_fan_status(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let fan_rpm = status_map
            .get("fan speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let fan_duty = status_map
            .get("fan duty")
            .and_then(parse_float)
            .map(valid_duty);
        if fan_rpm.is_some() || fan_duty.is_some() {
            refill.channel(
                "fan",
                ChannelSample {
                    rpm: fan_rpm,
                    duty: fan_duty,
                    ..Default::default()
                },
            );
        }
    }

    fn add_single_external_fans_status(
        &self,
        status_map: &StatusMap,
        refill: &mut StatusRefill<'_>,
    ) {
        // asus_ryujin: where the status comes in singular, but the control channel is plural
        let fan_duty = status_map
            .get("external fan duty")
            .and_then(parse_float)
            .map(valid_duty);
        if fan_duty.is_some() {
            refill.channel(
                "external-fans",
                ChannelSample {
                    duty: fan_duty,
                    ..Default::default()
                },
            );
        }
    }

    fn add_single_pump_status(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let pump_rpm = status_map
            .get("pump speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let pump_duty = status_map
            .get("pump duty")
            .and_then(parse_float)
            .map(valid_duty);
        if pump_rpm.is_some() || pump_duty.is_some() {
            refill.channel(
                "pump",
                ChannelSample {
                    rpm: pump_rpm,
                    duty: pump_duty,
                    ..Default::default()
                },
            );
        }
    }

    fn add_single_pump_fan_status(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let pump_rpm = status_map
            .get("pump fan speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let pump_duty = status_map
            .get("pump fan duty")
            .and_then(parse_float)
            .map(valid_duty);
        if pump_rpm.is_some() || pump_duty.is_some() {
            refill.channel(
                "pump-fan",
                ChannelSample {
                    rpm: pump_rpm,
                    duty: pump_duty,
                    ..Default::default()
                },
            );
        }
    }

    fn add_single_water_block_status(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        let water_block_rpm = status_map
            .get("water block speed")
            .and_then(parse_u32)
            .map(valid_rpm);
        let water_block_duty = status_map
            .get("water block duty")
            .and_then(parse_float)
            .map(valid_duty);
        if water_block_rpm.is_some() || water_block_duty.is_some() {
            refill.channel(
                "waterblock-fan",
                ChannelSample {
                    rpm: water_block_rpm,
                    duty: water_block_duty,
                    ..Default::default()
                },
            );
        }
    }

    fn add_psu_power_statuses(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        if let Some(watts) = status_map.get("total power output").and_then(parse_float) {
            refill.channel(
                "total-power",
                ChannelSample {
                    watts: Some(watts),
                    ..Default::default()
                },
            );
        }
        if let Some(watts) = status_map
            .get("estimated input power")
            .and_then(parse_float)
        {
            refill.channel(
                "estimated-input-power",
                ChannelSample {
                    watts: Some(watts),
                    ..Default::default()
                },
            );
        }
        if let Some(watts) = status_map.get("+12v output power").and_then(parse_float) {
            refill.channel(
                "12v-power",
                ChannelSample {
                    watts: Some(watts),
                    ..Default::default()
                },
            );
        }
        if let Some(watts) = status_map.get("+5v output power").and_then(parse_float) {
            refill.channel(
                "5v-power",
                ChannelSample {
                    watts: Some(watts),
                    ..Default::default()
                },
            );
        }
        if let Some(watts) = status_map.get("+3.3v output power").and_then(parse_float) {
            refill.channel(
                "3.3v-power",
                ChannelSample {
                    watts: Some(watts),
                    ..Default::default()
                },
            );
        }
    }

    #[allow(dead_code)]
    /// This is used for special devices with limited pump speeds that are named (str)
    fn get_pump_mode(&self, status_map: &StatusMap) -> Option<String> {
        status_map.get("pump mode").cloned()
    }

    fn add_multiple_fans_status(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        static MULTIPLE_FAN_SPEED: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^fan \d+ speed").unwrap());
        static MULTIPLE_FAN_SPEED_CORSAIR: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^fan speed \d+").unwrap());
        static MULTIPLE_FAN_DUTY: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"^fan \d+ duty").unwrap());
        let mut fans = [None; FAN_NUMBER_MAX + 1];
        for (name, value) in status_map {
            let Some(fan_number) = NUMBER_PATTERN
                .find_at(name, 3)
                .and_then(|number| parse_u32_inner(number.as_str()))
            else {
                continue;
            };
            let is_speed =
                MULTIPLE_FAN_SPEED.is_match(name) || MULTIPLE_FAN_SPEED_CORSAIR.is_match(name);
            let is_duty = is_speed.not() && MULTIPLE_FAN_DUTY.is_match(name);
            let is_fan_line = is_speed || is_duty;
            if is_fan_line.not() {
                continue;
            }
            let Some(fan) = fan_slot(&mut fans, fan_number, name) else {
                continue;
            };
            let (rpm, duty) = fan.get_or_insert((None, None));
            if is_speed {
                *rpm = parse_u32_inner(value).map(valid_rpm);
            } else {
                *duty = parse_float_inner(value).map(valid_duty);
            }
        }
        for (fan_number, fan) in fans.iter().enumerate() {
            if let Some((rpm, duty)) = *fan {
                let sample = ChannelSample {
                    rpm,
                    duty,
                    ..Default::default()
                };
                refill.channel_fmt(format_args!("fan{fan_number}"), sample);
            }
        }
    }

    /// Reports each of the device's init speed channels missing from this status as stopped,
    /// for drivers that drop a fan set to 0 from their status, as they do one with no fan.
    fn add_missing_speed_channels(
        &self,
        speed_channel_names: &[String],
        refill: &mut StatusRefill<'_>,
    ) {
        if refill.channels().len() < speed_channel_names.len() {
            for channel_name in speed_channel_names {
                let reported = refill
                    .channels()
                    .iter()
                    .any(|status| status.name == *channel_name);
                if reported.not() {
                    let sample = ChannelSample {
                        rpm: Some(0),
                        duty: Some(0.0),
                        ..Default::default()
                    };
                    refill.channel(channel_name, sample);
                }
            }
        }
    }

    fn add_multiple_external_fans_status(
        &self,
        status_map: &StatusMap,
        refill: &mut StatusRefill<'_>,
    ) {
        static NUMBER_PATTERN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
        static MULTIPLE_FAN_SPEED: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"external fan \d+ speed").unwrap());
        let mut fans = [None; FAN_NUMBER_MAX + 1];
        for (name, value) in status_map {
            if MULTIPLE_FAN_SPEED.is_match(name).not() {
                continue;
            }
            let Some(fan_number) = NUMBER_PATTERN
                .find_at(name, 12)
                .and_then(|number| parse_u32_inner(number.as_str()))
            else {
                continue;
            };
            if let Some(fan) = fan_slot(&mut fans, fan_number, name) {
                *fan = Some(parse_u32_inner(value).map(valid_rpm));
            }
        }
        for (fan_number, fan) in fans.iter().enumerate() {
            if let Some(rpm) = *fan {
                let sample = ChannelSample {
                    rpm,
                    ..Default::default()
                };
                refill.channel_fmt(format_args!("external-fan{fan_number}"), sample);
            }
        }
    }

    fn add_flow_sensor_status(&self, status_map: &StatusMap, refill: &mut StatusRefill<'_>) {
        // We use rpm speed as a proxy for flow speed
        let flow_speed = status_map
            .get("flow sensor")
            .and_then(parse_u32)
            .map(valid_rpm);
        if flow_speed.is_some() {
            refill.channel(
                FLOW_CHANNEL_NAME,
                ChannelSample {
                    rpm: flow_speed,
                    ..Default::default()
                },
            );
        }
    }

    /// Lists the flow sensor a status reports. Without it the reading has no label, and it is
    /// the label that says the value is a flow and not a speed.
    fn add_flow_sensor_info(&self, status: &Status, device_info: &mut DeviceInfo) {
        let reports_flow = status
            .channels
            .iter()
            .any(|channel| channel.name == FLOW_CHANNEL_NAME);
        if reports_flow.not() {
            return;
        }
        device_info
            .channels
            .entry(FLOW_CHANNEL_NAME.to_string())
            .or_insert_with(|| ChannelInfo {
                label: Some(FLOW_CHANNEL_LABEL.to_string()),
                kind: ChannelKind::InfoOnly,
            });
    }

    /// Maps a CC lighting channel name to the `liquidctl` color channel name.
    /// Override for devices where CC channel names differ from `liquidctl` channel names.
    fn liquidctl_color_channel_name<'a>(&self, channel_name: &'a str) -> &'a str {
        channel_name
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
    use crate::repositories::hwmon::fans;
    use crate::repositories::liquidctl::supported_devices::kraken_x3::KrakenX3Support;

    use super::*;

    /// What `add` writes, refilled into empty lists.
    fn refilled(add: impl FnOnce(&mut StatusRefill<'_>)) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut channels = Vec::new();
        let mut temps = Vec::new();
        add(&mut StatusRefill::new(&mut channels, &mut temps));
        (channels, temps)
    }

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
            let (_, result_temps) =
                refilled(|refill| device_support.add_noise_level(&given, refill));
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
            let (_, result_temps) =
                refilled(|refill| device_support.add_software_temp_sensors(&given, refill));
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
        let (_, result_temps) =
            refilled(|refill| device_support.add_software_temp_sensors(&given, refill));
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
        let (_, result_temps) =
            refilled(|refill| device_support.add_software_temp_sensors(&given, refill));
        assert!(
            expected.iter().all(|ts| result_temps.contains(ts)),
            "result missing expected entries: {result_temps:?}"
        );
    }

    #[test]
    fn add_flow_sensor_status() {
        // Confirms that a "flow sensor" key is parsed into a "flow" ChannelStatus
        // with the value stored in rpm (used as a proxy for flow rate in dL/h).
        let device_support = KrakenX3Support::new();
        let flow_speed: u32 = 250;
        let given = HashMap::from([("flow sensor".to_string(), flow_speed.to_string())]);
        let (result_statuses, _) =
            refilled(|refill| device_support.add_flow_sensor_status(&given, refill));
        assert_eq!(result_statuses.len(), 1);
        assert_eq!(result_statuses[0].name, "flow");
        assert_eq!(result_statuses[0].rpm, Some(flow_speed));
        assert_eq!(result_statuses[0].duty, None);
    }

    #[test]
    fn flow_channel_label_names_its_unit() {
        // The UI shows the unit a label names in its trailing brackets. Checks the flow label
        // against the rule the hwmon repo applies to driver labels, so the two cannot drift.
        assert_eq!(fans::label_unit(FLOW_CHANNEL_LABEL), Some("dL/h"));
    }

    #[test]
    fn add_flow_sensor_status_absent_key_produces_no_output() {
        // Confirms that no ChannelStatus is pushed when "flow sensor" is absent.
        let device_support = KrakenX3Support::new();
        let given = HashMap::from([("fan speed".to_string(), "1200".to_string())]);
        let (result_statuses, _) =
            refilled(|refill| device_support.add_flow_sensor_status(&given, refill));
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
        let (result_statuses, _) =
            refilled(|refill| device_support.add_flow_sensor_status(&given, refill));
        assert!(
            result_statuses.is_empty(),
            "expected no results, got: {result_statuses:?}"
        );
    }

    fn status_with_channels(channel_names: &[&str]) -> Status {
        Status {
            channels: channel_names
                .iter()
                .map(|channel_name| ChannelStatus {
                    name: (*channel_name).to_string(),
                    rpm: Some(120),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn info_with_fan1() -> DeviceInfo {
        DeviceInfo {
            channels: HashMap::from([(
                "fan1".to_string(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::InfoOnly,
                },
            )]),
            ..Default::default()
        }
    }

    #[test]
    fn add_flow_sensor_info_lists_a_reported_flow_sensor() {
        // A flow reading must come with a label naming its unit, on a channel that offers
        // no control.
        let device_support = KrakenX3Support::new();
        let mut device_info = info_with_fan1();
        device_support
            .add_flow_sensor_info(&status_with_channels(&["fan1", "flow"]), &mut device_info);
        assert_eq!(device_info.channels.len(), 2);
        let flow = &device_info.channels[FLOW_CHANNEL_NAME];
        assert_eq!(flow.label.as_deref(), Some(FLOW_CHANNEL_LABEL));
        assert!(matches!(flow.kind, ChannelKind::InfoOnly));
    }

    #[test]
    fn add_flow_sensor_info_lists_nothing_without_a_flow_reading() {
        // Only the status can say a device has a flow sensor, so a device that reports none
        // must not gain a channel that never has a value.
        let device_support = KrakenX3Support::new();
        let mut device_info = info_with_fan1();
        device_support
            .add_flow_sensor_info(&status_with_channels(&["fan1", "pump"]), &mut device_info);
        device_support.add_flow_sensor_info(&Status::default(), &mut device_info);
        assert_eq!(device_info.channels.len(), 1);
        assert!(device_info.channels.contains_key(FLOW_CHANNEL_NAME).not());
    }

    #[test]
    fn add_flow_sensor_info_keeps_a_channel_already_listed() {
        // A driver that describes its own flow channel knows it better than the shared label.
        let device_support = KrakenX3Support::new();
        let mut device_info = info_with_fan1();
        device_info.channels.insert(
            FLOW_CHANNEL_NAME.to_string(),
            ChannelInfo {
                label: Some("Coolant flow [L/h]".to_string()),
                kind: ChannelKind::InfoOnly,
            },
        );
        device_support.add_flow_sensor_info(&status_with_channels(&["flow"]), &mut device_info);
        assert_eq!(
            device_info.channels[FLOW_CHANNEL_NAME].label.as_deref(),
            Some("Coolant flow [L/h]")
        );
    }

    #[test]
    fn extend_info_from_status_adds_nothing_by_default() {
        // A driver opts in by overriding it, so one that does not must keep its info as
        // announced, whatever its status carries.
        let device_support = KrakenX3Support::new();
        let mut device_info = info_with_fan1();
        device_support
            .extend_info_from_status(&status_with_channels(&["fan1", "flow"]), &mut device_info);
        assert_eq!(device_info.channels.len(), 1);
        assert!(device_info.channels.contains_key("fan1"));
    }

    fn map_of(entries: &[(&str, &str)]) -> StatusMap {
        entries
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// Goal: a status refilled in place each tick holds what a fresh extract from the same map
    /// would, in the same order, through channels and temps that come and go. Method: refill one
    /// pair through three maps and compare each with `extract_status`.
    #[test]
    fn a_refilled_status_matches_a_fresh_extract() {
        let device_support = KrakenX3Support::new();
        let maps = [
            map_of(&[
                ("liquid temperature", "31.5"),
                ("temperature 1", "40"),
                ("pump speed", "2000"),
                ("fan 1 speed", "800"),
                ("fan 2 duty", "40"),
            ]),
            map_of(&[("liquid temperature", "31.25"), ("fan 2 speed", "790")]),
            map_of(&[
                ("temperature 2", "41"),
                ("fan speed 3", "700"),
                ("fan 1 speed", "810"),
                ("pump duty", "60"),
            ]),
        ];
        let mut channels = Vec::new();
        let mut temps = Vec::new();

        for status_map in maps {
            device_support.refill_status(&status_map, 1, &mut channels, &mut temps);

            let fresh = device_support.extract_status(&status_map, 1);
            assert_eq!(channels, fresh.channels);
            assert_eq!(temps, fresh.temps);
        }
    }

    /// Goal: once warm, refilling a device's status from a map with the same names allocates
    /// nothing, though the helpers report in an order the sort then changes. Method: refill a
    /// few ticks from one map, letting each kept name settle where the sort leaves it, then
    /// count one more.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_warm_refill_allocates_nothing() {
        let device_support = KrakenX3Support::new();
        let status_map = map_of(&[
            ("liquid temperature", "31.5"),
            ("temperature 1", "40"),
            ("pump speed", "2000"),
            ("pump duty", "60"),
            ("fan 1 speed", "800"),
            ("fan 2 speed", "790"),
            ("fan speed", "900"),
        ]);
        let mut channels = Vec::new();
        let mut temps = Vec::new();
        for _ in 0..4 {
            device_support.refill_status(&status_map, 1, &mut channels, &mut temps);
        }

        let ((), allocations) = crate::alloc_count::count_allocations(|| {
            device_support.refill_status(&status_map, 1, &mut channels, &mut temps);
        });

        assert_eq!(allocations, 0);
        assert_eq!(channels.len(), 4);
        assert_eq!(temps.len(), 2);
    }

    /// Goal: numbered fans up to the bound are reported, and a line numbering one past it is
    /// left out rather than indexing past the slots. Method: the highest allowed fan number and
    /// the one after it, as speed lines.
    #[test]
    fn a_fan_number_past_the_bound_is_left_out() {
        let device_support = KrakenX3Support::new();
        let highest = FAN_NUMBER_MAX.to_string();
        let past = (FAN_NUMBER_MAX + 1).to_string();
        let status_map = map_of(&[
            (&format!("fan {highest} speed"), "800"),
            (&format!("fan {past} speed"), "900"),
            (&format!("external fan {past} speed"), "900"),
        ]);

        let channels = device_support.get_channel_statuses(&status_map, 1);

        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].name, format!("fan{highest}"));
        assert_eq!(channels[0].rpm, Some(800));
    }
}
