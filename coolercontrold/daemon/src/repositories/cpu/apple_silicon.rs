// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};

use crate::cc_fs;
use crate::device::{ChannelStatus, Mhz};
use crate::repositories::cpu::topology::{CpuFreqs, PhysicalID};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};

const DEVICE_TREE_COMPATIBLE_PATH: &str = "/sys/firmware/devicetree/base/compatible";
const DEVICE_TREE_MODEL_PATH: &str = "/sys/firmware/devicetree/base/model";
const APPLE_SILICON_PLATFORM: &[u8] = b"apple,arm-platform";
const POLICY_CURRENT_FREQ_FILE: &str = "cpuinfo_cur_freq";
const POLICY_MAX_FREQ_FILE: &str = "cpuinfo_max_freq";
const POLICY_CORES_FILE: &str = "related_cpus";
const SCALING_DRIVER_FILE: &str = "cpufreq/policy0/scaling_driver";
const CLUSTER_FREQ_PREFIX: &str = "CPU Freq";
const FIRST_CLUSTER_CHANNEL_NUMBER: u8 = 3;
/// More clusters than any Apple chip has. Bounds the scan and the channel numbers.
const CLUSTER_COUNT_MAX: usize = 16;
pub const CPU_SYSFS_PATH: &str = "/sys/devices/system/cpu";
pub const CPUFREQ_POLICY_PATH: &str = "/sys/devices/system/cpu/cpufreq";
pub const DRIVER_NAME: &str = "cpufreq";
pub const PACKAGE: PhysicalID = 0;

// Frequency channels are numbered in blocks of 64 per package, so the clusters must stay
// inside the block of the one package.
const _: () = assert!(FIRST_CLUSTER_CHANNEL_NUMBER as usize + CLUSTER_COUNT_MAX <= 64);

/// What the CPU repository keeps for an Apple Silicon machine, and has for no other.
pub struct AppleSilicon {
    /// Found once at startup. Empty when cpufreq has no policy to read.
    clusters: Vec<Cluster>,
    /// Each cluster's clock as of the latest poll, for the cluster channels.
    cluster_freqs: RefCell<Vec<ChannelStatus>>,
}

/// Cores that share one clock, which cpufreq calls a policy.
#[derive(Debug, Clone, PartialEq)]
struct Cluster {
    channel_name: String,
    current_freq_path: PathBuf,
    core_count: u32,
}

/// One cluster's clock in kHz, for this poll.
type ClusterReading<'a> = (&'a Cluster, u32);

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

impl AppleSilicon {
    /// Finds every cluster of the machine. A cluster's top frequency and its cores never
    /// change, so they are read here once and each poll reads only the clock.
    pub async fn find(cpufreq_path: &Path) -> Self {
        let clusters = find_clusters_in(cpufreq_path).await;
        debug_assert!(clusters.len() <= CLUSTER_COUNT_MAX);
        Self {
            clusters,
            cluster_freqs: RefCell::new(Vec::new()),
        }
    }

    /// The package frequencies, with one read per cluster: every core of a cluster runs at the
    /// cluster's clock. Each cluster's clock is kept until the next poll.
    pub async fn read_freqs(&self) -> HashMap<PhysicalID, CpuFreqs> {
        let mut readings = Vec::with_capacity(self.clusters.len());
        for cluster in &self.clusters {
            if let Some(khz) = read_khz(&cluster.current_freq_path).await {
                readings.push((cluster, khz));
            }
        }
        self.cluster_freqs.replace(cluster_statuses(&readings));
        package_freqs(&readings)
    }

    pub fn cluster_channels(&self) -> Vec<HwmonChannelInfo> {
        (FIRST_CLUSTER_CHANNEL_NUMBER..)
            .zip(&self.clusters)
            .map(|(number, cluster)| HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Freq,
                number,
                name: cluster.channel_name.clone(),
                label: Some(cluster.channel_name.clone()),
                ..Default::default()
            })
            .collect()
    }

    /// Gives each cluster channel the clock of the latest poll. A cluster that poll could not
    /// read keeps the value the caller gave it.
    pub fn fill_cluster_freqs(&self, statuses: &mut [ChannelStatus]) {
        let cluster_freqs = self.cluster_freqs.borrow();
        for status in statuses {
            if let Some(cluster) = cluster_freqs.iter().find(|c| c.name == status.name) {
                status.freq = cluster.freq;
            }
        }
    }
}

async fn find_clusters_in(cpufreq_path: &Path) -> Vec<Cluster> {
    let Ok(entries) = cc_fs::read_dir(cpufreq_path) else {
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
        policies.push(Policy {
            number,
            max_khz,
            core_count: read_core_count(&path.join(POLICY_CORES_FILE)).await,
            current_freq_path: path.join(POLICY_CURRENT_FREQ_FILE),
        });
    }
    clusters_from(policies)
}

struct Policy {
    number: u32,
    max_khz: u32,
    core_count: u32,
    current_freq_path: PathBuf,
}

fn policy_number(name: &str) -> Option<u32> {
    name.strip_prefix("policy")?.parse().ok()
}

/// The cores of a policy, counted from its space-separated list. An unreadable list counts as
/// one core, so the cluster still weighs in the average.
async fn read_core_count(path: &Path) -> u32 {
    let core_count = cc_fs::read_sysfs(path)
        .await
        .map_or(0, |cores| cores.split_whitespace().count());
    u32::try_from(core_count).unwrap_or(u32::MAX).max(1)
}

/// Names the clusters in policy order, which is the order of their channels.
fn clusters_from(mut policies: Vec<Policy>) -> Vec<Cluster> {
    policies.sort_by_key(|policy| policy.number);
    policies.truncate(CLUSTER_COUNT_MAX);
    let max_khz: Vec<u32> = policies.iter().map(|policy| policy.max_khz).collect();
    cluster_channel_names(&max_khz)
        .into_iter()
        .zip(policies)
        .map(|(channel_name, policy)| Cluster {
            channel_name,
            current_freq_path: policy.current_freq_path,
            core_count: policy.core_count,
        })
        .collect()
}

fn cluster_statuses(readings: &[ClusterReading]) -> Vec<ChannelStatus> {
    readings
        .iter()
        .map(|(cluster, khz)| ChannelStatus {
            name: cluster.channel_name.clone(),
            freq: Some(khz_to_mhz(f64::from(*khz))),
            ..Default::default()
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

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn khz_to_mhz(khz: f64) -> Mhz {
    (khz / 1000.).round() as Mhz
}

/// The slowest and fastest cluster, and the average over every core of the clusters read.
fn package_freqs(readings: &[ClusterReading]) -> HashMap<PhysicalID, CpuFreqs> {
    let khz = readings.iter().map(|(_, khz)| *khz);
    let (Some(min), Some(max)) = (khz.clone().min(), khz.max()) else {
        return HashMap::new();
    };
    let core_count: u32 = readings.iter().map(|(cluster, _)| cluster.core_count).sum();
    debug_assert!(core_count > 0);
    let khz_of_all_cores: f64 = readings
        .iter()
        .map(|(cluster, khz)| f64::from(*khz) * f64::from(cluster.core_count))
        .sum();
    HashMap::from([(
        PACKAGE,
        CpuFreqs {
            min: khz_to_mhz(f64::from(min)),
            max: khz_to_mhz(f64::from(max)),
            avg: khz_to_mhz(khz_of_all_cores / f64::from(core_count)),
        },
    )])
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use uuid::Uuid;

    const M1_MAX_COMPATIBLE: &[u8] = b"apple,j316c\0apple,t6001\0apple,arm-platform\0";
    const M1_MAX_MODEL: &[u8] = b"Apple MacBook Pro (16-inch, M1 Max, 2021)\0";

    /// Goal: an Apple Silicon machine is named by its device tree model. Method: the
    /// `compatible` and `model` properties of an M1 Max, as the kernel exposes them.
    #[test]
    fn an_apple_silicon_device_tree_names_the_board() {
        assert_eq!(
            board_model_from(M1_MAX_COMPATIBLE, M1_MAX_MODEL).as_deref(),
            Some("Apple MacBook Pro (16-inch, M1 Max, 2021)")
        );
    }

    /// Goal: only an Apple Silicon device tree yields a name, and never an empty one. Method:
    /// another board's properties, a near miss of the Apple platform string, and an empty model.
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

    fn cluster(channel_name: &str, core_count: u32) -> Cluster {
        Cluster {
            channel_name: channel_name.to_string(),
            current_freq_path: PathBuf::new(),
            core_count,
        }
    }

    fn policy(number: u32, max_khz: u32) -> Policy {
        Policy {
            number,
            max_khz,
            core_count: 1,
            current_freq_path: PathBuf::from(format!("policy{number}")),
        }
    }

    /// Goal: only `policyN` directories count as clusters. Method: the names cpufreq has
    /// beside its policies.
    #[test]
    fn only_policy_directories_are_clusters() {
        assert_eq!(policy_number("policy6"), Some(6));
        assert_eq!(policy_number("policy"), None);
        assert_eq!(policy_number("boost"), None);
    }

    /// Goal: the package frequencies cover every core while reading each cluster once.
    /// Method: three clusters of unequal size, so a plain mean of the clusters would be wrong.
    #[test]
    fn frequencies_span_every_core_of_the_one_package() {
        let efficiency = cluster("E", 2);
        let performance = cluster("P", 4);
        let readings = [
            (&efficiency, 600_000),
            (&performance, 2_064_000),
            (&performance, 3_228_000),
        ];
        assert_eq!(
            package_freqs(&readings).get(&PACKAGE),
            Some(&CpuFreqs {
                min: 600,
                max: 3228,
                avg: 2237,
            })
        );
        assert!(package_freqs(&[]).is_empty());
    }

    /// Goal: clusters are named by kind from their top frequencies, numbered only when a kind
    /// has more than one, and given no kind when all are alike. Method: the top frequencies of
    /// machines with one and two clusters of each kind, and of two equal clusters.
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

    /// Goal: clusters keep policy order whatever order the directory lists them in, and the
    /// count is bounded. Method: policies out of order, then more than the limit.
    #[test]
    fn clusters_follow_policy_order_and_are_bounded() {
        let clusters = clusters_from(vec![
            policy(6, 3_228_000),
            policy(0, 2_064_000),
            policy(2, 3_228_000),
        ]);
        let named: Vec<(&str, &Path)> = clusters
            .iter()
            .map(|c| (c.channel_name.as_str(), c.current_freq_path.as_path()))
            .collect();
        assert_eq!(
            named,
            [
                ("CPU Freq Efficiency Cluster", Path::new("policy0")),
                ("CPU Freq Performance Cluster 1", Path::new("policy2")),
                ("CPU Freq Performance Cluster 2", Path::new("policy6")),
            ]
        );
        let too_many = (0..40).map(|number| policy(number, 2_000_000)).collect();
        assert_eq!(clusters_from(too_many).len(), CLUSTER_COUNT_MAX);
    }

    /// A cpufreq directory with one policy per entry: its name, top frequency, cores and,
    /// where it can be read, its clock. A `boost` file sits beside them, as on a real machine.
    async fn fake_cpufreq(policies: &[(&str, &str, &str, Option<&str>)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().to_path_buf();
        for (name, max_khz, cores, current_khz) in policies {
            let dir = base.join(name);
            cc_fs::create_dir_all(&dir).await.unwrap();
            let write = |file: &'static str, value: &str| {
                cc_fs::write_string(dir.join(file), format!("{value}\n"))
            };
            write(POLICY_MAX_FREQ_FILE, max_khz).await.unwrap();
            write(POLICY_CORES_FILE, cores).await.unwrap();
            if let Some(current_khz) = current_khz {
                write(POLICY_CURRENT_FREQ_FILE, current_khz).await.unwrap();
            }
        }
        cc_fs::write_string(base.join("boost"), "1\n".to_string())
            .await
            .unwrap();
        tmp
    }

    fn channel(name: &str) -> ChannelStatus {
        ChannelStatus {
            name: name.to_string(),
            freq: Some(0),
            ..Default::default()
        }
    }

    /// Goal: clusters are found once with their core counts, and a poll reads one clock per
    /// cluster and leaves out a cluster whose clock cannot be read. Method: a fake cpufreq
    /// directory shaped like the M1 Max, with the last cluster's clock missing.
    #[test]
    #[serial]
    fn clusters_are_found_once_and_read_one_clock_each() {
        cc_fs::test_runtime(async {
            let tmp = fake_cpufreq(&[
                ("policy0", "2064000", "0 1", Some("972000")),
                ("policy2", "3228000", "2 3 4 5", Some("3036000")),
                ("policy6", "3228000", "6 7 8 9", None),
            ])
            .await;
            let base = tmp.path().to_path_buf();

            let apple_silicon = AppleSilicon::find(&base).await;
            let freqs = apple_silicon.read_freqs().await;

            let core_counts: Vec<u32> = apple_silicon
                .clusters
                .iter()
                .map(|c| c.core_count)
                .collect();
            assert_eq!(core_counts, [2, 4, 4]);
            let cluster_freqs = apple_silicon.cluster_freqs.borrow();
            let named: Vec<(&str, Option<Mhz>)> = cluster_freqs
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
            assert_eq!(
                freqs.get(&PACKAGE),
                Some(&CpuFreqs {
                    min: 972,
                    max: 3036,
                    avg: 2348,
                })
            );
        });
    }

    /// Goal: a machine whose cpufreq has no policy is still an Apple Silicon machine, with no
    /// frequencies and no cluster channels. Method: a cpufreq directory that does not exist.
    #[test]
    #[serial]
    fn a_machine_without_policies_has_no_clusters() {
        cc_fs::test_runtime(async {
            let missing = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
            let apple_silicon = AppleSilicon::find(&missing).await;
            let mut statuses = [channel("CPU Freq Max")];

            assert!(apple_silicon.clusters.is_empty());
            assert!(apple_silicon.read_freqs().await.is_empty());
            assert!(apple_silicon.cluster_channels().is_empty());
            apple_silicon.fill_cluster_freqs(&mut statuses);
            assert_eq!(statuses[0].freq, Some(0));
        });
    }

    /// Goal: a cluster channel never carries a clock from an earlier poll. Method: two
    /// clusters polled twice, with one clock gone before the second poll.
    #[test]
    #[serial]
    fn a_poll_keeps_no_clock_from_the_poll_before() {
        cc_fs::test_runtime(async {
            let tmp = fake_cpufreq(&[
                ("policy0", "2064000", "0 1", Some("972000")),
                ("policy2", "3228000", "2 3 4 5", Some("3036000")),
            ])
            .await;
            let base = tmp.path().to_path_buf();
            let apple_silicon = AppleSilicon::find(&base).await;
            let names = [
                "CPU Freq Efficiency Cluster",
                "CPU Freq Performance Cluster",
            ];
            let poll = async || {
                apple_silicon.read_freqs().await;
                let mut statuses = names.map(channel);
                apple_silicon.fill_cluster_freqs(&mut statuses);
                statuses.map(|status| status.freq)
            };

            let first = poll().await;
            let performance_clock = base.join("policy2").join(POLICY_CURRENT_FREQ_FILE);
            cc_fs::remove_file(&performance_clock).await.unwrap();
            let second = poll().await;

            assert_eq!(first, [Some(972), Some(3036)]);
            assert_eq!(second, [Some(972), Some(0)]);
        });
    }

    /// Goal: every cluster gets a channel, whether or not its clock was readable, and a
    /// cluster clock only fills the channel of its own name. Method: two clusters with one
    /// reading, beside a package channel.
    #[test]
    fn cluster_clocks_fill_their_own_channels_only() {
        let efficiency = cluster("CPU Freq Efficiency Cluster", 2);
        let apple_silicon = AppleSilicon {
            cluster_freqs: RefCell::new(cluster_statuses(&[(&efficiency, 972_000)])),
            clusters: vec![efficiency, cluster("CPU Freq Performance Cluster", 8)],
        };
        let mut statuses = vec![
            channel("CPU Freq Efficiency Cluster"),
            ChannelStatus {
                name: "CPU Freq Max".to_string(),
                freq: Some(3036),
                ..Default::default()
            },
        ];
        apple_silicon.fill_cluster_freqs(&mut statuses);
        assert_eq!(statuses[0].freq, Some(972));
        assert_eq!(statuses[1].freq, Some(3036));
        let channels = apple_silicon.cluster_channels();
        assert_eq!(channels.len(), 2);
        assert_eq!(channels[0].number, FIRST_CLUSTER_CHANNEL_NUMBER);
        assert_eq!(channels[1].number, FIRST_CLUSTER_CHANNEL_NUMBER + 1);
        assert_eq!(channels[0].hwmon_type, HwmonChannelType::Freq);
    }
}
