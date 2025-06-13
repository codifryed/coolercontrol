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

use std::collections::HashMap;

use crate::device::{
    ChannelInfo, ChannelStatus, DeviceInfo, DriverInfo, DriverType, LightingMode, SpeedOptions,
    TempStatus,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_client::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{
    ColorMode, DeviceSupport, StatusMap,
};

#[derive(Debug)]
pub struct MpgCoolerSupport;
// msi.py

impl MpgCoolerSupport {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for MpgCoolerSupport {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::MpgCooler
    }

    #[allow(clippy::too_many_lines)]
    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 60,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "waterblock-fan".to_string(),
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
        channels.insert(
            "fan1".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 20,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "fan2".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 20,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );
        channels.insert(
            "fan3".to_string(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    min_duty: 20,
                    max_duty: 100,
                    profiles_enabled: false,
                    fixed_enabled: true,
                    manual_profiles_enabled: true,
                }),
                ..Default::default()
            },
        );

        let lighting_speeds = vec!["0".to_string(), "1".to_string(), "2".to_string()];
        let lighting_modes = self.get_color_channel_modes(None);
        channels.insert(
            "sync".to_string(),
            ChannelInfo {
                lighting_modes,
                ..Default::default()
            },
        );

        // todo: LCD support is very specialized for this device
        //  - will need significant changes to achieve full support
        //    and partial support isn't currently possible. (other than "disabled")
        // let lcd_resolution = device_response
        //     .properties
        //     .lcd_resolution
        //     .unwrap_or((320, 240));
        // channels.insert(
        //     "lcd".to_string(),
        //     ChannelInfo {
        //         lcd_modes: vec![
        //             LcdMode {
        //                 name: "liquid".to_string(),
        //                 frontend_name: "Liquid(default)".to_string(),
        //                 brightness: true,
        //                 orientation: true,
        //                 image: false,
        //                 colors_min: 0,
        //                 colors_max: 0,
        //                 type_: LcdModeType::Liquidctl,
        //             },
        //             LcdMode {
        //                 name: "image".to_string(),
        //                 frontend_name: "Image/gif".to_string(),
        //                 brightness: true,
        //                 orientation: true,
        //                 image: true,
        //                 colors_min: 0,
        //                 colors_max: 0,
        //                 type_: LcdModeType::Liquidctl,
        //             },
        //             LcdMode {
        //                 name: "temp".to_string(),
        //                 frontend_name: "Single Temp".to_string(),
        //                 brightness: true,
        //                 orientation: true,
        //                 image: false,
        //                 colors_min: 0, // for custom types
        //                 colors_max: 0,
        //                 type_: LcdModeType::Custom,
        //             },
        //             LcdMode {
        //                 name: "carousel".to_string(),
        //                 frontend_name: "Carousel".to_string(),
        //                 brightness: true,
        //                 orientation: true,
        //                 image: false,
        //                 colors_min: 0, // for custom types
        //                 colors_max: 0,
        //                 type_: LcdModeType::Custom,
        //             },
        //         ],
        //         lcd_info: Some(LcdInfo {
        //             screen_width: lcd_resolution.0,
        //             screen_height: lcd_resolution.1,
        //             max_image_size_bytes: 24_320 * 1024, // 24,320 KB/KiB
        //         }),
        //         ..Default::default()
        //     },
        // );

        DeviceInfo {
            channels,
            lighting_speeds,
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

    fn get_color_channel_modes(&self, _channel_name: Option<&str>) -> Vec<LightingMode> {
        let color_modes = vec![
            // some color modes have a "rainbow" option, which is used when colors = 0
            ColorMode::new("disable", 0, 0, false, false),
            ColorMode::new("steady", 1, 1, false, false),
            ColorMode::new("breathing", 1, 1, true, false),
            // 0 = rainbow:
            ColorMode::new("clock", 0, 2, true, false),
            ColorMode::new("color ring", 0, 0, true, false),
            ColorMode::new("color ring double flashing", 0, 0, true, false),
            ColorMode::new("color ring flashing", 0, 0, true, false),
            ColorMode::new("color shift", 0, 0, true, false),
            // 0 = rainbow:
            ColorMode::new("color wave", 0, 2, true, false),
            ColorMode::new("disable2", 0, 0, false, false),
            // 0 = rainbow:
            ColorMode::new("double flashing", 0, 1, true, false),
            ColorMode::new("double meteor", 0, 0, true, false),
            ColorMode::new("energy", 0, 0, true, false),
            ColorMode::new("fire", 1, 2, true, false),
            // 0 = rainbow:
            ColorMode::new("flashing", 0, 1, true, false),
            ColorMode::new("lightning", 1, 1, true, false),
            ColorMode::new("marquee", 1, 1, true, false),
            // 0 = rainbow:
            ColorMode::new("meteor", 0, 1, true, false),
            // 0 = rainbow:
            ColorMode::new("msi marquee", 0, 1, true, false),
            ColorMode::new("planetary", 0, 0, true, false),
            ColorMode::new("rainbow double flashing", 0, 0, true, false),
            ColorMode::new("rainbow flashing", 0, 0, true, false),
            ColorMode::new("rainbow wave", 0, 0, true, false),
            ColorMode::new("random", 0, 0, true, false),
            // 0 = rainbow:
            ColorMode::new("stack", 0, 1, true, false),
            // 0 = rainbow:
            ColorMode::new("visor", 0, 2, true, false),
            // 0 = rainbow:
            ColorMode::new("water drop", 0, 1, true, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }

    fn get_temperatures(&self, _status_map: &StatusMap) -> Vec<TempStatus> {
        Vec::new()
    }

    fn get_channel_statuses(
        &self,
        status_map: &StatusMap,
        _device_index: u8,
    ) -> Vec<ChannelStatus> {
        let mut channel_statuses = vec![];
        self.add_multiple_fans_status(status_map, &mut channel_statuses);
        self.add_single_pump_status(status_map, &mut channel_statuses);
        self.add_single_water_block_status(status_map, &mut channel_statuses);
        channel_statuses.sort_unstable_by(|s1, s2| s1.name.cmp(&s2.name));
        channel_statuses
    }
}
