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
use crate::device::{ChannelStatus, Mhz};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use anyhow::{Context, Result};
use log::{trace, warn};
use regex::Regex;
use std::io::{Error, ErrorKind};
use std::path::PathBuf;
use zbus::export::futures_util::future::join_all;

const PATTERN_FREQ_INPUT_NUMBER: &str = r"^freq(?P<number>\d+)_input$";

pub async fn init_freqs(base_path: &PathBuf) -> Result<Vec<HwmonChannelInfo>> {
    let mut freqs = vec![];
    let dir_entries = cc_fs::read_dir(base_path)?;
    let regex_freq_input = Regex::new(PATTERN_FREQ_INPUT_NUMBER)?;
    for entry in dir_entries {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if regex_freq_input.is_match(file_name) {
            let channel_number: u8 = regex_freq_input
                .captures(file_name)
                .context("Freq Number should exist")?
                .name("number")
                .context("Number Group should exist")?
                .as_str()
                .parse()?;
            if !sensor_is_usable(base_path, &channel_number).await {
                continue;
            }
            let channel_name = get_freq_channel_name(channel_number);
            let label = get_freq_channel_label(base_path, &channel_number).await;
            freqs.push(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Freq,
                number: channel_number,
                name: channel_name,
                label,
                ..Default::default()
            });
        }
    }
    freqs.sort_by(|f1, f2| f1.number.cmp(&f2.number));
    trace!(
        "Hwmon Frequencies detected: {:?} for {:?}",
        freqs,
        base_path
    );
    Ok(freqs)
}

pub async fn extract_freq_statuses(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut freqs = vec![];
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Freq {
            continue;
        }
        let freq = cc_fs::read_sysfs(driver.path.join(format!("freq{}_input", channel.number)))
            .await
            .and_then(check_parsing_64)
            .map(|hertz| (hertz / 1_000_000) as Mhz)
            .unwrap_or_default();
        freqs.push(ChannelStatus {
            name: channel.name.clone(),
            freq: Some(freq),
            ..Default::default()
        });
    }
    freqs
}

#[allow(dead_code)]
pub async fn extract_freq_statuses_concurrently(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut freq_tasks = vec![];
    moro_local::async_scope!(|scope| {
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Freq {
                continue;
            }
            let freq_task = scope.spawn(async {
                let freq =
                    cc_fs::read_sysfs(driver.path.join(format!("freq{}_input", channel.number)))
                        .await
                        .and_then(check_parsing_64)
                        .map(|hertz| (hertz / 1_000_000) as Mhz)
                        .unwrap_or_default();
                ChannelStatus {
                    name: channel.name.clone(),
                    freq: Some(freq),
                    ..Default::default()
                }
            });
            freq_tasks.push(freq_task);
        }
        join_all(freq_tasks).await
    })
    .await
}

async fn sensor_is_usable(base_path: &PathBuf, channel_number: &u8) -> bool {
    cc_fs::read_sysfs(base_path.join(format!("freq{channel_number}_input")))
        .await
        .and_then(check_parsing_64)
        .map(|hertz| (hertz / 1_000_000) as Mhz)
        .inspect_err(|err| {
            warn!(
                "Error reading frequency value from: {base_path:?}/freq{channel_number}_input - {err}"
            );
        })
        .is_ok()
}

fn check_parsing_64(content: String) -> Result<u64> {
    match content.trim().parse::<u64>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

async fn get_freq_channel_label(base_path: &PathBuf, channel_number: &u8) -> Option<String> {
    cc_fs::read_txt(base_path.join(format!("freq{channel_number}_label")))
        .await
        .ok()
        .and_then(|label| {
            let freq_label = label.trim();
            if freq_label.is_empty() {
                warn!(
                    "Freq label is empty: {:?}/freq{}_label",
                    base_path, channel_number
                );
                None
            } else {
                Some(freq_label.to_string())
            }
        })
}

fn get_freq_channel_name(channel_number: u8) -> String {
    format!("freq{channel_number}")
}
