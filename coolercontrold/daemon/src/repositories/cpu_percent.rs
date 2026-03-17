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

use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Sane upper bound for logical CPUs. Prevents unbounded allocation
/// if /proc/stat is somehow corrupted or pathological.
const MAX_LOGICAL_CPUS: usize = 2048;

/// Minimum number of /proc/stat fields required per cpu line.
/// Fields: user nice system idle (iowait irq softirq steal are optional).
const MIN_STAT_FIELDS: usize = 4;

/// Maximum number of fields per cpu line in /proc/stat.
/// Fields: user nice system idle iowait irq softirq steal guest `guest_nice`.
const MAX_STAT_FIELDS: usize = 10;

/// Per-CPU time snapshot from /proc/stat, in kernel jiffies.
#[derive(Clone)]
struct CpuTimes {
    idle_jiffies: u64,
    total_jiffies: u64,
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

    /// Returns per-CPU busy percentages (0.0–100.0) since the last
    /// call. The length matches the number of logical CPUs visible in
    /// /proc/stat at call time.
    #[allow(clippy::cast_precision_loss)]
    pub fn cpu_percent_per_cpu(&mut self) -> Result<Vec<f32>> {
        let content = fs::read_to_string(&self.stat_path)
            .with_context(|| format!("reading {}", self.stat_path.display()))?;
        let curr = parse_stat_content(&content);
        let percents = compute_percents(&self.prev, &curr);
        self.prev = curr;
        Ok(percents)
    }
}

/// Computes per-CPU busy percentages from two snapshots.
#[allow(clippy::cast_precision_loss)]
fn compute_percents(prev: &[CpuTimes], curr: &[CpuTimes]) -> Vec<f32> {
    let mut percents = Vec::with_capacity(curr.len());
    for (cur, prev) in curr.iter().zip(prev.iter()) {
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
        percents.push(percent);
    }
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
/// Returns `None` if the line has fewer than `MIN_STAT_FIELDS` numeric
/// fields or contains non-numeric tokens.
///
/// Fields: user nice system idle [iowait irq softirq steal guest `guest_nice`]
fn parse_cpu_line(line: &str) -> Option<CpuTimes> {
    let mut fields = [0_u64; MAX_STAT_FIELDS];
    let mut field_count = 0;
    for token in line.split_ascii_whitespace().skip(1) {
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
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let percents = compute_percents(&snapshot, &snapshot);
        assert_eq!(percents.len(), 1);
        assert!((percents[0] - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_percents_full_load() {
        // Goal: verify that all-busy delta produces 100%.
        let prev = vec![CpuTimes {
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let curr = vec![CpuTimes {
            idle_jiffies: 500, // idle did not increase.
            total_jiffies: 2000,
        }];
        let percents = compute_percents(&prev, &curr);
        assert!((percents[0] - 100.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_percents_half_load() {
        // Goal: verify 50% load is computed when idle grows by half
        // of the total delta.
        let prev = vec![CpuTimes {
            idle_jiffies: 0,
            total_jiffies: 0,
        }];
        let curr = vec![CpuTimes {
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let percents = compute_percents(&prev, &curr);
        assert!((percents[0] - 50.0).abs() < f32::EPSILON);
    }

    #[test]
    fn compute_percents_mismatched_length_uses_shorter() {
        // Goal: verify that if CPU count changes between snapshots,
        // zip uses the shorter (no panic, no out-of-bounds).
        let prev = vec![
            CpuTimes {
                idle_jiffies: 0,
                total_jiffies: 0,
            },
            CpuTimes {
                idle_jiffies: 0,
                total_jiffies: 0,
            },
        ];
        let curr = vec![CpuTimes {
            idle_jiffies: 500,
            total_jiffies: 1000,
        }];
        let percents = compute_percents(&prev, &curr);
        // zip stops at min(2, 1) = 1.
        assert_eq!(percents.len(), 1);
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
        for (i, pct) in percents.iter().enumerate() {
            assert!(*pct > 0.0, "cpu{i} should have nonzero load, got {pct}");
            assert!(*pct <= 100.0, "cpu{i} percent out of range: {pct}");
        }
        // cpu0: delta total = (127499+1601+48200+4275964+14051+25757+18230)
        //                    - (126499+1601+47200+4274964+14051+25757+18230)
        //       = 2000. delta idle = (4275964+14051)-(4274964+14051) = 1000.
        //       busy% = (2000-1000)/2000 * 100 = 50%.
        assert!(
            (percents[0] - 50.0).abs() < 0.1,
            "cpu0 expected ~50%, got {}",
            percents[0]
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
