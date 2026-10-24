// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelKind, DeviceInfo, DriverInfo, DriverType, LcdInfo,
    LcdMode, LcdModeType, LightingMode, SpeedOptions,
};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::liqctld_schema::DeviceResponse;
use crate::repositories::liquidctl::supported_devices::device_support::{ColorMode, DeviceSupport};

#[derive(Debug)]
pub struct KrakenZ3Support;
// kraken3.py

impl KrakenZ3Support {
    pub fn new() -> Self {
        Self {}
    }
}

impl DeviceSupport for KrakenZ3Support {
    fn supported_driver(&self) -> BaseDriver {
        BaseDriver::KrakenZ3
    }

    #[allow(clippy::too_many_lines)]
    fn extract_info(&self, device_response: &DeviceResponse) -> DeviceInfo {
        let mut channels = HashMap::new();
        channels.insert(
            "pump".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 20,
                    max_duty: 100,
                    fixed_enabled: true,
                    extension: Some(ChannelExtensionNames::AutoHWCurve),
                }),
            },
        );
        channels.insert(
            "fan".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    min_duty: 0,
                    max_duty: 100,
                    fixed_enabled: true,
                    extension: Some(ChannelExtensionNames::AutoHWCurve),
                }),
            },
        );
        // Kraken2023 and KrakenZ have different color channels:
        for channel_name in &device_response.properties.color_channels {
            let lighting_modes = self.get_color_channel_modes(None);
            channels.insert(
                channel_name.to_owned(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Lighting(lighting_modes),
                },
            );
        }
        let lighting_speeds = vec![
            "slowest".to_string(),
            "slower".to_string(),
            "normal".to_string(),
            "faster".to_string(),
            "fastest".to_string(),
        ];

        let lcd_resolution = device_response
            .properties
            .lcd_resolution
            .unwrap_or((320, 320));
        channels.insert(
            "lcd".to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Lcd {
                    modes: vec![
                        LcdMode {
                            name: "liquid".to_string(),
                            frontend_name: "Liquid(default)".to_string(),
                            brightness: true,
                            orientation: true,
                            image: false,
                            colors_min: 0,
                            colors_max: 0,
                            type_: LcdModeType::Liquidctl,
                        },
                        LcdMode {
                            name: "image".to_string(),
                            frontend_name: "Image/gif".to_string(),
                            brightness: true,
                            orientation: true,
                            image: true,
                            colors_min: 0,
                            colors_max: 0,
                            type_: LcdModeType::Liquidctl,
                        },
                        LcdMode {
                            name: "temp".to_string(),
                            frontend_name: "Single Temp".to_string(),
                            brightness: true,
                            orientation: true,
                            image: false,
                            colors_min: 0, // for custom types
                            colors_max: 0,
                            type_: LcdModeType::Custom,
                        },
                        LcdMode {
                            name: "carousel".to_string(),
                            frontend_name: "Carousel".to_string(),
                            brightness: true,
                            orientation: true,
                            image: false,
                            colors_min: 0, // for custom types
                            colors_max: 0,
                            type_: LcdModeType::Custom,
                        },
                    ],
                    info: Some(LcdInfo {
                        screen_width: lcd_resolution.0,
                        screen_height: lcd_resolution.1,
                        // liquidctl asserts a processed gif is under `_LCD_TOTAL_MEMORY`
                        // KB before it will send one, while its own bucket allocator
                        // counts that same number in KiB. The assert is the stricter of
                        // the two and the one a user hits: over it the upload dies inside
                        // liquidctl as a bare assertion, which the daemon can only report
                        // as a device fault. Capped here so the size is refused by size.
                        max_image_size_bytes: 24_320_000,
                        // Withdrawn after initialize, the first point the firmware version
                        // is known. Only the Kraken 2023 on firmware 2.x refuses gifs.
                        gif_supported: true,
                    }),
                },
            },
        );

        DeviceInfo {
            channels,
            lighting_speeds,
            temp_min: 20,
            temp_max: 60,
            profile_max_length: 9,
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
        // same as the KrakenX3
        let color_modes = vec![
            ColorMode::new("off", 0, 0, false, false),
            ColorMode::new("fixed", 1, 1, false, false),
            ColorMode::new("fading", 1, 8, true, false),
            ColorMode::new("super-fixed", 1, 40, false, false),
            ColorMode::new("spectrum-wave", 0, 0, true, true),
            ColorMode::new("marquee-3", 1, 1, true, true),
            ColorMode::new("marquee-4", 1, 1, true, true),
            ColorMode::new("marquee-5", 1, 1, true, true),
            ColorMode::new("marquee-6", 1, 1, true, true),
            ColorMode::new("covering-marquee", 1, 8, true, true),
            ColorMode::new("alternating-3", 1, 2, true, false),
            ColorMode::new("alternating-4", 1, 2, true, false),
            ColorMode::new("alternating-5", 1, 2, true, false),
            ColorMode::new("alternating-6", 1, 2, true, false),
            ColorMode::new("moving-alternating-3", 1, 2, true, true),
            ColorMode::new("moving-alternating-4", 1, 2, true, true),
            ColorMode::new("moving-alternating-5", 1, 2, true, true),
            ColorMode::new("moving-alternating-6", 1, 2, true, true),
            ColorMode::new("pulse", 1, 8, true, false),
            ColorMode::new("breathing", 1, 8, true, false),
            ColorMode::new("super-breathing", 1, 40, true, false),
            ColorMode::new("candle", 1, 1, false, false),
            ColorMode::new("starry-night", 1, 1, true, false),
            ColorMode::new("rainbow-flow", 0, 0, true, true),
            ColorMode::new("super-rainbow", 0, 0, true, true),
            ColorMode::new("rainbow-pulse", 0, 0, true, true),
            ColorMode::new("loading", 1, 1, true, false),
            ColorMode::new("tai-chi", 1, 2, true, false),
            ColorMode::new("water-cooler", 2, 2, true, false),
            ColorMode::new("wings", 1, 1, true, false),
        ];
        self.convert_to_channel_lighting_modes(color_modes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::ChannelKind;
    use crate::repositories::liquidctl::liqctld_schema::{DeviceProperties, DeviceResponse};

    fn kraken_response(lcd_resolution: Option<(u32, u32)>) -> DeviceResponse {
        DeviceResponse {
            id: 1,
            description: "NZXT Kraken 2024 Elite RGB".to_string(),
            device_type: "KrakenZ3".to_string(),
            serial_number: Some("1234567890".to_string()),
            properties: DeviceProperties {
                speed_channels: Vec::new(),
                color_channels: Vec::new(),
                supports_cooling: None,
                supports_cooling_profiles: None,
                supports_lighting: None,
                led_count: None,
                lcd_resolution,
            },
            liquidctl_version: Some("1.16.0".to_string()),
            hid_address: Some("/dev/hidraw0".to_string()),
            hwmon_address: None,
        }
    }

    fn lcd_info_of(device_info: &DeviceInfo) -> &LcdInfo {
        let ChannelKind::Lcd { info, .. } = &device_info.channels["lcd"].kind else {
            panic!("the lcd channel should be an lcd channel");
        };
        info.as_ref().expect("lcd info should be present")
    }

    /// Goal: liquidctl asserts on a processed gif over `_LCD_TOTAL_MEMORY` KB, and an
    /// assertion inside the driver reaches the user as an unexplained device fault. The cap
    /// has to refuse first, so it must not sit above that number. Method: read it back.
    #[test]
    fn the_image_cap_stays_under_liquidctls_own_assert() {
        let info = KrakenZ3Support::new().extract_info(&kraken_response(Some((640, 640))));

        // `assert len(data) / 1000 < 24320` in liquidctl's `set_screen`.
        assert_eq!(lcd_info_of(&info).max_image_size_bytes, 24_320_000);
        assert!(
            lcd_info_of(&info).max_image_size_bytes < 24_320 * 1024,
            "the KiB reading is the looser one and leaves a window that fails obscurely"
        );
    }

    /// Goal: the screen's own geometry drives image processing, and the 2024 Elite is the
    /// one model that is not 320x320. Method: the resolution liquidctl reports for it.
    #[test]
    fn the_screen_geometry_comes_from_the_driver() {
        let info = KrakenZ3Support::new().extract_info(&kraken_response(Some((640, 640))));
        assert_eq!(lcd_info_of(&info).screen_width, 640);
        assert_eq!(lcd_info_of(&info).screen_height, 640);

        // An older liquidctl reports no resolution at all; the Z-series default stands in.
        let fallback = KrakenZ3Support::new().extract_info(&kraken_response(None));
        assert_eq!(lcd_info_of(&fallback).screen_width, 320);
    }

    /// Goal: gifs are assumed until liqctld says otherwise, since the firmware version is
    /// not known this early. Method: the freshly extracted info.
    #[test]
    fn gifs_are_offered_until_the_firmware_is_known() {
        let info = KrakenZ3Support::new().extract_info(&kraken_response(Some((240, 240))));
        assert!(lcd_info_of(&info).gif_supported);
    }
}
