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

//! This module handles the `auto_points` for hwmon devices.
//! See: `https://www.kernel.org/doc/Documentation/hwmon/sysfs-interface`
//!
//! It seems not all device drivers 100% follow the standard above, so there is some flexibility
//! built in. The main access points:
//!
//! `pwm[1-*]_enable`
//! Works the same for normal pwm fans as well, with `2` being auto-mode.
//!
//! `pwm[1-*]_auto_channels_temp`
//! This is how temperature-associated curve points are assigned to specific fan channels.
//! If this is present, then multi-temperature auto curves should be supported.
//!
//! `temp[1-*]_auto_point[1-*]_[pwm|temp]`
//! These files contain the temperature-based curve points, or curves that are assigned
//! to specific temperature channels.
//! This looks to sometimes also appear for pwm-based curves, particularly if the *_temp point
//! is not available. (*_pwm only) In those cases, the temperature values for the points are fixed.
//! These devices unfortunately need to be handled individually - as there's no way to know what the
//! fixed temperature values are without looking at the kernel module documentation.
//!
//! `pwm[1-*]_auto_point[1-*]_[pwm|temp]`
//! These files contain the pwm-associated curve points that can be set, or curves
//! that are assigned to specific fan channels. i.e. pwm1 and pwm2.

use crate::device::{Duty, Temp};
use crate::repositories::hwmon::fans::{
    PWM_ENABLE_AUTO_VALUE, PWM_ENABLE_MANUAL_VALUE, PWM_ENABLE_NCT6775_SMART_FAN_IV_VALUE,
};
use crate::repositories::hwmon::hwmon_repo::{
    AutoCurveInfo, HwmonChannelCapabilities, HwmonChannelInfo,
};
use crate::repositories::hwmon::{fans, temps};
use crate::{cc_fs, engine};
use anyhow::{anyhow, Context, Result};
use log::{debug, error, warn};
use regex::Regex;
use std::collections::HashMap;
use std::ops::{Not, RangeInclusive};
use std::path::Path;

type CurveTemp = u32; // millidegrees Celsius
type CurvePWM = u8;

const PATTERN_TEMP_AUTO_POINT: &str =
    r"^temp(?P<temp_number>\d+)_auto_point(?P<point_number>\d+)_(?P<type>pwm|temp)$";
// This is used to assign temp-associated curves to pwm channels
macro_rules! format_pwm_auto_channels_temp { ($($arg:tt)*) => {{ format!("pwm{}_auto_channels_temp", $($arg)*) }}; }
// This is currently only used by the nct6775 driver (for many devices)
macro_rules! format_pwm_temp_sel { ($($arg:tt)*) => {{ format!("pwm{}_temp_sel", $($arg)*) }}; }
macro_rules! format_pwm_auto_point_exists { ($($arg:tt)*) => {{ format!("pwm{}_auto_point1_pwm", $($arg)*) }}; }
macro_rules! format_pwm_auto_point_regex { ($($arg:tt)*) => {{ format!(r"^pwm{}_auto_point(?P<point_number>\d+)_(?P<type>pwm|temp)$", $($arg)*) }}; }
macro_rules! format_temp_auto_point_regex { ($($arg:tt)*) => {{ format!(r"^temp{}_auto_point(?P<point_number>\d+)_(?P<type>pwm|temp)$", $($arg)*) }}; }
macro_rules! format_pwm_auto_point_pwm { ($($arg:tt)*) => {{ format!("pwm{}_auto_point{}_pwm", $($arg)*) }}; }
macro_rules! format_pwm_auto_point_temp { ($($arg:tt)*) => {{ format!("pwm{}_auto_point{}_temp", $($arg)*) }}; }
macro_rules! format_temp_auto_point_pwm { ($($arg:tt)*) => {{ format!("temp{}_auto_point{}_pwm", $($arg)*) }}; }
macro_rules! format_temp_auto_point_temp { ($($arg:tt)*) => {{ format!("temp{}_auto_point{}_temp", $($arg)*) }}; }
const CURVE_RANGE_PWM: RangeInclusive<CurvePWM> = 0..=255;
const CURVE_RANGE_DUTY: RangeInclusive<CurvePWM> = 0..=100;
const CURVE_RANGE_TEMP: RangeInclusive<CurveTemp> = 0..=100_000; // millidegrees
                                                                 // These devices from the nzxt-kraken3 driver require special handling
const DEVICE_NAMES_NZXT_KRAKEN3: [&str; 3] = ["z53", "kraken2023", "kraken2023elite"];
const POINT_LENGTH_NZXT_KRAKEN3: u8 = 40;
const CURVE_RANGE_TEMP_NZXT_KRAKEN3: RangeInclusive<CurveTemp> = 20..=59;

/// This initializes pwm channels that support auto curves.
///
/// This function currently supports fans that also have pwmN controls (fixed speed), which
/// looks to be the vast majority of drivers.
///
/// There are two types of auto curves:
/// 1. Temperature-based curves (for chips that support multiple temperature channels with curves)
/// 2. PWM-based curves
pub async fn init_auto_curve_fans(
    base_path: &Path,
    fans: &mut Vec<HwmonChannelInfo>,
    device_name: &str,
) -> Result<()> {
    for fan in fans {
        if fan.caps.is_fan_controllable().not() {
            continue; // we only support fans that have pwmN controls
        }
        if is_temp_sel(base_path, fan.number) {
            init_temp_sel_pwm_based_curve(base_path, fan, device_name).await?;
        } else if is_temp_based(base_path, fan.number) {
            init_temp_based_curve(base_path, fan)?;
        } else if DEVICE_NAMES_NZXT_KRAKEN3.contains(&device_name) {
            init_kraken3_auto_curve(base_path, fan)?;
        } else if is_pwm_based(base_path, fan.number) {
            init_pwm_based_curve(base_path, fan)?;
        }
    }
    Ok(())
}

/// Checks for the existence of `pwm[1-*]_auto_channels_temp` for this fan channel, which
/// indicates that this fan channel supports temperature-associated auto curves.
fn is_temp_based(base_path: &Path, fan_number: u8) -> bool {
    cc_fs::exists(base_path.join(format_pwm_auto_channels_temp!(fan_number).as_str()))
}

fn is_pwm_based(base_path: &Path, fan_number: u8) -> bool {
    cc_fs::exists(base_path.join(format_pwm_auto_point_exists!(fan_number).as_str()))
}

/// Checks for the existence of `pwm[1-*]_temp_sel` for this fan channel, which
/// indicates that this device supports temperature-based but PWM-associated auto curves,
/// with temperature selectors for each PWM channel.
/// This is currently only known to by used by the `nct6775` kernel driver.
fn is_temp_sel(base_path: &Path, fan_number: u8) -> bool {
    cc_fs::exists(base_path.join(format_pwm_temp_sel!(fan_number).as_str()))
        && is_pwm_based(base_path, fan_number)
}

fn init_temp_based_curve(base_path: &Path, fan: &mut HwmonChannelInfo) -> Result<()> {
    let regex_temp_auto_points = Regex::new(PATTERN_TEMP_AUTO_POINT)?;
    let mut found_auto_points = false;
    let mut can_set_pwm_point_values = false;
    let mut can_set_temp_point_values = false;
    let mut temp_point_lengths = HashMap::new();
    let dir_entries = cc_fs::read_dir(base_path)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_temp_auto_points.is_match(file_name).not() {
            continue;
        }
        found_auto_points = true;
        let captures = regex_temp_auto_points
            .captures(file_name)
            .context("Can't match captured regex")?;
        let temp_number: u8 = captures
            .name("temp_number")
            .context("temp_number group should exist")?
            .as_str()
            .parse()?;
        let point_number: u8 = captures
            .name("point_number")
            .context("point_number group should exist")?
            .as_str()
            .parse()?;
        let point_value_type = captures
            .name("type")
            .context("type group should exist")?
            .as_str();
        if point_value_type == "pwm" {
            can_set_pwm_point_values = true;
        } else if point_value_type == "temp" {
            can_set_temp_point_values = true;
        }
        let curr_num_points = temp_point_lengths
            .entry(temp_number)
            .or_insert(point_number);
        if &point_number > curr_num_points {
            *curr_num_points = point_number;
        }
    }
    if can_set_temp_point_values.not() {
        warn!(
            "HWMon Auto Curve temperature-associated points do not allow setting temperature values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if can_set_pwm_point_values.not() {
        warn!(
            "HWMon Auto Curve temperature-associated points do not allow setting pwm values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if found_auto_points.not() {
        warn!(
            "HWMon Auto Curve temperature-associated: Expected auto points, but found none. {}",
            base_path.display()
        );
        return Ok(());
    }
    let temp_lengths = temp_point_lengths
        .into_iter()
        .map(|(temp_number, max_points)| (format!("temp{temp_number}"), max_points))
        .collect();
    fan.auto_curve = AutoCurveInfo::Temp { temp_lengths };
    Ok(())
}

fn init_pwm_based_curve(base_path: &Path, fan: &mut HwmonChannelInfo) -> Result<()> {
    // standard case for pwm-associated curve points
    let regex_pwm_auto_points = Regex::new(format_pwm_auto_point_regex!(fan.number).as_str())?;
    let mut can_set_pwm_point_values = false;
    let mut can_set_temp_point_values = false;
    let mut max_points = 0;
    let dir_entries = cc_fs::read_dir(base_path)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_pwm_auto_points.is_match(file_name).not() {
            continue;
        }
        let captures = regex_pwm_auto_points
            .captures(file_name)
            .context("Can't match captured regex")?;
        let point_number: u8 = captures
            .name("point_number")
            .context("point_number group should exist")?
            .as_str()
            .parse()?;
        let point_value_type = captures
            .name("type")
            .context("type group should exist")?
            .as_str();
        if point_value_type == "pwm" {
            can_set_pwm_point_values = true;
        } else if point_value_type == "temp" {
            can_set_temp_point_values = true;
        }
        if point_number > max_points {
            max_points = point_number;
        }
    }
    if can_set_temp_point_values.not() {
        warn!(
            "HWMon Auto Curve pwm-associated points do not allow setting temperature values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if can_set_pwm_point_values.not() {
        warn!(
            "HWMon Auto Curve pwm-associated points do not allow setting pwm values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if max_points < 2 {
        error!(
            "HWMon Auto Curve: <2 points detected. This shouldn't happen. {}",
            base_path.display()
        );
        return Ok(());
    }
    fan.auto_curve = AutoCurveInfo::PWM {
        point_length: max_points,
    };
    Ok(())
}

async fn init_temp_sel_pwm_based_curve(
    base_path: &Path,
    fan: &mut HwmonChannelInfo,
    device_name: &str,
) -> Result<()> {
    // This is currently only known to by used by the `nct6775` kernel driver.
    let regex_pwm_auto_points = Regex::new(format_pwm_auto_point_regex!(fan.number).as_str())?;
    let mut can_set_pwm_point_values = false;
    let mut can_set_temp_point_values = false;
    let mut max_points = 0;
    let dir_entries = cc_fs::read_dir(base_path)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_pwm_auto_points.is_match(file_name).not() {
            continue;
        }
        let captures = regex_pwm_auto_points
            .captures(file_name)
            .context("Can't match captured regex")?;
        let point_number: u8 = captures
            .name("point_number")
            .context("point_number group should exist")?
            .as_str()
            .parse()?;
        let point_value_type = captures
            .name("type")
            .context("type group should exist")?
            .as_str();
        if point_value_type == "pwm" {
            can_set_pwm_point_values = true;
        } else if point_value_type == "temp" {
            can_set_temp_point_values = true;
        }
        if point_number > max_points {
            max_points = point_number;
        }
    }
    if can_set_temp_point_values.not() {
        warn!(
            "HWMon Auto Curve Temperature Select points do not allow setting temperature values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if can_set_pwm_point_values.not() {
        warn!(
            "HWMon Auto Curve Temperature Select points do not allow setting pwm values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if max_points < 2 {
        error!(
            "HWMon Auto Curve: <2 points detected. This shouldn't happen. {}",
            base_path.display()
        );
        return Ok(());
    }
    let temp_lengths = temps::init_temps(base_path, device_name)
        .await?
        .into_iter()
        .map(|channel_info| (channel_info.name, max_points))
        .collect();
    fan.auto_curve = AutoCurveInfo::Temp { temp_lengths };
    Ok(())
}

pub async fn apply_curve(
    base_path: &Path,
    fan_channel_info: &HwmonChannelInfo,
    speed_profile: &[(Temp, Duty)],
    temp_channel_info: &HwmonChannelInfo,
    device_name: &str,
) -> Result<()> {
    match &fan_channel_info.auto_curve {
        AutoCurveInfo::None => Ok(()),
        AutoCurveInfo::PWM { point_length } => {
            if DEVICE_NAMES_NZXT_KRAKEN3.contains(&device_name) {
                let interpolated_pwms = interpolate_kraken3_curve(speed_profile);
                fans::set_pwm_enable(PWM_ENABLE_MANUAL_VALUE, base_path, fan_channel_info).await?;
                apply_kraken3_curve(base_path, fan_channel_info.number, interpolated_pwms).await?;
            } else {
                let normalized_curve =
                    normalize_speed_profile(speed_profile, *point_length as usize);
                fans::set_pwm_enable(PWM_ENABLE_MANUAL_VALUE, base_path, fan_channel_info).await?;
                apply_pwm_curve(base_path, fan_channel_info.number, normalized_curve).await?;
            }
            fans::set_pwm_enable(PWM_ENABLE_AUTO_VALUE, base_path, fan_channel_info).await
        }
        AutoCurveInfo::Temp { temp_lengths } => {
            let point_length = temp_lengths
                .get(&temp_channel_info.name)
                .copied()
                .with_context(|| {
                    // If there is not a curve_length stored for this temp source channel
                    // than firmware-profiles are not supported for this temp source
                    let available_temp_names = temp_lengths.keys().map(std::string::ToString::to_string).collect::<Vec<String>>().join(", ");
                    format!("Firmware Curves for temperature channel: {} are not supported. Please use one of: [{available_temp_names}]", temp_channel_info.name)
                })?;
            let normalized_curve = normalize_speed_profile(speed_profile, point_length as usize);
            if is_temp_sel(base_path, fan_channel_info.number) {
                fans::set_pwm_enable(PWM_ENABLE_MANUAL_VALUE, base_path, fan_channel_info).await?;
                apply_pwm_curve(base_path, fan_channel_info.number, normalized_curve).await?;
                fans::set_pwm_enable(
                    PWM_ENABLE_NCT6775_SMART_FAN_IV_VALUE,
                    base_path,
                    fan_channel_info,
                )
                .await
            } else {
                fans::set_pwm_enable(PWM_ENABLE_MANUAL_VALUE, base_path, fan_channel_info).await?;
                apply_temp_curve(base_path, temp_channel_info.number, normalized_curve).await?;
                apply_temp_curve_to_pwm_channel(
                    base_path,
                    temp_channel_info.number,
                    fan_channel_info.number,
                )
                .await?;
                fans::set_pwm_enable(PWM_ENABLE_AUTO_VALUE, base_path, fan_channel_info).await
            }
        }
    }
}

fn normalize_speed_profile(
    speed_profile: &[(Temp, Duty)],
    curve_length: usize,
) -> Vec<(CurveTemp, CurvePWM)> {
    let mut normalized_curve = Vec::with_capacity(curve_length);
    let capped_profile = cap_speed_profile(speed_profile, curve_length);
    let capped_profile_length = capped_profile.len();
    for (temp, duty) in capped_profile {
        let pwm_value: CurvePWM = fans::duty_to_pwm_value(duty);
        let clamped_pwm = if CURVE_RANGE_PWM.contains(&pwm_value) {
            pwm_value
        } else {
            warn!(
                "HWMon Auto Curve - Only fan pwm values within range of {}% to {}% are allowed. \
                    Clamping passed duty of {pwm_value}% to nearest limit",
                CURVE_RANGE_PWM.start(),
                CURVE_RANGE_PWM.end(),
            );
            *CURVE_RANGE_PWM
                .end()
                .min(CURVE_RANGE_PWM.start().max(&pwm_value))
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        // to millidegrees
        let temp_millidegrees = (temp * 1000.0).round() as CurveTemp;
        let clamped_temp = if CURVE_RANGE_TEMP.contains(&temp_millidegrees) {
            temp_millidegrees
        } else {
            warn!(
                "HWMon Auto Curve - Only curve temps within range of {}C to {}C are allowed. \
                Clamping passed temp of {temp_millidegrees}% to nearest limit",
                CURVE_RANGE_TEMP.start(),
                CURVE_RANGE_TEMP.end(),
            );
            *CURVE_RANGE_TEMP
                .end()
                .min(CURVE_RANGE_TEMP.start().max(&temp_millidegrees))
        };
        normalized_curve.push((clamped_temp, clamped_pwm));
    }
    // add any missing points:
    let last_point = *normalized_curve
        .last()
        .expect("Should be at least one point");
    for _ in capped_profile_length..curve_length {
        normalized_curve.push((last_point.0, last_point.1));
    }
    normalized_curve
}

/// Caps the speed profile to the max number of points allowed by the hwmon curve.
///
/// If the speed profile is longer than the fan curve, we truncate the speed profile to the
/// max number of points allowed by the fan curve. We keep the last point as reference for
/// the fan curve, safety-wise, but allow setting it truncated.
fn cap_speed_profile(speed_profile: &[(Temp, Duty)], fan_curve_length: usize) -> Vec<(Temp, Duty)> {
    let mut capped_profile = speed_profile.to_vec();
    if capped_profile.len() > fan_curve_length {
        warn!(
            "HWMon Auto Curve - Max {fan_curve_length} fan curve points are allowed. \
                Truncating speed profile with {} points. Please adjust the \
                Graph Profile to match the number of points allowed by the device fan curve.",
            capped_profile.len()
        );
        capped_profile.truncate(fan_curve_length - 1); // remove all but the last point
        capped_profile.push(speed_profile.last().copied().unwrap_or((100., 100_u8)));
    }
    capped_profile
}

async fn apply_pwm_curve(
    base_path: &Path,
    pwm_channel_number: u8,
    normalized_curve: Vec<(CurveTemp, CurvePWM)>,
) -> Result<()> {
    for (index, (temp, pwm)) in normalized_curve.into_iter().enumerate() {
        let point = index + 1;
        set_pwm_auto_point_pwm(base_path, pwm_channel_number, point, pwm).await?;
        set_pwm_auto_point_temp(base_path, pwm_channel_number, point, temp).await?;
    }
    Ok(())
}

async fn set_pwm_auto_point_pwm(
    base_path: &Path,
    pwm_channel_number: u8,
    point_number: usize,
    pwm: CurvePWM,
) -> Result<()> {
    let auto_point_pwm_path =
        base_path.join(format_pwm_auto_point_pwm!(pwm_channel_number, point_number));
    cc_fs::write_string(&auto_point_pwm_path, pwm.to_string())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set Auto Point PWM value {pwm} for {} Reason: {err}",
                auto_point_pwm_path.display()
            )
        })
}

async fn set_pwm_auto_point_temp(
    base_path: &Path,
    pwm_channel_number: u8,
    point_number: usize,
    temp: CurveTemp,
) -> Result<()> {
    let auto_point_temp_path = base_path.join(format_pwm_auto_point_temp!(
        pwm_channel_number,
        point_number
    ));
    cc_fs::write_string(&auto_point_temp_path, temp.to_string())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set Auto Point Temperature value {temp} for {} Reason: {err}",
                auto_point_temp_path.display()
            )
        })
}

async fn apply_temp_curve(
    base_path: &Path,
    temp_channel_number: u8,
    normalized_curve: Vec<(CurveTemp, CurvePWM)>,
) -> Result<()> {
    for (index, (temp, pwm)) in normalized_curve.into_iter().enumerate() {
        let point = index + 1;
        set_temp_auto_point_pwm(base_path, temp_channel_number, point, pwm).await?;
        set_temp_auto_point_temp(base_path, temp_channel_number, point, temp).await?;
    }
    Ok(())
}

async fn set_temp_auto_point_pwm(
    base_path: &Path,
    temp_channel_number: u8,
    point_number: usize,
    pwm: CurvePWM,
) -> Result<()> {
    let auto_point_pwm_path = base_path.join(format_temp_auto_point_pwm!(
        temp_channel_number,
        point_number
    ));
    cc_fs::write_string(&auto_point_pwm_path, pwm.to_string())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set Auto Point PWM value {pwm} for {} Reason: {err}",
                auto_point_pwm_path.display()
            )
        })
}

async fn set_temp_auto_point_temp(
    base_path: &Path,
    temp_channel_number: u8,
    point_number: usize,
    temp: CurveTemp,
) -> Result<()> {
    let auto_point_temp_path = base_path.join(format_temp_auto_point_temp!(
        temp_channel_number,
        point_number
    ));
    cc_fs::write_string(&auto_point_temp_path, temp.to_string())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set Auto Point Temperature value {temp} for {} Reason: {err}",
                auto_point_temp_path.display()
            )
        })
}

/// This applies the temperature curve to the pwm channel, so that the pwm channel uses the
/// specified temperature channel's curve.
async fn apply_temp_curve_to_pwm_channel(
    base_path: &Path,
    temp_channel_number: u8,
    pwm_channel_number: u8,
) -> Result<()> {
    let pwm_auto_channel_path = base_path.join(format_pwm_auto_channels_temp!(pwm_channel_number));
    cc_fs::write_string(&pwm_auto_channel_path, temp_channel_number.to_string())
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set PWM Auto Channel temperature source value {temp_channel_number} for {} Reason: {err}",
                pwm_auto_channel_path.display()
            )
        })
}

/// This handles the special case of the `nzxt-kraken3` driver.
///
/// It contains a pwm-bases auto curve, but with the `temp` prefix and no `temp` suffixes.
/// This indicates that the temperature point values are fixed, and cannot be changed,
/// and so only setting `pwm` values is supported, for each temperature degree (point number) in
/// it's self-defined range: [20-49] degrees. Each of the 40 points is a fixed temperature value
/// in this range.
///
/// See: `https://docs.kernel.org/hwmon/nzxt-kraken3.html`
fn init_kraken3_auto_curve(base_path: &Path, fan: &mut HwmonChannelInfo) -> Result<()> {
    // the kraken auto points use the temp prefix, even though they're pwm-associated, so we treat
    // them like pwm_auto_points.
    let regex_temp_auto_points = Regex::new(format_temp_auto_point_regex!(fan.number).as_str())?;
    let mut found_auto_points = false;
    let mut can_set_pwm_point_values = false;
    let mut max_points = 0;
    let dir_entries = cc_fs::read_dir(base_path)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_temp_auto_points.is_match(file_name).not() {
            continue;
        }
        found_auto_points = true;
        let captures = regex_temp_auto_points
            .captures(file_name)
            .context("Can't match captured regex")?;
        let point_number: u8 = captures
            .name("point_number")
            .context("point_number group should exist")?
            .as_str()
            .parse()?;
        let point_value_type = captures
            .name("type")
            .context("type group should exist")?
            .as_str();
        if point_value_type == "pwm" {
            can_set_pwm_point_values = true;
        }
        if point_number > max_points {
            max_points = point_number;
        }
    }
    if found_auto_points.not() {
        warn!(
            "Kraken3 HWMon Auto Curve: Expected auto points, but found none. {}",
            base_path.display()
        );
        return Ok(());
    }
    if can_set_pwm_point_values.not() {
        warn!(
            "Kraken3 HWMon Auto Curve pwm-associated points do not allow setting pwm values. This is not supported. {}",
            base_path.display()
        );
        return Ok(());
    }
    if max_points != POINT_LENGTH_NZXT_KRAKEN3 {
        error!(
            "Kraken3 HWMon Auto Curve: detected {} points but {} is expected. This shouldn't happen. {}",
            max_points,
            POINT_LENGTH_NZXT_KRAKEN3,
            base_path.display()
        );
        return Ok(());
    }
    fan.auto_curve = AutoCurveInfo::PWM {
        point_length: max_points,
    };
    debug!("Auto curve for nzxt-krakens detected and enabled.");
    Ok(())
}

fn interpolate_kraken3_curve(speed_profile: &[(Temp, Duty)]) -> Vec<CurvePWM> {
    let mut interpolated_pwms = Vec::with_capacity(POINT_LENGTH_NZXT_KRAKEN3 as usize);
    let mut normalized_profile = Vec::with_capacity(speed_profile.len());
    for (temp, duty) in speed_profile {
        let clamped_duty = if CURVE_RANGE_DUTY.contains(duty) {
            *duty
        } else {
            warn!(
                "HWMon Kraken3 Auto Curve - Only fan duty values within range of {}% to {}% are allowed. \
                    Clamping passed duty of {duty}% to nearest limit",
                CURVE_RANGE_DUTY.start(),
                CURVE_RANGE_DUTY.end(),
            );
            *CURVE_RANGE_DUTY
                .end()
                .min(CURVE_RANGE_DUTY.start().max(duty))
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let temp_integer = temp.round() as CurveTemp;
        let clamped_temp = if CURVE_RANGE_TEMP_NZXT_KRAKEN3.contains(&temp_integer) {
            temp_integer
        } else {
            warn!(
                "HWMon Kraken3 Auto Curve - Only curve temps within range of {}C to {}C are allowed. \
                Clamping passed temp of {temp_integer}% to nearest limit",
                CURVE_RANGE_TEMP_NZXT_KRAKEN3.start(),
                CURVE_RANGE_TEMP_NZXT_KRAKEN3.end(),
            );
            *CURVE_RANGE_TEMP_NZXT_KRAKEN3
                .end()
                .min(CURVE_RANGE_TEMP_NZXT_KRAKEN3.start().max(&temp_integer))
        };
        normalized_profile.push((f64::from(clamped_temp), clamped_duty));
    }
    for temp in CURVE_RANGE_TEMP_NZXT_KRAKEN3 {
        // the kraken3 driver only allows setting pwm values and the temp values are fixed,
        // so we interpolate to get the full range of pwm values.
        let duty = engine::utils::interpolate_profile(&normalized_profile, f64::from(temp));
        let pwm_value = fans::duty_to_pwm_value(duty);
        interpolated_pwms.push(pwm_value);
    }
    interpolated_pwms
}

async fn apply_kraken3_curve(
    base_path: &Path,
    pwm_channel_number: u8,
    interpolated_pwms: Vec<CurvePWM>,
) -> Result<()> {
    if interpolated_pwms.len() != POINT_LENGTH_NZXT_KRAKEN3 as usize {
        return Err(anyhow!(
            "Kraken3 HWMon Auto Curve: detected {} points but {} is expected. This shouldn't happen. {}",
            interpolated_pwms.len(),
            POINT_LENGTH_NZXT_KRAKEN3,
            base_path.display()
        ));
    }
    for (index, pwm) in interpolated_pwms.into_iter().enumerate() {
        let point = index + 1;
        // the kraken3 uses temp, since it has fixed temp values (doesn't make sense to me, but hey)
        set_temp_auto_point_pwm(base_path, pwm_channel_number, point, pwm).await?;
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::path::{Path, PathBuf};
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
    fn is_temp_based_success() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_auto_channels_temp"),
                b"1".to_vec(),
            )
            .await
            .unwrap();

            // when
            let result = is_temp_based(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn is_temp_based_missing() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm8_auto_channels_temp"),
                b"1".to_vec(),
            )
            .await
            .unwrap();

            // when
            let result = is_temp_based(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(
                result.not(),
                "pwm number is different and should return false"
            );
        });
    }

    #[test]
    #[serial]
    fn is_pwm_based_success() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_auto_point1_pwm"), b"1".to_vec())
                .await
                .unwrap();

            // when
            let result = is_pwm_based(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn is_pwm_based_missing() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm8_auto_point1_pwm"), b"1".to_vec())
                .await
                .unwrap();

            // when
            let result = is_pwm_based(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(
                result.not(),
                "pwm number is different and should return false"
            );
        });
    }

    #[test]
    #[serial]
    fn cap_speed_profile_truncates_and_keeps_last() {
        // given
        let profile = vec![(20.0, 10u8), (40.0, 50u8), (60.0, 80u8), (80.0, 100u8)];
        let fan_curve_length = 3usize;

        // when
        let capped = cap_speed_profile(&profile, fan_curve_length);

        // then
        assert_eq!(capped.len(), fan_curve_length);
        assert_eq!(capped[0], (20.0, 10u8));
        assert_eq!(capped[1], (40.0, 50u8));
        // last element should be the original last from the input
        assert_eq!(capped[2], (80.0, 100u8));
    }

    #[test]
    #[serial]
    fn normalize_speed_profile_clamps_and_fills() {
        // given
        // Includes out-of-range values to exercise clamping
        let profile = vec![(
            -10.0, // below min temp
            255u8, // above 100% duty, but treated as duty in percent by duty_to_pwm_value
        )];
        let curve_len = 3usize;

        // when
        let normalized = normalize_speed_profile(&profile, curve_len);

        // then
        assert_eq!(normalized.len(), curve_len);
        // temp should be clamped to 0..=100_000 and rounded to u32
        assert!(
            normalized[0].0 >= *CURVE_RANGE_TEMP.start()
                && normalized[0].0 <= *CURVE_RANGE_TEMP.end()
        );
        assert_eq!(normalized[0].0, 0);
        // pwm value should be within 0..=255
        assert!(
            normalized[0].1 >= *CURVE_RANGE_PWM.start()
                && normalized[0].1 <= *CURVE_RANGE_PWM.end()
        );
        // remaining points should be filled with the last point
        assert_eq!(normalized[1], normalized[0]);
        assert_eq!(normalized[2], normalized[0]);
    }

    #[test]
    #[serial]
    fn normalize_speed_profile_output() {
        // given
        let profile = vec![(
            40.3, // 40.3C
            50u8, // 50%
        )];
        let curve_len = 2usize;

        // when
        let normalized = normalize_speed_profile(&profile, curve_len);

        // then
        assert_eq!(normalized[0].0, 40_300);
        assert_eq!(normalized[0].1, 128);
    }

    #[test]
    #[serial]
    fn interpolate_kraken3_curve_basic() {
        // given
        // A simple 2-point profile spanning the kraken range to check length and rough behavior
        let profile = vec![(20.0, 0u8), (59.0, 100u8)];

        // when
        let pwms = interpolate_kraken3_curve(&profile);

        // then
        assert_eq!(pwms.len(), POINT_LENGTH_NZXT_KRAKEN3 as usize);
        // endpoints should map close to min/max after conversion (within bounds 0..=255)
        assert!(!pwms.is_empty());
        assert!(pwms.last().is_some());
        assert!(pwms[0] <= *CURVE_RANGE_PWM.end());
        assert!(pwms[0] >= *CURVE_RANGE_PWM.start());
        assert!(pwms.last().unwrap() <= &CURVE_RANGE_PWM.end().to_owned());
        assert!(pwms.last().unwrap() >= &CURVE_RANGE_PWM.start().to_owned());
    }

    #[test]
    #[serial]
    fn read_z53_auto_curve_detects_and_sets_pwm_curve() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given: create temp1_auto_point{1..40}_pwm to simulate kraken3
            for i in 1..=POINT_LENGTH_NZXT_KRAKEN3 as usize {
                let name = format!("temp1_auto_point{i}_pwm");
                cc_fs::write(test_base_path.join(name), b"0".to_vec())
                    .await
                    .unwrap();
            }
            let mut fan = HwmonChannelInfo {
                number: 1,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                ..Default::default()
            };

            // when
            let res = init_kraken3_auto_curve(test_base_path, &mut fan);

            // then
            teardown(&ctx);
            assert!(res.is_ok());
            match fan.auto_curve {
                AutoCurveInfo::PWM { point_length } => {
                    assert_eq!(point_length, POINT_LENGTH_NZXT_KRAKEN3);
                }
                _ => panic!("Expected PWM auto curve for kraken3"),
            }
        });
    }

    #[test]
    #[serial]
    fn init_temp_based_curve_detects_points_and_sets_map() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given: two temps with different number of points
            cc_fs::write(
                test_base_path.join("temp1_auto_point1_temp"),
                b"20".to_vec(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("temp1_auto_point1_pwm"), b"10".to_vec())
                .await
                .unwrap();
            cc_fs::write(
                test_base_path.join("temp1_auto_point2_temp"),
                b"30".to_vec(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("temp1_auto_point2_pwm"), b"20".to_vec())
                .await
                .unwrap();
            cc_fs::write(
                test_base_path.join("temp2_auto_point1_temp"),
                b"25".to_vec(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("temp2_auto_point1_pwm"), b"15".to_vec())
                .await
                .unwrap();

            let mut fan = HwmonChannelInfo {
                number: 1,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                ..Default::default()
            };

            // when
            let res = init_temp_based_curve(test_base_path, &mut fan);

            // then
            teardown(&ctx);
            assert!(res.is_ok());
            match fan.auto_curve {
                AutoCurveInfo::Temp { temp_lengths } => {
                    assert_eq!(temp_lengths.get("temp1"), Some(&2));
                    assert_eq!(temp_lengths.get("temp2"), Some(&1));
                }
                _ => panic!("Expected Temp auto curve"),
            }
        });
    }

    #[test]
    #[serial]
    fn init_pwm_based_curve_detects_and_sets_length() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("pwm1_auto_point1_temp"), b"20".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point1_pwm"), b"10".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point2_temp"), b"30".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point2_pwm"), b"20".to_vec())
                .await
                .unwrap();

            let mut fan = HwmonChannelInfo {
                number: 1,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                ..Default::default()
            };

            // when
            let res = init_pwm_based_curve(test_base_path, &mut fan);

            // then
            teardown(&ctx);
            assert!(res.is_ok());
            match fan.auto_curve {
                AutoCurveInfo::PWM { point_length } => assert_eq!(point_length, 2),
                _ => panic!("Expected PWM auto curve"),
            }
        });
    }

    #[test]
    #[serial]
    fn apply_pwm_curve_writes_expected_files() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            // Pre-create files which apply_pwm_curve will write to
            cc_fs::write(test_base_path.join("pwm1_auto_point1_pwm"), b"".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point1_temp"), b"".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point2_pwm"), b"".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point2_temp"), b"".to_vec())
                .await
                .unwrap();

            let channel = HwmonChannelInfo {
                number: 1,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                ..Default::default()
            };
            let curve = vec![(30_000u32, 100u8), (40_000u32, 120u8)];

            // when
            let res = apply_pwm_curve(test_base_path, channel.number, curve.clone()).await;

            // then
            assert!(res.is_ok());
            let pwm1 = cc_fs::read_sysfs(test_base_path.join("pwm1_auto_point1_pwm"))
                .await
                .unwrap();
            let t1 = cc_fs::read_sysfs(test_base_path.join("pwm1_auto_point1_temp"))
                .await
                .unwrap();
            let pwm2 = cc_fs::read_sysfs(test_base_path.join("pwm1_auto_point2_pwm"))
                .await
                .unwrap();
            let t2 = cc_fs::read_sysfs(test_base_path.join("pwm1_auto_point2_temp"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(pwm1.trim(), curve[0].1.to_string());
            assert_eq!(t1.trim(), curve[0].0.to_string());
            assert_eq!(pwm2.trim(), curve[1].1.to_string());
            assert_eq!(t2.trim(), curve[1].0.to_string());
        });
    }

    #[test]
    #[serial]
    fn apply_temp_curve_writes_expected_files() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("temp1_auto_point1_pwm"), b"".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("temp1_auto_point1_temp"), b"".to_vec())
                .await
                .unwrap();
            let temp_source_channel = HwmonChannelInfo {
                number: 1,
                name: "temp1".to_string(),
                ..Default::default()
            };
            let curve = vec![(25_000u32, 80u8)];

            // when
            let res =
                apply_temp_curve(test_base_path, temp_source_channel.number, curve.clone()).await;

            // then
            assert!(res.is_ok());
            let pwm = cc_fs::read_sysfs(test_base_path.join("temp1_auto_point1_pwm"))
                .await
                .unwrap();
            let t = cc_fs::read_sysfs(test_base_path.join("temp1_auto_point1_temp"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(pwm.trim(), curve[0].1.to_string());
            assert_eq!(t.trim(), curve[0].0.to_string());
        });
    }

    #[test]
    #[serial]
    fn apply_temp_curve_to_pwm_channel_writes_expected_file() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("pwm1_auto_channels_temp"), b"".to_vec())
                .await
                .unwrap();
            let temp_channel_number: u8 = 2;
            let pwm_channel_number: u8 = 1;

            // when
            let res = apply_temp_curve_to_pwm_channel(
                test_base_path,
                temp_channel_number,
                pwm_channel_number,
            )
            .await;

            // then
            assert!(res.is_ok());
            let val = cc_fs::read_sysfs(test_base_path.join("pwm1_auto_channels_temp"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(val.trim(), temp_channel_number.to_string());
        });
    }

    #[test]
    #[serial]
    fn apply_kraken3_curve_writes_expected_pwm_points() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given: create files for all kraken points
            for i in 1..=POINT_LENGTH_NZXT_KRAKEN3 as usize {
                let name = format!("temp1_auto_point{i}_pwm");
                cc_fs::write(test_base_path.join(name), b"".to_vec())
                    .await
                    .unwrap();
            }
            let channel = HwmonChannelInfo {
                number: 1,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                ..Default::default()
            };
            let pwms = vec![128u8; POINT_LENGTH_NZXT_KRAKEN3 as usize];

            // when
            let res = apply_kraken3_curve(test_base_path, channel.number, pwms.clone()).await;

            // then
            assert!(res.is_ok());
            // spot check a few points
            let p1 = cc_fs::read_sysfs(test_base_path.join("temp1_auto_point1_pwm"))
                .await
                .unwrap();
            let p20 = cc_fs::read_sysfs(test_base_path.join("temp1_auto_point20_pwm"))
                .await
                .unwrap();
            let p40 = cc_fs::read_sysfs(test_base_path.join("temp1_auto_point40_pwm"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(p1.trim(), "128");
            assert_eq!(p20.trim(), "128");
            assert_eq!(p40.trim(), "128");
        });
    }

    #[test]
    #[serial]
    fn is_temp_sel_success() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_temp_sel"), b"1".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("pwm1_auto_point1_pwm"), b"1".to_vec())
                .await
                .unwrap();

            // when
            let result = is_temp_sel(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn is_temp_sel_missing_temp_sel() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_auto_point1_pwm"), b"1".to_vec())
                .await
                .unwrap();

            // when
            let result = is_temp_sel(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(
                result.not(),
                "should return false when pwm_temp_sel is missing"
            );
        });
    }

    #[test]
    #[serial]
    fn is_temp_sel_missing_pwm_based() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("pwm1_temp_sel"), b"1".to_vec())
                .await
                .unwrap();

            // when
            let result = is_temp_sel(test_base_path, 1);

            // then
            teardown(&ctx);
            assert!(
                result.not(),
                "should return false when pwm_auto_point1_pwm is missing"
            );
        });
    }

    #[test]
    #[serial]
    fn set_pwm_auto_point_pwm_writes_expected_value() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("pwm2_auto_point3_pwm"), b"".to_vec())
                .await
                .unwrap();

            // when
            let res = set_pwm_auto_point_pwm(test_base_path, 2, 3, 150).await;

            // then
            assert!(res.is_ok());
            let val = cc_fs::read_sysfs(test_base_path.join("pwm2_auto_point3_pwm"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(val.trim(), "150");
        });
    }

    #[test]
    #[serial]
    fn set_pwm_auto_point_temp_writes_expected_value() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("pwm3_auto_point2_temp"), b"".to_vec())
                .await
                .unwrap();

            // when
            let res = set_pwm_auto_point_temp(test_base_path, 3, 2, 45_000).await;

            // then
            assert!(res.is_ok());
            let val = cc_fs::read_sysfs(test_base_path.join("pwm3_auto_point2_temp"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(val.trim(), "45000");
        });
    }

    #[test]
    #[serial]
    fn set_temp_auto_point_pwm_writes_expected_value() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("temp2_auto_point1_pwm"), b"".to_vec())
                .await
                .unwrap();

            // when
            let res = set_temp_auto_point_pwm(test_base_path, 2, 1, 200).await;

            // then
            assert!(res.is_ok());
            let val = cc_fs::read_sysfs(test_base_path.join("temp2_auto_point1_pwm"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(val.trim(), "200");
        });
    }

    #[test]
    #[serial]
    fn set_temp_auto_point_temp_writes_expected_value() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            let test_base_path = &ctx.test_base_path;
            // given
            cc_fs::write(test_base_path.join("temp3_auto_point4_temp"), b"".to_vec())
                .await
                .unwrap();

            // when
            let res = set_temp_auto_point_temp(test_base_path, 3, 4, 60_000).await;

            // then
            assert!(res.is_ok());
            let val = cc_fs::read_sysfs(test_base_path.join("temp3_auto_point4_temp"))
                .await
                .unwrap();
            teardown(&ctx);
            assert_eq!(val.trim(), "60000");
        });
    }
}
