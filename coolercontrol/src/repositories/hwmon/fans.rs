/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
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
 ******************************************************************************/

use std::io::{Error, ErrorKind};
use std::path::PathBuf;

use anyhow::{Context, Result};
use log::{debug, error, warn};
use regex::Regex;

use crate::device::ChannelStatus;
use crate::repositories::hwmon::devices::DeviceFns;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};

const PATTERN_PWN_FILE_NUMBER: &str = r"^pwm(?P<number>\d+)$";

/// A struct containing Fan handling functions
pub struct FanFns {}

impl FanFns {
    /// Initialize all applicable fans
    pub async fn init_fans(
        base_path: &PathBuf, driver_name: &String,
    ) -> Result<Vec<HwmonChannelInfo>> {
        let mut fans = vec![];
        let mut dir_entries = tokio::fs::read_dir(base_path).await?;
        let regex_pwm_file = Regex::new(PATTERN_PWN_FILE_NUMBER)?;
        while let Some(entry) = dir_entries.next_entry().await? {
            let os_file_name = entry.file_name();
            let file_name = os_file_name.to_str().context("File Name should be a str")?;
            if regex_pwm_file.is_match(file_name) {
                let channel_number: u8 = regex_pwm_file
                    .captures(file_name).context("PWM Number should exist")?
                    .name("number").context("Number Group should exist")?.as_str().parse()?;
                let (sensor_is_usable, current_pwm_enable) =
                    Self::sensor_is_usable(base_path, &channel_number).await;
                if !sensor_is_usable {
                    continue;
                }
                let pwm_enable_default = Self::adjusted_pwm_default(&current_pwm_enable, driver_name);
                let channel_name = Self::get_fan_channel_name(base_path, &channel_number).await;
                let pwm_mode_supported = Self::determine_pwm_mode_support(base_path, &channel_number).await;
                fans.push(
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Fan,
                        number: channel_number,
                        pwm_enable_default,
                        name: channel_name,
                        pwm_mode_supported,
                    }
                )
            }
        }
        fans.sort_by(|c1, c2| c1.number.cmp(&c2.number));
        DeviceFns::handle_duplicate_channel_names(&mut fans);
        debug!("Hwmon pwm fans detected: {:?} for {:?}", fans, base_path);
        Ok(fans)
    }

    /// Return the fan statuses for all channels.
    /// Defaults to 0 for rpm and duty to handle temporary issues,
    /// as they were correctly detected on startup.
    pub async fn extract_fan_statuses(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
        let mut channels = vec![];
        for channel in driver.channels.iter() {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            let fan_rpm = tokio::fs::read_to_string(
                driver.path.join(format!("fan{}_input", channel.number))
            ).await
                .and_then(Self::check_parsing_32)
                .unwrap_or(0);
            let fan_duty = tokio::fs::read_to_string(
                driver.path.join(format!("pwm{}", channel.number))
            ).await
                .and_then(Self::check_parsing_8)
                // This give us a single decimal point of accuracy. More isn't needed:
                .map(|raw_duty| (raw_duty as f64 / 0.255f64).round() / 10f64)
                .unwrap_or(0f64);
            let fan_pwm_mode = if channel.pwm_mode_supported {
                tokio::fs::read_to_string(
                    driver.path.join(format!("pwm{}_mode", channel.number))
                ).await
                    .and_then(Self::check_parsing_8)
                    .ok()
            } else {
                None
            };
            channels.push(ChannelStatus {
                name: channel.name.clone(),
                rpm: Some(fan_rpm),
                duty: Some(fan_duty),
                pwm_mode: fan_pwm_mode,
            })
        }
        channels
    }

    /// Not all drivers have pwm_enable for their fans. In that case there is no "automatic" mode available.
    ///  pwm_enable setting options:
    ///  - 0 : full speed / off (not used/recommended)
    ///  - 1 : manual control (setting pwm* will adjust fan speed)
    ///  - 2 : automatic (primarily used by on-board/chip fan control, like laptops or mobos without smart fan control)
    ///  - 3 : "Fan Speed Cruise" mode (?)
    ///  - 4 : "Smart Fan III" mode (NCT6775F only)
    ///  - 5 : "Smart Fan IV" mode (modern MoBo's with build-in smart fan control probably use this)
    async fn sensor_is_usable(
        base_path: &PathBuf, channel_number: &u8,
    ) -> (bool, Option<u8>) {
        let current_pwm_enable: Option<u8> = tokio::fs::read_to_string(
            base_path.join(format!("pwm{}_enable", channel_number))
        ).await
            .and_then(Self::check_parsing_8)
            .ok();
        if current_pwm_enable == None {
            warn!("No pwm_enable found for fan#{}", channel_number);
        }
        let has_valid_pwm_value = tokio::fs::read_to_string(
            base_path.join(format!("pwm{}", channel_number))
        ).await
            .and_then(Self::check_parsing_8)
            .map_err(|err| warn!("Error reading fan pwm value: {}", err))
            .is_ok();
        let has_valid_fan_rpm = tokio::fs::read_to_string(
            base_path.join(format!("fan{}_input", channel_number))
        ).await
            .and_then(Self::check_parsing_32)
            .map_err(|err| warn!("Error reading fan rpm value: {}", err))
            .is_ok();
        if has_valid_pwm_value && has_valid_fan_rpm {
            (true, current_pwm_enable)
        } else {
            (false, None)
        }
    }

    fn check_parsing_32(content: String) -> Result<u32, Error> {
        match content.trim().parse::<u32>() {
            Ok(value) => Ok(value),
            Err(err) =>
                Err(Error::new(ErrorKind::InvalidData, err.to_string()))
        }
    }

    fn check_parsing_8(content: String) -> Result<u8, Error> {
        match content.trim().parse::<u8>() {
            Ok(value) => Ok(value),
            Err(err) =>
                Err(Error::new(ErrorKind::InvalidData, err.to_string()))
        }
    }

    /// Some drivers should have an automatic fallback for safety reasons,
    /// regardless of the current value.
    fn adjusted_pwm_default(current_pwm_enable: &Option<u8>, driver_name: &String) -> Option<u8> {
        current_pwm_enable.map(|original_value|
            if DeviceFns::driver_needs_pwm_fallback(&driver_name) {
                2
            } else {
                original_value
            })
    }

    async fn get_fan_channel_name(base_path: &PathBuf, channel_number: &u8) -> String {
        match tokio::fs::read_to_string(
            base_path.join(format!("fan{}_label", channel_number))
        ).await {
            Ok(label) => {
                let fan_label = label.trim();
                if fan_label.is_empty() {
                    warn!("Fan label is empty for {:?}/fan{}_label", base_path, channel_number)
                } else {
                    return fan_label.to_string();
                }
            }
            Err(_) =>
                warn!("Fan label doesn't exist: {:?}/fan{}_label", base_path, channel_number)
        };
        format!("fan{}", channel_number)
    }

    /// We need to verify that setting this option is indeed supported (per pwm channel)
    ///  0 = DC mode, 1 = PWM Mode. Not every device may have this option.
    async fn determine_pwm_mode_support(base_path: &PathBuf, channel_number: &u8) -> bool {
        let current_pwm_mode = tokio::fs::read_to_string(
            base_path.join(format!("pwm{}_mode", channel_number))
        ).await
            .map_err(|_| warn!("PWM Mode not found for fan #{} from {:?}", channel_number, base_path))
            .ok()
            .map(|mode_str| mode_str.trim().parse::<u8>()
                .map_err(|_| error!("PWM Mode is not an integer"))
                .ok())
            .flatten();
        if let Some(pwm_mode) = current_pwm_mode {
            let dc_mode_supported = tokio::fs::write(
                base_path.join(format!("pwm{}_mode", channel_number)),
                b"0",
            ).await.is_ok();
            let pwm_mode_supported = tokio::fs::write(
                base_path.join(format!("pwm{}_mode", channel_number)),
                b"1",
            ).await.is_ok();
            if let Err(err) = tokio::fs::write(
                base_path.join(format!("pwm{}_mode", channel_number)),
                pwm_mode.to_string().into_bytes(),
            ).await {
                error!("Error writing original pwm_mode: {} for {:?}/pwm{}_mode. Reason: {}",
                    &pwm_mode, base_path, channel_number, err);
            }
            if dc_mode_supported && pwm_mode_supported {
                return true;
            }
        }
        false
    }
}

/// Tests
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn find_fan_dir_not_exist() {
        // given:
        let test_base_path = Path::new("/tmp/does_not_exist").to_path_buf();
        let driver_name = "Test Driver".to_string();

        // when:
        let fans_result = FanFns::init_fans(
            &test_base_path, &driver_name,
        ).await;

        // then:
        assert!(fans_result.is_err());
        assert!(fans_result.map_err(|err| err.to_string().contains("No such file or directory")).unwrap_err())
    }

    #[tokio::test]
    async fn find_fan() {
        // given:
        let test_base_path = Path::new("/tmp/coolercontrol-test/fans_test").to_path_buf();
        tokio::fs::create_dir_all(&test_base_path).await.unwrap();
        tokio::fs::write(
            test_base_path.join("pwm1"),
            b"127", // duty
        ).await.unwrap();
        tokio::fs::write(
            test_base_path.join("fan1_input"),
            b"3000", // rpm
        ).await.unwrap();
        let driver_name = "Test Driver".to_string();

        // when:
        let fans_result = FanFns::init_fans(
            &test_base_path, &driver_name,
        ).await;

        // then:
        // println!("RESULT: {:?}", fans_result);
        tokio::fs::remove_dir_all(&test_base_path.parent().unwrap()).await.unwrap();
        assert!(fans_result.is_ok());
        let fans = fans_result.unwrap();
        assert_eq!(fans.len(), 1);
        assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
        assert_eq!(fans[0].name, "fan1");
        assert_eq!(fans[0].pwm_mode_supported, false);
        assert_eq!(fans[0].pwm_enable_default, None);
        assert_eq!(fans[0].number, 1);
    }
}
