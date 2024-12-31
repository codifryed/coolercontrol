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
}
