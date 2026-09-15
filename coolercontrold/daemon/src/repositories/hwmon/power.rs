// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::cc_fs;
use crate::device::{ChannelStatus, Watts};
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::probe;
use anyhow::{Context, Result};
use log::{debug, log_enabled, trace, warn};
use regex::Regex;
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const POWER_AVERAGE_SUFFIX: &str = "average";
const PATTERN_POWER_FILE_NUMBER: &str = r"^power(?P<number>\d+)_(average|input)$";
macro_rules! format_power_label { ($($arg:tt)*) => {{ format!("power{}_label", $($arg)*) }}; }

/// This initializes the `powerN` hwmon sysfs files. These are used to
/// measure power usage in microWatts.
/// See [kernel docs](https://docs.kernel.org/gpu/amdgpu/thermal.html)
pub async fn init_power(base_path: &PathBuf, io: &DeviceIo) -> Result<Vec<HwmonChannelInfo>> {
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
            io,
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
    powers.sort_by_key(|c| c.number);
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
    io: &DeviceIo,
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
    if sensor_is_not_usable(base_path, file_name, io).await {
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

/// Reads a set of power channels in one hop. See `temps::read_temp_statuses` for why the per-tick
/// path batches.
pub async fn read_power_statuses(
    driver: &HwmonDriverInfo,
    channels: &[&HwmonChannelInfo],
) -> Vec<Option<ChannelStatus>> {
    if channels.is_empty() {
        return Vec::new();
    }
    // In the Power case, channel.name is the sysfs file name.
    let paths: Vec<Arc<Path>> = channels
        .iter()
        .map(|channel| Arc::from(driver.path.join(&channel.name)))
        .collect();
    let results = driver.io.read_many(&paths).await;
    debug_assert_eq!(results.len(), channels.len());
    channels
        .iter()
        .zip(&paths)
        .zip(results)
        .map(|((channel, path), result)| power_status_from(channel, path, result))
        .collect()
}

/// Turns one raw power read into a `ChannelStatus`. Shared by the batched pass and the
/// single-channel path so both log and discard failures identically.
fn power_status_from(
    channel: &HwmonChannelInfo,
    power_path: &Path,
    result: Result<cc_fs::SysfsValue>,
) -> Option<ChannelStatus> {
    result
        .and_then(check_parsing_64)
        .map(convert_micro_watts_to_watts)
        .inspect(|watts| debug!("hwmon read {}: {watts} W", power_path.display()))
        .inspect_err(|err| {
            if log_enabled!(log::Level::Debug) {
                warn!(
                    "Could not read power value at {} ; {err}",
                    power_path.display()
                );
            }
        })
        .ok()
        .map(|watts| ChannelStatus {
            name: channel.name.clone(),
            watts: Some(watts),
            ..Default::default()
        })
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
    let power_path = driver.path.join(&channel.name);
    let result = driver.io.read_value(&power_path).await;
    power_status_from(channel, &power_path, result)
}

/// Every power channel in one hop, for callers that want an owned `Vec` (the reinit path).
pub async fn extract_power_status(driver: &HwmonDriverInfo) -> (Vec<ChannelStatus>, bool) {
    let channels: Vec<&HwmonChannelInfo> = driver
        .channels
        .iter()
        .filter(|channel| channel.hwmon_type == HwmonChannelType::Power)
        .collect();
    let mut powers = Vec::with_capacity(channels.len());
    let mut any_failure = false;
    for status in read_power_statuses(driver, &channels).await {
        match status {
            Some(status) => powers.push(status),
            None => any_failure = true,
        }
    }
    (powers, any_failure)
}

/// Check if the power channel is usable
async fn sensor_is_not_usable(base_path: &Path, file_name: &str, io: &DeviceIo) -> bool {
    let power_path = base_path.join(file_name);
    // Detection is one-shot, so a transient failure earns a re-read before the channel is lost
    // for the session.
    probe::read_until_ok(&power_path, async || {
        read_power_watts(io, &power_path).await
    })
    .await
    .is_err()
}

/// One power read in watts, error intact. Detection needs the errno to tell a transient failure
/// from a sensor that is simply not readable.
async fn read_power_watts(io: &DeviceIo, power_path: &Path) -> Result<f64> {
    io.read_value(power_path)
        .await
        .and_then(check_parsing_64)
        .map(convert_micro_watts_to_watts)
        .inspect_err(|err| {
            warn!(
                "Error reading power value from: {} - {err}",
                power_path.display()
            );
        })
}

/// Converts microWatts to Watts
fn convert_micro_watts_to_watts(micro_watts: f64) -> Watts {
    (micro_watts / 1_000_000.) as Watts
}

#[allow(clippy::needless_pass_by_value)]
/// Check and parse the content to f64
fn check_parsing_64(value: cc_fs::SysfsValue) -> Result<f64> {
    value.parse()
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
            let power_result = init_power(&test_base_path, &DeviceIo::default()).await;

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
            let power_result = init_power(test_base_path, &DeviceIo::default()).await;

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
            let power_result = init_power(test_base_path, &DeviceIo::default()).await;

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
            let power_result = init_power(test_base_path, &DeviceIo::default()).await;

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
            let power_result = init_power(test_base_path, &DeviceIo::default()).await;

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
            let power_result = init_power(test_base_path, &DeviceIo::default()).await;

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

    // --- extract_power_status: ordering and failures ---

    #[test]
    #[serial]
    fn extract_power_status_preserves_channel_order() {
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
            let (statuses, any_failure) = extract_power_status(&driver_info).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

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
    fn extract_power_status_skips_failed_channels() {
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
            let (statuses, any_failure) = extract_power_status(&driver_info).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(received, vec!["power1_input"]);
        });
    }

    #[test]
    #[serial]
    fn extract_power_status_empty_when_no_channels() {
        // Verifies the sink is never invoked for a driver with no
        // power channels, and any_failure is false.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let driver_info = HwmonDriverInfo {
                path: ctx.test_base_path.clone(),
                ..Default::default()
            };

            let (statuses, any_failure) = extract_power_status(&driver_info).await;

            teardown(&ctx).await;
            assert!(statuses.is_empty());
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
