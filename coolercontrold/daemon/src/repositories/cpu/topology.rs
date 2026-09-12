// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Not;

use anyhow::{anyhow, Result};

use crate::device::Mhz;
use crate::repositories::cpu::percent::{CpuPercent, MAX_LOGICAL_CPUS};

/// The ID of the actual physical CPU. On most systems, there is only one:
pub type PhysicalID = u8;

/// Packages on a dual-socket board, which is as wide as commodity x86 goes. Only a capacity hint,
/// so a larger machine still parses, it just grows the map once.
const EXPECTED_PACKAGE_COUNT: usize = 2;

#[derive(Default, Debug, PartialEq)]
pub struct CpuFreqs {
    pub min: Mhz,
    pub max: Mhz,
    pub avg: Mhz,
}

/// What `/proc/cpuinfo` says about this machine's processors.
///
/// Every fact here is parsed from that one file, so it is parsed once into this rather than
/// separately at each place that needs a piece of it. Only the processor map changes after
/// startup, when a CPU goes offline or comes online.
///
/// The default is the empty topology a repository holds before it has read cpuinfo. Every
/// accessor answers safely for it: no packages, no names, no processors.
#[derive(Debug, Default)]
pub struct CpuTopology {
    /// Every physical id cpuinfo names, ascending.
    packages: BTreeSet<PhysicalID>,
    model_names: BTreeMap<PhysicalID, String>,
    /// Physical ids in the order the kernel numbers package zones, i.e. ascending APIC id.
    apic_order: Vec<PhysicalID>,
    /// Each online processor's physical id, indexed by processor id.
    processor_owners: RefCell<Vec<Option<PhysicalID>>>,
}

impl CpuTopology {
    /// Parses cpuinfo text. Errors when it names no processor that devices can be keyed by, which
    /// is what the caller needs before it can build any CPU device at all.
    pub fn parse(cpu_info_data: &str) -> Result<Self> {
        let entries = parse_processor_entries(cpu_info_data)?;
        let mut packages = BTreeSet::new();
        let mut model_names = BTreeMap::new();
        for entry in &entries {
            // A package is only known by a processor that names both, which is what ties a model
            // name to it.
            let (Some(physical_id), Some(model_name)) = (entry.physical_id, &entry.model_name)
            else {
                continue;
            };
            packages.insert(physical_id);
            model_names.insert(physical_id, model_name.clone());
        }
        if packages.is_empty() && model_names.is_empty() {
            // Some CPUs, like the Raspberry Pi, don't have a physical id, so we need to fake one,
            // they do have a model name though.
            if let Some(board_model) = board_model(cpu_info_data) {
                packages.insert(0);
                model_names.insert(0, board_model.to_owned());
            }
        }
        if packages.is_empty() || model_names.is_empty() {
            return Err(anyhow!(
                "cpuinfo either not found or missing data on this system!"
            ));
        }
        debug_assert_eq!(packages.len(), model_names.len());
        Ok(Self {
            apic_order: apic_order(&entries),
            processor_owners: RefCell::new(processor_owners(&entries)),
            packages,
            model_names,
        })
    }

    /// The average load of one package's online processors, or `None` when none are online.
    pub fn package_load_percent(
        &self,
        cpu_load_percents: &[CpuPercent],
        physical_id: PhysicalID,
    ) -> Option<f64> {
        let processor_owners = self.processor_owners.borrow();
        let mut percent_sum = 0.0;
        let mut processor_count: u32 = 0;
        for cpu_load in cpu_load_percents {
            let owner = processor_owners
                .get(usize::from(cpu_load.cpu_id))
                .copied()
                .flatten();
            if owner == Some(physical_id) {
                percent_sum += f64::from(cpu_load.percent);
                processor_count += 1;
            }
        }
        debug_assert!(processor_count as usize <= cpu_load_percents.len());
        (processor_count > 0).then(|| percent_sum / f64::from(processor_count))
    }

    /// Whether the processor map lists exactly the processors that are online now.
    pub fn owners_are_current(&self, online_cpu_ids: impl Iterator<Item = u16>) -> bool {
        let processor_owners = self.processor_owners.borrow();
        let mut online_count = 0;
        for cpu_id in online_cpu_ids {
            let mapped = processor_owners
                .get(usize::from(cpu_id))
                .is_some_and(Option::is_some);
            if mapped.not() {
                return false;
            }
            online_count += 1;
        }
        online_count == processor_owners.iter().flatten().count()
    }

    /// Re-reads which physical id each online processor belongs to, so load follows processors
    /// going offline and coming online.
    ///
    /// Returns the new per-package counts when they changed, and nothing when they did not. An
    /// unreadable cpuinfo is an error and leaves the current map in place: a map built from data
    /// this bad would put a package's load on the wrong processors.
    pub fn refresh_owners(
        &self,
        cpu_info_data: &str,
    ) -> Result<Option<BTreeMap<PhysicalID, usize>>> {
        let refreshed = processor_owners(&parse_processor_entries(cpu_info_data)?);
        if *self.processor_owners.borrow() == refreshed {
            return Ok(None);
        }
        let mut processor_counts: BTreeMap<PhysicalID, usize> = BTreeMap::new();
        for physical_id in refreshed.iter().flatten() {
            *processor_counts.entry(*physical_id).or_default() += 1;
        }
        *self.processor_owners.borrow_mut() = refreshed;
        Ok(Some(processor_counts))
    }

    /// The model name to show for a CPU device.
    ///
    /// A device keyed by its package zone has no physical id, so it takes the fallback.
    /// Multi-socket x86 requires identical processors, so any known name describes every package.
    /// The lowest id is used rather than any, so the name and the UID derived from it cannot vary
    /// between runs.
    pub fn model_name(&self, physical_id: Option<PhysicalID>) -> Option<String> {
        if let Some(model_name) = physical_id.and_then(|id| self.model_names.get(&id)) {
            return Some(model_name.clone());
        }
        self.model_names
            .first_key_value()
            .map(|(_, model_name)| model_name.clone())
    }

    pub fn package_count(&self) -> usize {
        self.packages.len()
    }

    pub fn contains(&self, physical_id: PhysicalID) -> bool {
        self.packages.contains(&physical_id)
    }

    /// The physical ids cpuinfo names, ascending.
    pub fn packages(&self) -> impl DoubleEndedIterator<Item = PhysicalID> + '_ {
        self.packages.iter().copied()
    }

    /// The one package of a single-socket machine, whose physical id is not always 0.
    pub fn only_package(&self) -> Option<PhysicalID> {
        if self.packages.len() != 1 {
            return None;
        }
        self.packages.first().copied()
    }

    pub fn apic_order(&self) -> &[PhysicalID] {
        &self.apic_order
    }

    /// How many online processors a package holds. Counted from the processor map rather than
    /// stored, so it can never disagree with the map the load percentages are summed over.
    ///
    /// Nothing in the daemon needs this: the count that used to be stored per package was written
    /// and never read. It is kept for the tests that assert what a given cpuinfo parses to.
    #[cfg(test)]
    pub fn processor_count(&self, physical_id: PhysicalID) -> usize {
        self.processor_owners
            .borrow()
            .iter()
            .flatten()
            .filter(|owner| **owner == physical_id)
            .count()
    }
}

/// One processor's block in cpuinfo. cpuinfo prints `processor` first in each block, so every
/// following key belongs to the block currently being filled.
#[derive(Default)]
struct ProcessorEntry {
    processor_id: Option<usize>,
    physical_id: Option<PhysicalID>,
    model_name: Option<String>,
    apic_id: Option<u32>,
}

/// Splits one cpuinfo line into its key and value. Skips empty and non-key-value lines.
fn cpuinfo_field(line: &str) -> Option<(&str, &str)> {
    let mut fields = line.split(':');
    match (fields.next(), fields.next()) {
        (Some(key), Some(value)) => Some((key.trim(), value.trim())),
        _ => None,
    }
}

/// Reads every processor block in one pass.
///
/// A physical id that does not parse is an error: it is the one value every CPU device is keyed
/// by, so a machine that reports a bad one is doing something unexpected and should say so rather
/// than quietly show fewer processors than it has. The others are left out on a bad parse, since
/// nothing is keyed by them.
fn parse_processor_entries(cpu_info_data: &str) -> Result<Vec<ProcessorEntry>> {
    let mut entries: Vec<ProcessorEntry> = Vec::new();
    for line in cpu_info_data.lines() {
        let Some((key, value)) = cpuinfo_field(line) else {
            continue;
        };
        if key == "processor" {
            entries.push(ProcessorEntry {
                processor_id: value.parse().ok(),
                ..Default::default()
            });
            continue;
        }
        let Some(entry) = entries.last_mut() else {
            continue; // A key before the first processor block belongs to no processor.
        };
        match key {
            "model name" => entry.model_name = Some(value.to_owned()),
            "physical id" => {
                entry.physical_id = Some(value.parse().map_err(|_| {
                    anyhow!("cpuinfo reported an unreadable physical id: \"{value}\"")
                })?);
            }
            // `initial apicid` is a different key and never lands here.
            "apicid" => entry.apic_id = value.parse().ok(),
            _ => (),
        }
    }
    Ok(entries)
}

/// The board's model name, which is what a machine with no physical id at all, such as the
/// Raspberry Pi, names its processor by.
fn board_model(cpu_info_data: &str) -> Option<&str> {
    cpu_info_data
        .lines()
        .filter_map(cpuinfo_field)
        .find(|(key, _)| *key == "Model")
        .map(|(_, value)| value)
}

/// Orders physical processors the way the kernel numbers package zones: by ascending APIC id.
///
/// `topology_get_logical_id()` counts the set APIC id bits below a domain's own, so a
/// `coretemp.N` zone number is that package's rank in this order. cpuinfo prints `apicid` in
/// the same block as `physical id`, so wherever there is more than one package to tell apart,
/// both are present.
fn apic_order(entries: &[ProcessorEntry]) -> Vec<PhysicalID> {
    let mut lowest_apic_ids: HashMap<PhysicalID, u32> =
        HashMap::with_capacity(EXPECTED_PACKAGE_COUNT);
    for entry in entries {
        let (Some(physical_id), Some(apic_id)) = (entry.physical_id, entry.apic_id) else {
            continue;
        };
        lowest_apic_ids
            .entry(physical_id)
            .and_modify(|lowest| *lowest = (*lowest).min(apic_id))
            .or_insert(apic_id);
    }
    let mut ordered = lowest_apic_ids
        .into_iter()
        .collect::<Vec<(PhysicalID, u32)>>();
    // The physical id breaks ties so the order cannot vary between runs.
    ordered.sort_unstable_by_key(|(physical_id, apic_id)| (*apic_id, *physical_id));
    ordered
        .into_iter()
        .map(|(physical_id, _)| physical_id)
        .collect()
}

/// Each online processor's physical id, indexed by processor id.
///
/// cpuinfo lists only online processors. Where there is no physical id at all, as on the
/// Raspberry Pi, every processor belongs to the single package 0 that `parse` fakes for it.
fn processor_owners(entries: &[ProcessorEntry]) -> Vec<Option<PhysicalID>> {
    let has_physical_ids = entries.iter().any(|entry| entry.physical_id.is_some());
    // The same bound the load collector puts on the processors it reads.
    let processors = entries
        .iter()
        .filter_map(|entry| entry.processor_id.map(|id| (id, entry.physical_id)))
        .filter(|(processor_id, _)| *processor_id < MAX_LOGICAL_CPUS);
    let mut processor_owners: Vec<Option<PhysicalID>> = Vec::new();
    for (processor_id, physical_id) in processors {
        if processor_id >= processor_owners.len() {
            processor_owners.resize(processor_id + 1, None);
        }
        processor_owners[processor_id] = if has_physical_ids {
            physical_id
        } else {
            Some(0)
        };
    }
    debug_assert!(processor_owners.len() <= MAX_LOGICAL_CPUS);
    processor_owners
}

/// The average, highest and lowest frequency of every package, from cpuinfo.
///
/// This is read every poll, unlike the rest of the topology, so it keeps its own tight loop. The
/// most reliable source is cpuinfo: it says which frequency belongs to which physical CPU, which
/// `CoolerControl`'s multi-processor support needs, and it is cached and so consistently fast.
/// See: <https://github.com/giampaolo/psutil/issues/1851>
/// The alternatives are `/sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq` and
/// `/sys/devices/system/cpu/cpufreq/policy[0-9]*/scaling_cur_freq`, which have been reported as
/// significantly slower on some systems, and give no way to tie a frequency to a package.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn parse_freqs(cpu_info_data: &str) -> HashMap<PhysicalID, CpuFreqs> {
    let mut cpu_freqs = HashMap::new();
    let mut cpu_info_freqs: HashMap<PhysicalID, Vec<f64>> = HashMap::new();
    let mut physical_id: PhysicalID = 0;
    let mut freq: f64 = 0.;
    let mut physical_id_present = false;
    let mut freq_present = false;
    for line in cpu_info_data.lines() {
        // Checked before splitting, since this runs on every poll over the whole file.
        if line.starts_with("physical id").not() && line.starts_with("cpu MHz").not() {
            continue;
        }
        let Some((key, value)) = cpuinfo_field(line) else {
            continue;
        };
        if key == "physical id" {
            let Ok(parsed) = value.parse() else {
                return cpu_freqs;
            };
            physical_id = parsed;
            physical_id_present = true;
        }
        if key == "cpu MHz" {
            let Ok(parsed) = value.parse() else {
                return cpu_freqs;
            };
            freq = parsed;
            freq_present = true;
        }
        if physical_id_present && freq_present {
            // after each processor's entry
            cpu_info_freqs.entry(physical_id).or_default().push(freq);
            physical_id_present = false;
            freq_present = false;
        }
    }
    for (physical_id, freqs) in cpu_info_freqs {
        let min = freqs
            .iter()
            .min_by(|a, b| a.total_cmp(b))
            .unwrap_or(&0.)
            .round() as Mhz;
        let max = freqs
            .iter()
            .max_by(|a, b| a.total_cmp(b))
            .unwrap_or(&0.)
            .round() as Mhz;
        #[allow(clippy::cast_precision_loss)]
        let avg = (freqs.iter().sum::<f64>() / freqs.len() as f64).round() as Mhz;
        cpu_freqs.insert(physical_id, CpuFreqs { min, max, avg });
    }
    cpu_freqs
}

#[cfg(test)]
mod tests {
    use super::{parse_freqs, CpuFreqs, CpuTopology, Not, PhysicalID};
    use crate::repositories::cpu::fixtures;
    use crate::repositories::cpu::percent::CpuPercent;

    /// Goal: a single-package machine parses to one package, named by its model name. Method: the
    /// AMD and Intel single-socket dumps.
    #[test]
    fn a_single_package_parses_to_one_processor() {
        for (cpu_info_data, model_name) in [
            (
                fixtures::AMD_SINGLE_CPU,
                "AMD Ryzen 7 5800X 8-Core Processor",
            ),
            (
                fixtures::INTEL_SINGLE_CPU,
                "Intel(R) Core(TM) i5-8265U CPU @ 1.60GHz",
            ),
        ] {
            // when:
            let topology = CpuTopology::parse(cpu_info_data).unwrap();

            // then:
            assert_eq!(topology.package_count(), 1);
            assert_eq!(topology.model_name(Some(0)).as_deref(), Some(model_name));
            // then: the one package answers without a zone id, whatever its physical id is.
            assert_eq!(topology.only_package(), Some(0));
        }
    }

    /// Goal: a dual-socket machine parses to two packages, each with its own model name. Method:
    /// the AMD double dump, whose two names differ so they cannot be confused.
    #[test]
    fn a_double_package_parses_to_two_processors() {
        // when:
        let topology = CpuTopology::parse(fixtures::AMD_DOUBLE_CPU).unwrap();

        // then:
        assert_eq!(topology.package_count(), 2);
        assert_eq!(topology.packages().collect::<Vec<PhysicalID>>(), vec![0, 1]);
        assert_eq!(
            topology.model_name(Some(0)).as_deref(),
            Some("AMD Ryzen 7 5800X 8-Core Processor#1")
        );
        assert_eq!(
            topology.model_name(Some(1)).as_deref(),
            Some("AMD Ryzen 7 5800X 8-Core Processor#2")
        );
        // then: with more than one package there is no single package to short circuit to.
        assert_eq!(topology.only_package(), None);
    }

    /// Goal: a machine with no physical id at all still gets one package to hang its device on,
    /// named by the board model. Method: the Raspberry Pi 5 dump, which has a `Model` line and no
    /// `physical id` line.
    #[test]
    fn a_machine_without_physical_ids_fakes_one_package() {
        // when:
        let topology = CpuTopology::parse(fixtures::RASPBERRY_PI_5).unwrap();

        // then:
        assert_eq!(topology.package_count(), 1);
        assert_eq!(
            topology.model_name(Some(0)).as_deref(),
            Some("Raspberry Pi Compute Module 5 Rev 1.0")
        );
        // then: all four of its processors belong to that one faked package.
        assert_eq!(topology.processor_count(0), 4);
    }

    /// Goal: cpuinfo that names no processor is an error, since no CPU device can be keyed by it.
    /// Method: empty text, and text with no processor block.
    #[test]
    fn cpuinfo_without_a_processor_is_an_error() {
        assert!(CpuTopology::parse("").is_err());
        assert!(CpuTopology::parse("some other file entirely\n").is_err());
    }

    /// Goal: a physical id that cannot be read means the machine is reporting something
    /// unexpected, and every CPU device is keyed by that id, so it must fail loudly rather than
    /// silently show fewer processors. Method: a cpuinfo whose second package has a bad id, and
    /// one with a bad value under a key nothing is keyed by.
    #[test]
    fn an_unreadable_physical_id_is_an_error() {
        // given: a machine that would otherwise parse to two packages.
        let bad_physical_id = concat!(
            "processor\t: 0\n",
            "model name\t: Test CPU\n",
            "physical id\t: 0\n",
            "\n",
            "processor\t: 1\n",
            "model name\t: Test CPU\n",
            "physical id\t: not a number\n",
        );

        // then: the whole parse fails, rather than reporting a single package.
        let result = CpuTopology::parse(bad_physical_id);
        assert!(result.is_err(), "expected an error, got: {result:?}");
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unreadable physical id"));

        // then: a bad value nothing is keyed by is left out instead, since it costs no device.
        let bad_apic_id = concat!(
            "processor\t: 0\n",
            "model name\t: Test CPU\n",
            "physical id\t: 0\n",
            "apicid\t\t: not a number\n",
        );
        let topology = CpuTopology::parse(bad_apic_id).unwrap();
        assert_eq!(topology.package_count(), 1);
        assert!(topology.apic_order().is_empty());
    }

    /// Goal: each package's processor count must be its own, not a running total across packages,
    /// which is what left every package but the last without a load channel. Method: the dual
    /// Xeon dump, 8 logical processors per package.
    #[test]
    fn each_package_counts_its_own_processors() {
        // when:
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();

        // then:
        assert_eq!(topology.processor_count(0), 8);
        assert_eq!(topology.processor_count(1), 8);
    }

    /// Goal: load is counted per package by processor id, so the map must follow cpuinfo's own
    /// pairing of processor and physical id. Method: the dual Xeon dump, whose kernel interleaves
    /// the packages (even processors on package 0, odd on package 1).
    #[test]
    fn the_processor_map_follows_cpuinfo() {
        // given:
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();

        // then: every processor is owned by the package cpuinfo pairs it with.
        let owners = topology.processor_owners.borrow();
        assert_eq!(owners.len(), 16);
        for (processor_id, physical_id) in owners.iter().enumerate() {
            assert_eq!(*physical_id, Some((processor_id % 2) as PhysicalID));
        }
    }

    /// Goal: every package reports the load of its own processors, which 3.1.0 lost when it moved
    /// to a single whole-system load that only the last package could match. Method: the dual
    /// Xeon topology with package 0's processors fully busy and package 1's idle.
    #[test]
    fn every_package_gets_its_own_load() {
        // given:
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();
        let percents = (0..16)
            .map(|cpu_id| CpuPercent {
                cpu_id,
                percent: if cpu_id % 2 == 0 { 100.0 } else { 0.0 },
            })
            .collect::<Vec<CpuPercent>>();

        // then:
        assert_eq!(topology.package_load_percent(&percents, 0), Some(100.0));
        assert_eq!(topology.package_load_percent(&percents, 1), Some(0.0));
        // then: a package cpuinfo does not know has no load at all.
        assert_eq!(topology.package_load_percent(&percents, 2), None);
    }

    /// Goal: load must follow processors going offline and coming online, and the map is only
    /// re-read when the online set actually changed. Method: the dual Xeon topology checked
    /// against online sets that match it and that do not, then refreshed from a smaller cpuinfo.
    #[test]
    fn the_processor_map_is_refreshed_when_the_online_set_changes() {
        // given:
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();

        // then: the map matches the processors it was parsed from.
        assert!(topology.owners_are_current(0..16));
        // then: a processor going offline or coming online no longer matches.
        assert!(topology.owners_are_current(0..15).not());
        assert!(topology.owners_are_current(0..17).not());

        // when: refreshed from a cpuinfo listing three packages instead.
        let processor_counts = topology
            .refresh_owners(fixtures::PACKAGE_1_OFFLINE)
            .unwrap();

        // then: the new counts are reported, one processor per package.
        assert_eq!(
            processor_counts.unwrap().into_iter().collect::<Vec<_>>(),
            vec![(0, 1), (2, 1), (3, 1)]
        );
        // then: refreshing again with no change reports nothing, so nothing is logged.
        assert_eq!(
            topology
                .refresh_owners(fixtures::PACKAGE_1_OFFLINE)
                .unwrap(),
            None
        );
    }

    /// Goal: load follows processors going offline and online at runtime, without an error and
    /// without a fabricated 0. Method: the dual Xeon topology given samples covering only some of
    /// package 0's processors, then none of package 1's.
    #[test]
    fn a_package_averages_only_the_processors_that_reported() {
        // given:
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();
        let percent = |cpu_id: u16, percent: f32| CpuPercent { cpu_id, percent };

        // then: package 0 averages only the processors it has online.
        let some_offline = [percent(0, 20.0), percent(2, 40.0), percent(1, 90.0)];
        assert_eq!(topology.package_load_percent(&some_offline, 0), Some(30.0));
        assert_eq!(topology.package_load_percent(&some_offline, 1), Some(90.0));

        // then: a package with every processor offline has no average, which `collect_load`
        // reports as 0 so the channel stays in the status.
        let package_1_offline = [percent(0, 20.0), percent(2, 40.0)];
        assert_eq!(topology.package_load_percent(&package_1_offline, 1), None);
    }

    /// Goal: package zones are numbered by ascending APIC id, not by physical id, so the order
    /// must come from the APIC ids. Method: a machine whose two physical ids run opposite to
    /// their APIC ids, which is the only shape that tells the two apart.
    #[test]
    fn packages_are_ordered_by_apic_id() {
        // then: the ordinary case, where the two orderings agree.
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();
        assert_eq!(topology.apic_order(), [0, 1]);

        // then: with the APIC ids inverted, the order follows them and not the physical ids.
        let topology = CpuTopology::parse(fixtures::INVERTED_APIC).unwrap();
        assert_eq!(topology.apic_order(), [1, 0]);
    }

    /// Goal: a device keyed by a package zone has no physical id, and still needs a name that
    /// cannot vary between runs. Method: a two-package topology asked for a known id, an unknown
    /// id, and no id at all.
    #[test]
    fn a_nameless_device_falls_back_to_the_lowest_known_name() {
        // given:
        let topology = CpuTopology::parse(fixtures::INTEL_DOUBLE_CPU).unwrap();
        let expected = topology.model_name(Some(0)).unwrap();

        // then: an unknown id and no id both take the lowest known name.
        assert_eq!(topology.model_name(Some(7)), Some(expected.clone()));
        assert_eq!(topology.model_name(None), Some(expected));
    }

    /// Goal: frequencies are read every poll and must be reported per package. Method: the dumps
    /// with known frequencies, plus the two that have none.
    #[test]
    fn frequencies_are_parsed_per_package() {
        // then: one package.
        let freqs = parse_freqs(fixtures::AMD_SINGLE_CPU);
        assert_eq!(freqs.len(), 1);
        assert_eq!(
            freqs.get(&0),
            Some(&CpuFreqs {
                avg: 3006,
                max: 4200,
                min: 1754
            })
        );

        // then: two packages, each with its own.
        let freqs = parse_freqs(fixtures::AMD_DOUBLE_CPU);
        assert_eq!(freqs.len(), 2);
        assert_eq!(
            freqs.get(&0),
            Some(&CpuFreqs {
                avg: 3006,
                max: 4200,
                min: 1754
            })
        );
        assert_eq!(
            freqs.get(&1),
            Some(&CpuFreqs {
                avg: 819,
                max: 1754,
                min: 196
            })
        );

        // then: a package whose processors all report the same frequency.
        let freqs = parse_freqs(fixtures::INTEL_SINGLE_CPU);
        assert_eq!(
            freqs.get(&0),
            Some(&CpuFreqs {
                avg: 800,
                max: 800,
                min: 800
            })
        );

        // then: cpuinfo without frequencies gives none, rather than a fabricated 0.
        assert!(parse_freqs(fixtures::RASPBERRY_PI_5).is_empty());
        assert!(parse_freqs("").is_empty());
    }
}
