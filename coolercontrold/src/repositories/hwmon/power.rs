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

use crate::cc_fs;
use crate::device::{ChannelStatus, Watts};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use anyhow::{anyhow, Context, Result};
use log::{info, trace};
use std::io::{Error, ErrorKind};
use std::path::PathBuf;

const POWER_AVERAGE_SUFFIX: &str = "average";
const POWER_INPUT_SUFFIX: &str = "input";
const POWER_LABEL: &str = "power1_label";

/// This initializes the `powerN` hwmon sysfs files. These are mainly used to
/// measure power usage in microWatts for `amdgpu` drivers.
/// See [kernel docs](https://docs.kernel.org/gpu/amdgpu/thermal.html)
pub async fn init_power(base_path: &PathBuf) -> Result<Vec<HwmonChannelInfo>> {
    let mut power_channels = vec![];
    // Prefer Average to Input
    for suffix in [POWER_AVERAGE_SUFFIX, POWER_INPUT_SUFFIX] {
        if let Ok(channel) = find_power(base_path, suffix).await {
            power_channels.push(channel);
            break; // Only one power channel for now (input doesn't help much if average is present)
        }
    }
    trace!("Hwmon Power detected: {power_channels:?} for {base_path:?}");
    Ok(power_channels)
}

/// Find the power channel by name.
async fn find_power(base_path: &PathBuf, suffix: &str) -> Result<HwmonChannelInfo> {
    let power_channel_name = power_channel_name(suffix);
    for entry in cc_fs::read_dir(base_path)? {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if file_name != power_channel_name {
            continue;
        }
        if sensor_is_not_usable(base_path, suffix).await {
            return Err(anyhow!("Power channel {power_channel_name} NOT usable."));
        }
        let label = get_power_channel_label(base_path).await;
        return Ok(HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Power,
            number: 1,
            name: power_channel_name,
            label,
            ..Default::default()
        });
    }
    Err(anyhow!("Power channel not found"))
}

/// Extract the power status
pub async fn extract_power_status(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut power = vec![];
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Power {
            continue;
        }
        // In the Power case, channel.name is the real name of the sysfs file.
        let watts = cc_fs::read_sysfs(driver.path.join(&channel.name))
            .await
            .and_then(check_parsing_64)
            .map(convert_micro_to_watts)
            .unwrap_or_default();
        power.push(ChannelStatus {
            name: channel.name.clone(),
            watts: Some(watts),
            ..Default::default()
        });
    }
    power
}

/// Check if the power channel is usable
async fn sensor_is_not_usable(base_path: &PathBuf, suffix: &str) -> bool {
    cc_fs::read_sysfs(base_path.join(format!("power1_{suffix}")))
        .await
        .and_then(check_parsing_64)
        .map(convert_micro_to_watts)
        .inspect_err(|err| {
            info!("Error reading power value from: {base_path:?}/power1_{suffix} - {err}");
        })
        .is_err()
}

/// Converts microWatts to Watts
fn convert_micro_to_watts(micro_watts: f64) -> Watts {
    (micro_watts / 1_000_000.) as Watts
}

/// Check and parse the content to f64
fn check_parsing_64(content: String) -> Result<f64> {
    match content.trim().parse::<f64>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

/// Read the power label
async fn get_power_channel_label(base_path: &PathBuf) -> Option<String> {
    cc_fs::read_txt(base_path.join(POWER_LABEL))
        .await
        .ok()
        .and_then(|label| {
            let power_label = label.trim();
            if power_label.is_empty() {
                info!("Power label is empty: {base_path:?}/{POWER_LABEL}");
                None
            } else {
                Some(power_label.to_string())
            }
        })
}

/// Create the power channel name
fn power_channel_name(suffix: &str) -> String {
    format!("power1_{suffix}")
/// Tests
#[cfg(test)]
mod tests {
    use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    use super::*;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    struct HwmonFileContext {
        test_base_path: PathBuf,
    }

    fn setup() -> HwmonFileContext {
        let test_base_path =
            Path::new(&(TEST_BASE_PATH_STR.to_string() + &Uuid::new_v4().to_string()))
                .to_path_buf();
        cc_fs::create_dir_all(&test_base_path).unwrap();
        HwmonFileContext { test_base_path }
    }

    fn teardown(ctx: &HwmonFileContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).unwrap();
    }

    #[test]
    #[serial]
    fn init_no_power() {
        cc_fs::test_runtime(async {
            // given:
            let test_base_path = Path::new("/tmp/does_not_exist").to_path_buf();

            // when:
            let power_result = init_power(&test_base_path).await;

            // then:
            assert!(power_result.is_err()); // does not currently error no matter what
            assert!(power_result
                .map_err(|err| err.to_string().contains("No such file or directory"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn init_power_average() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_average"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(
                test_base_path.join("power1_label"),
                b"IHaveTheAveragePower".to_vec(),
            )
            .await
            .unwrap();

            // when:
            let power_result = init_power(test_base_path).await;

            // then:
            teardown(&ctx);
            assert!(power_result.is_ok());
            let powers = power_result.unwrap();
            assert_eq!(1, powers.len());
            assert_eq!(HwmonChannelType::Power, powers[0].hwmon_type);
            assert_eq!("power1_average", &powers[0].name);
            assert_eq!(1, powers[0].number);
            assert_eq!("IHaveTheAveragePower", powers[0].label.as_ref().unwrap());
        });
    }

    #[test]
    #[serial]
    fn init_power_input() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_input"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(
                test_base_path.join("power1_label"),
                b"IHaveTheInputPower".to_vec(),
            )
            .await
            .unwrap();

            // when:
            let power_result = init_power(test_base_path).await;

            // then:
            teardown(&ctx);
            assert!(power_result.is_ok());
            let powers = power_result.unwrap();
            assert_eq!(1, powers.len());
            assert_eq!(HwmonChannelType::Power, powers[0].hwmon_type);
            assert_eq!("power1_input", &powers[0].name);
            assert_eq!(1, powers[0].number);
            assert_eq!("IHaveTheInputPower", powers[0].label.as_ref().unwrap());
        });
    }

    #[test]
    #[serial]
    fn init_power_not_usable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_average"), b"ABC".to_vec()) // wrong format
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power1_label"), b"Power1".to_vec())
                .await
                .unwrap();

            // when:
            let power_result = init_power(test_base_path).await;

            // then:
            teardown(&ctx);
            assert!(power_result.is_ok());
            println!("{power_result:?}");
            assert!(power_result.unwrap().is_empty());
        });
    }

    #[test]
    #[serial]
    fn init_only_power_average() {
        // test that given both powerN_average and powerN_input, that we prefer & use only _average
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_average"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power1_input"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(
                test_base_path.join("power1_label"),
                b"IHaveTheAveragePower".to_vec(),
            )
            .await
            .unwrap();

            // when:
            let power_result = init_power(test_base_path).await;

            // then:
            teardown(&ctx);
            assert!(power_result.is_ok());
            let powers = power_result.unwrap();
            assert_eq!(1, powers.len());
            assert_eq!(HwmonChannelType::Power, powers[0].hwmon_type);
            assert_eq!("power1_average", &powers[0].name);
            assert_eq!(1, powers[0].number);
            assert_eq!("IHaveTheAveragePower", powers[0].label.as_ref().unwrap());
        });
    }

    #[test]
    #[serial]
    fn init_multiple_powers() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_average"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power1_label"), b"Power1".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power2_input"), b"1000000".to_vec())
                .await
                .unwrap();
            // no label for power2
            cc_fs::write(test_base_path.join("power3_average"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power3_input"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power3_label"), b"Power3".to_vec())
                .await
                .unwrap();

            // when:
            let power_result = init_power(test_base_path).await;

            // then:
            teardown(&ctx);
            assert!(power_result.is_ok());
            let powers = power_result.unwrap();
            assert_eq!(3, powers.len());
            assert_eq!(HwmonChannelType::Power, powers[0].hwmon_type);
            assert_eq!(HwmonChannelType::Power, powers[1].hwmon_type);
            assert_eq!(HwmonChannelType::Power, powers[2].hwmon_type);
            assert_eq!("power1_average", &powers[0].name);
            assert_eq!("power2_input", &powers[1].name);
            assert_eq!("power3_average", &powers[2].name);
            assert_eq!(1, powers[0].number);
            assert_eq!(2, powers[1].number);
            assert_eq!(3, powers[2].number);
            assert_eq!("Power1", powers[0].label.as_ref().unwrap());
            assert_eq!(None, powers[1].label);
            assert_eq!("Power3", powers[2].label.as_ref().unwrap());
        });
    }

    #[test]
    #[serial]
    fn extract_power_average_status() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("power1_average"),
                b"36000000".to_vec(), // 36 watts (microwatts)
            )
            .await
            .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Power,
                    name: "power1_average".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };

            // when:
            let power_result = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx);
            assert_eq!(1, power_result.len());
            assert_eq!(Some(36.), power_result[0].watts);
        });
    }

    #[test]
    #[serial]
    fn extract_power_input_status() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("power1_input"),
                b"6123456".to_vec(), // 6.123456 watts (microwatts)
            )
            .await
            .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Power,
                    name: "power1_input".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };

            // when:
            let power_result = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx);
            assert_eq!(1, power_result.len());
            assert_eq!(Some(6.123_456), power_result[0].watts);
        });
    }

    #[test]
    #[serial]
    fn extract_no_power_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                ..Default::default()
            };

            // when:
            let power_result = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx);
            assert_eq!(0, power_result.len());
        });
    }

    #[test]
    #[serial]
    fn extract_status_reading_problem_returns_zero() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Power,
                    name: "power1_input".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };

            // when:
            let power_result = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx);
            assert_eq!(1, power_result.len());
            assert_eq!(Some(0.), power_result[0].watts);
        });
    }
}
