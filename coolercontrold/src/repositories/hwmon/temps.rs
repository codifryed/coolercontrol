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
use heck::ToTitleCase;
use log::{debug, error, warn};
use regex::Regex;

use crate::device::TempStatus;
use crate::repositories::cpu_repo::PSUTIL_CPU_SENSOR_NAMES;
use crate::repositories::hwmon::devices;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};

const PATTERN_TEMP_INPUT_NUMBER: &str = r"^temp(?P<number>\d+)_input$";

/// Initialize all applicable temp sensors
pub async fn init_temps(
    base_path: &PathBuf, device_name: &String,
) -> Result<Vec<HwmonChannelInfo>> {
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
                .captures(file_name).context("Temp Number should exist")?
                .name("number").context("Number Group should exist")?.as_str().parse()?;
            if !sensor_is_usable(base_path, &channel_number).await {
                continue;
            }
            let channel_name = get_temp_channel_name(base_path, &channel_number).await;
            temps.push(
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Temp,
                    number: channel_number,
                    name: channel_name,
                    ..Default::default()
                }
            )
        }
    }
    temps.sort_by(|t1, t2| t1.number.cmp(&t2.number));
    devices::handle_duplicate_channel_names(&mut temps);
    debug!("Hwmon Temps detected: {:?} for {:?}", temps, base_path);
    Ok(temps)
}

/// Return the temp statuses for all channels.
/// Defaults to 0 for all temps, to handle temporary issues,
/// as they were correctly detected on startup.
pub async fn extract_temp_statuses(device_id: &u8, driver: &HwmonDriverInfo) -> Vec<TempStatus> {
    let mut temps = vec![];
    for channel in driver.channels.iter() {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        let temp = tokio::fs::read_to_string(
            driver.path.join(format!("temp{}_input", channel.number))
        ).await
            .and_then(check_parsing_32)
            // hwmon temps are in millidegrees:
            .map(|degrees| degrees as f64 / 1000.0f64)
            .unwrap_or(0f64);
        temps.push(TempStatus {
            name: channel.name.clone(),
            temp,
            frontend_name: channel.name.to_title_case(),
            external_name: format!("HW#{} {}", device_id, channel.name.to_title_case()),
        })
    }
    temps
}

/// This is used to remove cpu & gpu temps, as we already have repos for that that use hwmon.
fn temps_used_by_another_repo(device_name: &str) -> bool {
    PSUTIL_CPU_SENSOR_NAMES.contains(&device_name)
        // thinkpad is an exception, as it contains other temperature sensors as well
        && device_name != "thinkpad"
}

/// Returns whether the temperature sensor is returning valid and sane values
/// Note: temp sensor readings come in millidegrees by default, i.e. 35.0C == 35000
async fn sensor_is_usable(base_path: &PathBuf, channel_number: &u8) -> bool {
    let possible_degrees = tokio::fs::read_to_string(
        base_path.join(format!("temp{}_input", channel_number))
    ).await
        .and_then(check_parsing_32)
        .map(|degrees| degrees as f64 / 1000.0f64)
        .map_err(|err|
            error!("Error reading temperature value from: {:?}/temp{}_input - {}",
                    base_path, channel_number, err))
        .ok();
    if let Some(degrees) = possible_degrees {
        let has_sane_value = degrees >= 0.0f64 && degrees <= 100.0f64;
        if !has_sane_value {
            warn!("Temperature value: {} at {:?}/temp{}_input is outside of usable range. \
                Most likely the sensor is not reporting real readings",
                    degrees, base_path, channel_number
                )
        }
        return has_sane_value;
    }
    false
}

fn check_parsing_32(content: String) -> Result<i32, Error> {
    match content.trim().parse::<i32>() {
        Ok(value) => Ok(value),
        Err(err) =>
            Err(Error::new(ErrorKind::InvalidData, err.to_string()))
    }
}

async fn get_temp_channel_name(base_path: &PathBuf, channel_number: &u8) -> String {
    match tokio::fs::read_to_string(
        base_path.join(format!("temp{}_label", channel_number))
    ).await {
        Ok(label) => {
            let temp_label = label.trim();
            if temp_label.is_empty() {
                warn!("Temp label is empty: {:?}/temp{}_label", base_path, channel_number);
            } else {
                return temp_label.to_string();
            }
        }
        Err(_) =>
            warn!("Temp label doesn't exist for {:?}/temp{}_label",base_path, channel_number)
    };
    format!("temp{}", channel_number)
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
        let temps_result = init_temps(
            &test_base_path, &device_name,
        ).await;

        // then:
        assert!(temps_result.is_err());
        assert!(temps_result.map_err(|err| err.to_string().contains("No such file or directory")).unwrap_err())
    }

    #[tokio::test]
    async fn find_temp() {
        // given:
        let test_base_path = Path::new("/tmp/coolercontrol-test/temps_test").to_path_buf();
        tokio::fs::create_dir_all(&test_base_path).await.unwrap();
        tokio::fs::write(
            test_base_path.join("temp1_input"),
            b"30000", // temp
        ).await.unwrap();
        tokio::fs::write(
            test_base_path.join("temp1_label"),
            b"Temp 1", // label
        ).await.unwrap();
        let device_name = "Test Driver".to_string();

        // when:
        let temps_result = init_temps(
            &test_base_path, &device_name,
        ).await;

        // then:
        // println!("RESULT: {:?}", fans_result);
        tokio::fs::remove_dir_all(&test_base_path.parent().unwrap()).await.unwrap();
        assert!(temps_result.is_ok());
        let temps = temps_result.unwrap();
        assert_eq!(temps.len(), 1);
        assert_eq!(temps[0].hwmon_type, HwmonChannelType::Temp);
        assert_eq!(temps[0].name, "Temp 1");
        assert_eq!(temps[0].pwm_mode_supported, false);
        assert_eq!(temps[0].pwm_enable_default, None);
        assert_eq!(temps[0].number, 1);
    }
}