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
use crate::device::{ChannelStatus, Mhz};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use anyhow::{Context, Result};
use futures_util::future::join_all;
use log::{info, trace};
use regex::Regex;
use std::io::{Error, ErrorKind};
use std::path::{Path, PathBuf};

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
    freqs.sort_by_key(|f| f.number);
    trace!(
        "Hwmon Frequencies detected: {freqs:?} for {}",
        base_path.display()
    );
    Ok(freqs)
}

/// Extract frequency statuses. Failed reads are omitted from the
/// returned vector rather than reported as 0 MHz so downstream does not
/// see a fabricated frequency.
pub async fn extract_freq_statuses(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let freq_channel_count = driver
        .channels
        .iter()
        .filter(|c| c.hwmon_type == HwmonChannelType::Freq)
        .count();
    let mut freqs = Vec::with_capacity(freq_channel_count);
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Freq {
            continue;
        }
        let result = cc_fs::read_sysfs(driver.path.join(format!("freq{}_input", channel.number)))
            .await
            .and_then(check_parsing_64)
            .map(hertz_to_megahertz);
        if let Ok(freq) = result {
            freqs.push(ChannelStatus {
                name: channel.name.clone(),
                freq: Some(freq),
                ..Default::default()
            });
        }
    }
    freqs
}

#[allow(dead_code)]
pub async fn extract_freq_statuses_concurrently(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
    let mut freq_tasks = vec![];
    moro_local::async_scope!(|scope| -> Vec<Result<ChannelStatus, anyhow::Error>> {
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Freq {
                continue;
            }
            let freq_task = scope.spawn(async {
                let result =
                    cc_fs::read_sysfs(driver.path.join(format!("freq{}_input", channel.number)))
                        .await
                        .and_then(check_parsing_64)
                        .map(hertz_to_megahertz);
                result.map(|freq| ChannelStatus {
                    name: channel.name.clone(),
                    freq: Some(freq),
                    ..Default::default()
                })
            });
            freq_tasks.push(freq_task);
        }
        join_all(freq_tasks).await
    })
    .await
    .into_iter()
    .filter_map(Result::ok)
    .collect()
}

async fn sensor_is_usable(base_path: &Path, channel_number: &u8) -> bool {
    cc_fs::read_sysfs(base_path.join(format!("freq{channel_number}_input")))
        .await
        .and_then(check_parsing_64)
        .map(hertz_to_megahertz)
        .inspect_err(|err| {
            info!(
                "Error reading frequency value from: {}/freq{channel_number}_input - {err}",
                base_path.display(),
            );
        })
        .is_ok()
}

#[allow(clippy::cast_possible_truncation)]
fn hertz_to_megahertz(hertz: u64) -> Mhz {
    (hertz / 1_000_000) as Mhz
}

#[allow(clippy::needless_pass_by_value)]
fn check_parsing_64(content: String) -> Result<u64> {
    match content.trim().parse::<u64>() {
        Ok(value) => Ok(value),
        Err(err) => Err(Error::new(ErrorKind::InvalidData, err.to_string()).into()),
    }
}

async fn get_freq_channel_label(base_path: &Path, channel_number: &u8) -> Option<String> {
    cc_fs::read_txt(base_path.join(format!("freq{channel_number}_label")))
        .await
        .ok()
        .and_then(|label| {
            let freq_label = label.trim();
            if freq_label.is_empty() {
                info!(
                    "Freq label is empty: {}/freq{channel_number}_label",
                    base_path.display()
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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::ops::Not;
    use std::path::Path;
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
    fn extract_freq_all_channels_succeed() {
        // Verifies freq channels return real MHz values when readable.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: two freq channels, both readable (hertz values).
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("freq1_input"),
                b"3500000000".to_vec(), // 3500 MHz
            )
            .await
            .unwrap();
            cc_fs::write(
                test_base_path.join("freq2_input"),
                b"1800000000".to_vec(), // 1800 MHz
            )
            .await
            .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Freq,
                        number: 1,
                        name: "freq1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Freq,
                        number: 2,
                        name: "freq2".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let freqs = extract_freq_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(freqs.len(), 2);
            assert_eq!(freqs[0].name, "freq1");
            assert_eq!(freqs[0].freq, Some(3500));
            assert_eq!(freqs[1].name, "freq2");
            assert_eq!(freqs[1].freq, Some(1800));
        });
    }

    #[test]
    #[serial]
    fn extract_freq_skips_failed_reads() {
        // Verifies a missing sysfs file omits the entry entirely rather
        // than fabricating a 0 MHz reading.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: single freq channel with no sysfs file.
            let test_base_path = &ctx.test_base_path;
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Freq,
                    number: 1,
                    name: "freq1".to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };

            // when:
            let freqs = extract_freq_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(freqs.len(), 0);
        });
    }

    #[test]
    #[serial]
    fn extract_freq_partial_failure_skips_only_failing_channels() {
        // Verifies that when one freq channel reads successfully and
        // another fails, only the successful one is returned.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: two freq channels, only the first one readable.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("freq1_input"),
                b"2500000000".to_vec(), // 2500 MHz
            )
            .await
            .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Freq,
                        number: 1,
                        name: "freq1".to_string(),
                        ..Default::default()
                    },
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Freq,
                        number: 2,
                        name: "freq2".to_string(),
                        ..Default::default()
                    },
                ],
                ..Default::default()
            };

            // when:
            let freqs = extract_freq_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(freqs.len(), 1);
            assert_eq!(freqs[0].name, "freq1");
            assert_eq!(freqs[0].freq, Some(2500));
        });
    }

    #[test]
    #[serial]
    fn extract_freq_ignores_non_freq_channels() {
        // Verifies non-Freq channels are ignored so their sysfs paths are
        // never queried here.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: one Freq and one non-Freq channel.
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("freq1_input"), b"1000000000".to_vec())
                .await
                .unwrap();
            let driver_info = HwmonDriverInfo {
                path: test_base_path.to_owned(),
                channels: vec![
                    HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Freq,
                        number: 1,
                        name: "freq1".to_string(),
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
            let freqs = extract_freq_statuses(&driver_info).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(freqs.len(), 1);
            assert_eq!(freqs[0].name, "freq1");
            // Sanity: only one entry despite two channels.
            assert!(freqs.iter().any(|f| f.name == "fan1").not());
        });
    }
}
