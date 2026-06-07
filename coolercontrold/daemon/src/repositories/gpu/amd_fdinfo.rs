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

//! DRM fdinfo based GPU load for AMD GPUs whose hwmon load sensor
//! (`gpu_busy_percent`) is unsupported, e.g. older GCN cards that return an
//! error from `amdgpu_dpm_read_sensor`. Mirrors nvtop: match DRM clients by PCI
//! address, dedup by `drm-client-id`, sum the `drm-engine-gfx` +
//! `drm-engine-compute` busy-nanosecond counters, and convert per-client deltas
//! over the poll interval into a 0..=100 percentage.

use std::collections::HashMap;
use std::ops::Not;
use std::path::Path;
use std::time::Instant;

use crate::cc_fs;

const PROC_ROOT: &str = "/proc";
const DRM_NODE_PREFIX: &str = "/dev/dri/";
const KEY_PDEV: &str = "drm-pdev";
const KEY_CLIENT_ID: &str = "drm-client-id";
const KEY_ENGINE_GFX: &str = "drm-engine-gfx";
const KEY_ENGINE_COMPUTE: &str = "drm-engine-compute";
const ENGINE_NS_SUFFIX: &str = " ns";

/// Marks a GPU whose load is derived from DRM fdinfo. Held in `AMDDriverInfo`.
/// `pci_slot` is the `drm-pdev` value (e.g. `0000:01:00.0`) used to match
/// fdinfo clients to this GPU.
#[derive(Debug, Clone)]
pub struct FdInfoLoad {
    pci_slot: String,
}

impl FdInfoLoad {
    pub fn new(pci_slot: String) -> Self {
        Self { pci_slot }
    }

    pub fn pci_slot(&self) -> &str {
        &self.pci_slot
    }
}

/// Per-device delta state across polls. `last_clients` maps `drm-client-id` to
/// its cumulative gfx+compute busy ns as of `last_sample`.
#[derive(Default)]
pub struct FdInfoLoadState {
    last_clients: HashMap<u64, u64>,
    last_sample: Option<Instant>,
}

impl FdInfoLoadState {
    /// Folds a fresh scan into the state and returns the device busy percentage.
    /// The first sample has no baseline so it reports 0 rather than omitting:
    /// once fdinfo is the chosen source we always emit a value.
    pub fn update(&mut self, clients: HashMap<u64, u64>, now: Instant) -> f64 {
        let percent = match self.last_sample {
            Some(last) => {
                let elapsed_ns = u64::try_from(now.saturating_duration_since(last).as_nanos())
                    .unwrap_or(u64::MAX);
                busy_percent(&self.last_clients, &clients, elapsed_ns)
            }
            None => 0.0,
        };
        self.last_clients = clients;
        self.last_sample = Some(now);
        percent
    }
}

/// Sums per-client busy-ns deltas over `elapsed_ns` into a 0..=100 percentage.
/// Each client is capped at the interval (a client cannot be busier than wall
/// time) and the device total is capped at 100, matching nvtop. Clients absent
/// from the previous sample contribute nothing; a counter that went backwards
/// (client reset) is skipped.
fn busy_percent(last: &HashMap<u64, u64>, cur: &HashMap<u64, u64>, elapsed_ns: u64) -> f64 {
    if elapsed_ns == 0 {
        return 0.0;
    }
    let mut busy_ns: u64 = 0;
    for (client_id, &cur_ns) in cur {
        let Some(&last_ns) = last.get(client_id) else {
            continue;
        };
        if cur_ns >= last_ns {
            busy_ns = busy_ns.saturating_add((cur_ns - last_ns).min(elapsed_ns));
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let percent = ((busy_ns as f64 / elapsed_ns as f64) * 100.0).min(100.0);
    debug_assert!((0.0..=100.0).contains(&percent));
    percent
}

/// Scans `/proc` for DRM clients of `pci_slot` and returns client-id ->
/// cumulative gfx+compute busy ns. Small `/proc` reads (no hardware ioctl):
/// fdinfo content via async `cc_fs::read_txt`, enumeration via the sync `cc_fs`
/// dir/link helpers (compio has no async equivalent).
pub async fn scan_clients(pci_slot: &str) -> HashMap<u64, u64> {
    scan_clients_in(Path::new(PROC_ROOT), pci_slot).await
}

async fn scan_clients_in(proc_root: &Path, pci_slot: &str) -> HashMap<u64, u64> {
    let mut clients = HashMap::new();
    let Ok(entries) = cc_fs::read_dir(proc_root) else {
        return clients;
    };
    for proc_entry in entries.flatten() {
        let pid = proc_entry.file_name();
        let Some(pid) = pid.to_str() else { continue };
        if pid.is_empty() || pid.bytes().all(|b| b.is_ascii_digit()).not() {
            continue;
        }
        accumulate_pid_clients(proc_root, pid, pci_slot, &mut clients).await;
    }
    clients
}

/// Walks one process's open fds, parsing the fdinfo of any that point at a DRM
/// node (`/dev/dri/...`). Read errors (process exited, no permission) are
/// skipped rather than propagated.
async fn accumulate_pid_clients(
    proc_root: &Path,
    pid: &str,
    pci_slot: &str,
    clients: &mut HashMap<u64, u64>,
) {
    let Ok(fds) = cc_fs::read_dir(proc_root.join(pid).join("fd")) else {
        return;
    };
    for fd_entry in fds.flatten() {
        let Ok(target) = cc_fs::read_link(fd_entry.path()) else {
            continue;
        };
        if target
            .to_str()
            .is_some_and(|t| t.starts_with(DRM_NODE_PREFIX))
            .not()
        {
            continue;
        }
        let fd_name = fd_entry.file_name();
        let Some(fd_name) = fd_name.to_str() else {
            continue;
        };
        let fdinfo_path = proc_root.join(pid).join("fdinfo").join(fd_name);
        if let Ok(content) = cc_fs::read_txt(&fdinfo_path).await {
            accumulate_fdinfo(&content, pci_slot, clients);
        }
    }
}

/// Parses one fdinfo file; if it is an amdgpu DRM client for `pci_slot` that
/// exposes engine accounting, records its cumulative gfx+compute ns keyed by
/// `drm-client-id` (deduping a client opened via multiple fds).
fn accumulate_fdinfo(content: &str, pci_slot: &str, clients: &mut HashMap<u64, u64>) {
    let mut pdev_matches = false;
    let mut client_id: Option<u64> = None;
    let mut busy_ns: u64 = 0;
    let mut engine_seen = false;
    for line in content.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            KEY_PDEV => pdev_matches = value.trim() == pci_slot,
            KEY_CLIENT_ID => client_id = value.trim().parse().ok(),
            KEY_ENGINE_GFX | KEY_ENGINE_COMPUTE => {
                if let Some(ns) = parse_engine_ns(value.trim()) {
                    busy_ns = busy_ns.saturating_add(ns);
                    engine_seen = true;
                }
            }
            _ => {}
        }
    }
    if pdev_matches && engine_seen {
        if let Some(client_id) = client_id {
            clients.insert(client_id, busy_ns);
        }
    }
}

/// True if `fdinfo_content` comes from the standard DRM fdinfo framework,
/// identified by the always-present `drm-client-id` key. amdgpu emits that key
/// only via the same framework that carries `drm-engine-*` accounting, so it is
/// a usage-independent signal that engine accounting is available, even for a
/// freshly opened client that submitted no work (the engine lines themselves are
/// gated on usage > 0 in the kernel and so are absent until the GPU does work).
/// Lets us gate the fallback without needing any client present.
pub fn engine_accounting_available(fdinfo_content: &str) -> bool {
    fdinfo_content.lines().any(|line| {
        line.split_once(':')
            .is_some_and(|(key, _)| key.trim() == KEY_CLIENT_ID)
    })
}

/// Parses an engine value like `"123456 ns"` into nanoseconds. Returns None for
/// the legacy percentage format (no ` ns` suffix), which we do not support.
fn parse_engine_ns(value: &str) -> Option<u64> {
    value
        .strip_suffix(ENGINE_NS_SUFFIX)?
        .trim_end()
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use uuid::Uuid;

    const PDEV: &str = "0000:01:00.0";

    #[test]
    fn accumulate_fdinfo_sums_gfx_and_compute_for_matching_pdev() {
        // Goal: a matching client records gfx+compute ns keyed by client-id.
        let content = format!(
            "pos:\t0\ndrm-driver:\tamdgpu\ndrm-pdev:\t{PDEV}\ndrm-client-id:\t42\n\
             drm-engine-gfx:\t1000 ns\ndrm-engine-compute:\t500 ns\n"
        );
        let mut clients = HashMap::new();
        accumulate_fdinfo(&content, PDEV, &mut clients);
        assert_eq!(clients.get(&42), Some(&1500));
    }

    #[test]
    fn engine_accounting_available_detects_standard_framework() {
        // Goal: the unconditional drm-client-id key signals the standard fdinfo
        // framework (and thus engine accounting) even with no drm-engine-* lines;
        // the legacy amdgpu format without it does not.
        assert!(engine_accounting_available(
            "drm-driver:\tamdgpu\ndrm-client-id:\t5\ndrm-pdev:\t0000:01:00.0\n"
        ));
        assert!(engine_accounting_available("pos:\t0\npasid:\t32769\ngfx:\t12%\n").not());
    }

    #[test]
    fn accumulate_fdinfo_skips_other_pdev() {
        // Goal: a client on a different GPU is ignored.
        let content = "drm-pdev:\t0000:09:00.0\ndrm-client-id:\t7\ndrm-engine-gfx:\t900 ns\n";
        let mut clients = HashMap::new();
        accumulate_fdinfo(content, PDEV, &mut clients);
        assert!(clients.is_empty());
    }

    #[test]
    fn accumulate_fdinfo_skips_without_engine_or_client_id() {
        // Goal: a matching client with no engine accounting (old kernel) or no
        // client-id is not recorded, so the device is not falsely enabled.
        let no_engine = format!("drm-pdev:\t{PDEV}\ndrm-client-id:\t1\n");
        let no_cid = format!("drm-pdev:\t{PDEV}\ndrm-engine-gfx:\t10 ns\n");
        let legacy_pct = format!("drm-pdev:\t{PDEV}\ndrm-client-id:\t1\ndrm-engine-gfx:\t55%\n");
        let mut clients = HashMap::new();
        accumulate_fdinfo(&no_engine, PDEV, &mut clients);
        accumulate_fdinfo(&no_cid, PDEV, &mut clients);
        accumulate_fdinfo(&legacy_pct, PDEV, &mut clients);
        assert!(clients.is_empty());
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact ns/elapsed ratios, no rounding
    fn busy_percent_basic_and_caps() {
        // Goal: delta/elapsed -> %, per-client capped at the interval, device
        // total capped at 100, new/backward clients ignored.
        let sec = 1_000_000_000u64;
        // one client busy 500ms of 1s -> 50%.
        let last = HashMap::from([(1u64, 0u64)]);
        let cur = HashMap::from([(1u64, sec / 2)]);
        assert!((busy_percent(&last, &cur, sec) - 50.0).abs() < f64::EPSILON);
        // two clients each busy 60% -> capped to 100, not 120.
        let last2 = HashMap::from([(1u64, 0u64), (2u64, 0u64)]);
        let cur2 = HashMap::from([(1u64, sec * 6 / 10), (2u64, sec * 6 / 10)]);
        assert!((busy_percent(&last2, &cur2, sec) - 100.0).abs() < f64::EPSILON);
        // single client busier than wall time -> capped at 100.
        let cur3 = HashMap::from([(1u64, sec * 2)]);
        assert!((busy_percent(&last, &cur3, sec) - 100.0).abs() < f64::EPSILON);
        // new client (not in last) and zero elapsed -> 0.
        let cur_new = HashMap::from([(9u64, sec)]);
        assert_eq!(busy_percent(&last, &cur_new, sec), 0.0);
        assert_eq!(busy_percent(&last, &cur, 0), 0.0);
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact 0.0 baseline
    fn update_reports_zero_first_sample_then_delta() {
        // Goal: the first update has no baseline so it reports 0 (not omitted);
        // the next update reports the delta-derived percentage.
        let mut state = FdInfoLoadState::default();
        let t0 = Instant::now();
        assert_eq!(state.update(HashMap::from([(1u64, 0u64)]), t0), 0.0);
        let t1 = t0 + std::time::Duration::from_secs(1);
        let pct = state.update(HashMap::from([(1u64, 250_000_000u64)]), t1);
        assert!((pct - 25.0).abs() < 0.5, "expected ~25%, got {pct}");
    }

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cc-fdinfo-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write_client(proc_root: &Path, pid: &str, fd: &str, target: &str, fdinfo: &str) {
        let fd_dir = proc_root.join(pid).join("fd");
        let fdinfo_dir = proc_root.join(pid).join("fdinfo");
        std::fs::create_dir_all(&fd_dir).unwrap();
        std::fs::create_dir_all(&fdinfo_dir).unwrap();
        std::os::unix::fs::symlink(target, fd_dir.join(fd)).unwrap();
        std::fs::write(fdinfo_dir.join(fd), fdinfo).unwrap();
    }

    #[test]
    fn scan_clients_in_collects_matching_drm_clients() {
        // Goal: end-to-end scan over a fake /proc picks up only DRM fds for the
        // target pdev and dedups a client opened via two fds (same client-id).
        cc_fs::test_runtime(async {
            let proc_root = temp_dir();
            let drm = format!("drm-pdev:\t{PDEV}\ndrm-client-id:\t100\ndrm-engine-gfx:\t2000 ns\n");
            // pid 123: one DRM fd (client 100) and one non-DRM fd (ignored).
            write_client(&proc_root, "123", "3", "/dev/dri/renderD128", &drm);
            std::os::unix::fs::symlink("/dev/null", proc_root.join("123").join("fd").join("4"))
                .unwrap();
            std::fs::write(proc_root.join("123").join("fdinfo").join("4"), "pos:\t0\n").unwrap();
            // pid 456: same client-id 100 via a second fd -> deduped, counted once.
            write_client(&proc_root, "456", "5", "/dev/dri/renderD128", &drm);
            // non-numeric dir ignored.
            std::fs::create_dir_all(proc_root.join("self")).unwrap();

            let clients = scan_clients_in(&proc_root, PDEV).await;
            std::fs::remove_dir_all(&proc_root).ok();

            assert_eq!(clients.len(), 1);
            assert_eq!(clients.get(&100), Some(&2000));
        });
    }
}
