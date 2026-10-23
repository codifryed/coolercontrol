// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! DRM fdinfo based GPU load for AMD GPUs whose hwmon load sensor
//! (`gpu_busy_percent`) is unsupported, e.g. older GCN cards that return an
//! error from `amdgpu_dpm_read_sensor`. Mirrors nvtop: match DRM clients by PCI
//! address, dedup by `drm-client-id`, sum the `drm-engine-gfx` +
//! `drm-engine-compute` busy-nanosecond counters, and convert per-client deltas
//! over the poll interval into a 0..=100 percentage.

use std::collections::HashMap;
use std::fs::File;
use std::ops::Not;
use std::path::Path;
use std::time::{Duration, Instant};

use log::debug;

use crate::cc_fs;
use crate::rt;

const PROC_ROOT: &str = "/proc";
/// Bound on one fdinfo read. amdgpu's is well under a page; this only stops a runaway file.
const FDINFO_BYTES_MAX: usize = 64 * 1024;
const DRM_NODE_PREFIX: &str = "/dev/dri/";
const KEY_PDEV: &str = "drm-pdev";
const KEY_CLIENT_ID: &str = "drm-client-id";
const KEY_ENGINE_GFX: &str = "drm-engine-gfx";
const KEY_ENGINE_COMPUTE: &str = "drm-engine-compute";
const ENGINE_NS_SUFFIX: &str = " ns";

/// How often `/proc` is walked for DRM clients. A walk reads every fd link on the system, about
/// 25 ms of CPU on a desktop with 17,000 fds, while a tick needs only the dozen that are DRM
/// clients. Between walks only the clients already found are read, so one that appears in between
/// counts from the walk after it.
const CLIENT_WALK_INTERVAL: Duration = Duration::from_secs(10);

/// Bound on the fdinfo files held per GPU. A desktop holds a few dozen DRM fds; past this a walk
/// stops collecting rather than holding descriptors without limit.
const DRM_FDS_MAX: usize = 1024;

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
    ///
    /// `clients` comes back holding the previous scan, for the caller to clear and refill, so
    /// neither map reallocates once both have room.
    pub fn update(&mut self, clients: &mut HashMap<u64, u64>, now: Instant) -> f64 {
        let percent = match self.last_sample {
            Some(last) => {
                let elapsed_ns = u64::try_from(now.saturating_duration_since(last).as_nanos())
                    .unwrap_or(u64::MAX);
                busy_percent(&self.last_clients, clients, elapsed_ns)
            }
            None => 0.0,
        };
        std::mem::swap(&mut self.last_clients, clients);
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

/// One GPU's fdinfo load: the DRM clients found by walking `/proc`, their busy counters this tick,
/// and the delta state.
#[derive(Default)]
pub struct FdInfoTracker {
    clients: DrmClients,
    /// Client-id to cumulative gfx+compute busy ns, refilled each tick.
    scanned: HashMap<u64, u64>,
    load: FdInfoLoadState,
}

impl FdInfoTracker {
    /// One tick: reads this GPU's DRM clients, walking `/proc` first when a walk is due, and
    /// returns the device busy percentage.
    pub async fn sample(&mut self, pci_slot: &str, buf: &mut Vec<u8>, now: Instant) -> f64 {
        self.sample_in(Path::new(PROC_ROOT), pci_slot, buf, now)
            .await
    }

    async fn sample_in(
        &mut self,
        proc_root: &Path,
        pci_slot: &str,
        buf: &mut Vec<u8>,
        now: Instant,
    ) -> f64 {
        self.clients
            .scan_in(proc_root, pci_slot, buf, now, &mut self.scanned)
            .await;
        self.load.update(&mut self.scanned, now)
    }
}

/// The DRM client fds of one GPU, found by an occasional walk of `/proc` and held open between
/// walks.
///
/// Every read is of `/proc`, with no device behind it, so this runs on the runtime thread under
/// the `ProcFile` rule: positional reads into the caller's buffer, yielding after each read and
/// after each process a walk covers.
#[derive(Default)]
struct DrmClients {
    /// The fdinfo of each DRM fd the last walk found, so a tick costs one read per client.
    fdinfos: Vec<File>,
    /// When `/proc` was last walked. `None` until the first tick.
    walked_at: Option<Instant>,
}

impl DrmClients {
    /// Fills `scanned` with this tick's client-id to busy ns for `pci_slot`.
    async fn scan_in(
        &mut self,
        proc_root: &Path,
        pci_slot: &str,
        buf: &mut Vec<u8>,
        now: Instant,
        scanned: &mut HashMap<u64, u64>,
    ) {
        if walk_due(self.walked_at, now) {
            self.fdinfos.clear();
            walk_drm_fds(proc_root, &mut self.fdinfos).await;
            self.walked_at = Some(now);
        }
        debug_assert!(self.fdinfos.len() <= DRM_FDS_MAX);
        scanned.clear();
        read_clients(&mut self.fdinfos, pci_slot, buf, scanned).await;
    }
}

/// Whether `/proc` is due another walk: on the first tick, then once the interval has passed.
fn walk_due(walked_at: Option<Instant>, now: Instant) -> bool {
    walked_at.is_none_or(|walked| now.saturating_duration_since(walked) >= CLIENT_WALK_INTERVAL)
}

/// Reads each held fdinfo into `scanned`, dropping any that no longer belongs to a DRM client:
/// the process closed the fd or exited, or reused the fd number for another file.
async fn read_clients(
    fdinfos: &mut Vec<File>,
    pci_slot: &str,
    buf: &mut Vec<u8>,
    scanned: &mut HashMap<u64, u64>,
) {
    let held_count = fdinfos.len();
    let mut index = 0;
    // Bounded: each pass either moves past one file or drops one.
    while index < fdinfos.len() {
        let read = cc_fs::read_proc_held(&fdinfos[index], buf, FDINFO_BYTES_MAX, |content| {
            accumulate_fdinfo(content, pci_slot, scanned)
        })
        .await;
        if matches!(read, Ok(true)) {
            index += 1;
        } else {
            // Order does not matter: each file is read on its own.
            fdinfos.swap_remove(index);
        }
    }
    debug_assert!(fdinfos.len() <= held_count);
}

/// Walks `/proc` for every fd that points at a DRM node and opens its fdinfo into `fdinfos`.
async fn walk_drm_fds(proc_root: &Path, fdinfos: &mut Vec<File>) {
    debug_assert!(fdinfos.is_empty());
    let Ok(entries) = cc_fs::read_dir(proc_root) else {
        return;
    };
    for proc_entry in entries.flatten() {
        let pid = proc_entry.file_name();
        let Some(pid) = pid.to_str() else { continue };
        if pid.is_empty() || pid.bytes().all(|b| b.is_ascii_digit()).not() {
            continue;
        }
        if collect_pid_drm_fds(proc_root, pid, fdinfos).not() {
            debug!("Holding {DRM_FDS_MAX} DRM fds, the most kept; the rest are not read");
            return;
        }
        // A process's fd walk is one readlink per fd, so the runtime gets a turn between
        // processes even when none of them is a DRM client.
        rt::yield_now().await;
    }
}

/// Opens the fdinfo of each of one process's fds that points at a DRM node (`/dev/dri/...`).
/// Errors (process exited, no permission) skip the fd rather than propagate. Returns false once
/// `DRM_FDS_MAX` files are held.
fn collect_pid_drm_fds(proc_root: &Path, pid: &str, fdinfos: &mut Vec<File>) -> bool {
    let Ok(fds) = cc_fs::read_dir(proc_root.join(pid).join("fd")) else {
        return true;
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
        if fdinfos.len() >= DRM_FDS_MAX {
            return false;
        }
        // The process can close the fd or exit between the readlink and this open.
        if let Ok(fdinfo) = File::open(proc_root.join(pid).join("fdinfo").join(fd_name)) {
            fdinfos.push(fdinfo);
        }
    }
    true
}

/// Parses one fdinfo file; if it is an amdgpu DRM client for `pci_slot` that
/// exposes engine accounting, records its cumulative gfx+compute ns keyed by
/// `drm-client-id` (deduping a client opened via multiple fds).
///
/// Returns whether the file belongs to a DRM client at all, which an idle client or one of
/// another GPU still does.
fn accumulate_fdinfo(content: &str, pci_slot: &str, clients: &mut HashMap<u64, u64>) -> bool {
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
    client_id.is_some()
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
        assert_eq!(state.update(&mut HashMap::from([(1u64, 0u64)]), t0), 0.0);
        let t1 = t0 + std::time::Duration::from_secs(1);
        let pct = state.update(&mut HashMap::from([(1u64, 250_000_000u64)]), t1);
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

            let clients = scan_once(&proc_root, Instant::now()).await;
            std::fs::remove_dir_all(&proc_root).ok();

            assert_eq!(clients.len(), 1);
            assert_eq!(clients.get(&100), Some(&2000));
        });
    }

    #[test]
    fn a_scan_yields_after_each_process() {
        // Goal: a busy /proc never holds the runtime for the whole scan. Each process's fd walk
        // ends in a yield, even when it reads no fdinfo. Method: three processes with only
        // non-DRM fds, polled to the end with a waker that does nothing, counting the polls.
        let proc_root = tempfile::tempdir().unwrap();
        for pid in ["11", "12", "13"] {
            let fd_dir = proc_root.path().join(pid).join("fd");
            std::fs::create_dir_all(&fd_dir).unwrap();
            std::os::unix::fs::symlink("/dev/null", fd_dir.join("0")).unwrap();
        }
        let mut drm_clients = DrmClients::default();
        let mut buf = Vec::new();
        let mut clients = HashMap::new();

        let ((), polls) = crate::rt::poll_to_end(drm_clients.scan_in(
            proc_root.path(),
            PDEV,
            &mut buf,
            Instant::now(),
            &mut clients,
        ));

        assert!(clients.is_empty());
        // One yield per process, then the poll that finds the scan done.
        assert_eq!(polls, 4);
        assert!(
            buf.is_empty(),
            "no fdinfo was read, so the buffer never grew"
        );
    }

    /// One scan against a fresh client cache, as the first tick does.
    async fn scan_once(proc_root: &Path, now: Instant) -> HashMap<u64, u64> {
        let mut clients = HashMap::new();
        DrmClients::default()
            .scan_in(proc_root, PDEV, &mut Vec::new(), now, &mut clients)
            .await;
        clients
    }

    fn drm_fdinfo(client_id: u64, busy_ns: u64) -> String {
        format!("drm-pdev:\t{PDEV}\ndrm-client-id:\t{client_id}\ndrm-engine-gfx:\t{busy_ns} ns\n")
    }

    #[test]
    fn a_walk_is_due_on_the_first_tick_and_after_the_interval() {
        // Goal: the walk runs on the first tick and once the interval has passed, and not in
        // between. Method: a table of last-walk times against one tick time.
        let now = Instant::now();
        let cases = [
            (None, true),
            (Some(now), false),
            (Some(now - Duration::from_secs(1)), false),
            (Some(now - CLIENT_WALK_INTERVAL), true),
            (Some(now - CLIENT_WALK_INTERVAL * 3), true),
        ];
        for (walked_at, due) in cases {
            assert_eq!(walk_due(walked_at, now), due, "walked at {walked_at:?}");
        }
        // A clock that reads earlier than the last walk is not a reason to walk.
        assert!(walk_due(Some(now + Duration::from_secs(1)), now).not());
    }

    #[test]
    fn a_client_appearing_between_walks_counts_from_the_next_walk() {
        // Goal: between walks only the clients already found are read, and the next walk picks
        // up the rest. Method: one client at the first tick, a second added, then ticks before
        // and after the walk interval.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            let mut drm_clients = DrmClients::default();
            let mut buf = Vec::new();
            let mut clients = HashMap::new();
            let t0 = Instant::now();
            drm_clients
                .scan_in(root, PDEV, &mut buf, t0, &mut clients)
                .await;
            assert_eq!(clients, HashMap::from([(1, 100)]));

            write_client(root, "20", "4", "/dev/dri/renderD128", &drm_fdinfo(2, 200));
            let before_walk = t0 + Duration::from_secs(1);
            drm_clients
                .scan_in(root, PDEV, &mut buf, before_walk, &mut clients)
                .await;
            assert_eq!(clients, HashMap::from([(1, 100)]), "no walk yet");

            let at_walk = t0 + CLIENT_WALK_INTERVAL;
            drm_clients
                .scan_in(root, PDEV, &mut buf, at_walk, &mut clients)
                .await;
            assert_eq!(clients, HashMap::from([(1, 100), (2, 200)]));
        });
    }

    #[test]
    fn a_held_client_reports_its_current_counter() {
        // Goal: a held client is read afresh each tick, so its counter advances without a walk.
        // Method: rewrite a cached client's fdinfo in place between two ticks.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            let mut drm_clients = DrmClients::default();
            let mut buf = Vec::new();
            let mut clients = HashMap::new();
            let t0 = Instant::now();
            drm_clients
                .scan_in(root, PDEV, &mut buf, t0, &mut clients)
                .await;

            std::fs::write(root.join("10/fdinfo/3"), drm_fdinfo(1, 900)).unwrap();
            let t1 = t0 + Duration::from_secs(1);
            drm_clients
                .scan_in(root, PDEV, &mut buf, t1, &mut clients)
                .await;

            assert_eq!(clients, HashMap::from([(1, 900)]));
        });
    }

    #[test]
    fn a_held_file_that_stops_being_a_drm_client_is_dropped() {
        // Goal: a cached fd whose number the process reused for another file stops being read,
        // and is not read again even if it turns back into a client before the next walk.
        // An idle client (no engine lines) and one of another GPU stay held. Method: three
        // cached files, one rewritten as an ordinary fdinfo, then as a client again.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            let idle = format!("drm-pdev:\t{PDEV}\ndrm-client-id:\t2\n");
            write_client(root, "20", "4", "/dev/dri/renderD128", &idle);
            let other_gpu = "drm-pdev:\t0000:09:00.0\ndrm-client-id:\t3\n";
            write_client(root, "30", "5", "/dev/dri/renderD129", other_gpu);
            let mut drm_clients = DrmClients::default();
            let mut buf = Vec::new();
            let mut clients = HashMap::new();
            let t0 = Instant::now();
            drm_clients
                .scan_in(root, PDEV, &mut buf, t0, &mut clients)
                .await;
            assert_eq!(drm_clients.fdinfos.len(), 3);

            std::fs::write(root.join("10/fdinfo/3"), "pos:\t0\nflags:\t02\n").unwrap();
            let t1 = t0 + Duration::from_secs(1);
            drm_clients
                .scan_in(root, PDEV, &mut buf, t1, &mut clients)
                .await;
            assert!(clients.is_empty());
            assert_eq!(
                drm_clients.fdinfos.len(),
                2,
                "the idle and other-GPU clients stay"
            );

            std::fs::write(root.join("10/fdinfo/3"), drm_fdinfo(1, 500)).unwrap();
            let t2 = t0 + Duration::from_secs(2);
            drm_clients
                .scan_in(root, PDEV, &mut buf, t2, &mut clients)
                .await;
            assert!(clients.is_empty(), "a dropped file waits for the next walk");
        });
    }

    #[test]
    fn a_held_file_that_fails_to_read_is_dropped() {
        // Goal: a cached fd the process closed, or a process that exited, stops being read.
        // Method: a fake fdinfo that opens but cannot be read (a directory), as a closed fd's
        // fdinfo fails its read.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            let fd_dir = root.join("20/fd");
            std::fs::create_dir_all(&fd_dir).unwrap();
            std::os::unix::fs::symlink("/dev/dri/renderD128", fd_dir.join("4")).unwrap();
            std::fs::create_dir_all(root.join("20/fdinfo/4")).unwrap();
            let mut drm_clients = DrmClients::default();
            let mut clients = HashMap::new();

            drm_clients
                .scan_in(root, PDEV, &mut Vec::new(), Instant::now(), &mut clients)
                .await;

            assert_eq!(clients, HashMap::from([(1, 100)]));
            assert_eq!(
                drm_clients.fdinfos.len(),
                1,
                "the unreadable file is dropped"
            );
        });
    }

    #[test]
    fn a_walk_holds_at_most_the_bound() {
        // Goal: a walk never holds more fdinfo files than its bound, however many DRM fds the
        // system has. Method: one process with one DRM fd past the bound.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            let fd_dir = root.join("10/fd");
            let fdinfo_dir = root.join("10/fdinfo");
            std::fs::create_dir_all(&fd_dir).unwrap();
            std::fs::create_dir_all(&fdinfo_dir).unwrap();
            for fd in 0..=DRM_FDS_MAX {
                std::os::unix::fs::symlink("/dev/dri/renderD128", fd_dir.join(fd.to_string()))
                    .unwrap();
                std::fs::write(fdinfo_dir.join(fd.to_string()), drm_fdinfo(1, 100)).unwrap();
            }
            let mut fdinfos = Vec::new();

            walk_drm_fds(root, &mut fdinfos).await;

            assert_eq!(fdinfos.len(), DRM_FDS_MAX);
        });
    }

    /// Goal: a tick between walks allocates nothing, which is what the cache is for. Method: warm
    /// the cache and both client maps with a walk and one more tick, then count the allocations
    /// of the next tick.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_tick_between_walks_allocates_nothing() {
        let proc_root = tempfile::tempdir().unwrap();
        let root = proc_root.path();
        write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
        write_client(root, "20", "4", "/dev/dri/renderD128", &drm_fdinfo(2, 200));
        let mut tracker = FdInfoTracker::default();
        let mut buf = Vec::new();
        let t0 = Instant::now();
        crate::rt::poll_to_end(tracker.sample_in(root, PDEV, &mut buf, t0));
        let t1 = t0 + Duration::from_secs(1);
        crate::rt::poll_to_end(tracker.sample_in(root, PDEV, &mut buf, t1));
        let t2 = t0 + Duration::from_secs(2);

        let ((percent, _), allocations) = crate::alloc_count::count_allocations(|| {
            crate::rt::poll_to_end(tracker.sample_in(root, PDEV, &mut buf, t2))
        });

        assert_eq!(allocations, 0);
        assert!((0.0..=100.0).contains(&percent));
    }
}
