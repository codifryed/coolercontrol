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

use crate::device::{
    ChannelInfo, ChannelKind, ChannelStatus, DeviceInfo, DriverInfo, DriverType, LightingMode,
    SpeedOptions, TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{
    ColorMode, DeviceSupport, StatusMap,
};
use log::error;
use std::collections::HashMap;

const MIN_DUTY: u8 = 0;
const MAX_DUTY: u8 = 100;
const MAX_COLORS: u8 = 4;
const SPEED_CHANNEL_PUMP: &str = "pump";
const SPEED_CHANNEL_FAN: &str = "fan";
const LIQUIDCTL_PUMP_COLOR_CHANNEL: &str = "pump";
const LIQUIDCTL_FAN_COLOR_CHANNEL: &str = "fan";
const CC_PUMP_COLOR_CHANNEL: &str = "led-pump";
const CC_FAN_COLOR_CHANNEL: &str = "led-fan";

#[derive(Debug)]
pub struct Ga2LcdSupport;
// ga2_lcd.py

impl Ga2LcdSupport {
    pub fn new() -> Self {
        Self
    }

    fn pump_color_modes() -> Vec<ColorMode> {
        vec![
            ColorMode::new("rainbow", 0, 0, true, true),
            ColorMode::new("rainbow-morph", 0, 0, true, true),
            ColorMode::new("static", 1, MAX_COLORS, false, false),
            ColorMode::new("breathing", 1, MAX_COLORS, true, true),
            ColorMode::new("runway", 1, MAX_COLORS, true, true),
            ColorMode::new("meteor", 1, MAX_COLORS, true, true),
            ColorMode::new("ticker-tape", 1, MAX_COLORS, true, true),
            ColorMode::new("fluctuation", 1, MAX_COLORS, true, true),
            ColorMode::new("transmit", 1, MAX_COLORS, true, true),
            ColorMode::new("colorful-starry-night", 1, MAX_COLORS, true, true),
            ColorMode::new("static-starry-night", 1, MAX_COLORS, true, true),
            ColorMode::new("big-bang", 1, MAX_COLORS, true, true),
            ColorMode::new("burst", 1, MAX_COLORS, true, true),
            ColorMode::new("color-morph", 1, MAX_COLORS, true, true),
            ColorMode::new("bounce", 1, MAX_COLORS, true, true),
        ]
    }

    fn fan_color_modes() -> Vec<ColorMode> {
        vec![
            ColorMode::new("rainbow", 0, 0, true, true),
            ColorMode::new("rainbow-morph", 0, 0, true, true),
            ColorMode::new("static", 1, MAX_COLORS, false, false),
            ColorMode::new("breathing", 1, MAX_COLORS, true, true),
            ColorMode::new("runway", 1, MAX_COLORS, true, true),
            ColorMode::new("meteor", 1, MAX_COLORS, true, true),
        ]
    }
}

impl DeviceSupport for Ga2LcdSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::GA2LCD
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::with_capacity(4);
        for speed_channel in [SPEED_CHANNEL_PUMP, SPEED_CHANNEL_FAN] {
            channels.insert(
                speed_channel.to_string(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: MIN_DUTY,
                        max_duty: MAX_DUTY,
                        fixed_enabled: true,
                        extension: None,
                    }),
                },
            );
        }

        // CC requires that all channels use unique names per channel,
        // but this driver doesn't do that.
        // We map our custom channel names to liquidctl's "pump"/"fan" via
        // `liquidctl_color_channel_name()`.
        channels.insert(
            CC_PUMP_COLOR_CHANNEL.to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Lighting(
                    self.get_color_channel_modes(Some(LIQUIDCTL_PUMP_COLOR_CHANNEL)),
                ),
            },
        );
        channels.insert(
            CC_FAN_COLOR_CHANNEL.to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Lighting(
                    self.get_color_channel_modes(Some(LIQUIDCTL_FAN_COLOR_CHANNEL)),
                ),
            },
        );
        let lighting_speeds = vec![
            "slowest".to_string(),
            "slower".to_string(),
            "normal".to_string(),
            "faster".to_string(),
            "fastest".to_string(),
        ];
        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 0,
            temp_max: 60,
            driver_info: DriverInfo {
                drv_type: DriverType::Liquidctl,
                name: Some(self.supported_driver().to_string()),
                version: device_response.liquidctl_version.clone(),
                locations: self.collect_driver_locations(device_response),
            },
            ..Default::default()
        }
    }

    fn get_color_channel_modes(&self, channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes = match channel_name {
            Some(LIQUIDCTL_PUMP_COLOR_CHANNEL) => Self::pump_color_modes(),
            Some(LIQUIDCTL_FAN_COLOR_CHANNEL) => Self::fan_color_modes(),
            _ => {
                error!("Unknown lighting channel name {channel_name:?}");
                Vec::with_capacity(0)
            }
        };
        self.convert_to_channel_lighting_modes(color_modes)
    }

    fn get_temperatures(&self, status_map: &StatusMap) -> Vec<TempStatus> {
        let mut temps = Vec::with_capacity(1);
        self.add_liquid_temp(status_map, &mut temps);
        temps
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = Vec::with_capacity(2);
        self.add_single_fan_status(status_map, &mut channel_statuses);
        self.add_single_pump_status(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }

    /// This device driver does not have unique channel names, so we have to map them on the fly.
    fn liquidctl_color_channel_name<'a>(&self, channel_name: &'a str) -> &'a str {
        match channel_name {
            CC_PUMP_COLOR_CHANNEL => LIQUIDCTL_PUMP_COLOR_CHANNEL,
            CC_FAN_COLOR_CHANNEL => LIQUIDCTL_FAN_COLOR_CHANNEL,
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Not;

    use super::*;
    use crate::repositories::liquidctl::liqctld_client::{DeviceProperties, DeviceResponse};

    fn create_test_device_response() -> DeviceResponse {
        DeviceResponse {
            id: 1,
            description: "Lian Li GA II LCD".to_string(),
            device_type: "GA2LCD".to_string(),
            serial_number: Some("1234567890".to_string()),
            properties: DeviceProperties {
                speed_channels: vec!["fan".to_string(), "pump".to_string()],
                color_channels: Vec::new(),
                supports_cooling: Some(true),
                supports_cooling_profiles: None,
                supports_lighting: None,
                led_count: None,
                lcd_resolution: None,
            },
            liquidctl_version: Some("1.16.0".to_string()),
            hid_address: Some("/dev/hidraw0".to_string()),
            hwmon_address: None,
        }
    }

    fn create_test_status_map() -> StatusMap {
        let mut status_map = StatusMap::new();
        status_map.insert("liquid temperature".to_string(), "30.3".to_string());
        status_map.insert("fan speed".to_string(), "1560".to_string());
        status_map.insert("fan duty".to_string(), "62.0".to_string());
        status_map.insert("pump speed".to_string(), "3000".to_string());
        status_map.insert("pump duty".to_string(), "83.0".to_string());
        status_map
    }

    #[test]
    fn supported_driver_returns_ga2lcd() {
        let support = Ga2LcdSupport::new();
        assert_eq!(support.supported_driver(), BaseDriver::GA2LCD);
    }

    #[test]
    fn extract_info_has_fan_and_pump_speed_channels() {
        let support = Ga2LcdSupport::new();
        let response = create_test_device_response();
        let info = support.extract_info(&response);

        let fan_channel = info.channels.get("fan").expect("fan channel should exist");
        let fan_speed = fan_channel
            .speed_options()
            .expect("fan should have speed options");
        assert_eq!(fan_speed.min_duty, 0);
        assert_eq!(fan_speed.max_duty, 100);
        assert!(fan_speed.fixed_enabled);
        assert!(fan_speed.extension.is_none());

        let pump_channel = info
            .channels
            .get("pump")
            .expect("pump channel should exist");
        let pump_speed = pump_channel
            .speed_options()
            .expect("pump should have speed options");
        assert_eq!(pump_speed.min_duty, 0);
        assert_eq!(pump_speed.max_duty, 100);
        assert!(pump_speed.fixed_enabled);
        assert!(pump_speed.extension.is_none());
    }

    #[test]
    fn extract_info_has_separate_lighting_channels() {
        // Lighting channels use distinct names from speed channels.
        let support = Ga2LcdSupport::new();
        let response = create_test_device_response();
        let info = support.extract_info(&response);

        let pump_led = info
            .channels
            .get("led-pump")
            .expect("LED-pump channel should exist");
        assert!(pump_led.speed_options().is_none());
        assert!(pump_led.lighting_modes().is_empty().not());

        let fan_led = info
            .channels
            .get("led-fan")
            .expect("LED-fan channel should exist");
        assert!(fan_led.speed_options().is_none());
        assert!(fan_led.lighting_modes().is_empty().not());
        // Fan has fewer lighting modes than pump.
        assert!(fan_led.lighting_modes().len() < pump_led.lighting_modes().len());

        // Speed channels must not have lighting modes.
        let pump_speed = info
            .channels
            .get("pump")
            .expect("pump channel should exist");
        assert!(pump_speed.lighting_modes().is_empty());
        let fan_speed = info.channels.get("fan").expect("fan channel should exist");
        assert!(fan_speed.lighting_modes().is_empty());
    }

    #[test]
    fn liquidctl_color_channel_name_maps_correctly() {
        let support = Ga2LcdSupport::new();
        assert_eq!(support.liquidctl_color_channel_name("led-pump"), "pump");
        assert_eq!(support.liquidctl_color_channel_name("led-fan"), "fan");
        assert_eq!(support.liquidctl_color_channel_name("other"), "other");
    }

    #[test]
    fn extract_info_has_lighting_speeds() {
        let support = Ga2LcdSupport::new();
        let response = create_test_device_response();
        let info = support.extract_info(&response);

        assert_eq!(info.lighting_speeds.len(), 5);
        assert_eq!(info.lighting_speeds[0], "slowest");
        assert_eq!(info.lighting_speeds[4], "fastest");
    }

    #[test]
    fn extract_info_has_driver_info() {
        let support = Ga2LcdSupport::new();
        let response = create_test_device_response();
        let info = support.extract_info(&response);

        assert_eq!(info.driver_info.drv_type, DriverType::Liquidctl);
        assert_eq!(info.driver_info.name, Some("GA2LCD".to_string()));
        assert_eq!(info.driver_info.version, Some("1.16.0".to_string()));
        assert_eq!(info.driver_info.locations.len(), 1);
    }

    #[test]
    fn extract_info_has_temp_range() {
        let support = Ga2LcdSupport::new();
        let response = create_test_device_response();
        let info = support.extract_info(&response);

        assert_eq!(info.temp_min, 0);
        assert_eq!(info.temp_max, 60);
    }

    #[test]
    fn get_temperatures_extracts_liquid_temp() {
        let support = Ga2LcdSupport::new();
        let status_map = create_test_status_map();
        let temps = support.get_temperatures(&status_map);

        assert_eq!(temps.len(), 1);
        assert_eq!(temps[0].name, "liquid");
        assert!((temps[0].temp - 30.3).abs() < f64::EPSILON);
    }

    #[test]
    fn get_temperatures_returns_empty_when_no_temp() {
        let support = Ga2LcdSupport::new();
        let status_map = StatusMap::new();
        let temps = support.get_temperatures(&status_map);

        assert!(temps.is_empty());
    }

    #[test]
    fn get_channel_statuses_extracts_fan_and_pump() {
        let support = Ga2LcdSupport::new();
        let status_map = create_test_status_map();
        let statuses = support.get_channel_statuses(&status_map, 0);

        assert_eq!(statuses.len(), 2);
        // Sorted by name: fan comes before pump.
        assert_eq!(statuses[0].name, "fan");
        assert_eq!(statuses[0].rpm, Some(1560));
        assert!((statuses[0].duty.unwrap() - 62.0).abs() < f64::EPSILON);

        assert_eq!(statuses[1].name, "pump");
        assert_eq!(statuses[1].rpm, Some(3000));
        assert!((statuses[1].duty.unwrap() - 83.0).abs() < f64::EPSILON);
    }

    #[test]
    fn get_channel_statuses_returns_empty_when_no_data() {
        let support = Ga2LcdSupport::new();
        let status_map = StatusMap::new();
        let statuses = support.get_channel_statuses(&status_map, 0);

        assert!(statuses.is_empty());
    }

    #[test]
    fn pump_color_modes_count() {
        // Pump has 15 lighting modes matching the liquidctl driver.
        let modes = Ga2LcdSupport::pump_color_modes();
        assert_eq!(modes.len(), 15);
    }

    #[test]
    fn fan_color_modes_count() {
        // Fan has 6 lighting modes matching the liquidctl driver.
        let modes = Ga2LcdSupport::fan_color_modes();
        assert_eq!(modes.len(), 6);
    }

    #[test]
    fn channel_mode_selection_by_name() {
        // Verify that get_color_channel_modes dispatches correctly.
        let support = Ga2LcdSupport::new();
        let pump_modes = support.get_color_channel_modes(Some("pump"));
        let fan_modes = support.get_color_channel_modes(Some("fan"));
        let default_modes = support.get_color_channel_modes(None);

        assert!(fan_modes.len() < pump_modes.len());
        // Default (None) returns pump modes.
        assert_eq!(default_modes.len(), 0);
    }
}
