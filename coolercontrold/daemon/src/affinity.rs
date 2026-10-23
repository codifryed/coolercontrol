// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CPU affinity. Before the device workers start, the main thread is pinned to the CPU it runs on
//! and the workers go on the other hardware threads of its core. A tick's requests and replies then
//! stay in that core's caches, and a tick wakes one core rather than one per device. The rest keep
//! the process's CPUs: the sidecar, so an API flood cannot slow device reads, and the runtime's
//! blocking pool, whose jobs (LCD frames, one-shot reads) would otherwise crowd the main loop.
//!
//! Workers share a core only on a kernel that preempts code spinning in a driver (full or lazy
//! preemption). Elsewhere a driver that busy-waits, as amdgpu does for a firmware that stops
//! answering, would hold the shared CPU for seconds and stall every device's reads with it; the
//! workers then keep every CPU, so only that device stalls.
//!
//! Linux copies the creating thread's mask into each thread it starts, so a thread started from the
//! pinned main thread would share its one CPU. The pool takes the process's mask back
//! (`release_inherited_pin`) and the workers take theirs (`spawn_worker`).
use std::ops::Not;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread::{Builder, JoinHandle};

use anyhow::Result;
use log::debug;
use nix::sched::{sched_getaffinity, sched_getcpu, sched_setaffinity, CpuSet};
use nix::unistd::Pid;

use crate::cc_fs;

/// The main thread's mask before its pin, which threads started later take back.
static PROCESS_MASK: OnceLock<CpuSet> = OnceLock::new();

/// The CPUs device workers start on. Unset when the kernel may not preempt a spinning driver, or
/// the main thread's core has no other hardware thread the process may use; the workers then keep
/// the process's CPUs.
static WORKER_MASK: OnceLock<CpuSet> = OnceLock::new();

/// The active preemption model of a `PREEMPT_DYNAMIC` kernel, in parentheses among the ones it
/// offers: "none voluntary (full) lazy". Needs debugfs.
const PREEMPT_MODE_PATH: &str = "/sys/kernel/debug/sched/preempt";

/// The kernel's build flags, which name a fixed preemption model: "#1 SMP PREEMPT ...".
const KERNEL_VERSION_PATH: &str = "/proc/sys/kernel/version";

/// Pin the calling thread, the main one, to the CPU it is running on, and place the device workers
/// started from now on beside it. Call before the device workers start.
pub async fn pin_main_thread() -> Result<()> {
    let (main_cpu, process_mask) = pin_to_current_cpu()?;
    let preempts = kernel_preempts_spinning_drivers().await;
    let siblings = if preempts {
        read_siblings(main_cpu).await
    } else {
        debug!("Device workers keep every CPU: the kernel may not preempt a driver that spins");
        None
    };
    match choose_worker_mask(preempts, main_cpu, siblings.as_deref(), &process_mask) {
        Some(workers) => {
            let siblings = siblings.as_deref().unwrap_or_default().trim();
            debug!("Device workers run beside the main thread's CPU {main_cpu}, core {siblings}");
            WORKER_MASK.get_or_init(|| workers);
        }
        None => {
            if siblings.is_some() {
                debug!("Device workers keep every CPU: CPU {main_cpu} has no usable sibling");
            }
        }
    }
    Ok(())
}

/// Read on the blocking pool, whose new threads already take the process's mask back.
async fn read_siblings(main_cpu: usize) -> Option<String> {
    match cc_fs::read_txt(siblings_path(main_cpu)).await {
        Ok(siblings) => Some(siblings),
        Err(err) => {
            debug!("Device workers keep every CPU: no sibling list for CPU {main_cpu}: {err}");
            None
        }
    }
}

/// The CPUs device workers start on: the main thread's core siblings, only when the kernel
/// preempts spinning drivers. `None` keeps the workers on the process's CPUs.
fn choose_worker_mask(
    preempts: bool,
    main_cpu: usize,
    siblings: Option<&str>,
    process_mask: &CpuSet,
) -> Option<CpuSet> {
    if preempts.not() {
        return None;
    }
    worker_cpus(main_cpu, siblings?, process_mask)
}

/// Pin the calling thread to the CPU it is running on, recording the mask it had first, for the
/// threads it starts later. Returns the CPU and that mask.
fn pin_to_current_cpu() -> Result<(usize, CpuSet)> {
    let process_mask = sched_getaffinity(Pid::from_raw(0))?;
    PROCESS_MASK.get_or_init(|| process_mask);
    let main_cpu = sched_getcpu()?;
    let mut main_mask = CpuSet::new();
    main_mask.set(main_cpu)?;
    debug_assert!(process_mask.is_set(main_cpu).unwrap_or(false));
    sched_setaffinity(Pid::from_raw(0), &main_mask)?;
    Ok((main_cpu, process_mask))
}

/// Whether the kernel says it preempts code spinning in a driver.
async fn kernel_preempts_spinning_drivers() -> bool {
    let modes = cc_fs::read_txt(PREEMPT_MODE_PATH).await.ok();
    let version = match modes {
        Some(_) => None,
        None => cc_fs::read_txt(KERNEL_VERSION_PATH).await.ok(),
    };
    preempts_spinning_drivers(modes.as_deref(), version.as_deref())
}

/// A dynamic kernel's readable active mode decides; without it, only a fixed model the build
/// flags name counts. Unknown is no.
fn preempts_spinning_drivers(modes: Option<&str>, version: Option<&str>) -> bool {
    match (modes, version) {
        (Some(modes), _) => active_mode_preempts(modes),
        (None, Some(version)) => fixed_model_preempts(version),
        (None, None) => false,
    }
}

/// Full and lazy preemption both preempt kernel code within a tick; none and voluntary wait for
/// the code to give up the CPU, which a busy-wait never does.
fn active_mode_preempts(modes: &str) -> bool {
    let active = modes
        .split_whitespace()
        .find_map(|mode| mode.strip_prefix('(')?.strip_suffix(')'));
    matches!(active, Some("full" | "lazy"))
}

/// A kernel built with one fixed model names it in its flags: `PREEMPT` for full preemption and
/// `PREEMPT_RT` for real-time. A dynamic kernel names only `PREEMPT_DYNAMIC`, which says nothing
/// about the mode it runs.
fn fixed_model_preempts(version: &str) -> bool {
    version
        .split_whitespace()
        .any(|flag| matches!(flag, "PREEMPT" | "PREEMPT_RT"))
}

fn siblings_path(cpu: usize) -> PathBuf {
    PathBuf::from(format!(
        "/sys/devices/system/cpu/cpu{cpu}/topology/thread_siblings_list"
    ))
}

/// The other hardware threads of the main thread's core that the process may use, from the core's
/// sibling list ("9,25", "0-1"). `None` when there are none, or the list is malformed.
fn worker_cpus(main_cpu: usize, siblings: &str, process_mask: &CpuSet) -> Option<CpuSet> {
    let mut workers = CpuSet::new();
    let mut found = false;
    for cpu in parse_cpu_list(siblings)? {
        if cpu == main_cpu {
            continue;
        }
        if process_mask.is_set(cpu).unwrap_or(false) {
            workers.set(cpu).ok()?;
            found = true;
        }
    }
    debug_assert!(workers.is_set(main_cpu).unwrap_or(false).not());
    debug_assert!((0..CpuSet::count())
        .filter(|&cpu| workers.is_set(cpu).unwrap_or(false))
        .all(|cpu| process_mask.is_set(cpu).unwrap_or(false)));
    found.then_some(workers)
}

/// Parses a kernel CPU list ("9,25", "0-3,8"). `None` when malformed or past the largest CPU a
/// mask can hold.
fn parse_cpu_list(text: &str) -> Option<Vec<usize>> {
    let mut cpus = Vec::new();
    for part in text.trim().split(',') {
        let (first, last): (usize, usize) = if let Some((start, end)) = part.split_once('-') {
            (start.parse().ok()?, end.parse().ok()?)
        } else {
            let cpu = part.parse().ok()?;
            (cpu, cpu)
        };
        if first > last {
            return None;
        }
        if last >= CpuSet::count() {
            return None;
        }
        cpus.extend(first..=last);
        // A list naming more CPUs than a mask holds repeats itself: refuse it, which also bounds
        // what a long list can allocate.
        if cpus.len() > CpuSet::count() {
            return None;
        }
    }
    Some(cpus)
}

/// Give the calling thread the process's mask back, undoing the main thread's pin it inherited.
/// Does nothing before the pin.
pub fn release_inherited_pin() {
    if let Some(process_mask) = PROCESS_MASK.get() {
        set_mask(process_mask);
    }
}

/// `Builder::spawn` for a device worker: the thread starts on the worker CPUs, or on the process's
/// when there are none. Before the pin it keeps the creating thread's mask.
pub fn spawn_worker<F, T>(builder: Builder, body: F) -> std::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    spawn_on(
        builder,
        start_mask(WORKER_MASK.get(), PROCESS_MASK.get()),
        body,
    )
}

/// The worker mask when chosen, else the process's.
fn start_mask(worker: Option<&CpuSet>, process: Option<&CpuSet>) -> Option<CpuSet> {
    worker.or(process).copied()
}

fn spawn_on<F, T>(builder: Builder, mask: Option<CpuSet>, body: F) -> std::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    builder.spawn(move || {
        if let Some(mask) = mask {
            set_mask(&mask);
        }
        body()
    })
}

fn set_mask(mask: &CpuSet) {
    // Fails only when no CPU of the mask is online or allowed any more. The thread then keeps the
    // mask it inherited: slower, still correct.
    if let Err(err) = sched_setaffinity(Pid::from_raw(0), mask) {
        debug!("Could not set a new thread's CPUs: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mask_of(cpus: &[usize]) -> CpuSet {
        let mut mask = CpuSet::new();
        for &cpu in cpus {
            mask.set(cpu).expect("a CPU the mask can hold");
        }
        mask
    }

    /// Goal: the sibling lists the kernel writes parse, in both its forms, and anything else is
    /// refused rather than guessed at. Method: parse each form and a set of malformed lists.
    #[test]
    fn cpu_lists_parse_in_both_kernel_forms() {
        assert_eq!(parse_cpu_list("9,25\n"), Some(vec![9, 25]));
        assert_eq!(parse_cpu_list("0-1"), Some(vec![0, 1]));
        assert_eq!(parse_cpu_list("8-9,24-25"), Some(vec![8, 9, 24, 25]));
        assert_eq!(parse_cpu_list("7"), Some(vec![7]));
        for malformed in ["", "a", "3-1", "1,", "-1", "4096", "0-1023,0"] {
            assert_eq!(parse_cpu_list(malformed), None, "{malformed:?}");
        }
    }

    /// Goal: workers share a core only when a dynamic kernel's active mode preempts spinning
    /// kernel code, full or lazy; none and voluntary, and anything unreadable, keep them spread.
    /// Method: read the modes file in the shapes kernels write it, with each mode active.
    #[test]
    fn only_full_and_lazy_preemption_confine_the_workers() {
        assert!(active_mode_preempts("(full) lazy\n"));
        assert!(active_mode_preempts("none voluntary (full) lazy"));
        assert!(active_mode_preempts("none voluntary full (lazy)"));
        assert!(active_mode_preempts("none (voluntary) full lazy").not());
        assert!(active_mode_preempts("(none) voluntary full lazy").not());
        assert!(active_mode_preempts("none voluntary full lazy").not());
        assert!(active_mode_preempts("").not());
    }

    /// Goal: without the modes file, only a kernel built with a fixed preemptible model confines
    /// the workers; a dynamic one, whose mode the flags do not show, keeps them spread. Method:
    /// read version strings of each build kind.
    #[test]
    fn only_fixed_preemptible_builds_confine_without_the_modes_file() {
        assert!(fixed_model_preempts(
            "#1 SMP PREEMPT Tue Oct  1 10:00:00 UTC 2024"
        ));
        assert!(fixed_model_preempts(
            "#1 SMP PREEMPT_RT Wed Oct  2 10:00:00 UTC 2024"
        ));
        let dynamic = "#1 SMP PREEMPT_DYNAMIC Mon, 14 Sep 2026 19:55:01 +0000";
        assert!(fixed_model_preempts(dynamic).not());
        assert!(fixed_model_preempts("#38-Ubuntu SMP Thu Oct  3 10:00:00 UTC 2024").not());
    }

    const PREEMPT_BUILD: &str = "#1 SMP PREEMPT Tue Oct  1 10:00:00 UTC 2024";
    const RT_BUILD: &str = "#1 SMP PREEMPT_RT Wed Oct  2 10:00:00 UTC 2024";
    const DYNAMIC_BUILD: &str = "#1 SMP PREEMPT_DYNAMIC Mon, 14 Sep 2026 19:55:01 +0000";

    /// Goal: a readable modes file decides alone, and the build flags stand in only when it
    /// cannot be read; with neither, the workers spread. Method: decide over a table of modes
    /// file and version string pairs, readable and not.
    #[test]
    fn a_readable_modes_file_decides_before_the_build_flags() {
        let cases = [
            (Some("none voluntary (full) lazy"), None, true),
            (
                Some("none voluntary full (lazy)"),
                Some(DYNAMIC_BUILD),
                true,
            ),
            (
                Some("(none) voluntary full lazy"),
                Some(PREEMPT_BUILD),
                false,
            ),
            (Some("none (voluntary) full lazy"), Some(RT_BUILD), false),
            (None, Some(PREEMPT_BUILD), true),
            (None, Some(RT_BUILD), true),
            (None, Some(DYNAMIC_BUILD), false),
            (None, None, false),
        ];
        for (modes, version, confine) in cases {
            let decided = preempts_spinning_drivers(modes, version);
            assert_eq!(decided, confine, "{modes:?} {version:?}");
        }
    }

    /// Goal: the worker mask is set only when the kernel preempts spinning drivers and the
    /// main thread's core lists a usable sibling. Method: choose across the gate, an unreadable
    /// sibling list, and a readable one.
    #[test]
    fn the_worker_mask_needs_preemption_and_a_sibling_list() {
        let all = mask_of(&(0..32).collect::<Vec<_>>());
        assert_eq!(choose_worker_mask(false, 25, Some("9,25"), &all), None);
        assert_eq!(choose_worker_mask(true, 25, None, &all), None);
        let confined = choose_worker_mask(true, 25, Some("9,25"), &all);
        assert_eq!(confined, Some(mask_of(&[9])));
    }

    /// Goal: a worker starts on the worker mask when one is chosen, else on the process's, and
    /// keeps its inherited mask before the pin. Method: pick the start mask from each pairing.
    #[test]
    fn the_worker_mask_wins_over_the_process_mask() {
        let worker = mask_of(&[9]);
        let process = mask_of(&[0, 1, 9, 25]);
        assert_eq!(start_mask(Some(&worker), Some(&process)), Some(worker));
        assert_eq!(start_mask(None, Some(&process)), Some(process));
        assert_eq!(start_mask(None, None), None);
    }

    /// Goal: workers go on the other hardware threads of the main thread's core, never on the main
    /// thread's own CPU and never outside the process's CPUs; with nothing left they keep every
    /// CPU. Method: ask for the worker CPUs across SMT, no SMT, and a restricted process mask.
    #[test]
    fn workers_go_beside_the_main_thread_within_the_process_mask() {
        let all = mask_of(&(0..32).collect::<Vec<_>>());
        assert_eq!(worker_cpus(25, "9,25", &all), Some(mask_of(&[9])));
        assert_eq!(worker_cpus(0, "0-1", &all), Some(mask_of(&[1])));
        assert_eq!(worker_cpus(4, "4-7", &all), Some(mask_of(&[5, 6, 7])));
        assert_eq!(worker_cpus(25, "25", &all), None);
        assert_eq!(worker_cpus(25, "9,25", &mask_of(&[24, 25])), None);
        assert_eq!(worker_cpus(25, "garbled", &all), None);
    }

    // Gated: the masks depend on the host's CPUs and on the affinity the tests start with.
    #[cfg(feature = "gated-tests")]
    mod host {
        use super::super::*;
        use crate::rt;

        fn current_mask() -> CpuSet {
            sched_getaffinity(Pid::from_raw(0)).expect("read this thread's mask")
        }

        fn cpu_count(mask: &CpuSet) -> usize {
            (0..CpuSet::count())
                .filter(|&cpu| mask.is_set(cpu).unwrap_or(false))
                .count()
        }

        /// Pins the test thread as `pin_main_thread` pins main, without placing the workers, and
        /// puts its mask back on drop, so a failed assertion cannot strand a runner thread that
        /// later tests reuse. The worker mask stays unset: set, it would move every other test's
        /// device workers onto one CPU.
        struct PinnedLikeMain {
            before: CpuSet,
            cpu: usize,
        }

        impl PinnedLikeMain {
            fn new() -> Self {
                let before = current_mask();
                assert!(cpu_count(&before) > 1, "a pin cannot be seen on one CPU");
                let (cpu, _) = pin_to_current_cpu().expect("pin the test thread");
                assert_eq!(cpu_count(&current_mask()), 1);
                Self { before, cpu }
            }
        }

        impl Drop for PinnedLikeMain {
            fn drop(&mut self) {
                let _ = sched_setaffinity(Pid::from_raw(0), &self.before);
            }
        }

        /// Goal: with no worker CPUs chosen, a worker started after the main thread's pin runs on
        /// the process's CPUs, not on the main loop's one. Method: pin this thread like main,
        /// start a worker, and compare its mask with the one this thread had before the pin.
        #[test]
        fn a_worker_without_worker_cpus_takes_the_process_mask() {
            let pinned = PinnedLikeMain::new();

            let started = spawn_worker(Builder::new(), current_mask)
                .expect("start the thread")
                .join()
                .expect("the thread returns");

            assert_eq!(started, pinned.before);
        }

        /// Goal: a worker starts on the CPUs chosen for it, whatever the creating thread's mask.
        /// Method: pin this thread like main, start a thread on a mask of every allowed CPU but
        /// this one, and read the new thread's mask.
        #[test]
        fn a_worker_starts_on_the_cpus_chosen_for_it() {
            let pinned = PinnedLikeMain::new();
            let mut chosen = pinned.before;
            chosen.unset(pinned.cpu).expect("a CPU the mask holds");

            let started = spawn_on(Builder::new(), Some(chosen), current_mask)
                .expect("start the thread")
                .join()
                .expect("the thread returns");

            assert_eq!(started, chosen);
        }

        /// Goal: a blocking-pool thread started after the pin runs on the process's CPUs, so the
        /// jobs it carries (LCD frames, one-shot reads) stay off the main loop's CPU once the
        /// pool's first threads retire. Method: pin this thread like main, build a runtime from
        /// it, whose pool starts its first thread from here, and read a blocking job's mask.
        #[test]
        fn a_blocking_thread_started_after_the_pin_takes_the_process_mask() {
            let pinned = PinnedLikeMain::new();

            let started = rt::test_runtime(async { rt::spawn_blocking(current_mask).await })
                .expect("the blocking job returns");

            assert_eq!(started, pinned.before);
        }
    }
}
