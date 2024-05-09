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

use std::io::{Error, ErrorKind};
use std::path::PathBuf;

use anyhow::{Context, Result};
use log::{trace, warn};
use regex::Regex;

use crate::device::TempStatus;
use crate::repositories::cpu_repo::CPU_DEVICE_NAMES_ORDERED;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};

const PATTERN_TEMP_INPUT_NUMBER: &str = r"^temp(?P<number>\d+)_input$";
const TEMP_SANITY_MIN: f64 = 0.0;
const TEMP_SANITY_MAX: f64 = 120.0;

/// Initialize all applicable temp sensors
pub async fn init_temps(base_path: &PathBuf, device_name: &str) -> Result<Vec<HwmonChannelInfo>> {
    if temps_used_by_another_repo(device_name) {
        return Ok(vec![]);
    }
    let mut temps = vec![];
    let mut dir_entries = tokio::fs::read_dir(base_path).await?;
    let regex_temp_input = Regex::new(PATTERN_TEMP_INPUT_NUMBER)?;
    while let Some(entry) = dir_entries.next_entry().await? {
        let os_file_name = entry.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_temp_input.is_match(file_name) {
            let channel_number: u8 = regex_temp_input
                .captures(file_name)
                .context("Temp Number should exist")?
                .name("number")
                .context("Number Group should exist")?
                .as_str()
                .parse()?;
            if !sensor_is_usable(base_path, &channel_number).await {
                continue;
            }
            let channel_name = get_temp_channel_name(&channel_number).await;
            let label = get_temp_channel_label(base_path, &channel_number).await;
            temps.push(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Temp,
                number: channel_number,
                name: channel_name,
                label,
                ..Default::default()
            });
        }
    }
    temps.sort_by(|t1, t2| t1.number.cmp(&t2.number));
    trace!("Hwmon Temps detected: {:?} for {:?}", temps, base_path);
    Ok(temps)
}

/// Return the temp statuses for all channels.
/// Defaults to 0 for all temps, to handle temporary issues,
/// as they were correctly detected on startup.
pub async fn extract_temp_statuses(driver: &HwmonDriverInfo) -> Vec<TempStatus> {
    let mut temps = vec![];
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        let temp =
            tokio::fs::read_to_string(driver.path.join(format!("temp{}_input", channel.number)))
                .await
                .and_then(check_parsing_32)
                // hwmon temps are in millidegrees:
                .map(|degrees| f64::from(degrees) / 1000.0f64)
                .unwrap_or(0f64);
        temps.push(TempStatus {
            name: channel.name.clone(),
            temp,
        });
    }
    temps
}

/// This is used to remove cpu temps, as we already have repos for that that use hwmon.
fn temps_used_by_another_repo(device_name: &str) -> bool {
    CPU_DEVICE_NAMES_ORDERED.contains(&device_name)
}

/// Returns whether the temperature sensor is returning valid and sane values
/// Note: temp sensor readings come in millidegrees by default, i.e. 35.0C == 35000
async fn sensor_is_usable(base_path: &PathBuf, channel_number: &u8) -> bool {
    let possible_degrees =
        tokio::fs::read_to_string(base_path.join(format!("temp{channel_number}_input")))
            .await
            .and_then(check_parsing_32)
            .map(|degrees| f64::from(degrees) / 1000.0f64)
            .map_err(|err| {
                warn!(
                    "Error reading temperature value from: {:?}/temp{}_input - {}",
                    base_path, channel_number, err
                );
            })
            .ok();
    if let Some(degrees) = possible_degrees {
        let has_sane_value = (TEMP_SANITY_MIN..=TEMP_SANITY_MAX).contains(&degrees);
        if !has_sane_value {
            warn!(
                "Temperature value: {} at {:?}/temp{}_input is outside of usable range. \
                Most likely the sensor is not reporting real readings",
                degrees, base_path, channel_number
            );
        }
        return has_sane_value;
    }
    false
}

fn check_parsing_32(content: String) -> Result<i32, Error> {
    match content.trim().parse::<i32>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string())),
    }
}

/// Reads the contents of the temp?_label file specified by `base_path` and
/// `channel_number`, trims any leading or trailing whitespace, and returns the resulting string if it
/// is not empty.
///
/// Arguments:
///
/// * `base_path`: A `PathBuf` object representing the base path where the file `temp{}_label` is
/// located.
/// * `channel_number`: The `channel_number` parameter is an unsigned 8-bit integer that represents the
/// channel number. It is used to construct the file path for reading the label.
///
/// Returns:
///
/// an `Option<String>`.
async fn get_temp_channel_label(base_path: &PathBuf, channel_number: &u8) -> Option<String> {
    tokio::fs::read_to_string(base_path.join(format!("temp{channel_number}_label")))
        .await
        .ok()
        .and_then(|label| {
            let temp_label = label.trim();
            if temp_label.is_empty() {
                warn!(
                    "Temp label is empty: {:?}/temp{}_label",
                    base_path, channel_number
                );
                None
            } else {
                Some(temp_label.to_string())
            }
        })
}

/// Returns a string that represents a unique channel name/ID.
///
/// Arguments:
///
/// * `channel_number`: The `channel_number` parameter is a reference to an unsigned 8-bit integer
/// (`&u8`).
///
/// Returns:
///
/// * A `String` that represents a unique channel name/ID.
async fn get_temp_channel_name(channel_number: &u8) -> String {
    format!("temp{channel_number}")
}

/// Tests
#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[tokio::test]
    async fn find_temp_dir_not_exist() {
        // given:
        let test_base_path = Path::new("/tmp/does_not_exist").to_path_buf();
        let device_name = "Test Driver".to_string();

        // when:
        let temps_result = init_temps(&test_base_path, &device_name).await;

        // then:
        assert!(temps_result.is_err());
        assert!(temps_result
            .map_err(|err| err.to_string().contains("No such file or directory"))
            .unwrap_err());
    }

    #[tokio::test]
    async fn find_temp() {
        // given:
        let test_base_path = Path::new("/tmp/coolercontrol-test/temps_test").to_path_buf();
        tokio::fs::create_dir_all(&test_base_path).await.unwrap();
        tokio::fs::write(
            test_base_path.join("temp1_input"),
            b"30000", // temp
        )
        .await
        .unwrap();
        tokio::fs::write(
            test_base_path.join("temp1_label"),
            b"Temp 1", // label
        )
        .await
        .unwrap();
        let device_name = "Test Driver".to_string();

        // when:
        let temps_result = init_temps(&test_base_path, &device_name).await;

        // then:
        // println!("RESULT: {:?}", fans_result);
        tokio::fs::remove_dir_all(&test_base_path.parent().unwrap())
            .await
            .unwrap();
        assert!(temps_result.is_ok());
        let temps = temps_result.unwrap();
        assert_eq!(temps.len(), 1);
        assert_eq!(temps[0].hwmon_type, HwmonChannelType::Temp);
        assert_eq!(temps[0].name, "temp1");
        assert_eq!(temps[0].label, Some("Temp 1".to_string()));
        assert!(!temps[0].pwm_mode_supported);
        assert_eq!(temps[0].pwm_enable_default, None);
        assert_eq!(temps[0].number, 1);
    }
}
