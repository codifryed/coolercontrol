// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LightingMode, StatusRefill,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{
    ColorMode, DeviceSupport, StatusMap,
};

#[derive(Debug)]
pub struct AuraLedSupport;
// aura_led.py

impl AuraLedSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AuraLedSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::AuraLed
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        let color_channels = vec![
            "led1".to_string(),
            "led2".to_string(),
            "led3".to_string(),
            "led4".to_string(),
            "sync".to_string(),
        ];
        for channel_name in color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name,
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Lighting(lighting_modes),
                },
            );
        }
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
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
            ColorMode::new("static", 1, 1, false, false),
            ColorMode::new("breathing", 1, 1, false, false),
            ColorMode::new("flashing", 1, 1, false, false),
            ColorMode::new("spectrum_cycle", 0, 0, false, false),
            ColorMode::new("rainbow", 0, 0, false, false),
            ColorMode::new("spectrum_cycle_breathing", 0, 0, false, false),
            ColorMode::new("chase_fade", 1, 1, false, false),
            ColorMode::new("spectrum_cycle_chase_fade", 0, 0, false, false),
            ColorMode::new("chase", 1, 1, false, false),
            ColorMode::new("spectrum_cycle_chase", 0, 0, false, false),
            ColorMode::new("spectrum_cycle_wave", 0, 0, false, false),
            ColorMode::new("chase_rainbow_pulse", 0, 0, false, false),
            ColorMode::new("rainbow_flicker", 0, 0, false, false),
            ColorMode::new("gentle_transition", 0, 0, false, false),
            ColorMode::new("wave_propagation", 0, 0, false, false),
            ColorMode::new("wave_propagation_pause", 0, 0, false, false),
            ColorMode::new("red_pulse", 0, 0, false, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }

    fn refill_channel_statuses(
        &self,
        _status_map: &StatusMap,
        _device_index: u8,
        _refill: &mut StatusRefill<'_>,
    ) {
        // no standard statuses for AuraLed (RGB only)
    }
}
