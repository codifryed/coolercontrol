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
use crate::device::ChannelStatus;
use crate::repositories::hwmon::hwmon_repo::{
    AutoCurveInfo, HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
};
use crate::repositories::hwmon::{auto_curve, devices};
use anyhow::{anyhow, Context, Result};
use futures_util::future::{join3, join_all};
use log::{debug, error, info, trace, warn};
use regex::Regex;
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::{Path, PathBuf};

const PATTERN_PWM_FILE_NUMBER: &str = r"^pwm(?P<number>\d+)$";
const PATTERN_FAN_INPUT_FILE_NUMBER: &str = r"^fan(?P<number>\d+)_input$";
pub const PWM_ENABLE_MANUAL_VALUE: u8 = 1;
pub const PWM_ENABLE_AUTO_VALUE: u8 = 2;
const PWM_ENABLE_THINKPAD_FULL_SPEED: u8 = 0;
macro_rules! format_fan_input { ($($arg:tt)*) => {{ format!("fan{}_input", $($arg)*) }}; }
macro_rules! format_fan_label { ($($arg:tt)*) => {{ format!("fan{}_label", $($arg)*) }}; }
macro_rules! format_pwm { ($($arg:tt)*) => {{ format!("pwm{}", $($arg)*) }}; }
macro_rules! format_pwm_mode { ($($arg:tt)*) => {{ format!("pwm{}_mode", $($arg)*) }}; }
macro_rules! format_pwm_enable { ($($arg:tt)*) => {{ format!("pwm{}_enable", $($arg)*) }}; }

/// Initialize all applicable fans
pub async fn init_fans(base_path: &PathBuf, device_name: &str) -> Result<Vec<HwmonChannelInfo>> {
    let mut fans = vec![];
    let dir_entries = cc_fs::read_dir(base_path)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        init_pwm_fan(base_path, file_name, &mut fans, device_name).await?;
        init_rpm_only_fan(base_path, file_name, &mut fans, device_name).await?;
    }
    fans.sort_by(|c1, c2| c1.number.cmp(&c2.number));
    auto_curve::init_auto_curve_fans(base_path, &mut fans, device_name)?;
    trace!(
        "Hwmon pwm fans detected: {fans:?} for {}",
        base_path.display()
    );
    Ok(fans)
}

/// Initialize a PWM fan if certain conditions are met.
/// Most all fans that are controllable have a pwm file.
///
/// This function initializes a PWM fan if the given `file_name` matches the PWM file pattern.
/// It reads various attributes of the fan and adds an `HwmonChannelInfo` entry to the `fans` vector.
///
/// # Arguments
///
/// * `base_path` - A reference to a `PathBuf` representing the base directory.
/// * `file_name` - A string slice representing the name of the file to check.
/// * `fans` - A mutable reference to a vector of `HwmonChannelInfo` to which the fan info will be added.
/// * `device_name` - A string slice representing the name of the device.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// This function will return an error if it fails to read or parse any of the fan attributes.
async fn init_pwm_fan(
    base_path: &Path,
    file_name: &str,
    fans: &mut Vec<HwmonChannelInfo>,
    device_name: &str,
) -> Result<()> {
    let regex_pwm_file = Regex::new(PATTERN_PWM_FILE_NUMBER)?;
    if regex_pwm_file.is_match(file_name).not() {
        return Ok(()); // skip if not a pwm file
    }
    let channel_number: u8 = regex_pwm_file
        .captures(file_name)
        .context("PWM Number should exist")?
        .name("number")
        .context("Number Group should exist")?
        .as_str()
        .parse()?;
    if get_pwm_duty(base_path, &channel_number, true)
        .await
        .is_none()
    {
        return Ok(()); // skip if pwm file isn't readable
    }
    let current_pwm_enable = get_current_pwm_enable(base_path, &channel_number).await;
    let pwm_writable = determine_pwm_writable(base_path, channel_number);
    let pwm_enable_default = adjusted_pwm_default(current_pwm_enable, device_name);
    let channel_name = get_fan_channel_name(channel_number);
    let label = get_fan_channel_label(base_path, &channel_number).await;
    // deprecated setting:
    let pwm_mode_supported = false;
    // determine_pwm_mode_support(base_path, &channel_number).await;
    fans.push(HwmonChannelInfo {
        hwmon_type: HwmonChannelType::Fan,
        number: channel_number,
        pwm_enable_default,
        name: channel_name,
        label,
        pwm_mode_supported,
        pwm_writable,
        auto_curve: AutoCurveInfo::None,
    });
    Ok(())
}

/// Initialize an RPM-only fan.
/// There some fans that are RPM only (display-only), and do not have a pwm file for controlling.
///
/// This function initializes an RPM-only fan if the given `file_name` matches the RPM file pattern.
/// It reads various attributes of the fan and adds an `HwmonChannelInfo` entry to the `fans` vector.
///
/// # Arguments
///
/// * `base_path` - A reference to a `PathBuf` representing the base directory.
/// * `file_name` - A string slice representing the name of the file to check.
/// * `fans` - A mutable reference to a vector of `HwmonChannelInfo` to which the fan info will be added.
/// * `device_name` - A string slice representing the name of the device.
///
/// # Returns
///
/// A `Result` indicating success or failure.
///
/// # Errors
///
/// This function will return an error if it fails to read or parse any of the fan attributes.
async fn init_rpm_only_fan(
    base_path: &Path,
    file_name: &str,
    fans: &mut Vec<HwmonChannelInfo>,
    device_name: &str,
) -> Result<()> {
    let regex_fan_input_file = Regex::new(PATTERN_FAN_INPUT_FILE_NUMBER)?;
    if regex_fan_input_file.is_match(file_name).not() {
        return Ok(()); // skip if not a pwm file
    }
    let channel_number: u8 = regex_fan_input_file
        .captures(file_name)
        .context("Fan Number should exist")?
        .name("number")
        .context("Number Group should exist")?
        .as_str()
        .parse()?;
    if get_pwm_duty(base_path, &channel_number, false)
        .await
        .is_some()
    {
        return Ok(()); // skip if this has a pwm file (it's a pwm fan w/ rpm)
    }
    if get_fan_rpm(base_path, &channel_number, true)
        .await
        .is_none()
    {
        return Ok(()); // skip if rpm file isn't readable
    }
    let current_pwm_enable = get_current_pwm_enable(base_path, &channel_number).await;
    let pwm_enable_default = adjusted_pwm_default(current_pwm_enable, device_name);
    let channel_name = get_fan_channel_name(channel_number);
    let label = get_fan_channel_label(base_path, &channel_number).await;
    info!(
        "Uncontrollable RPM-only fan found at {}/{file_name}",
        base_path.display()
    );
    fans.push(HwmonChannelInfo {
        hwmon_type: HwmonChannelType::Fan,
        number: channel_number,
        pwm_enable_default,
        name: channel_name,
        label,
        pwm_mode_supported: false,
        pwm_writable: false,
        auto_curve: AutoCurveInfo::None,
    });
    Ok(())
}

/// Return the fan statuses for all channels.
/// Defaults to 0 for rpm and duty to handle temporary issues,
/// as they were correctly detected on startup.
/// This function calls all fan channels and data points sequentially. See the `concurrently`
/// version of this function for concurrent execution.
pub async fn extract_fan_statuses(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut fans = vec![];
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Fan {
            continue;
        }
        let fan_rpm = get_fan_rpm(&driver.path, &channel.number, false).await;
        let fan_duty = get_pwm_duty(&driver.path, &channel.number, false).await;
        fans.push(ChannelStatus {
            name: channel.name.clone(),
            rpm: fan_rpm,
            duty: fan_duty,
            ..Default::default()
        });
    }
    fans
}

#[allow(dead_code)]
/// This is the concurrent version of the `extract_fan_statuses` function.
pub async fn extract_fan_statuses_concurrently(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut fan_tasks = vec![];
    moro_local::async_scope!(|scope| {
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            let fan_task = scope.spawn(async {
                moro_local::async_scope!(|channel_scope| {
                    let fan_rpm_task =
                        channel_scope.spawn(get_fan_rpm(&driver.path, &channel.number, false));
                    let fan_duty_task =
                        channel_scope.spawn(get_pwm_duty(&driver.path, &channel.number, false));
                    let fan_pwm_mode_task = channel_scope.spawn(async {
                        if channel.pwm_mode_supported {
                            cc_fs::read_sysfs(driver.path.join(format_pwm_mode!(channel.number)))
                                .await
                                .and_then(check_parsing_8)
                                .ok()
                        } else {
                            None
                        }
                    });
                    let (fan_rpm, fan_duty, fan_pwm_mode) =
                        join3(fan_rpm_task, fan_duty_task, fan_pwm_mode_task).await;
                    ChannelStatus {
                        name: channel.name.clone(),
                        rpm: fan_rpm,
                        duty: fan_duty,
                        pwm_mode: fan_pwm_mode,
                        ..Default::default()
                    }
                })
                .await
            });
            fan_tasks.push(fan_task);
        }
        join_all(fan_tasks).await
    })
    .await
}

async fn get_pwm_duty(base_path: &Path, channel_number: &u8, log_error: bool) -> Option<f64> {
    let pwm_path = base_path.join(format_pwm!(channel_number));
    cc_fs::read_sysfs(&pwm_path)
        .await
        .and_then(check_parsing_8)
        .map(pwm_value_to_duty)
        .inspect_err(|err| {
            if log_error {
                warn!(
                    "Could not read fan pwm value at {} ; {err}",
                    pwm_path.display()
                );
            }
        })
        .ok()
}

async fn get_fan_rpm(base_path: &Path, channel_number: &u8, log_error: bool) -> Option<u32> {
    let fan_input_path = base_path.join(format_fan_input!(channel_number));
    cc_fs::read_sysfs(&fan_input_path)
        .await
        .and_then(check_parsing_32)
        // Edge case where on spin-up the output is max value until it begins moving
        .map(|rpm| if rpm >= u32::from(u16::MAX) { 0 } else { rpm })
        .inspect_err(|err| {
            if log_error {
                warn!(
                    "Could not read fan rpm value at {}: {err}",
                    fan_input_path.display()
                );
            }
        })
        .ok()
}

/// Not all drivers have `pwm_enable` for their fans. In that case there is no "automatic" mode available.
///  `pwm_enable` setting options:
///  - 0 : full speed / off (not used/recommended)
///  - 1 : manual control (setting pwm* will adjust fan speed)
///  - 2 : automatic (primarily used by on-board/chip fan control, like laptops or mobos without smart fan control)
///  - 3 : "Fan Speed Cruise" mode (?)
///  - 4 : "Smart Fan III" mode (NCT6775F only)
///  - 5 : "Smart Fan IV" mode (modern `MoBo`'s with build-in smart fan control probably use this)
async fn get_current_pwm_enable(base_path: &Path, channel_number: &u8) -> Option<u8> {
    let pwm_enable_path = base_path.join(format_pwm_enable!(channel_number));
    let current_pwm_enable = cc_fs::read_sysfs(&pwm_enable_path)
        .await
        .and_then(check_parsing_8)
        .ok();
    if current_pwm_enable.is_none() {
        debug!(
            "No pwm_enable found for fan#{channel_number} at location:{}",
            pwm_enable_path.display()
        );
    }
    current_pwm_enable
}

#[allow(clippy::needless_pass_by_value)]
pub fn check_parsing_8(content: String) -> Result<u8> {
    match content.trim().parse::<u8>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

#[allow(clippy::needless_pass_by_value)]
fn check_parsing_32(content: String) -> Result<u32> {
    match content.trim().parse::<u32>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

/// If a `HWMon` driver has not set the writable bit on the sysfs file, then that
/// indicates that the pwm value is read-only and not configurable.
fn determine_pwm_writable(base_path: &Path, channel_number: u8) -> bool {
    let pwm_path = base_path.join(format_pwm!(channel_number));
    let pwm_writable = cc_fs::metadata(&pwm_path)
        .inspect_err(|_| error!("PWM file metadata is not readable: {}", pwm_path.display()))
        // This check should be sufficient, as we're running as root:
        .is_ok_and(|att| att.permissions().readonly().not());
    if pwm_writable.not() {
        warn!(
            "PWM fan at {} is NOT writable - \
            Fan control is not currently supported by the installed driver.",
            pwm_path.display()
        );
    }
    pwm_writable
}

/// We save the existing `pwm_enable` setting and applying the Default Profile/shutting down the
/// service will then revert to that setting - which is usually 'auto' set by the bios on boot -
/// but not necessarily and not all devices support an auto setting.
///
/// This means we can not safely apply 'auto' to `pwm_enable` indiscriminately and therefor we use
/// whatever the initial setting was as the Default.
///
/// Note: Some drivers should have an automatic fallback for safety reasons,
/// regardless of the current value.
fn adjusted_pwm_default(current_pwm_enable: Option<u8>, device_name: &str) -> Option<u8> {
    current_pwm_enable.map(|original_value| {
        if devices::device_needs_pwm_fallback(device_name) {
            2
        } else {
            original_value
        }
    })
}

/// Reads the contents of the fan?_label file specified by `base_path` and
/// `channel_number`, trims any leading or trailing whitespace, and returns the resulting string if it
/// is not empty.
///
/// Arguments:
///
/// * `base_path`: A `PathBuf` object representing the base path where the file `fan{}_label` is
///   located.
/// * `channel_number`: The `channel_number` parameter is an unsigned 8-bit integer that represents
///   the channel number. It is used to construct the file path for reading the label.
///
/// Returns:
///
/// an `Option<String>`.
async fn get_fan_channel_label(base_path: &Path, channel_number: &u8) -> Option<String> {
    cc_fs::read_txt(base_path.join(format_fan_label!(channel_number)))
        .await
        .ok()
        .and_then(|label| {
            let fan_label = label.trim();
            if fan_label.is_empty() {
                warn!(
                    "Fan label is empty for {}/fan{channel_number}_label",
                    base_path.display()
                );
                None
            } else {
                Some(fan_label.to_string())
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
fn get_fan_channel_name(channel_number: u8) -> String {
    format!("fan{channel_number}")
}

pub async fn set_pwm_enable_to_default(
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
) -> Result<()> {
    let Some(default_value) = channel_info.pwm_enable_default else {
        // not all devices have pwm_enable available
        return Ok(());
    };
    if let Err(err) = set_pwm_enable_if_not_already(default_value, base_path, channel_info).await {
        warn!("Failed to reset pwm_enable to default: {err}");
    }
    debug!(
        "Reset Hwmon value at {}/pwm{}_enable to starting default value of {default_value}",
        base_path.display(),
        channel_info.number
    );
    Ok(())
}

/// This sets `pwm_enable` to the desired value. Unlike other operations,
/// it will not check if it's already set to the desired value.
/// See also `get_current_pwm_enable`.
pub async fn set_pwm_enable(
    pwm_enable_value: u8,
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
) -> Result<()> {
    if channel_info.pwm_enable_default.is_none() {
        // not all devices have pwm_enable available
        return Ok(());
    }
    if pwm_enable_value > 5 {
        return Err(anyhow!(
            "pwm_enable value must be between 0 and 5 (inclusive)"
        ));
    }
    let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
    write_pwm_enable(&path_pwm_enable, pwm_enable_value).await
}

/// This sets `pwm_enable` to the desired value if it's not already set to the desired value.
/// See also `get_current_pwm_enable`.
pub async fn set_pwm_enable_if_not_already(
    pwm_enable_value: u8,
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
) -> Result<()> {
    if channel_info.pwm_enable_default.is_none() {
        // not all devices have pwm_enable available
        return Ok(());
    }
    let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
    let current_pwm_enable = cc_fs::read_sysfs(&path_pwm_enable)
        .await
        .and_then(check_parsing_8)?;
    if current_pwm_enable == pwm_enable_value {
        Ok(())
    } else {
        write_pwm_enable(&path_pwm_enable, pwm_enable_value).await
    }
}

async fn write_pwm_enable(path_pwm_enable: &Path, pwm_enable_value: u8) -> Result<()> {
    cc_fs::write_string(&path_pwm_enable, pwm_enable_value.to_string())
        .await
        .inspect(|()| {
            debug!(
                "Applied pwm_enable for {} of {pwm_enable_value}",
                path_pwm_enable.display()
            );
        })
        .map_err(|err| {
            anyhow!(
                "Unable to set pwm_enable for {} to {pwm_enable_value}. \
                    Most likely because of a limitation set by the driver or a BIOS setting; \
                    Error: {err}",
                path_pwm_enable.display()
            )
        })
}

pub async fn set_pwm_duty(
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
    speed_duty: u8,
) -> Result<()> {
    let pwm_value = duty_to_pwm_value(speed_duty);
    let pwm_path = base_path.join(format_pwm!(channel_info.number));
    cc_fs::write_string(&pwm_path, pwm_value.to_string())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set PWM value {pwm_value} for {} Reason: {err}",
                pwm_path.display()
            )
        })
}

/// Converts a pwm value (0-255) to a duty value (0-100%)
fn pwm_value_to_duty(pwm_value: u8) -> f64 {
    ((f64::from(pwm_value) / 0.255).round() / 10.0).round()
}

/// Converts a duty value (0-100%) to a pwm value (0-255)
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn duty_to_pwm_value(speed_duty: u8) -> u8 {
    let clamped_duty = f64::from(speed_duty.clamp(0, 100));
    // round only takes the first decimal digit into consideration, so we adjust to have it take the first two digits into consideration.
    ((clamped_duty * 25.5).round() / 10.0).round() as u8
}

/// submodule for `ThinkPad` fan logic
pub mod thinkpad {
    use crate::cc_fs;
    use crate::config::Config;
    use crate::repositories::hwmon::fans::{
        check_parsing_8, set_pwm_duty, set_pwm_enable_if_not_already, PWM_ENABLE_MANUAL_VALUE,
        PWM_ENABLE_THINKPAD_FULL_SPEED,
    };
    use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonDriverInfo};
    use anyhow::{anyhow, Result};
    use log::debug;
    use std::path::Path;
    use std::rc::Rc;

    pub async fn apply_speed_fixed(
        config: &Rc<Config>,
        hwmon_driver: &Rc<HwmonDriverInfo>,
        channel_info: &HwmonChannelInfo,
        speed_fixed: u8,
    ) -> Result<()> {
        if speed_fixed == 100 && config.get_settings()?.thinkpad_full_speed {
            set_to_full_speed(&hwmon_driver.path, channel_info).await
        } else {
            set_pwm_enable_if_not_already(
                PWM_ENABLE_MANUAL_VALUE,
                &hwmon_driver.path,
                channel_info,
            )
            .await?;
            set_pwm_duty(&hwmon_driver.path, channel_info, speed_fixed)
                .await
                .map_err(|err| {
                    anyhow!(
                        "Error on {}:{} for duty {speed_fixed} - {err}",
                        hwmon_driver.name,
                        channel_info.name
                    )
                })
        }
    }

    /// This sets `pwm_enable` to 0. The effect of this is dependent on the device, but is primarily used
    /// for `ThinkPads` where this means "full-speed". See:
    /// [Kernel Doc](https://www.kernel.org/doc/html/latest/admin-guide/laptops/thinkpad-acpi.html#fan-control-and-monitoring-fan-speed-fan-enable-disable)
    pub async fn set_to_full_speed(
        base_path: &Path,
        channel_info: &HwmonChannelInfo,
    ) -> Result<()> {
        // set to 100% first for consistent pwm duty-reporting behavior
        // (the driver doesn't automatically set the duty to 100% in full-speed mode)
        set_pwm_duty(base_path, channel_info, 100).await?;
        let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
        let current_pwm_enable = cc_fs::read_sysfs(&path_pwm_enable)
            .await
            .and_then(check_parsing_8)?;
        if current_pwm_enable != PWM_ENABLE_THINKPAD_FULL_SPEED {
            cc_fs::write_string(&path_pwm_enable, PWM_ENABLE_THINKPAD_FULL_SPEED.to_string())
                .await
                .inspect(|()| {
                    debug!("Applied pwm_enable for {} of {PWM_ENABLE_THINKPAD_FULL_SPEED}", path_pwm_enable.display());
                })
                .map_err(|err| {
                    anyhow!(
                        "Not able to set pwm_enable of {PWM_ENABLE_THINKPAD_FULL_SPEED}. \
                        Most likely because of a permissions issue or driver limitation; Error: {err}"
                    )
                })?;
        }
        Ok(())
    }
}

/// Tests
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::setting::CoolerControlSettings;
    use serial_test::serial;
    use std::path::Path;
    use std::rc::Rc;
    use uuid::Uuid;

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
    fn find_fan_dir_not_exist() {
        cc_fs::test_runtime(async {
            // given:
            let test_base_path = Path::new("/tmp/does_not_exist").to_path_buf();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(&test_base_path, &device_name).await;

            // then:
            assert!(fans_result.is_err());
            assert!(fans_result
                .map_err(|err| err.to_string().contains("No such file or directory"))
                .unwrap_err());
        });
    }

    #[test]
    #[serial]
    fn find_fan() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();
            cc_fs::write(
                test_base_path.join("fan1_input"),
                b"3000".to_vec(), // rpm
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(test_base_path, &device_name).await;

            // then:
            // println!("RESULT: {:?}", fans_result);
            teardown(&ctx);
            assert!(fans_result.is_ok());
            let fans = fans_result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
            assert_eq!(fans[0].name, "fan1");
            assert!(fans[0].pwm_mode_supported.not());
            assert_eq!(fans[0].pwm_enable_default, None);
            assert_eq!(fans[0].number, 1);
            assert!(fans[0].pwm_writable);
        });
    }

    #[test]
    #[serial]
    fn find_fan_pwm_only() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(test_base_path, &device_name).await;

            // then:
            teardown(&ctx);
            assert!(fans_result.is_ok());
            let fans = fans_result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
            assert_eq!(fans[0].name, "fan1");
            assert!(fans[0].pwm_mode_supported.not());
            assert_eq!(fans[0].pwm_enable_default, None);
            assert_eq!(fans[0].number, 1);
            assert!(fans[0].pwm_writable);
        });
    }

    #[test]
    #[serial]
    fn find_fan_rpm_only() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("fan1_input"),
                b"3000".to_vec(), // rpm
            )
            .await
            .unwrap();
            let device_name = "Test Driver".to_string();

            // when:
            let fans_result = init_fans(test_base_path, &device_name).await;

            // then:
            // println!("RESULT: {:?}", fans_result);
            teardown(&ctx);
            assert!(fans_result.is_ok());
            let fans = fans_result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].hwmon_type, HwmonChannelType::Fan);
            assert_eq!(fans[0].name, "fan1");
            assert!(fans[0].pwm_mode_supported.not());
            assert_eq!(fans[0].pwm_enable_default, None);
            assert_eq!(fans[0].number, 1);
            assert!(fans[0].pwm_writable.not());
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_to_default() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_enable"), b"1".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result = set_pwm_enable_to_default(test_base_path, &channel_info).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, "2");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_to_default_doesnt_exist() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result = set_pwm_enable_to_default(test_base_path, &channel_info).await;

            // then:
            let pwm_enable_doesnt_exist = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .is_err();
            teardown(&ctx);
            assert!(result.is_ok());
            assert!(pwm_enable_doesnt_exist);
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_enable"), b"2".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result =
                set_pwm_enable(PWM_ENABLE_MANUAL_VALUE, test_base_path, &channel_info).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, "1");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_doesnt_exist() {
        // test to make sure we don't return an Err if pwm_enable doesn't exist
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result =
                set_pwm_enable(PWM_ENABLE_MANUAL_VALUE, test_base_path, &channel_info).await;

            // then:
            let pwm_enable_doesnt_exist = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .is_err();
            teardown(&ctx);
            assert!(result.is_ok());
            assert!(pwm_enable_doesnt_exist);
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_enable_if_not_already_set() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_enable"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result = set_pwm_enable_if_not_already(
                PWM_ENABLE_MANUAL_VALUE,
                test_base_path,
                &channel_info,
            )
            .await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, "1");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_duty() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result = set_pwm_duty(test_base_path, &channel_info, 50).await;

            // then:
            let current_duty = cc_fs::read_sysfs(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(format!("{current_duty:.1}"), "50.0");
        });
    }

    #[test]
    #[serial]
    fn test_set_pwm_duty_no_pwm_enable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result = set_pwm_duty(test_base_path, &channel_info, 50).await;

            // then:
            let current_duty = cc_fs::read_sysfs(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(current_duty.to_string(), "50");
        });
    }

    #[test]
    #[serial]
    fn thinkpad_apply_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_MANUAL_VALUE.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            // set full_speed setting
            let cc_settings = CoolerControlSettings {
                thinkpad_full_speed: true,
                ..Default::default()
            };
            config.set_settings(&cc_settings);
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result = thinkpad::apply_speed_fixed(&config, &hwmon_info, &channel_info, 50).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, PWM_ENABLE_MANUAL_VALUE.to_string());
            assert_eq!(current_duty, 50.);
        });
    }

    #[test]
    #[serial]
    fn thinkpad_apply_speed_full_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_MANUAL_VALUE.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            // set full_speed setting
            let cc_settings = CoolerControlSettings {
                thinkpad_full_speed: true,
                ..Default::default()
            };
            config.set_settings(&cc_settings);
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result =
                thinkpad::apply_speed_fixed(&config, &hwmon_info, &channel_info, 100).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(
                current_pwm_enable,
                PWM_ENABLE_THINKPAD_FULL_SPEED.to_string()
            );
            assert_eq!(current_duty, 100.);
        });
    }

    #[test]
    #[serial]
    fn thinkpad_apply_speed_after_full_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_THINKPAD_FULL_SPEED.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            // set full_speed setting
            let cc_settings = CoolerControlSettings {
                thinkpad_full_speed: true,
                ..Default::default()
            };
            config.set_settings(&cc_settings);
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result = thinkpad::apply_speed_fixed(&config, &hwmon_info, &channel_info, 50).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, PWM_ENABLE_MANUAL_VALUE.to_string(),);
            assert_eq!(current_duty, 50.);
        });
    }

    #[test]
    #[serial]
    fn thinkpad_set_full_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_MANUAL_VALUE.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                pwm_mode_supported: false,
                pwm_writable: true,
                auto_curve: AutoCurveInfo::None,
            };

            // when:
            let result = thinkpad::set_to_full_speed(test_base_path, &channel_info).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(
                current_pwm_enable,
                PWM_ENABLE_THINKPAD_FULL_SPEED.to_string()
            );
            assert_eq!(current_duty, 100.);
        });
    }
}
