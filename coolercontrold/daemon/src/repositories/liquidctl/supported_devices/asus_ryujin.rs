// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::device::{
    ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
    StatusRefill,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{DeviceSupport, StatusMap};
use std::collections::HashMap;

#[derive(Debug)]
pub struct AsusRyujinSupport;
// asus_ryujin.py

#[deprecated(
    since = "2.2.1",
    note = "HWMon driver is preferred. Will likely be removed in the future."
)]
impl AsusRyujinSupport {
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for AsusRyujinSupport {
    fn supported_driver(&self) -> BaseDriver {
        // BaseDriver::AsusRyujin
        // This liquidctl driver currently doesn't support reading from the hwmon driver,
        // and so has problems when used in conjunction with that driver. The hwmon driver also
        // offers feature-parity, and the workarounds needed in CC for control are non-intuitive.
        // For these reasons, the hwmon driver is preferred.
        // see: https://gitlab.com/coolercontrol/coolercontrol/-/issues/457
        BaseDriver::NotSupported
    }

    #[allow(clippy::too_many_lines)]
    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
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
        channels.insert(
            "pump-fan".to_string(),
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
        channels.insert(
            "external-fans".to_string(),
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
        channels.insert(
            "external-fan1".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    fixed_enabled: false,
                    extension: None,
                }),
            },
        );
        channels.insert(
            "external-fan2".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    fixed_enabled: false,
                    extension: None,
                }),
            },
        );
        channels.insert(
            "external-fan3".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    fixed_enabled: false,
                    extension: None,
                }),
            },
        );
        channels.insert(
            "external-fan4".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    fixed_enabled: false,
                    extension: None,
                }),
            },
        );
        DeviceInfo {
            channels,
            lighting_speeds: Vec::new(),
            // liquid temp:
            temp_min: 20,
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

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        Vec::new()
    }

    fn refill_temperatures(&self, status_map: &StatusMap, temps: &mut StatusRefill<'_>) {
        self.add_liquid_temp(status_map, temps);
    }

    fn refill_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
        channel_statuses: &mut StatusRefill<'_>,
    ) {
        self.add_single_pump_status(status_map, channel_statuses);
        self.add_single_pump_fan_status(status_map, channel_statuses);
        self.add_single_external_fans_status(status_map, channel_statuses);
        self.add_multiple_external_fans_status(status_map, channel_statuses);
        channel_statuses
            .channels_mut()
            .sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
    }
}
