// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! CPU affinity. Once initialization completes, the main thread is pinned to the CPU it runs on.
//! Linux copies the creating thread's mask into each thread it starts, so a thread started from
//! the main thread after that would share its one CPU. Threads that can start late take the
//! process's mask back first: the runtime's blocking pool, which retires idle threads and starts
//! new ones on demand, and the device workers, which go through `spawn`.
use std::sync::OnceLock;
use std::thread::{Builder, JoinHandle};

use anyhow::Result;
use log::debug;
use nix::sched::{sched_getaffinity, sched_getcpu, sched_setaffinity, CpuSet};
use nix::unistd::Pid;

/// The main thread's mask before its pin, which threads started later take back.
static PROCESS_MASK: OnceLock<CpuSet> = OnceLock::new();

/// Pin the calling thread, the main one, to the CPU it is running on, reducing unnecessary
/// context switching. Records the mask it had first, for `release_inherited_pin`.
pub fn pin_main_thread() -> Result<()> {
    let process_mask = sched_getaffinity(Pid::from_raw(0))?;
    PROCESS_MASK.get_or_init(|| process_mask);
    let mut cpu_set = CpuSet::new();
    cpu_set.set(sched_getcpu()?)?;
    sched_setaffinity(Pid::from_raw(0), &cpu_set)?;
    Ok(())
}

/// Give the calling thread the process's mask back, undoing the main thread's pin it inherited.
/// Does nothing before the pin.
pub fn release_inherited_pin() {
    let Some(process_mask) = PROCESS_MASK.get() else {
        return;
    };
    // Fails only when no CPU of the mask is online or allowed any more. The thread then keeps the
    // main thread's CPU: slower, still correct.
    if let Err(err) = sched_setaffinity(Pid::from_raw(0), process_mask) {
        debug!("Could not release the CPU pin a new thread inherited: {err}");
    }
}

/// `Builder::spawn`, releasing the new thread from the main thread's pin before `body` runs.
pub fn spawn<F, T>(builder: Builder, body: F) -> std::io::Result<JoinHandle<T>>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    builder.spawn(move || {
        release_inherited_pin();
        body()
    })
}

// Gated: the masks depend on the host's CPUs and on the affinity the tests start with.
#[cfg(all(test, feature = "gated-tests"))]
mod tests {
    use super::*;
    use crate::rt;

    fn current_mask() -> CpuSet {
        sched_getaffinity(Pid::from_raw(0)).expect("read this thread's mask")
    }

    fn cpu_count(mask: &CpuSet) -> usize {
        (0..CpuSet::count())
            .filter(|&cpu| mask.is_set(cpu).unwrap_or(false))
            .count()
    }

    /// Pins the test thread as `pin_main_thread` pins main, and puts its mask back on drop, so a
    /// failed assertion cannot strand a runner thread that later tests reuse.
    struct PinnedLikeMain {
        before: CpuSet,
    }

    impl PinnedLikeMain {
        fn new() -> Self {
            let before = current_mask();
            assert!(cpu_count(&before) > 1, "a pin cannot be seen on one CPU");
            pin_main_thread().expect("pin the test thread");
            assert_eq!(cpu_count(&current_mask()), 1);
            Self { before }
        }
    }

    impl Drop for PinnedLikeMain {
        fn drop(&mut self) {
            let _ = sched_setaffinity(Pid::from_raw(0), &self.before);
        }
    }

    /// Goal: a worker started after the main thread's pin runs on the process's CPUs, not on the
    /// main loop's one. Method: pin this thread like main, start a thread through `spawn`, and
    /// compare its mask with the one this thread had before the pin.
    #[test]
    fn a_thread_started_after_the_pin_takes_the_process_mask() {
        let pinned = PinnedLikeMain::new();

        let started = spawn(Builder::new(), current_mask)
            .expect("start the thread")
            .join()
            .expect("the thread returns");

        assert_eq!(started, pinned.before);
    }

    /// Goal: a blocking-pool thread started after the pin runs on the process's CPUs, so the jobs
    /// it carries (LCD frames, one-shot reads) stay off the main loop's CPU once the pool's first
    /// threads retire. Method: pin this thread like main, build a runtime from it, whose pool
    /// starts its first thread from here, and read a blocking job's mask.
    #[test]
    fn a_blocking_thread_started_after_the_pin_takes_the_process_mask() {
        let pinned = PinnedLikeMain::new();

        let started = rt::test_runtime(async { rt::spawn_blocking(current_mask).await })
            .expect("the blocking job returns");

        assert_eq!(started, pinned.before);
    }
}
