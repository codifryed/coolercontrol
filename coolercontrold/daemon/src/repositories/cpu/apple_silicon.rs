// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::ops::Not;
use std::path::Path;

use crate::cc_fs;
use crate::device::{ChannelStatus, Mhz};
use crate::repositories::cpu::topology::{CpuFreqs, PhysicalID};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};

const DEVICE_TREE_COMPATIBLE_PATH: &str = "/sys/firmware/devicetree/base/compatible";
const DEVICE_TREE_MODEL_PATH: &str = "/sys/firmware/devicetree/base/model";
const APPLE_SILICON_PLATFORM: &[u8] = b"apple,arm-platform";
const CPUFREQ_POLICY_PATH: &str = "/sys/devices/system/cpu/cpufreq";
const CURRENT_FREQ_FILE: &str = "cpufreq/cpuinfo_cur_freq";
const POLICY_CURRENT_FREQ_FILE: &str = "cpuinfo_cur_freq";
const POLICY_MAX_FREQ_FILE: &str = "cpuinfo_max_freq";
const SCALING_DRIVER_FILE: &str = "cpufreq/policy0/scaling_driver";
const CLUSTER_FREQ_PREFIX: &str = "CPU Freq";
const FIRST_CLUSTER_CHANNEL_NUMBER: u8 = 3;
pub const CPU_SYSFS_PATH: &str = "/sys/devices/system/cpu";
pub const DRIVER_NAME: &str = "cpufreq";
pub const PACKAGE: PhysicalID = 0;

pub async fn board_model() -> Option<String> {
    let compatible = cc_fs::read_bytes(DEVICE_TREE_COMPATIBLE_PATH).await.ok()?;
    let model = cc_fs::read_bytes(DEVICE_TREE_MODEL_PATH).await.ok()?;
    board_model_from(&compatible, &model)
}

fn board_model_from(compatible: &[u8], model: &[u8]) -> Option<String> {
    let is_apple_silicon = compatible
        .split(|byte| *byte == 0)
        .any(|entry| entry == APPLE_SILICON_PLATFORM);
    if is_apple_silicon.not() {
        return None;
    }
    let model = String::from_utf8_lossy(model);
    let model = model.trim_end_matches('\0').trim();
    model.is_empty().not().then(|| model.to_owned())
}

pub async fn scaling_driver() -> Option<String> {
    cc_fs::read_sysfs(format!("{CPU_SYSFS_PATH}/{SCALING_DRIVER_FILE}"))
        .await
        .ok()
        .map(|name| name.trim().to_owned())
        .filter(|name| name.is_empty().not())
}

async fn read_khz(path: &Path) -> Option<u32> {
    cc_fs::read_sysfs(path).await.ok()?.trim().parse().ok()
}

pub async fn collect_freqs() -> HashMap<PhysicalID, CpuFreqs> {
    let Ok(entries) = cc_fs::read_dir(CPU_SYSFS_PATH) else {
        return HashMap::new();
    };
    let mut current_khz = Vec::new();
    for entry in entries.flatten() {
        if entry
            .file_name()
            .to_str()
            .is_none_or(|name| is_cpu_dir(name).not())
        {
            continue;
        }
        if let Some(khz) = read_khz(&entry.path().join(CURRENT_FREQ_FILE)).await {
            current_khz.push(khz);
        }
    }
    freqs_from_khz(&current_khz)
}

pub async fn collect_cluster_freqs() -> Vec<ChannelStatus> {
    let Ok(entries) = cc_fs::read_dir(CPUFREQ_POLICY_PATH) else {
        return Vec::new();
    };
    let mut policies = Vec::new();
    for entry in entries.flatten() {
        let Some(number) = entry.file_name().to_str().and_then(policy_number) else {
            continue;
        };
        let path = entry.path();
        let Some(max_khz) = read_khz(&path.join(POLICY_MAX_FREQ_FILE)).await else {
            continue;
        };
        let current_khz = read_khz(&path.join(POLICY_CURRENT_FREQ_FILE)).await;
        policies.push(Policy {
            number,
            max_khz,
            current_khz,
        });
    }
    cluster_statuses(policies)
}

pub fn cluster_channels(cluster_freqs: &[ChannelStatus]) -> Vec<HwmonChannelInfo> {
    (FIRST_CLUSTER_CHANNEL_NUMBER..)
        .zip(cluster_freqs)
        .map(|(number, cluster)| HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Freq,
            number,
            name: cluster.name.clone(),
            label: Some(cluster.name.clone()),
            ..Default::default()
        })
        .collect()
}

pub fn fill_cluster_freqs(cluster_freqs: &[ChannelStatus], statuses: &mut [ChannelStatus]) {
    for status in statuses {
        if let Some(cluster) = cluster_freqs.iter().find(|c| c.name == status.name) {
            status.freq = cluster.freq;
        }
    }
}

struct Policy {
    number: u32,
    max_khz: u32,
    current_khz: Option<u32>,
}

fn policy_number(name: &str) -> Option<u32> {
    name.strip_prefix("policy")?.parse().ok()
}

fn cluster_statuses(mut policies: Vec<Policy>) -> Vec<ChannelStatus> {
    policies.sort_by_key(|policy| policy.number);
    let max_khz: Vec<u32> = policies.iter().map(|policy| policy.max_khz).collect();
    cluster_channel_names(&max_khz)
        .into_iter()
        .zip(policies)
        .filter_map(|(name, policy)| {
            policy.current_khz.map(|khz| ChannelStatus {
                name,
                freq: Some(khz_to_mhz(f64::from(khz))),
                ..Default::default()
            })
        })
        .collect()
}

fn cluster_channel_names(max_khz: &[u32]) -> Vec<String> {
    let lowest = max_khz.iter().min().copied();
    let mixed = max_khz.iter().any(|max| Some(*max) != lowest);
    let kind = |max: u32| match (mixed, Some(max) == lowest) {
        (false, _) => "",
        (true, true) => "Efficiency ",
        (true, false) => "Performance ",
    };
    let mut seen: HashMap<&str, usize> = HashMap::new();
    max_khz
        .iter()
        .map(|max| {
            let cluster_kind = kind(*max);
            let of_kind = max_khz.iter().filter(|m| kind(**m) == cluster_kind).count();
            let nth = seen.entry(cluster_kind).or_default();
            *nth += 1;
            if of_kind > 1 {
                format!("{CLUSTER_FREQ_PREFIX} {cluster_kind}Cluster {nth}")
            } else {
                format!("{CLUSTER_FREQ_PREFIX} {cluster_kind}Cluster")
            }
        })
        .collect()
}

fn is_cpu_dir(name: &str) -> bool {
    name.strip_prefix("cpu")
        .is_some_and(|number| number.is_empty().not() && number.bytes().all(|b| b.is_ascii_digit()))
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn khz_to_mhz(khz: f64) -> Mhz {
    (khz / 1000.).round() as Mhz
}

fn freqs_from_khz(current_khz: &[u32]) -> HashMap<PhysicalID, CpuFreqs> {
    let (Some(min), Some(max)) = (current_khz.iter().min(), current_khz.iter().max()) else {
        return HashMap::new();
    };
    #[allow(clippy::cast_precision_loss)]
    let avg = current_khz.iter().copied().map(f64::from).sum::<f64>() / current_khz.len() as f64;
    HashMap::from([(
        PACKAGE,
        CpuFreqs {
            min: khz_to_mhz(f64::from(*min)),
            max: khz_to_mhz(f64::from(*max)),
            avg: khz_to_mhz(avg),
        },
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    const M1_MAX_COMPATIBLE: &[u8] = b"apple,j316c\0apple,t6001\0apple,arm-platform\0";
    const M1_MAX_MODEL: &[u8] = b"Apple MacBook Pro (16-inch, M1 Max, 2021)\0";

    #[test]
    fn an_apple_silicon_device_tree_names_the_board() {
        assert_eq!(
            board_model_from(M1_MAX_COMPATIBLE, M1_MAX_MODEL).as_deref(),
            Some("Apple MacBook Pro (16-inch, M1 Max, 2021)")
        );
    }

    #[test]
    fn any_other_device_tree_is_left_alone() {
        let raspberry_pi = b"raspberrypi,5-model-b\0brcm,bcm2712\0";
        assert_eq!(board_model_from(raspberry_pi, b"Raspberry Pi 5\0"), None);
        assert_eq!(
            board_model_from(b"apple,arm-platform-x\0", M1_MAX_MODEL),
            None
        );
        assert_eq!(board_model_from(M1_MAX_COMPATIBLE, b"\0"), None);
    }

    #[test]
    fn only_numbered_cpu_directories_are_cores() {
        assert!(is_cpu_dir("cpu0"));
        assert!(is_cpu_dir("cpu10"));
        assert!(is_cpu_dir("cpu").not());
        assert!(is_cpu_dir("cpufreq").not());
        assert!(is_cpu_dir("cpuidle").not());
        assert_eq!(policy_number("policy6"), Some(6));
        assert_eq!(policy_number("policy"), None);
        assert_eq!(policy_number("boost"), None);
    }

    #[test]
    fn frequencies_span_every_core_of_the_one_package() {
        let freqs = freqs_from_khz(&[600_000, 2_064_000, 3_228_000, 3_036_000]);
        assert_eq!(
            freqs.get(&PACKAGE),
            Some(&CpuFreqs {
                min: 600,
                max: 3228,
                avg: 2232,
            })
        );
        assert!(freqs_from_khz(&[]).is_empty());
    }

    #[test]
    fn clusters_are_named_by_their_kind() {
        assert_eq!(
            cluster_channel_names(&[2_064_000, 3_228_000, 3_228_000]),
            [
                "CPU Freq Efficiency Cluster",
                "CPU Freq Performance Cluster 1",
                "CPU Freq Performance Cluster 2"
            ]
        );
        assert_eq!(
            cluster_channel_names(&[2_064_000, 3_204_000]),
            [
                "CPU Freq Efficiency Cluster",
                "CPU Freq Performance Cluster"
            ]
        );
        assert_eq!(
            cluster_channel_names(&[2_064_000, 2_064_000, 3_228_000, 3_228_000]),
            [
                "CPU Freq Efficiency Cluster 1",
                "CPU Freq Efficiency Cluster 2",
                "CPU Freq Performance Cluster 1",
                "CPU Freq Performance Cluster 2"
            ]
        );
        assert_eq!(
            cluster_channel_names(&[2_000_000, 2_000_000]),
            ["CPU Freq Cluster 1", "CPU Freq Cluster 2"]
        );
    }

    #[test]
    fn cluster_statuses_follow_policy_order_and_skip_an_unreadable_clock() {
        let statuses = cluster_statuses(vec![
            Policy {
                number: 6,
                max_khz: 3_228_000,
                current_khz: None,
            },
            Policy {
                number: 0,
                max_khz: 2_064_000,
                current_khz: Some(972_000),
            },
            Policy {
                number: 2,
                max_khz: 3_228_000,
                current_khz: Some(3_036_000),
            },
        ]);
        let named: Vec<(&str, Option<Mhz>)> = statuses
            .iter()
            .map(|status| (status.name.as_str(), status.freq))
            .collect();
        assert_eq!(
            named,
            [
                ("CPU Freq Efficiency Cluster", Some(972)),
                ("CPU Freq Performance Cluster 1", Some(3036))
            ]
        );
    }

    #[test]
    fn cluster_clocks_fill_their_own_channels_only() {
        let clusters = [ChannelStatus {
            name: "CPU Freq Efficiency Cluster".to_string(),
            freq: Some(972),
            ..Default::default()
        }];
        let mut statuses = vec![
            ChannelStatus {
                name: "CPU Freq Efficiency Cluster".to_string(),
                freq: Some(0),
                ..Default::default()
            },
            ChannelStatus {
                name: "CPU Freq Max".to_string(),
                freq: Some(3036),
                ..Default::default()
            },
        ];
        fill_cluster_freqs(&clusters, &mut statuses);
        assert_eq!(statuses[0].freq, Some(972));
        assert_eq!(statuses[1].freq, Some(3036));
        let channels = cluster_channels(&clusters);
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].number, FIRST_CLUSTER_CHANNEL_NUMBER);
        assert_eq!(channels[0].hwmon_type, HwmonChannelType::Freq);
    }
}
