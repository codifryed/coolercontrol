// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Sane upper bound for logical CPUs. Prevents unbounded allocation
/// if /proc/stat is somehow corrupted or pathological.
pub const MAX_LOGICAL_CPUS: usize = 2048;

/// Minimum number of /proc/stat fields required per cpu line.
/// Fields: user nice system idle (iowait irq softirq steal are optional).
const MIN_STAT_FIELDS: usize = 4;

/// Maximum number of fields per cpu line in /proc/stat.
/// Fields: user nice system idle iowait irq softirq steal guest `guest_nice`.
const MAX_STAT_FIELDS: usize = 10;

/// Per-CPU time snapshot from /proc/stat, in kernel jiffies.
#[derive(Clone)]
struct CpuTimes {
    /// The logical CPU number, `N` in `cpuN`. /proc/stat lists only online CPUs, so this is what
    /// pairs a CPU with its previous snapshot once CPUs go offline or come online.
    cpu_id: u16,
    idle_jiffies: u64,
    total_jiffies: u64,
}

/// A logical CPU's busy percentage since the previous snapshot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CpuPercent {
    pub cpu_id: u16,
    pub percent: f32,
}

/// Reads /proc/stat and computes per-CPU usage percentages from the
/// delta between consecutive calls.
pub struct CpuPercentCollector {
    stat_path: Box<Path>,
    prev: Vec<CpuTimes>,
}

impl CpuPercentCollector {
    pub fn new() -> Result<Self> {
        Self::with_path(Path::new("/proc/stat"))
    }

    /// Creates a collector reading from an arbitrary path.
    /// Used in tests to inject fixture data.
    fn with_path(stat_path: &Path) -> Result<Self> {
        let content = fs::read_to_string(stat_path)
            .with_context(|| format!("reading {}", stat_path.display()))?;
        let prev = parse_stat_content(&content);
        assert!(!prev.is_empty(), "no per-cpu lines found");
        Ok(Self {
            stat_path: stat_path.into(),
            prev,
        })
    }

    /// Returns per-CPU busy percentages (0.0 to 100.0) since the last call, ascending by CPU id.
    ///
    /// Only CPUs online in both snapshots are included: a CPU that came online since has no
    /// baseline yet, and one that went offline has no current reading.
    #[allow(clippy::cast_precision_loss)]
    pub fn cpu_percent_per_cpu(&mut self) -> Result<Vec<CpuPercent>> {
        let content = fs::read_to_string(&self.stat_path)
            .with_context(|| format!("reading {}", self.stat_path.display()))?;
        let curr = parse_stat_content(&content);
        let percents = compute_percents(&self.prev, &curr);
        self.prev = curr;
        Ok(percents)
    }

    /// The CPUs online at the latest snapshot, ascending.
    pub fn online_cpu_ids(&self) -> impl Iterator<Item = u16> + '_ {
        self.prev.iter().map(|times| times.cpu_id)
    }
}

/// Computes per-CPU busy percentages from two snapshots, pairing each CPU by id.
///
/// Pairing by position would compare different CPUs once one goes offline or comes online. Both
/// snapshots are ascending by CPU id, as /proc/stat prints them, so this is a single merge.
#[allow(clippy::cast_precision_loss)]
fn compute_percents(prev: &[CpuTimes], curr: &[CpuTimes]) -> Vec<CpuPercent> {
    debug_assert!(prev.is_sorted_by_key(|times| times.cpu_id));
    debug_assert!(curr.is_sorted_by_key(|times| times.cpu_id));
    let mut percents = Vec::with_capacity(curr.len());
    let mut prev_times = prev.iter().peekable();
    for cur in curr {
        // Skip the CPUs that went offline since the previous snapshot.
        while prev_times
            .next_if(|prev| prev.cpu_id < cur.cpu_id)
            .is_some()
        {}
        let Some(prev) = prev_times.next_if(|prev| prev.cpu_id == cur.cpu_id) else {
            continue; // came online since, so there is no baseline to measure from yet
        };
        let total_delta = cur.total_jiffies.saturating_sub(prev.total_jiffies);
        let idle_delta = cur.idle_jiffies.saturating_sub(prev.idle_jiffies);
        debug_assert!(idle_delta <= total_delta);
        let percent = if total_delta == 0 {
            0.0_f32
        } else {
            ((total_delta - idle_delta) as f32 / total_delta as f32) * 100.0
        };
        debug_assert!(
            (0.0..=100.0).contains(&percent),
            "CPU percent out of range: {percent}"
        );
        percents.push(CpuPercent {
            cpu_id: cur.cpu_id,
            percent,
        });
    }
    debug_assert!(percents.len() <= curr.len());
    percents
}

/// Parses /proc/stat content for per-cpu lines (`cpu0`, `cpu1`, …)
/// and extracts idle and total jiffies for each logical CPU.
fn parse_stat_content(content: &str) -> Vec<CpuTimes> {
    let mut cpus = Vec::with_capacity(num_cpus_hint());
    for line in content.lines() {
        if !line.starts_with("cpu") {
            // Per-cpu lines are grouped at the top of /proc/stat.
            // Once we pass them, no more will appear.
            break;
        }
        let name = line.split_ascii_whitespace().next().unwrap_or("");
        // Skip the aggregate "cpu" line; only parse "cpu0", "cpu1", …
        if name == "cpu" {
            continue;
        }
        if let Some(times) = parse_cpu_line(line) {
            cpus.push(times);
        }
        assert!(
            cpus.len() <= MAX_LOGICAL_CPUS,
            "/proc/stat reports more than {MAX_LOGICAL_CPUS} CPUs"
        );
    }
    cpus
}

/// Parses a single per-cpu line from /proc/stat into jiffies.
/// Returns `None` if the line has no `cpuN` id, has fewer than `MIN_STAT_FIELDS` numeric
/// fields, or contains non-numeric tokens.
///
/// Fields: user nice system idle [iowait irq softirq steal guest `guest_nice`]
fn parse_cpu_line(line: &str) -> Option<CpuTimes> {
    let mut tokens = line.split_ascii_whitespace();
    let cpu_id = tokens.next()?.strip_prefix("cpu")?.parse().ok()?;
    let mut fields = [0_u64; MAX_STAT_FIELDS];
    let mut field_count = 0;
    for token in tokens {
        if field_count >= fields.len() {
            break;
        }
        fields[field_count] = token.parse().ok()?;
        field_count += 1;
    }
    if field_count < MIN_STAT_FIELDS {
        return None;
    }
    // idle + iowait (iowait defaults to 0 if absent).
    let idle_jiffies = fields[3] + fields[4];
    let total_jiffies: u64 = fields[..field_count].iter().sum();
    debug_assert!(idle_jiffies <= total_jiffies);
    Some(CpuTimes {
        cpu_id,
        idle_jiffies,
        total_jiffies,
    })
}

/// Rough hint for initial capacity. A reasonable default avoids
/// reallocation in most cases without over-allocating.
fn num_cpus_hint() -> usize {
    16
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/resources/tests/proc_stat");

    // -- parse_cpu_line tests --

    #[test]
    fn parse_cpu_line_full_10_fields() {
        // Goal: verify a standard 10-field cpu line is parsed correctly
        // and idle = field[3] + field[4] (idle + iowait).
        let line = "cpu0 126499 1601 47200 4274964 14051 25757 18230 0 0 0";
        let times = parse_cpu_line(line).expect("should parse");
        assert_eq!(times.idle_jiffies, 4_274_964 + 14_051);
        let expected_total: u64 = 126_499 + 1_601 + 47_200 + 4_274_964 + 14_051 + 25_757 + 18_230;
        assert_eq!(times.total_jiffies, expected_total);
        assert!(times.idle_jiffies <= times.total_jiffies);
    }

    #[test]
    fn parse_cpu_line_minimum_4_fields() {
        // Goal: verify a legacy 4-field line (no iowait) is accepted
        // with iowait defaulting to 0.
        let line = "cpu0 100 10 50 1000";
        let times = parse_cpu_line(line).expect("should parse");
        assert_eq!(times.idle_jiffies, 1000); // idle only, no iowait.
        assert_eq!(times.total_jiffies, 100 + 10 + 50 + 1000);
    }

    #[test]
    fn parse_cpu_line_too_few_fields_returns_none() {
        // Goal: verify lines with fewer than 4 fields are rejected.
        assert!(parse_cpu_line("cpu0 100 10").is_none());
        assert!(parse_cpu_line("cpu0 100").is_none());
        assert!(parse_cpu_line("cpu0").is_none());
    }

    #[test]
    fn parse_cpu_line_non_numeric_field_returns_none() {
        // Goal: verify that a non-numeric field causes the entire
        // line to be rejected (no silent partial parsing).
        let line = "cpu0 100 10 abc 1000 20 5 3 0 0 0";
        assert!(parse_cpu_line(line).is_none());
    }

    #[test]
    fn parse_cpu_line_extra_fields_beyond_10_are_ignored() {
        // Goal: verify we cap at MAX_STAT_FIELDS and don't overflow.
        let line = "cpu0 1 2 3 4 5 6 7 8 9 10 11 12 13";
        let times = parse_cpu_line(line).expect("should parse");
        // Only first 10 fields are summed.
        assert_eq!(times.total_jiffies, 1 + 2 + 3 + 4 + 5 + 6 + 7 + 8 + 9 + 10);
        assert_eq!(times.idle_jiffies, 4 + 5); // idle + iowait
    }

    // -- parse_stat_content tests --

    #[test]
    fn parse_stat_content_8_cpu_fixture() {
        // Goal: verify the full 8-cpu fixture produces exactly 8
        // CpuTimes entries with correct values.
        let content = fs::read_to_string(format!("{TEST_DIR}/8_cpu_normal")).unwrap();
        let cpus = parse_stat_content(&content);
        assert_eq!(cpus.len(), 8);
        // Spot-check cpu0.
        assert_eq!(cpus[0].idle_jiffies, 4_274_964 + 14_051);
        // Spot-check cpu7.
        assert_eq!(cpus[7].idle_jiffies, 4_253_134 + 35_382);
    }

    #[test]
    fn parse_stat_content_single_cpu() {
        // Goal: verify a single-cpu fixture produces 1 entry.
        let content = fs::read_to_string(format!("{TEST_DIR}/1_cpu_minimal")).unwrap();
        let cpus = parse_stat_content(&content);
        assert_eq!(cpus.len(), 1);
        assert_eq!(cpus[0].idle_jiffies, 1000 + 20);
        assert_eq!(cpus[0].total_jiffies, 100 + 10 + 50 + 1000 + 20 + 5 + 3);
    }

    #[test]
    fn parse_stat_content_legacy_4_fields() {
        // Goal: verify 4-field-only format still parses correctly.
        let content = fs::read_to_string(format!("{TEST_DIR}/4_field_legacy")).unwrap();
        let cpus = parse_stat_content(&content);
        assert_eq!(cpus.len(), 2);
        assert_eq!(cpus[0].idle_jiffies, 1000);
        assert_eq!(cpus[1].idle_jiffies, 2000);
    }

    #[test]
    fn parse_stat_content_empty_returns_no_cpus() {
        // Goal: verify a /proc/stat with no cpu lines returns an
        // empty vec (caller decides whether that is an error).
        let content = fs::read_to_string(format!("{TEST_DIR}/empty_no_cpus")).unwrap();
        let cpus = parse_stat_content(&content);
        assert!(cpus.is_empty());
    }

    #[test]
    fn parse_stat_content_malformed_skips_bad_lines() {
        // Goal: verify that lines with non-numeric or too-few fields
        // are skipped, while valid lines are still collected.
        let content = fs::read_to_string(format!("{TEST_DIR}/malformed_fields")).unwrap();
        let cpus = parse_stat_content(&content);
        // cpu0 has "abc" → skipped, cpu1 is valid, cpu2 has too few → skipped.
        assert_eq!(cpus.len(), 1);
    }

    // -- compute_percents tests --

    #[test]
    fn compute_percents_zero_delta_yields_zero() {
        // Goal: verify that identical snapshots produce 0% for all CPUs.
        let snapshot = vec![CpuTimes {
            cpu_id: 0,
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let percents = compute_percents(&snapshot, &snapshot);
        assert_eq!(percents.len(), 1);
        assert!((percents[0].percent - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_percents_full_load() {
        // Goal: verify that all-busy delta produces 100%.
        let prev = vec![CpuTimes {
            cpu_id: 0,
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let curr = vec![CpuTimes {
            cpu_id: 0,
            idle_jiffies: 500, // idle did not increase.
            total_jiffies: 2000,
        }];
        let percents = compute_percents(&prev, &curr);
        assert!((percents[0].percent - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_percents_half_load() {
        // Goal: verify 50% load is computed when idle grows by half
        // of the total delta.
        let prev = vec![CpuTimes {
            cpu_id: 0,
            idle_jiffies: 0,
            total_jiffies: 0,
        }];
        let curr = vec![CpuTimes {
            cpu_id: 0,
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let percents = compute_percents(&prev, &curr);
        assert!((percents[0].percent - 50.0).abs() < f32::EPSILON);
    }

    fn times(cpu_id: u16, idle_jiffies: u64, total_jiffies: u64) -> CpuTimes {
        CpuTimes {
            cpu_id,
            idle_jiffies,
            total_jiffies,
        }
    }

    #[test]
    fn compute_percents_pairs_cpus_by_id_when_one_goes_offline() {
        // Goal: a CPU going offline must not shift the others onto the wrong baseline, which is
        // what pairing by position did. Method: cpu1 disappears between snapshots, and cpu2 is
        // given a distinct load so a positional pairing would compute the wrong value for it.
        let prev = vec![times(0, 0, 0), times(1, 0, 0), times(2, 500, 1000)];
        let curr = vec![times(0, 500, 1000), times(2, 500, 2000)];
        let percents = compute_percents(&prev, &curr);
        assert_eq!(
            percents,
            vec![
                CpuPercent {
                    cpu_id: 0,
                    percent: 50.0
                },
                CpuPercent {
                    cpu_id: 2,
                    percent: 100.0
                },
            ]
        );
    }

    #[test]
    fn compute_percents_skips_a_cpu_that_came_online() {
        // Goal: a CPU with no previous snapshot has no baseline, so it is left out for this
        // interval rather than measured from boot or reported as 0. Method: cpu1 appears only in
        // the current snapshot, between two CPUs that were already online.
        let prev = vec![times(0, 0, 0), times(2, 0, 0)];
        let curr = vec![times(0, 500, 1000), times(1, 5, 10), times(2, 250, 1000)];
        let percents = compute_percents(&prev, &curr);
        let cpu_ids: Vec<u16> = percents.iter().map(|percent| percent.cpu_id).collect();
        assert_eq!(cpu_ids, vec![0, 2]);
        assert!((percents[1].percent - 75.0).abs() < f32::EPSILON);
    }

    #[test]
    fn parse_stat_content_keeps_the_cpu_id() {
        // Goal: the id comes from the `cpuN` name, not the line position, since /proc/stat
        // leaves offline CPUs out. Method: a snapshot with cpu1 missing.
        let content = "cpu  3 0 3 3000\ncpu0 1 0 1 1000\ncpu2 2 0 2 2000\nintr 0\n";
        let cpu_ids: Vec<u16> = parse_stat_content(content)
            .iter()
            .map(|times| times.cpu_id)
            .collect();
        assert_eq!(cpu_ids, vec![0, 2]);
        assert!(parse_cpu_line("cpux 1 0 1 1000").is_none());
    }

    // -- Integration: from fixture files --

    #[test]
    fn percent_from_two_fixtures() {
        // Goal: verify end-to-end percent calculation using two
        // fixture files representing a before/after snapshot.
        let content_before = fs::read_to_string(format!("{TEST_DIR}/8_cpu_normal")).unwrap();
        let content_after = fs::read_to_string(format!("{TEST_DIR}/8_cpu_after_load")).unwrap();
        let prev = parse_stat_content(&content_before);
        let curr = parse_stat_content(&content_after);
        assert_eq!(prev.len(), 8);
        assert_eq!(curr.len(), 8);
        let percents = compute_percents(&prev, &curr);
        assert_eq!(percents.len(), 8);
        // All CPUs should show some load (the "after" fixture has
        // higher user+system and slightly higher idle).
        for (i, pct) in percents.iter().map(|percent| percent.percent).enumerate() {
            assert!(pct > 0.0, "cpu{i} should have nonzero load, got {pct}");
            assert!(pct <= 100.0, "cpu{i} percent out of range: {pct}");
        }
        // cpu0: delta total = (127499+1601+48200+4275964+14051+25757+18230)
        //                    - (126499+1601+47200+4274964+14051+25757+18230)
        //       = 2000. delta idle = (4275964+14051)-(4274964+14051) = 1000.
        //       busy% = (2000-1000)/2000 * 100 = 50%.
        assert!(
            (percents[0].percent - 50.0).abs() < 0.1,
            "cpu0 expected ~50%, got {}",
            percents[0].percent
        );
    }

    #[test]
    fn collector_with_path_works_from_fixture() {
        // Goal: verify CpuPercentCollector::with_path initializes
        // from a fixture without panicking.
        let path = format!("{TEST_DIR}/8_cpu_normal");
        let collector = CpuPercentCollector::with_path(Path::new(&path));
        assert!(collector.is_ok());
    }

    #[test]
    #[should_panic(expected = "no per-cpu lines found")]
    fn collector_panics_on_empty_stat() {
        // Goal: verify the assert fires when /proc/stat has no cpu lines.
        let path = format!("{TEST_DIR}/empty_no_cpus");
        let _ = CpuPercentCollector::with_path(Path::new(&path));
    }

    #[test]
    fn collector_with_nonexistent_path_returns_error() {
        // Goal: verify a missing file returns an Err, not a panic.
        let result = CpuPercentCollector::with_path(Path::new("/tmp/does_not_exist_proc_stat"));
        assert!(result.is_err());
    }
}
