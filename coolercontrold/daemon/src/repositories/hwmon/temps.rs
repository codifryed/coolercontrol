// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use crate::repositories::hwmon::device_io::{pair_with_results, DeviceIo};
use std::io::Error;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::cc_fs::{self, SysfsValue};
use crate::device::{Temp, TempStatus};
use crate::repositories::cpu::CPU_DEVICE_NAMES_ORDERED;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::probe;
use crate::repositories::hwmon::{devices, fans, power};
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
static TAS2764_ENODATA_LOGGED: AtomicBool = AtomicBool::new(false);
const DEVICE_NAME_TAS2764: &str = "tas2764";

/// Initialize all applicable temp sensors
pub async fn init_temps(
    base_path: &Path,
    device_name: &str,
    io: &DeviceIo,
) -> Result<Vec<HwmonChannelInfo>> {
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
            if sensor_is_usable(base_path, &channel_number, device_name, io)
                .await
                .not()
            {
                continue;
            }
            let channel_name = get_temp_channel_name(channel_number);
            let label = get_temp_channel_label(base_path, &channel_number, io).await;
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
    temps.sort_by_key(|t| t.number);
    trace!(
        "Hwmon Temps detected: {temps:?} for {}",
        base_path.display()
    );
    Ok(temps)
}

/// Reads a set of temp channels in one hop.
///
/// The device permit is held across the whole pass anyway, so the reads were going to happen back
/// to back; one round trip is cheaper than one per channel. `on_temp` is called once per channel,
/// in order, with `None` for a channel that has nothing usable. A callback with the channel rather
/// than a returned `Vec` of statuses, because the per-tick caller writes each value into the status
/// it keeps, under the channel's name, as it comes.
pub async fn read_temps<'a>(
    driver: &'a HwmonDriverInfo,
    channels: impl Iterator<Item = &'a HwmonChannelInfo> + Clone,
    mut on_temp: impl FnMut(&'a HwmonChannelInfo, Option<Temp>),
) {
    debug_assert!(
        channels
            .clone()
            .all(|channel| channel.read_slot.value.is_some()),
        "every temp channel needs a read slot; was the registry installed?"
    );
    if cfg!(debug_assertions) {
        for channel in channels.clone() {
            if let Some(slot) = channel.read_slot.value {
                driver
                    .io
                    .debug_assert_slot(slot, &temp_path_for(driver, channel));
            }
        }
    }
    let slots = channels
        .clone()
        .filter_map(|channel| channel.read_slot.value);
    let mut batch = driver.io.read_many(slots).await;
    // Paired, not zipped: an unslotted channel would take the next sensor's reading, and drive the
    // wrong fan curve.
    let has_slot = |channel: &&HwmonChannelInfo| channel.read_slot.value.is_some();
    for (channel, result) in pair_with_results(channels, has_slot, batch.take_all()) {
        on_temp(
            channel,
            result.and_then(|result| temp_from(driver, channel, result)),
        );
    }
}

/// Where one channel's temp value lives.
fn temp_path_for(driver: &HwmonDriverInfo, channel: &HwmonChannelInfo) -> PathBuf {
    channel
        .temp_path
        .clone()
        .unwrap_or_else(|| driver.path.join(format_temp_input!(channel.number)))
}

/// Reads the temp file for one channel and returns its temp, or `None` if the read failed. Pulled
/// out so the preload loop can acquire the device permit per channel and avoid holding it across
/// the whole device's temp set.
pub async fn read_one_temp(driver: &HwmonDriverInfo, channel: &HwmonChannelInfo) -> Option<Temp> {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Temp);
    let result = driver
        .io
        .read_one(channel.read_slot.value, || temp_path_for(driver, channel))
        .await;
    temp_from(driver, channel, result)
}

/// Turns one raw read into a temp, or `None` when the channel has nothing usable to report. Shared
/// by the batched pass and the single-channel path so both interpret a failure, and the `ThinkPad`
/// powered-down carve-out, identically.
fn temp_from(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
    result: Result<SysfsValue>,
) -> Option<Temp> {
    // Only the log arms need it, and `log::debug!` evaluates its arguments only when enabled, so
    // the per-tick pass never builds a path.
    let temp_path = || temp_path_for(driver, channel);
    match result
        .and_then(check_parsing_32)
        // hwmon temps are in millidegrees:
        .map(|degrees| f64::from(degrees) / 1000.0f64)
    {
        Ok(temp) => {
            debug!("hwmon read {}: {temp} C", temp_path().display());
            Some(temp)
        }
        Err(err) => {
            if is_thinkpad_gpu_powerdown(&driver.name, &err) {
                log_thinkpad_gpu_powerdown_once(
                    &channel.name,
                    channel.label.as_deref(),
                    &temp_path(),
                );
                return Some(0.0);
            }
            if is_tas2764_unsampled(&driver.name, &err) {
                log_tas2764_unsampled_once(&temp_path());
                return Some(0.0);
            }
            if log_enabled!(log::Level::Debug) {
                warn!(
                    "Could not read temp value at {} ; {err}",
                    temp_path().display()
                );
            }
            None
        }
    }
}

/// Every temp channel in one hop, for callers that want an owned `Vec` (the reinit path).
pub async fn extract_temp_statuses(driver: &HwmonDriverInfo) -> (Vec<TempStatus>, bool) {
    let channels = driver
        .channels
        .iter()
        .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp);
    let mut temps = Vec::with_capacity(channels.clone().count());
    let mut any_failure = false;
    read_temps(driver, channels, |channel, temp| match temp {
        Some(temp) => temps.push(TempStatus {
            name: channel.name.clone(),
            temp,
        }),
        None => any_failure = true,
    })
    .await;
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
                let result = driver
                    .io
                    .read_value(&driver.path.join(format_temp_input!(channel.number)))
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

/// Whether a chip has nothing for the hwmon repository: its temps belong to the CPU repository,
/// and its listing names no attribute that fan or power detection reads.
///
/// Decided from the listing alone, so such a chip never gets a worker that detection would only
/// drop again: the CPU repository starts the chip's one worker. zenpower is the CPU chip this
/// keeps, for its power channels.
pub fn only_temps_of_another_repo(base_path: &Path, device_name: &str) -> bool {
    if temps_used_by_another_repo(device_name).not() {
        return false;
    }
    let Ok(entries) = cc_fs::read_dir(base_path) else {
        // Detection reports an unreadable directory, as it always has.
        return false;
    };
    let offers_channels = entries.filter_map(Result::ok).any(|entry| {
        entry.file_name().to_str().is_some_and(|file_name| {
            fans::is_fan_attribute(file_name) || power::is_power_attribute(file_name)
        })
    });
    offers_channels.not()
}

/// Returns whether the temperature sensor is returning valid and sane values
/// Note: temp sensor readings come in millidegrees by default, i.e. 35.0C == 35000
async fn sensor_is_usable(
    base_path: &Path,
    channel_number: &u8,
    driver_name: &str,
    io: &DeviceIo,
) -> bool {
    let temp_path = base_path.join(format_temp_input!(channel_number));
    // Re-probe a transient failure before giving the channel up for the session. The sanity-range
    // verdict below is not a read failure and is deliberately left as a single shot.
    match probe::read_until_ok(&temp_path, async || read_temp_degrees(io, &temp_path).await).await {
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
            if is_tas2764_unsampled(driver_name, &err) {
                log_tas2764_unsampled_once(&temp_path);
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

/// One temp read in degrees, error intact. Detection needs the errno to tell a transient failure
/// from a sensor that is simply not readable.
async fn read_temp_degrees(io: &DeviceIo, temp_path: &Path) -> Result<f64> {
    io.read_value(temp_path)
        .await
        .and_then(check_parsing_32)
        // hwmon temps are in millidegrees:
        .map(|degrees| f64::from(degrees) / 1000.0f64)
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

fn is_tas2764_unsampled(driver_name: &str, err: &anyhow::Error) -> bool {
    if driver_name != DEVICE_NAME_TAS2764 {
        return false;
    }
    err.downcast_ref::<Error>()
        .and_then(Error::raw_os_error)
        .is_some_and(|errno| errno == libc::ENODATA)
}

fn log_tas2764_unsampled_once(temp_path: &Path) {
    if TAS2764_ENODATA_LOGGED
        .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
        .is_ok()
    {
        info!(
            "TAS2764 speaker amplifier temp at {} has no sample since the amplifier powered up. \
             Treating as 0.0 C until audio plays.",
            temp_path.display()
        );
    }
}

#[allow(clippy::needless_pass_by_value)]
fn check_parsing_32(value: cc_fs::SysfsValue) -> Result<i32> {
    value.parse()
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
async fn get_temp_channel_label(
    base_path: &Path,
    channel_number: &u8,
    io: &DeviceIo,
) -> Option<String> {
    // On the device's thread: the label comes from the driver, which may have to ask the hardware.
    io.read_text(&base_path.join(format!("temp{channel_number}_label")))
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
    #[cfg(feature = "gated-tests")]
    use std::path::Path;
    use std::path::PathBuf;

    struct HwmonFileContext {
        /// Owns the fixture directory, which is removed when the context drops.
        _dir: tempfile::TempDir,
        test_base_path: PathBuf,
    }

    fn setup() -> HwmonFileContext {
        let dir = tempfile::tempdir().unwrap();
        let test_base_path = dir.path().to_path_buf();
        HwmonFileContext {
            _dir: dir,
            test_base_path,
        }
    }

    #[test]
    #[serial]
    #[cfg(feature = "gated-tests")]
    fn find_temp_dir_not_exist() {
        cc_fs::test_runtime(async {
            // given:
            let test_base_path = Path::new("/tmp/does_not_exist").to_path_buf();
            let device_name = "Test Driver".to_string();

            // when:
            let temps_result =
                init_temps(&test_base_path, &device_name, &DeviceIo::default()).await;

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
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
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
            let temps_result = init_temps(test_base_path, &device_name, &DeviceIo::default()).await;

            // then:
            // println!("RESULT: {:?}", fans_result);
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
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            assert!(any_failure.not());
            assert_eq!(temps.len(), 2);
            assert_eq!(temps[0].name, "temp1");
            assert!((temps[0].temp - 35.0).abs() < f64::EPSILON);
            assert_eq!(temps[1].name, "temp2");
            assert!((temps[1].temp - 42.0).abs() < f64::EPSILON);
        });
    }

    /// Goal: the tick path must read through the driver's descriptor cache, so repeated ticks
    /// stop paying for a reopen, and must still report the current value. Method: extract twice
    /// over two channels, asserting one held descriptor per channel and no growth on the second
    /// pass, then rewrite both files and extract again to confirm the values are live.
    #[test]
    #[serial]
    fn extract_temp_reuses_descriptors_across_ticks() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            let (first_temps, _) = extract_temp_statuses(&driver_info).await;
            let held_after_first_tick = driver_info.io.descriptor_count().await;
            let (second_temps, _) = extract_temp_statuses(&driver_info).await;
            cc_fs::write(test_base_path.join("temp1_input"), b"55000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("temp2_input"), b"60000".to_vec())
                .await
                .unwrap();
            let (third_temps, _) = extract_temp_statuses(&driver_info).await;

            // Both temp files stay open in the fd cache across ticks.
            let expected_held = 2;
            assert_eq!(held_after_first_tick, expected_held);
            assert_eq!(
                driver_info.io.descriptor_count().await,
                expected_held,
                "descriptors grew"
            );
            assert!((first_temps[0].temp - 35.0).abs() < f64::EPSILON);
            assert!((second_temps[1].temp - 42.0).abs() < f64::EPSILON);
            assert!((third_temps[0].temp - 55.0).abs() < f64::EPSILON);
            assert!((third_temps[1].temp - 60.0).abs() < f64::EPSILON);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_skips_failed_reads_and_signals_failure() {
        // Verifies a missing sysfs file results in the channel being
        // omitted and the failure indicator being set. Fabricating 0.0
        // would be read by downstream as "very cold, drop fan duty".
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
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
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
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
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            assert!(any_failure);
            assert_eq!(temps.len(), 0);
        });
    }

    // --- extract_temp_statuses: ordering and failures ---
    #[test]
    #[serial]
    fn extract_temp_statuses_preserves_channel_order() {
        // Verifies the streaming variant invokes the sink once per
        // successful temp channel in channel-number order.
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (statuses, any_failure) = extract_temp_statuses(&driver_info).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            assert!(any_failure.not());
            assert_eq!(received, vec!["temp1", "temp2", "temp3"]);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_statuses_skips_failed_channels() {
        // Verifies the sink is not invoked for a temp channel whose
        // sysfs file is missing, and any_failure is set.
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (statuses, any_failure) = extract_temp_statuses(&driver_info).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            assert!(any_failure);
            assert_eq!(received, vec!["temp1"]);
        });
    }

    #[test]
    #[serial]
    fn extract_temp_statuses_empty_when_no_channels() {
        // Verifies the sink is never invoked when there are no temp
        // channels, and any_failure is false.
        cc_fs::test_runtime(async {
            let ctx = setup();
            let driver_info = HwmonDriverInfo {
                path: ctx.test_base_path.clone(),
                ..Default::default()
            }
            .with_read_registry()
            .await;

            let (statuses, any_failure) = extract_temp_statuses(&driver_info).await;

            assert!(statuses.is_empty());
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
        let parse_err =
            check_parsing_32(cc_fs::SysfsValue::from_bytes(b"not a number")).unwrap_err();
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &parse_err).not());
    }

    #[test]
    fn is_thinkpad_gpu_powerdown_false_for_non_io_error() {
        // Verifies an arbitrary anyhow error (no io::Error in the chain)
        // is rejected, so we never substitute 0 on unrelated failures.
        let err = anyhow::anyhow!("something else went wrong");
        assert!(is_thinkpad_gpu_powerdown(devices::DEVICE_NAME_THINK_PAD, &err).not());
    }

    /// Goal: only ENODATA from a `tas2764` counts as an amplifier that has not sampled yet.
    /// Method: that error against another errno, other drivers, and a non-io error.
    #[test]
    fn tas2764_enodata_is_an_unsampled_amplifier_and_nothing_else_is() {
        let enodata = || -> anyhow::Error { Error::from_raw_os_error(libc::ENODATA).into() };
        let eio: anyhow::Error = Error::from_raw_os_error(libc::EIO).into();
        assert!(is_tas2764_unsampled("tas2764", &enodata()));
        assert!(is_tas2764_unsampled("tas2764", &eio).not());
        assert!(is_tas2764_unsampled("nct6798", &enodata()).not());
        assert!(is_tas2764_unsampled(devices::DEVICE_NAME_THINK_PAD, &enodata()).not());
        assert!(is_tas2764_unsampled("tas2764", &anyhow::anyhow!("not an io error")).not());
    }

    /// Goal: an unsampled `tas2764` gives a 0 degree status and stays listed, while the same
    /// error drops any other driver's channel. Method: an ENODATA read under both driver names.
    #[test]
    fn an_unsampled_tas2764_reads_zero_instead_of_failing() {
        let channel = HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            number: 1,
            name: "temp1".to_string(),
            ..Default::default()
        };
        let driver_named = |name: &str| HwmonDriverInfo {
            name: name.to_string(),
            ..Default::default()
        };
        let enodata = || Err(Error::from_raw_os_error(libc::ENODATA).into());
        let temp = temp_from(&driver_named("tas2764"), &channel, enodata()).unwrap();
        assert_eq!(temp, 0.0);
        assert!(temp_from(&driver_named("nct6798"), &channel, enodata()).is_none());
    }

    #[test]
    #[serial]
    fn extract_temp_ignores_non_temp_channels() {
        // Verifies non-Temp channels are ignored even if named similarly.
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            }
            .with_read_registry()
            .await;

            // when:
            let (temps, any_failure) = extract_temp_statuses(&driver_info).await;

            // then:
            assert!(any_failure.not());
            assert_eq!(temps.len(), 1);
            assert_eq!(temps[0].name, "temp1");
        });
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_label_is_read_whole_on_the_device_thread() {
        // Goal: labels come through the device's worker and lose nothing past a `SysfsValue`'s 64
        // bytes. Method: detect a temp with a long label through a real worker thread.
        cc_fs::test_runtime(async {
            let ctx = setup();
            let label = "Package id 0 on the second die of the first socket, long on purpose";
            assert!(label.len() > cc_fs::SYSFS_VALUE_MAX_BYTES);
            std::fs::write(ctx.test_base_path.join("temp1_input"), "30000").unwrap();
            std::fs::write(ctx.test_base_path.join("temp1_label"), format!("{label}\n")).unwrap();
            let io = DeviceIo::threaded("testdev", std::time::Duration::from_secs(10)).unwrap();

            let temps = init_temps(&ctx.test_base_path, "Test Driver", &io)
                .await
                .unwrap();

            assert_eq!(temps.len(), 1);
            assert_eq!(temps[0].label.as_deref(), Some(label));
        });
    }

    /// Seeds a chip directory holding exactly `files`, each with a plausible value.
    fn chip_dir_with(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            std::fs::write(dir.path().join(file), "1000\n").unwrap();
        }
        dir
    }

    #[test]
    fn a_cpu_chip_with_only_temps_offers_nothing_here() {
        // Goal: k10temp and coretemp, whose temps the CPU repository reads, start no hwmon worker.
        // Method: list a chip directory with temps, labels and a name but no fan or power file.
        let dir = chip_dir_with(&["name", "temp1_input", "temp1_label", "temp3_input"]);

        assert!(only_temps_of_another_repo(dir.path(), "k10temp"));
        assert!(only_temps_of_another_repo(dir.path(), "coretemp"));
        assert!(only_temps_of_another_repo(dir.path(), "cpu_thermal"));
    }

    #[test]
    fn a_cpu_chip_with_power_or_fans_is_kept() {
        // Goal: zenpower keeps its power channels, and a CPU chip with a fan keeps it. Method: list
        // directories holding each attribute detection reads, beside the chip's temps.
        for file in ["power1_input", "power2_average", "fan1_input", "pwm1"] {
            let dir = chip_dir_with(&["name", "temp1_input", file]);

            assert!(
                only_temps_of_another_repo(dir.path(), "zenpower").not(),
                "{file} must keep the chip"
            );
        }
    }

    #[test]
    fn a_chip_the_cpu_repository_does_not_read_is_never_ruled_out() {
        // Goal: the listing check applies to CPU chips alone, whose temps go elsewhere. Method: a
        // temps-only directory under a board chip's name, and a missing directory under a CPU
        // chip's name, which detection must still get to report.
        let dir = chip_dir_with(&["name", "temp1_input"]);
        let missing = dir.path().join("gone");

        assert!(only_temps_of_another_repo(dir.path(), "nct6687").not());
        assert!(only_temps_of_another_repo(&missing, "k10temp").not());
    }

    #[test]
    fn the_listing_check_names_exactly_what_detection_reads() {
        // Goal: the check rules out no chip that detection would give a channel, and does not keep
        // a chip for files detection skips. Method: match detection's attribute names and their
        // near misses.
        assert!(fans::is_fan_attribute("pwm1"));
        assert!(fans::is_fan_attribute("fan12_input"));
        assert!(fans::is_fan_attribute("pwm1_enable").not());
        assert!(fans::is_fan_attribute("fan1_label").not());
        assert!(fans::is_fan_attribute("temp1_input").not());
        assert!(power::is_power_attribute("power1_average"));
        assert!(power::is_power_attribute("power2_input"));
        assert!(power::is_power_attribute("power1_cap").not());
        assert!(power::is_power_attribute("power1_label").not());
    }
}
