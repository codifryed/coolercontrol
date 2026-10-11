// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelKind, ChannelStatus, DeviceInfo, DriverInfo, DriverType, LightingMode,
    SpeedOptions, Status, TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};

#[derive(Debug)]
pub struct AquaComputerSupport;
// aquacomputer.py

impl AquaComputerSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AquaComputerSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::Aquacomputer
    }

    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::with_capacity(device_response.properties.speed_channels.len());
        for channel_name in &device_response.properties.speed_channels {
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: 0,
                        max_duty: 100,
                        fixed_enabled: true,
                        extension: None,
                    }),
                },
            );
        }
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            temp_min: 0,
            temp_max: 100,
            driver_info: DriverInfo {
                drv_type: DriverType::Liquidctl,
                name: Some(self.supported_driver().to_string()),
                version: device_response.liquidctl_version.clone(),
                locations: self.collect_driver_locations(device_response),
            },
            ..Default::default()
        }
    }

    fn extend_info_from_status(&self, status: &Status, device_info: &mut DeviceInfo) {
        // Quadro
        self.add_flow_sensor_info(status, device_info);
    }

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        Vec::new()
    }

    fn get_temperatures(&self, status_map: &StatusMap) -> Vec<TempStatus> {
        let mut temps = Vec::with_capacity(status_map.len());
        // D5
        self.add_liquid_temp(status_map, &mut temps);
        // Farbwerk, Farbwerk 360, Octo, Quadro
        self.add_temp_sensors(status_map, &mut temps);
        // D5, Farbwerk 360, Octo, Quadro
        self.add_software_temp_sensors(status_map, &mut temps);
        temps.sort_unstable_by(|a, b| a.name.cmp(&b.name));
        temps
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = Vec::with_capacity(status_map.len());
        // D5
        self.add_single_fan_status(status_map, &mut channel_statuses);
        self.add_single_pump_status(status_map, &mut channel_statuses);
        // Octo, Quadro
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
        // Quadro
        self.add_flow_sensor_status(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Not;

    use super::*;
    use crate::repositories::liquidctl::liqctld_client::DeviceProperties;
    use crate::repositories::liquidctl::supported_devices::device_support::{
        FLOW_CHANNEL_LABEL, FLOW_CHANNEL_NAME,
    };

    fn aquacomputer_response(speed_channels: &[&str]) -> DeviceResponse {
        DeviceResponse {
            id: 1,
            description: "Aquacomputer Quadro".to_string(),
            device_type: "Aquacomputer".to_string(),
            serial_number: Some("1234567890".to_string()),
            properties: DeviceProperties {
                speed_channels: speed_channels
                    .iter()
                    .map(|channel_name| (*channel_name).to_string())
                    .collect(),
                color_channels: Vec::new(),
                supports_cooling: None,
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

    fn status_map_of(readings: &[(&str, &str)]) -> StatusMap {
        readings
            .iter()
            .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
            .collect()
    }

    /// Goal: liquidctl announces a Quadro's fans but not its flow sensor, so the reading
    /// would have no label to carry its unit. Method: take the info and the first status the
    /// way the repository does, and read back what the info lists.
    #[test]
    fn a_flow_sensor_in_the_first_status_is_listed_with_its_unit() {
        let device_support = AquaComputerSupport::new();
        let mut device_info = device_support.extract_info(&aquacomputer_response(&["fan1"]));
        let status = device_support.extract_status(
            &status_map_of(&[("fan 1 speed", "600"), ("flow sensor", "250")]),
            1,
        );

        device_support.extend_info_from_status(&status, &mut device_info);

        assert_eq!(device_info.channels.len(), 2);
        let flow = &device_info.channels[FLOW_CHANNEL_NAME];
        assert_eq!(flow.label.as_deref(), Some(FLOW_CHANNEL_LABEL));
        assert!(matches!(flow.kind, ChannelKind::InfoOnly));
        assert!(matches!(
            device_info.channels["fan1"].kind,
            ChannelKind::Speed(_)
        ));
    }

    /// Goal: the same driver serves the Octo and the D5 Next, which report no flow, and they
    /// must not gain a channel that never has a value. Method: a first status without one.
    #[test]
    fn a_device_without_a_flow_reading_lists_no_flow_channel() {
        let device_support = AquaComputerSupport::new();
        let mut device_info = device_support.extract_info(&aquacomputer_response(&["fan1"]));
        let status = device_support.extract_status(&status_map_of(&[("fan 1 speed", "600")]), 1);

        device_support.extend_info_from_status(&status, &mut device_info);

        assert_eq!(device_info.channels.len(), 1);
        assert!(device_info.channels.contains_key(FLOW_CHANNEL_NAME).not());
    }
}
