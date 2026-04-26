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

use crate::cc_fs;
use crate::device::{ChannelStatus, Watts};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use anyhow::{Context, Result};
use log::{trace, warn};
use regex::Regex;
use std::collections::HashMap;
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::{Path, PathBuf};

const POWER_AVERAGE_SUFFIX: &str = "average";
const PATTERN_POWER_FILE_NUMBER: &str = r"^power(?P<number>\d+)_(average|input)$";
macro_rules! format_power_label { ($($arg:tt)*) => {{ format!("power{}_label", $($arg)*) }}; }

/// This initializes the `powerN` hwmon sysfs files. These are used to
/// measure power usage in microWatts.
/// See [kernel docs](https://docs.kernel.org/gpu/amdgpu/thermal.html)
pub async fn init_power(base_path: &PathBuf) -> Result<Vec<HwmonChannelInfo>> {
    let mut powers = vec![];
    let mut preferred_powers = HashMap::new();
    let mut power_inputs = vec![];
    for entry in cc_fs::read_dir(base_path)? {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name
            .to_str()
            .context("File Name should be a UTF-8 String")?;
        insert_power_metrics(
            base_path,
            file_name,
            &mut preferred_powers,
            &mut power_inputs,
        )
        .await?;
    }
    for (channel_number, power_input) in power_inputs {
        if preferred_powers.contains_key(&channel_number) {
            // already contains a preferred power average metric for this channel_number
            continue;
        }
        preferred_powers.insert(channel_number, power_input);
    }
    for (channel_number, power_channel_name) in preferred_powers {
        let label = get_power_channel_label(base_path, channel_number).await;
        powers.push(HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Power,
            number: channel_number,
            name: power_channel_name,
            label,
            ..Default::default()
        });
    }
    powers.sort_by(|c1, c2| c1.number.cmp(&c2.number));
    trace!(
        "Hwmon Power detected: {powers:?} for {}",
        base_path.display()
    );
    Ok(powers)
}
async fn insert_power_metrics(
    base_path: &Path,
    file_name: &str,
    preferred_powers: &mut HashMap<u8, String>,
    power_inputs: &mut Vec<(u8, String)>,
) -> Result<()> {
    let regex_power_file = Regex::new(PATTERN_POWER_FILE_NUMBER)?;
    if regex_power_file.is_match(file_name).not() {
        return Ok(()); // skip if not a power file
    }
    let channel_number: u8 = regex_power_file
        .captures(file_name)
        .context("Power Number should exist")?
        .name("number")
        .context("Number Group should exist")?
        .as_str()
        .parse()?;
    if sensor_is_not_usable(base_path, file_name).await {
        return Ok(()); // skip if pwm file isn't readable
    }
    if file_name.ends_with(POWER_AVERAGE_SUFFIX) {
        // average metric is preferred to input and no need to display both
        preferred_powers.insert(channel_number, file_name.to_string());
    } else {
        power_inputs.push((channel_number, file_name.to_string()));
    }
    Ok(())
}

/// Streams power statuses to `sink` one channel at a time as each
/// read completes, returning whether any read failed. Failed reads
/// are omitted so the upstream cache keeps the last-known-good value
/// until the failsafe threshold merges in the proper failsafe watts.
/// Callers that want a buffered `Vec` should use `extract_power_status`.
pub async fn stream_power_status<F>(driver: &HwmonDriverInfo, mut sink: F) -> bool
where
    F: FnMut(ChannelStatus),
{
    let mut any_failure = false;
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Power {
            continue;
        }
        match read_one_power_status(driver, channel).await {
            Some(status) => sink(status),
            None => any_failure = true,
        }
    }
    any_failure
}

/// Reads the power-input file for one channel and returns the
/// resulting `ChannelStatus`, or `None` if the read failed.
/// Pulled out so the preload loop can acquire the device permit
/// per channel and avoid holding it across the whole device's
/// power-channel set.
pub async fn read_one_power_status(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
) -> Option<ChannelStatus> {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Power);
    // In the Power case, channel.name is the real name of the sysfs file.
    cc_fs::read_sysfs(driver.path.join(&channel.name))
        .await
        .and_then(check_parsing_64)
        .map(convert_micro_watts_to_watts)
        .ok()
        .map(|watts| ChannelStatus {
            name: channel.name.clone(),
            watts: Some(watts),
            ..Default::default()
        })
}

/// Buffered wrapper over `stream_power_status` for callers that want
/// an owned `Vec<ChannelStatus>` (for example, the reinit path).
pub async fn extract_power_status(driver: &HwmonDriverInfo) -> (Vec<ChannelStatus>, bool) {
    let power_channel_count = driver
        .channels
        .iter()
        .filter(|c| c.hwmon_type == HwmonChannelType::Power)
        .count();
    let mut powers = Vec::with_capacity(power_channel_count);
    let any_failure = stream_power_status(driver, |status| powers.push(status)).await;
    (powers, any_failure)
}

/// Check if the power channel is usable
async fn sensor_is_not_usable(base_path: &Path, file_name: &str) -> bool {
    cc_fs::read_sysfs(base_path.join(file_name))
        .await
        .and_then(check_parsing_64)
        .map(convert_micro_watts_to_watts)
        .inspect_err(|err| {
            warn!(
                "Error reading power value from: {}/{file_name} - {err}",
                base_path.display()
            );
        })
        .is_err()
}

/// Converts microWatts to Watts
fn convert_micro_watts_to_watts(micro_watts: f64) -> Watts {
    (micro_watts / 1_000_000.) as Watts
}

#[allow(clippy::needless_pass_by_value)]
/// Check and parse the content to f64
fn check_parsing_64(content: String) -> Result<f64> {
    match content.trim().parse::<f64>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

/// Read the power label
async fn get_power_channel_label(base_path: &Path, channel_number: u8) -> Option<String> {
    cc_fs::read_txt(base_path.join(format_power_label!(channel_number)))
        .await
        .ok()
        .and_then(|label| {
            let power_label = label.trim();
            if power_label.is_empty() {
                warn!(
                    "Power label is empty: {}/power{channel_number}_label",
                    base_path.display()
                );
                None
            } else {
                Some(power_label.to_string())
            }
        })
}

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

    async fn setup() -> HwmonFileContext {
        let test_base_path =
            Path::new(&(TEST_BASE_PATH_STR.to_string() + &Uuid::new_v4().to_string()))
                .to_path_buf();
        cc_fs::create_dir_all(&test_base_path).await.unwrap();
        HwmonFileContext { test_base_path }
    }

    async fn teardown(ctx: &HwmonFileContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).await.unwrap();
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
            let ctx = setup().await;
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
            teardown(&ctx).await;
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
            let ctx = setup().await;
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
            teardown(&ctx).await;
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
            let ctx = setup().await;
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
            teardown(&ctx).await;
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
            let ctx = setup().await;
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
            teardown(&ctx).await;
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
            let ctx = setup().await;
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
            teardown(&ctx).await;
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
            let ctx = setup().await;
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
            let (power_result, any_failure) = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(1, power_result.len());
            assert_eq!(Some(36.), power_result[0].watts);
        });
    }

    #[test]
    #[serial]
    fn extract_power_input_status() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
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
            let (power_result, any_failure) = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(1, power_result.len());
            assert_eq!(Some(6.123_456), power_result[0].watts);
        });
    }

    #[test]
    #[serial]
    fn extract_no_power_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                ..Default::default()
            };

            // when:
            let (power_result, any_failure) = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(0, power_result.len());
        });
    }

    #[test]
    #[serial]
    fn extract_status_skips_failed_reads_and_signals_failure() {
        // Verifies that when the sysfs file is missing, the channel is
        // omitted from the result and the failure indicator is set.
        // Fabricating a 0.0 watts entry would lie to downstream; the
        // upstream failsafe merge handles the missing entry.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: power channel exists but sysfs file does not.
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
            let (power_result, any_failure) = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(0, power_result.len());
        });
    }

    // --- stream_power_status: sink contract ---

    #[test]
    #[serial]
    fn stream_power_status_invokes_sink_in_channel_order() {
        // Verifies the streaming variant invokes the sink once per
        // successful channel in the order channels are defined.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: three power channels, all readable.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_input"), b"1000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power2_input"), b"2000000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("power3_input"), b"3000000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power1_input".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power2_input".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power3_input".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let mut received: Vec<String> = Vec::new();
            let any_failure =
                stream_power_status(&driver_info, |status| received.push(status.name)).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(
                received,
                vec!["power1_input", "power2_input", "power3_input"]
            );
        });
    }

    #[test]
    #[serial]
    fn stream_power_status_skips_sink_on_failure() {
        // Verifies the sink is not invoked for a channel whose sysfs
        // read fails; any_failure is set and the successful channel
        // alone is streamed.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: power1 readable, power2 missing.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("power1_input"), b"4000000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power1_input".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power2_input".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let mut received: Vec<String> = Vec::new();
            let any_failure =
                stream_power_status(&driver_info, |status| received.push(status.name)).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(received, vec!["power1_input"]);
        });
    }

    #[test]
    #[serial]
    fn stream_power_status_no_invocation_when_no_channels() {
        // Verifies the sink is never invoked for a driver with no
        // power channels, and any_failure is false.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let driver_info = HwmonDriverInfo {
                path: ctx.test_base_path.clone(),
                ..Default::default()
            };

            let mut invocations: u32 = 0;
            let any_failure = stream_power_status(&driver_info, |_| invocations += 1).await;

            teardown(&ctx).await;
            assert_eq!(invocations, 0);
            assert!(any_failure.not());
        });
    }

    #[test]
    #[serial]
    fn extract_status_partial_failure_skips_only_failing_channels() {
        // Verifies that when one power channel reads successfully and
        // another fails, only the successful one is returned and
        // any_failure is set.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: two power channels, one readable, one not.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("power1_input"),
                b"5000000".to_vec(), // 5.0 W in microwatts
            )
            .await
            .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power1_input".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Power,
                        name: "power2_input".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let (power_result, any_failure) = extract_power_status(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(1, power_result.len());
            assert_eq!("power1_input", power_result[0].name);
            assert_eq!(Some(5.0), power_result[0].watts);
        });
    }

    use std::ops::Not;
}
