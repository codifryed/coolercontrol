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
use std::ops::Not;
use std::path::{Path, PathBuf};

use crate::cc_fs;
use crate::device::TempStatus;
use crate::repositories::cpu_repo::CPU_DEVICE_NAMES_ORDERED;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use anyhow::{Context, Result};
use futures_util::future::join_all;
use log::{debug, info, trace};
use regex::Regex;

const PATTERN_TEMP_INPUT_NUMBER: &str = r"^temp(?P<number>\d+)_input$";
const TEMP_SANITY_MIN: f64 = 0.0;
const TEMP_SANITY_MAX: f64 = 140.0;

/// Initialize all applicable temp sensors
pub async fn init_temps(base_path: &PathBuf, device_name: &str) -> Result<Vec<HwmonChannelInfo>> {
    if temps_used_by_another_repo(device_name) {
        return Ok(vec![]);
    }
    let mut temps = vec![];
    let dir_entries = cc_fs::read_dir(base_path)?;
    let regex_temp_input = Regex::new(PATTERN_TEMP_INPUT_NUMBER)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_temp_input.is_match(file_name) {
            let channel_number: u8 = regex_temp_input
                .captures(file_name)
                .context("Temp Number should exist")?
                .name("number")
                .context("Number Group should exist")?
                .as_str()
                .parse()?;
            if sensor_is_usable(base_path, &channel_number).await.not() {
                continue;
            }
            let channel_name = get_temp_channel_name(channel_number);
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
    trace!(
        "Hwmon Temps detected: {temps:?} for {}",
        base_path.display()
    );
    Ok(temps)
}

/// Return the temp statuses for all channels.
/// Defaults to 0 for all temps, to handle temporary issues,
/// as they were correctly detected on startup.
/// This function calls all temp channels sequentially. See the `concurrently`
/// version of this function for concurrent execution.
pub async fn extract_temp_statuses(driver: &HwmonDriverInfo) -> Vec<TempStatus> {
    let mut temps = vec![];
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        let temp = cc_fs::read_sysfs(driver.path.join(format!("temp{}_input", channel.number)))
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

#[allow(dead_code)]
/// This is the concurrent version of the `extract_temp_statuses` function.
pub async fn extract_temp_statuses_concurrently(driver: &HwmonDriverInfo) -> Vec<TempStatus> {
    let mut temp_tasks = vec![];
    moro_local::async_scope!(|scope| {
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Temp {
                continue;
            }
            let temp_task = scope.spawn(async {
                let temp =
                    cc_fs::read_sysfs(driver.path.join(format!("temp{}_input", channel.number)))
                        .await
                        .and_then(check_parsing_32)
                        // hwmon temps are in millidegrees:
                        .map(|degrees| f64::from(degrees) / 1000.0f64)
                        .unwrap_or(0f64);
                TempStatus {
                    name: channel.name.clone(),
                    temp,
                }
            });
            temp_tasks.push(temp_task);
        }
        join_all(temp_tasks).await
    })
    .await
}

/// This is used to remove cpu temps, as we already have repos for that that use `HWMon`.
fn temps_used_by_another_repo(device_name: &str) -> bool {
    CPU_DEVICE_NAMES_ORDERED.contains(&device_name)
}

/// Returns whether the temperature sensor is returning valid and sane values
/// Note: temp sensor readings come in millidegrees by default, i.e. 35.0C == 35000
async fn sensor_is_usable(base_path: &Path, channel_number: &u8) -> bool {
    let temp_path = base_path.join(format!("temp{channel_number}_input"));
    let possible_degrees = cc_fs::read_sysfs(&temp_path)
        .await
        .and_then(check_parsing_32)
        .map(|degrees| f64::from(degrees) / 1000.0f64)
        .inspect_err(|err| {
            debug!(
                "Error reading temperature value from: {} ; {err}",
                temp_path.display()
            );
        })
        .ok();
    if let Some(degrees) = possible_degrees {
        let has_sane_value = (TEMP_SANITY_MIN..=TEMP_SANITY_MAX).contains(&degrees);
        if !has_sane_value {
            debug!(
                "Ignoring temperature sensor at {} as value: {degrees} is outside of \
                usable range",
                temp_path.display()
            );
        }
        return has_sane_value;
    }
    false
}

#[allow(clippy::needless_pass_by_value)]
fn check_parsing_32(content: String) -> Result<i32> {
    match content.trim().parse::<i32>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

/// Reads the contents of the temp?_label file specified by `base_path` and
/// `channel_number`, trims any leading or trailing whitespace, and returns the resulting string if it
/// is not empty.
///
/// Arguments:
///
/// * `base_path`: A `PathBuf` object representing the base path where the file `temp{}_label` is
///   located.
/// * `channel_number`: The `channel_number` parameter is an unsigned 8-bit integer that represents the
///   channel number. It is used to construct the file path for reading the label.
///
/// Returns:
///
/// an `Option<String>`.
async fn get_temp_channel_label(base_path: &Path, channel_number: &u8) -> Option<String> {
    cc_fs::read_txt(base_path.join(format!("temp{channel_number}_label")))
        .await
        .ok()
        .and_then(|label| {
            let temp_label = label.trim();
            if temp_label.is_empty() {
                info!(
                    "Temp label is empty: {}/temp{channel_number}_label",
                    base_path.display()
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
///   (`&u8`).
///
/// Returns:
///
/// * A `String` that represents a unique channel name/ID.
fn get_temp_channel_name(channel_number: u8) -> String {
    format!("temp{channel_number}")
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::Path;

    #[test]
    #[serial]
    fn find_temp_dir_not_exist() {
        cc_fs::test_runtime(async {
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
        });
    }

    #[test]
    #[serial]
    fn find_temp() {
        cc_fs::test_runtime(async {
            // given:
            let test_base_path = Path::new("/tmp/coolercontrol-test/temps_test").to_path_buf();
            cc_fs::create_dir_all(&test_base_path).unwrap();
            cc_fs::write(
                test_base_path.join("temp1_input"),
                b"30000".to_vec(), // temp
            )
            .await
            .unwrap();
            cc_fs::write(
                test_base_path.join("temp1_label"),
                b"Temp 1".to_vec(), // label
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let temps_result = init_temps(&test_base_path, &device_name).await;

            // then:
            // println!("RESULT: {:?}", fans_result);
            cc_fs::remove_dir_all(test_base_path.parent().unwrap()).unwrap();
            assert!(temps_result.is_ok());
            let temps = temps_result.unwrap();
            assert_eq!(temps.len(), 1);
            assert_eq!(temps[0].hwmon_type, HwmonChannelType::Temp);
            assert_eq!(temps[0].name, "temp1");
            assert_eq!(temps[0].label, Some("Temp 1".to_string()));
            assert!(!temps[0].pwm_mode_supported);
            assert_eq!(temps[0].pwm_enable_default, None);
            assert_eq!(temps[0].number, 1);
        });
    }
}
