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

/// Bound on the clients held per GPU, one fdinfo file each. A desktop has a dozen or so. Any local
/// user can open the render node, so past this a walk holds no more descriptors, and every tick
/// walks instead, as before the cache: more clients can cost the old per-tick walk, never the
/// daemon's open files or an uncounted client.
const DRM_CLIENTS_HELD_MAX: usize = 64;
const _: () = assert!(DRM_CLIENTS_HELD_MAX > 0);

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
    pub async fn sample(&mut self, pci_slot: &str, buf: &mut Vec<u8>) -> f64 {
        self.sample_in(Path::new(PROC_ROOT), pci_slot, buf, Instant::now)
            .await
    }

    /// `clock` is read before the scan, to decide on a walk, and again after it, to stamp the
    /// counters: a walk takes long enough that its start would skew the load.
    async fn sample_in(
        &mut self,
        proc_root: &Path,
        pci_slot: &str,
        buf: &mut Vec<u8>,
        mut clock: impl FnMut() -> Instant,
    ) -> f64 {
        let started = clock();
        self.clients
            .scan_in(proc_root, pci_slot, buf, started, &mut self.scanned)
            .await;
        let scanned_at = clock();
        debug_assert!(scanned_at >= started);
        let percent = self.load.update(&mut self.scanned, scanned_at);
        debug_assert!((0.0..=100.0).contains(&percent));
        percent
    }
}

/// The DRM clients of one GPU, found by an occasional walk of `/proc` and held open between walks,
/// one fdinfo file per client.
///
/// Every read is of `/proc`, with no device behind it, so this runs on the runtime thread under
/// the `ProcFile` rule: positional reads into the caller's buffer, yielding after each read and
/// after each process a walk covers.
#[derive(Default)]
struct DrmClients {
    /// The fdinfo of each client the last walk found, so a tick costs one read per client.
    fdinfos: Vec<File>,
    /// The client-id each held file had when found, in step with `fdinfos`, so a client open
    /// through several fds is held once.
    held_ids: Vec<u64>,
    /// When `/proc` was last walked. `None` until the first tick.
    walked_at: Option<Instant>,
    /// Whether the last walk found more clients than it may hold. Every tick walks until one
    /// finds room again, so no client goes uncounted.
    overflowed: bool,
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
        scanned.clear();
        if self.overflowed || walk_due(self.walked_at, now) {
            // A walk reads every client it finds, so the held files need no read this tick.
            self.walk(proc_root, pci_slot, buf, scanned).await;
            self.walked_at = Some(now);
        } else {
            self.read_held(pci_slot, buf, scanned).await;
        }
        debug_assert_eq!(self.fdinfos.len(), self.held_ids.len());
        debug_assert!(self.fdinfos.len() <= DRM_CLIENTS_HELD_MAX);
    }

    /// Reads each held fdinfo into `scanned`, dropping any that no longer belongs to one of this
    /// GPU's clients: the process closed the fd or exited, or reused the fd number for another
    /// file.
    async fn read_held(
        &mut self,
        pci_slot: &str,
        buf: &mut Vec<u8>,
        scanned: &mut HashMap<u64, u64>,
    ) {
        let held_count = self.fdinfos.len();
        let mut index = 0;
        // Bounded: each pass either moves past one file or drops one.
        while index < self.fdinfos.len() {
            let read =
                cc_fs::read_proc_held(&self.fdinfos[index], buf, FDINFO_BYTES_MAX, |content| {
                    accumulate_fdinfo(content, pci_slot, scanned)
                })
                .await;
            if matches!(read, Ok(Some(_))) {
                index += 1;
            } else {
                // Order does not matter: each file is read on its own.
                self.fdinfos.swap_remove(index);
                self.held_ids.swap_remove(index);
            }
        }
        debug_assert!(self.fdinfos.len() <= held_count);
    }

    /// Walks `/proc` for this GPU's DRM clients, reading each into `scanned` and holding its
    /// fdinfo for the ticks until the next walk.
    async fn walk(
        &mut self,
        proc_root: &Path,
        pci_slot: &str,
        buf: &mut Vec<u8>,
        scanned: &mut HashMap<u64, u64>,
    ) {
        self.fdinfos.clear();
        self.held_ids.clear();
        self.overflowed = false;
        let Ok(entries) = cc_fs::read_dir(proc_root) else {
            return;
        };
        for proc_entry in entries.flatten() {
            let pid = proc_entry.file_name();
            let Some(pid) = pid.to_str() else { continue };
            if pid.is_empty() || pid.bytes().all(|b| b.is_ascii_digit()).not() {
                continue;
            }
            self.walk_pid(proc_root, pid, pci_slot, buf, scanned).await;
            // A process's fd walk is one readlink per fd, so the runtime gets a turn between
            // processes even when none of them is a DRM client.
            rt::yield_now().await;
        }
        debug_assert!(self.held_ids.len() <= DRM_CLIENTS_HELD_MAX);
        debug_assert!(self.overflowed.not() || self.held_ids.len() == DRM_CLIENTS_HELD_MAX);
        if self.overflowed {
            debug!(
                "More than {DRM_CLIENTS_HELD_MAX} DRM clients on {pci_slot}; walking /proc \
                 every tick until fewer remain"
            );
        }
    }

    /// Reads the fdinfo of each of one process's fds that points at a DRM node (`/dev/dri/...`).
    /// Errors (process exited, no permission) skip the fd rather than propagate.
    async fn walk_pid(
        &mut self,
        proc_root: &Path,
        pid: &str,
        pci_slot: &str,
        buf: &mut Vec<u8>,
        scanned: &mut HashMap<u64, u64>,
    ) {
        debug_assert!(self.held_ids.len() <= DRM_CLIENTS_HELD_MAX);
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
            // The process can close the fd or exit between the readlink and this open.
            let Ok(fdinfo) = File::open(proc_root.join(pid).join("fdinfo").join(fd_name)) else {
                continue;
            };
            let read = cc_fs::read_proc_held(&fdinfo, buf, FDINFO_BYTES_MAX, |content| {
                accumulate_fdinfo(content, pci_slot, scanned)
            })
            .await;
            if let Ok(Some(client_id)) = read {
                self.hold(fdinfo, client_id);
            }
        }
        debug_assert!(self.held_ids.len() <= DRM_CLIENTS_HELD_MAX);
    }

    /// Holds a client's fdinfo for the ticks until the next walk, unless the client is held
    /// already or the bound is reached.
    fn hold(&mut self, fdinfo: File, client_id: u64) {
        if self.held_ids.contains(&client_id) {
            return;
        }
        if self.held_ids.len() < DRM_CLIENTS_HELD_MAX {
            self.fdinfos.push(fdinfo);
            self.held_ids.push(client_id);
        } else {
            self.overflowed = true;
        }
        debug_assert_eq!(self.fdinfos.len(), self.held_ids.len());
    }
}

/// Whether `/proc` is due another walk: on the first tick, then once the interval has passed.
fn walk_due(walked_at: Option<Instant>, now: Instant) -> bool {
    walked_at.is_none_or(|walked| now.saturating_duration_since(walked) >= CLIENT_WALK_INTERVAL)
}

/// Parses one fdinfo file; if it is an amdgpu DRM client for `pci_slot` that
/// exposes engine accounting, records its cumulative gfx+compute ns keyed by
/// `drm-client-id` (deduping a client opened via multiple fds).
///
/// Returns the client-id when the file belongs to a DRM client of `pci_slot`, which an idle
/// client with no engine lines yet still does.
fn accumulate_fdinfo(
    content: &str,
    pci_slot: &str,
    clients: &mut HashMap<u64, u64>,
) -> Option<u64> {
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
    if pdev_matches.not() {
        return None;
    }
    if engine_seen {
        if let Some(client_id) = client_id {
            clients.insert(client_id, busy_ns);
        }
    }
    client_id
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
        // Goal: a held fd whose number the process reused for another file stops being read,
        // and is not read again even if it turns back into a client before the next walk. An
        // idle client (no engine lines yet) stays held; a client of another GPU is never held.
        // Method: three clients, one rewritten as an ordinary fdinfo, then as a client again.
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
            assert_eq!(
                drm_clients.held_ids.len(),
                2,
                "the other GPU's client is not held"
            );

            std::fs::write(root.join("10/fdinfo/3"), "pos:\t0\nflags:\t02\n").unwrap();
            let t1 = t0 + Duration::from_secs(1);
            drm_clients
                .scan_in(root, PDEV, &mut buf, t1, &mut clients)
                .await;
            assert!(clients.is_empty());
            assert_eq!(drm_clients.held_ids, vec![2], "the idle client stays");

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
        // Goal: a held fd the process closed, or a process that exited, stops being read.
        // Method: hold a file that opens but cannot be read (a directory), as a closed fd's
        // fdinfo fails its read, beside a readable client, and read the held set.
        cc_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let readable = dir.path().join("readable");
            std::fs::write(&readable, drm_fdinfo(1, 100)).unwrap();
            let mut drm_clients = DrmClients {
                fdinfos: vec![
                    File::open(dir.path()).unwrap(),
                    File::open(&readable).unwrap(),
                ],
                held_ids: vec![7, 1],
                walked_at: Some(Instant::now()),
                overflowed: false,
            };
            let mut clients = HashMap::new();

            drm_clients
                .read_held(PDEV, &mut Vec::new(), &mut clients)
                .await;

            assert_eq!(clients, HashMap::from([(1, 100)]));
            assert_eq!(
                drm_clients.held_ids,
                vec![1],
                "the unreadable file is dropped"
            );
            assert_eq!(drm_clients.fdinfos.len(), 1);
        });
    }

    #[test]
    fn a_client_open_through_several_fds_is_held_once() {
        // Goal: a client is held by one fd however many it is open through, so the held set
        // grows with clients, not fds. Method: one client behind two fds of two processes.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            write_client(root, "10", "4", "/dev/dri/card0", &drm_fdinfo(1, 100));
            write_client(root, "20", "5", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            let mut drm_clients = DrmClients::default();
            let mut clients = HashMap::new();

            drm_clients
                .scan_in(root, PDEV, &mut Vec::new(), Instant::now(), &mut clients)
                .await;

            assert_eq!(clients, HashMap::from([(1, 100)]));
            assert_eq!(drm_clients.held_ids, vec![1]);
            assert_eq!(drm_clients.fdinfos.len(), 1);
        });
    }

    #[test]
    fn clients_past_the_bound_are_counted_and_walked_for() {
        // Goal: more clients than the bound cost no more held descriptors and no uncounted
        // load: the extra clients are counted on the walk, and every tick walks until there is
        // room again. Method: one more client than the bound, then a tick inside the interval,
        // then the extra clients gone.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            let client_count = DRM_CLIENTS_HELD_MAX + 1;
            for client in 0..client_count {
                let fd = client.to_string();
                let client_id = u64::try_from(client).unwrap();
                write_client(
                    root,
                    "10",
                    &fd,
                    "/dev/dri/renderD128",
                    &drm_fdinfo(client_id, 100),
                );
            }
            let mut drm_clients = DrmClients::default();
            let mut buf = Vec::new();
            let mut clients = HashMap::new();
            let t0 = Instant::now();

            drm_clients
                .scan_in(root, PDEV, &mut buf, t0, &mut clients)
                .await;
            assert_eq!(clients.len(), client_count, "every client is counted");
            assert_eq!(drm_clients.fdinfos.len(), DRM_CLIENTS_HELD_MAX);
            assert!(drm_clients.overflowed);

            // Inside the interval, a new client would wait for the next walk; past the bound it
            // must not, since the walk is what counts the clients that are not held.
            write_client(
                root,
                "20",
                "3",
                "/dev/dri/renderD128",
                &drm_fdinfo(999, 100),
            );
            let t1 = t0 + Duration::from_secs(1);
            drm_clients
                .scan_in(root, PDEV, &mut buf, t1, &mut clients)
                .await;
            assert_eq!(
                clients.len(),
                client_count + 1,
                "an overflowed cache walks every tick"
            );

            std::fs::remove_dir_all(root.join("10")).unwrap();
            let t2 = t0 + Duration::from_secs(2);
            drm_clients
                .scan_in(root, PDEV, &mut buf, t2, &mut clients)
                .await;
            assert_eq!(clients, HashMap::from([(999, 100)]));
            assert!(
                drm_clients.overflowed.not(),
                "room again, so ticks read the held set"
            );
        });
    }

    #[test]
    fn load_is_stamped_after_the_scan() {
        // Goal: the counters are stamped when the scan has read them, not when it started, so a
        // slow walk does not skew the load. The walk decision still uses the start. Method: a
        // clock that reads later on its second call.
        cc_fs::test_runtime(async {
            let proc_root = tempfile::tempdir().unwrap();
            let root = proc_root.path();
            write_client(root, "10", "3", "/dev/dri/renderD128", &drm_fdinfo(1, 100));
            let mut tracker = FdInfoTracker::default();
            let started = Instant::now();
            let scanned_at = started + Duration::from_millis(50);
            let mut reads = [started, scanned_at].into_iter();

            tracker
                .sample_in(root, PDEV, &mut Vec::new(), || reads.next().unwrap())
                .await;

            assert_eq!(tracker.clients.walked_at, Some(started));
            assert_eq!(tracker.load.last_sample, Some(scanned_at));
            assert!(reads.next().is_none(), "the clock is read twice");
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
        crate::rt::poll_to_end(tracker.sample_in(root, PDEV, &mut buf, || t0));
        let t1 = t0 + Duration::from_secs(1);
        crate::rt::poll_to_end(tracker.sample_in(root, PDEV, &mut buf, || t1));
        let t2 = t0 + Duration::from_secs(2);

        let ((percent, _), allocations) = crate::alloc_count::count_allocations(|| {
            crate::rt::poll_to_end(tracker.sample_in(root, PDEV, &mut buf, || t2))
        });

        assert_eq!(allocations, 0);
        assert!((0.0..=100.0).contains(&percent));
    }
}
