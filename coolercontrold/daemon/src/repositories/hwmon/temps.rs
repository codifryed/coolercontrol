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

use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cc_fs;
use crate::device::TempStatus;
use crate::repositories::cpu_repo::CPU_DEVICE_NAMES_ORDERED;
use crate::repositories::hwmon::devices;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use anyhow::{Context, Result};
use futures_util::future::join_all;
use log::{debug, info, log_enabled, trace, warn};
use nix::libc;
use regex::Regex;

const PATTERN_TEMP_INPUT_NUMBER: &str = r"^temp(?P<number>\d+)_input$";
const TEMP_SANITY_MIN: f64 = 0.0;
const TEMP_SANITY_MAX: f64 = 140.0;
macro_rules! format_temp_input { ($($arg:tt)*) => {{ format!("temp{}_input", $($arg)*) }}; }
static THINKPAD_GPU_ENXIO_LOGGED: AtomicBool = AtomicBool::new(false);

/// Initialize all applicable temp sensors
pub async fn init_temps(base_path: &Path, device_name: &str) -> Result<Vec<HwmonChannelInfo>> {
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
            if sensor_is_usable(base_path, &channel_number, device_name)
                .await
                .not()
            {
                continue;
            }
            let channel_name = get_temp_channel_name(channel_number);
            let label = get_temp_channel_label(base_path, &channel_number).await;
            temps.push(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Temp,
                number: channel_number,
                name: channel_name,
                label,
                temp_path: Some(base_path.join(format_temp_input!(channel_number))),
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

/// Streams temp statuses to `sink` one channel at a time as each
/// read completes, returning whether any read failed. Failed reads
/// are omitted so the upstream cache keeps the last-known-good value
/// until the failsafe threshold merges in `MISSING_TEMP_FAILSAFE`.
/// Fabricating a value on failure would lie to downstream controllers.
/// Callers that want a buffered `Vec` should use `extract_temp_statuses`.
pub async fn stream_temp_statuses<F>(driver: &HwmonDriverInfo, mut sink: F) -> bool
where
    F: FnMut(TempStatus),
{
    let mut any_failure = false;
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        match read_one_temp_status(driver, channel).await {
            Some(status) => sink(status),
            None => any_failure = true,
        }
    }
    any_failure
}

/// Reads the temp file for one channel and returns the resulting
/// `TempStatus`, or `None` if the read failed. Pulled out so the
/// preload loop can acquire the device permit per channel and
/// avoid holding it across the whole device's temp set.
pub async fn read_one_temp_status(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
) -> Option<TempStatus> {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Temp);
    let temp_path = match channel.temp_path.as_ref() {
        Some(path) => path,
        None => &driver.path.join(format_temp_input!(channel.number)),
    };
    match cc_fs::read_sysfs(temp_path)
        .await
        .and_then(check_parsing_32)
        // hwmon temps are in millidegrees:
        .map(|degrees| f64::from(degrees) / 1000.0f64)
    {
        Ok(temp) => {
            debug!("hwmon read {}: {temp} C", temp_path.display());
            Some(TempStatus {
                name: channel.name.clone(),
                temp,
            })
        }
        Err(err) => {
            if is_thinkpad_gpu_powerdown(&driver.name, &err) {
                log_thinkpad_gpu_powerdown_once(&channel.name, channel.label.as_deref(), temp_path);
                return Some(TempStatus {
                    name: channel.name.clone(),
                    temp: 0.0,
                });
            }
            if log_enabled!(log::Level::Debug) {
                warn!(
                    "Could not read temp value at {} ; {err}",
                    temp_path.display()
                );
            }
            None
        }
    }
}

/// Buffered wrapper over `stream_temp_statuses` for callers that want
/// an owned `Vec<TempStatus>` (for example, the reinit path).
pub async fn extract_temp_statuses(driver: &HwmonDriverInfo) -> (Vec<TempStatus>, bool) {
    let temp_channel_count = driver
        .channels
        .iter()
        .filter(|c| c.hwmon_type == HwmonChannelType::Temp)
        .count();
    let mut temps = Vec::with_capacity(temp_channel_count);
    let any_failure = stream_temp_statuses(driver, |status| temps.push(status)).await;
    (temps, any_failure)
}

#[allow(dead_code)]
/// Concurrent version of `extract_temp_statuses`. Failed reads are
/// omitted rather than pushed with a fabricated value.
pub async fn extract_temp_statuses_concurrently(
    driver: &HwmonDriverInfo,
) -> (Vec<TempStatus>, bool) {
    let mut temp_tasks = vec![];
    let results = moro_local::async_scope!(|scope| {
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Temp {
                continue;
            }
            let temp_task = scope.spawn(async {
                let result =
                    cc_fs::read_sysfs(driver.path.join(format_temp_input!(channel.number)))
                        .await
                        .and_then(check_parsing_32)
                        // hwmon temps are in millidegrees:
                        .map(|degrees| f64::from(degrees) / 1000.0f64);
                result.map(|temp| TempStatus {
                    name: channel.name.clone(),
                    temp,
                })
            });
            temp_tasks.push(temp_task);
        }
        join_all(temp_tasks).await
    })
    .await;
    let mut temps = Vec::with_capacity(results.len());
    let mut any_failure = false;
    for result in results {
        match result {
            Ok(temp) => temps.push(temp),
            Err(_) => any_failure = true,
        }
    }
    (temps, any_failure)
}

/// This is used to remove cpu temps, as we already have repos for that that use `HWMon`.
fn temps_used_by_another_repo(device_name: &str) -> bool {
    CPU_DEVICE_NAMES_ORDERED.contains(&device_name)
}

/// Returns whether the temperature sensor is returning valid and sane values
/// Note: temp sensor readings come in millidegrees by default, i.e. 35.0C == 35000
async fn sensor_is_usable(base_path: &Path, channel_number: &u8, driver_name: &str) -> bool {
    let temp_path = base_path.join(format_temp_input!(channel_number));
    match cc_fs::read_sysfs(&temp_path)
        .await
        .and_then(check_parsing_32)
        .map(|degrees| f64::from(degrees) / 1000.0f64)
    {
        Ok(degrees) => {
            let has_sane_value = (TEMP_SANITY_MIN..=TEMP_SANITY_MAX).contains(&degrees);
            if !has_sane_value {
                debug!(
                    "Ignoring temperature sensor at {} as value: {degrees} is outside of \
                    usable range",
                    temp_path.display()
                );
            }
            has_sane_value
        }
        Err(err) => {
            if is_thinkpad_gpu_powerdown(driver_name, &err) {
                // dGPU is currently powered down. Register the channel
                // anyway; the runtime read path substitutes 0.0 C until
                // the GPU powers up and the sysfs starts responding.
                let channel_name = get_temp_channel_name(*channel_number);
                log_thinkpad_gpu_powerdown_once(&channel_name, None, &temp_path);
                return true;
            }
            debug!(
                "Error reading temperature value from: {} ; {err}",
                temp_path.display()
            );
            false
        }
    }
}

/// Returns true when `err` is the well-known `thinkpad_hwmon` ENXIO that
/// fires on temp reads when the dedicated GPU is powered down. The temp
/// sysfs starts responding again on its own when the GPU powers back up.
/// Integrated GPUs do not produce this error.
///
/// This is a deliberate exception to the daemon's "no sentinel on read
/// failure" rule. The cause is known and the substitute value (0.0 C) is
/// semantically meaningful, not a fabricated stand-in for missing data.
fn is_thinkpad_gpu_powerdown(driver_name: &str, err: &anyhow::Error) -> bool {
    if driver_name != devices::DEVICE_NAME_THINK_PAD {
        return false;
    }
    err.downcast_ref::<Error>()
        .and_then(Error::raw_os_error)
        .is_some_and(|errno| errno == libc::ENXIO)
}

/// Emits one info-level log on the first thinkpad dGPU power-down ENXIO
/// observed in this process. Subsequent ENXIO reads are silent so the
/// log doesn't fill up while the GPU stays off. The flag is never reset,
/// so power cycles within the same daemon run are not re-announced.
fn log_thinkpad_gpu_powerdown_once(
    channel_name: &str,
    channel_label: Option<&str>,
    temp_path: &Path,
) {
    if THINKPAD_GPU_ENXIO_LOGGED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        let label = channel_label.unwrap_or("-");
        info!(
            "ThinkPad hwmon temp channel '{channel_name}' (label={label}) at {} \
             returned ENXIO. Treating as 0.0 C; expected when no dedicated GPU \
             is present or it is powered down.",
            temp_path.display()
        );
    }
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
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

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
            cc_fs::create_dir_all(&test_base_path).await.unwrap();
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
            cc_fs::remove_dir_all(test_base_path.parent().unwrap())
                .await
                .unwrap();
            assert!(temps_result.is_ok());
            let temps = temps_result.unwrap();
            assert_eq!(temps.len(), 1);
            assert_eq!(temps[0].hwmon_type, HwmonChannelType::Temp);
            assert_eq!(temps[0].name, "temp1");
            assert_eq!(temps[0].label, Some("Temp 1".to_string()));
            assert_eq!(temps[0].pwm_enable_default, None);
            assert_eq!(temps[0].number, 1);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_all_channels_succeed() {
        // Verifies a successful read yields one entry per channel and
        // clears the failure indicator.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: two temp channels, both readable.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("temp1_input"), b"35000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("temp2_input"), b"42000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 1,
                        name: "temp1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 2,
                        name: "temp2".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(temps.len(), 2);
            assert_eq!(temps[0].name, "temp1");
            assert!((temps[0].temp - 35.0).abs() < f64::EPSILON);
            assert_eq!(temps[1].name, "temp2");
            assert!((temps[1].temp - 42.0).abs() < f64::EPSILON);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_skips_failed_reads_and_signals_failure() {
        // Verifies a missing sysfs file results in the channel being
        // omitted and the failure indicator being set. Fabricating 0.0
        // would be read by downstream as "very cold, drop fan duty".
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: single temp channel but no sysfs file.
            let test_base_path = &ctx.test_base_path;
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Temp,
                    number: 1,
                    name: "temp1".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(temps.len(), 0);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_partial_failure_skips_only_failing_channels() {
        // Verifies working sensors keep serving real values while the
        // failing sensor is simply absent from the result.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: two temp channels, only the first one readable.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("temp1_input"), b"50000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 1,
                        name: "temp1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 2,
                        name: "temp2".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(temps.len(), 1);
            assert_eq!(temps[0].name, "temp1");
            assert!((temps[0].temp - 50.0).abs() < f64::EPSILON);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_all_fail_returns_empty() {
        // Verifies a device with all failing reads returns an empty vec
        // rather than one fabricated entry per channel.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: two temp channels, neither readable.
            let test_base_path = &ctx.test_base_path;
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 1,
                        name: "temp1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 2,
                        name: "temp2".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(temps.len(), 0);
        });
    }

    // --- stream_temp_statuses: sink contract ---

    #[test]
    #[serial]
    fn stream_temp_statuses_invokes_sink_in_channel_order() {
        // Verifies the streaming variant invokes the sink once per
        // successful temp channel in channel-number order.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let test_base_path = &ctx.test_base_path;
            for number in 1u8..=3 {
                cc_fs::write(
                    test_base_path.join(format!("temp{number}_input")),
                    b"40000".to_vec(),
                )
                .await
                .unwrap();
            }
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 1,
                        name: "temp1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 2,
                        name: "temp2".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 3,
                        name: "temp3".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let mut received: Vec<String> = Vec::new();
            let any_failure =
                stream_temp_statuses(&driver_info, |status| received.push(status.name)).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(received, vec!["temp1", "temp2", "temp3"]);
        });
    }

    #[test]
    #[serial]
    fn stream_temp_statuses_skips_sink_on_failure() {
        // Verifies the sink is not invoked for a temp channel whose
        // sysfs file is missing, and any_failure is set.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("temp1_input"), b"30000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 1,
                        name: "temp1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 2,
                        name: "temp2".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let mut received: Vec<String> = Vec::new();
            let any_failure =
                stream_temp_statuses(&driver_info, |status| received.push(status.name)).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(received, vec!["temp1"]);
        });
    }

    #[test]
    #[serial]
    fn stream_temp_statuses_no_invocation_when_no_channels() {
        // Verifies the sink is never invoked when there are no temp
        // channels, and any_failure is false.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let driver_info = HwmonDriverInfo {
                path: ctx.test_base_path.clone(),
                ..Default::default()
            };

            let mut invocations: u32 = 0;
            let any_failure = stream_temp_statuses(&driver_info, |_| invocations += 1).await;

            teardown(&ctx).await;
            assert_eq!(invocations, 0);
            assert!(any_failure.not());
        });
    }

    // --- is_thinkpad_gpu_powerdown classifier ---

    #[test]
    fn is_thinkpad_gpu_powerdown_true_for_thinkpad_enxio() {
        // Verifies the canonical case: thinkpad driver name + io::Error
        // carrying ENXIO is recognized as the dGPU-off signal.
        let err: anyhow::Error = Error::from_raw_os_error(libc::ENXIO).into();
        assert!(is_thinkpad_gpu_powerdown(
            devices::DEVICE_NAME_THINK_PAD,
            &err,
        ));
    }

    #[test]
    fn is_thinkpad_gpu_powerdown_false_for_other_driver() {
        // Verifies the carve-out is gated on driver name. The same
        // ENXIO from a non-thinkpad driver must not be silently
        // converted to 0.0 C; it stays a real failure.
        let err: anyhow::Error = Error::from_raw_os_error(libc::ENXIO).into();
        assert!(is_thinkpad_gpu_powerdown("k10temp", &err).not());
        assert!(is_thinkpad_gpu_powerdown("nct6798", &err).not());
        assert!(is_thinkpad_gpu_powerdown("", &err).not());
    }

    #[test]
    fn is_thinkpad_gpu_powerdown_false_for_other_errno() {
        // Verifies the classifier is errno-specific. Other errnos from
        // thinkpad must propagate as real failures.
        let enodata: anyhow::Error = Error::from_raw_os_error(libc::ENODATA).into();
        let eopnotsupp: anyhow::Error = Error::from_raw_os_error(libc::EOPNOTSUPP).into();
        let eio: anyhow::Error = Error::from_raw_os_error(libc::EIO).into();
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &enodata).not());
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &eopnotsupp).not());
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &eio).not());
    }

    #[test]
    fn is_thinkpad_gpu_powerdown_false_for_parse_error() {
        // Verifies a non-OS error (e.g. the InvalidData produced by
        // check_parsing_32 on garbage) is not mistaken for ENXIO. Those
        // errors carry no raw_os_error.
        let parse_err = check_parsing_32("not a number".to_string()).unwrap_err();
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &parse_err).not());
    }

    #[test]
    fn is_thinkpad_gpu_powerdown_false_for_non_io_error() {
        // Verifies an arbitrary anyhow error (no io::Error in the chain)
        // is rejected, so we never substitute 0 on unrelated failures.
        let err = anyhow::anyhow!("something else went wrong");
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &err).not());
    }

    #[test]
    #[serial]
    fn extract_temp_ignores_non_temp_channels() {
        // Verifies non-Temp channels are ignored even if named similarly.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: one Temp and one non-Temp channel.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("temp1_input"), b"30000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        number: 1,
                        name: "temp1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Fan,
                        number: 1,
                        name: "fan1".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(temps.len(), 1);
            assert_eq!(temps[0].name, "temp1");
        });
    }
}
