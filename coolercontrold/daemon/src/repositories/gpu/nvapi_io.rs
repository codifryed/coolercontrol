// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The nvapi library, isolated on one thread of its own.
//!
//! It is a blocking vendor FFI into nvidia.ko with no timeout. Loading it, initializing it and
//! every hotspot read run on this thread, so the main thread never touches the library. One
//! thread serves every GPU: nvapi's thread safety is undocumented, so its calls stay serialized.
use std::cell::RefCell;
use std::collections::HashSet;
use std::ops::Not;
use std::thread;
use std::time::Duration;

use log::{debug, info, warn};
use tokio::sync::{mpsc, oneshot};

use crate::affinity;
use crate::repositories::gpu::nvapi::NvApi;
use crate::repositories::hwmon::device_io::{
    DeviceHealth, HealthState, QUEUE_DEPTH, UNREACHABLE_PROBE_INTERVAL,
};
use crate::rt;

const THREAD_NAME: &str = "nvapi";
/// Bound on loading and initializing nvapi, which enumerates every GPU and probes each one's
/// hotspot. Startup waits no longer than this; a library that has not answered by then is off.
/// The first hotspot reads of all GPUs share one more such budget.
pub const INIT_TIMEOUT: Duration = Duration::from_secs(10);
const HEALTH_NAME: &str = "nvapi";
// PCI bus numbers are 8-bit, so the latch holds at most one entry per bus.
const PCI_BUSES_MAX: usize = 256;

// Linux caps thread names at 15 bytes.
const _: () = assert!(THREAD_NAME.len() <= 15);

/// What the thread reads hotspots from: nvapi, or a fake in tests.
pub trait HotspotReader {
    fn hotspot_temp(&self, pci_bus: u32) -> Option<f64>;
}

impl HotspotReader for NvApi {
    fn hotspot_temp(&self, pci_bus: u32) -> Option<f64> {
        self.get_hotspot_temp(pci_bus)
    }
}

/// One hotspot read of the GPU on `pci_bus`.
struct Request {
    pci_bus: u32,
    reply: oneshot::Sender<Option<f64>>,
}

/// The main-thread half of the nvapi thread.
pub struct NvApiIo {
    tx: mpsc::Sender<Request>,
    reply_timeout: Duration,
    state: HealthState,
    /// Buses whose last answered hotspot was impossible, so it is logged once on the way in.
    non_finite_buses: RefCell<HashSet<u32>>,
}

impl NvApiIo {
    /// Starts the nvapi thread, which loads and initializes the library before serving reads.
    ///
    /// `register_hotspot_buses` holds the PCI buses of GPUs that keep hotspot out of the thermals
    /// array. None when nvapi is unavailable, failed to initialize, has no GPU with a hotspot, or
    /// did not finish within `INIT_TIMEOUT`.
    pub async fn start(
        register_hotspot_buses: HashSet<u32>,
        reply_timeout: Duration,
    ) -> Option<Self> {
        let init = move || NvApi::try_init(&register_hotspot_buses);
        Self::start_with(init, INIT_TIMEOUT, reply_timeout).await
    }

    /// nvapi served by `reader` instead of the library, for tests.
    #[cfg(test)]
    pub async fn start_for_test<R>(reader: R, reply_timeout: Duration) -> Option<Self>
    where
        R: HotspotReader + Send + 'static,
    {
        Self::start_with(move || Some(reader), INIT_TIMEOUT, reply_timeout).await
    }

    /// `init` runs on the new thread, and what it returns never leaves that thread.
    async fn start_with<R, I>(
        init: I,
        init_timeout: Duration,
        reply_timeout: Duration,
    ) -> Option<Self>
    where
        R: HotspotReader + 'static,
        I: FnOnce() -> Option<R> + Send + 'static,
    {
        assert!(init_timeout > Duration::ZERO);
        assert!(reply_timeout > Duration::ZERO);
        let (tx, rx) = mpsc::channel::<Request>(QUEUE_DEPTH);
        let (ready_tx, ready_rx) = oneshot::channel::<bool>();
        // Detached deliberately: a wedged thread never returns, so a handle would only invite a
        // `join` that cannot be bounded.
        let builder = thread::Builder::new().name(THREAD_NAME.to_owned());
        let spawned = affinity::spawn_worker(builder, move || run_worker(init, ready_tx, rx));
        if let Err(err) = spawned {
            warn!(
                "Could not start the nvapi thread: {err}. GPU hotspot temperatures are not \
                 shown."
            );
            return None;
        }
        match rt::timeout(init_timeout, ready_rx).await {
            Ok(Ok(true)) => {
                let state = HealthState::new(HEALTH_NAME.to_owned());
                state.leave_announcements_to_owner();
                Some(Self {
                    tx,
                    reply_timeout,
                    state,
                    non_finite_buses: RefCell::new(HashSet::new()),
                })
            }
            // Unavailable or failed, which the library load already logged, or the thread died.
            Ok(Ok(false) | Err(_)) => None,
            Err(_elapsed) => {
                warn!(
                    "nvapi did not finish initializing within {init_timeout:?}. GPU hotspot \
                     temperatures are not shown."
                );
                None
            }
        }
    }

    /// The hotspot of the GPU on `pci_bus`, read on the nvapi thread. None when nvapi has no
    /// hotspot for it, answered an impossible one, did not answer in time, or is not answering.
    pub async fn hotspot_temp(&self, pci_bus: u32) -> Option<f64> {
        debug_assert!(self.reply_timeout > Duration::ZERO);
        // A thread that is not answering gets no new request until its next probe is due, or
        // every tick would queue a read nobody serves and wait a full timeout to learn that.
        if self.state.dispatchable().not() {
            return None;
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let exchange = async {
            let request = Request {
                pci_bus,
                reply: reply_tx,
            };
            self.tx.send(request).await.ok()?;
            reply_rx.await.ok()
        };
        match rt::timeout(self.reply_timeout, exchange).await {
            Ok(Some(temp)) => {
                self.record_answered();
                temp.filter(|temp| self.record_finite(pci_bus, *temp))
            }
            // The thread is gone. Nothing will answer again.
            Ok(None) => None,
            Err(_elapsed) => {
                self.record_timeout();
                None
            }
        }
    }

    fn record_answered(&self) {
        let was_unreachable = matches!(self.state.health(), DeviceHealth::Unreachable { .. });
        self.state.record_answered();
        debug_assert_eq!(self.state.health(), DeviceHealth::Healthy);
        if was_unreachable {
            info!("nvapi is answering again. GPU hotspot temperatures are back.");
        }
    }

    /// Whether `temp` is finite, latching an impossible one per bus so it is logged only once.
    fn record_finite(&self, pci_bus: u32, temp: f64) -> bool {
        let mut non_finite_buses = self.non_finite_buses.borrow_mut();
        if temp.is_finite() {
            if non_finite_buses.remove(&pci_bus) {
                info!("nvapi reports a valid hotspot temperature again for PCI bus {pci_bus}.");
            }
            return true;
        }
        if non_finite_buses.insert(pci_bus) {
            debug_assert!(non_finite_buses.len() <= PCI_BUSES_MAX);
            info!(
                "nvapi reported an impossible hotspot temperature of {temp} for PCI bus \
                 {pci_bus}. It is omitted."
            );
        }
        false
    }

    fn record_timeout(&self) {
        let was_unreachable = matches!(self.state.health(), DeviceHealth::Unreachable { .. });
        self.state.record_timeout(&"hotspot read");
        debug_assert_ne!(self.state.health(), DeviceHealth::Healthy);
        let is_unreachable = matches!(self.state.health(), DeviceHealth::Unreachable { .. });
        // Once, on the way in. An nvapi that stays wedged must not log every probe.
        if is_unreachable {
            if was_unreachable.not() {
                warn!(
                    "nvapi stopped answering. GPU hotspot temperatures are omitted; retrying \
                     every {} seconds.",
                    UNREACHABLE_PROBE_INTERVAL.as_secs()
                );
            }
        }
    }

    #[cfg(test)]
    fn health(&self) -> DeviceHealth {
        self.state.health()
    }

    #[cfg(test)]
    fn non_finite_latched(&self, pci_bus: u32) -> bool {
        self.non_finite_buses.borrow().contains(&pci_bus)
    }
}

/// The thread body: initialize, report whether that worked, then serve one read at a time until
/// the queue closes.
fn run_worker<R, I>(init: I, ready: oneshot::Sender<bool>, mut rx: mpsc::Receiver<Request>)
where
    R: HotspotReader,
    I: FnOnce() -> Option<R>,
{
    let Some(reader) = init() else {
        let _ = ready.send(false);
        return;
    };
    if ready.send(true).is_err() {
        // Startup stopped waiting, so nothing will ever read through this. Dropping the reader
        // unloads the library.
        return;
    }
    while let Some(request) = rx.blocking_recv() {
        // A caller that timed out no longer wants the reading; skip the call.
        if request.reply.is_closed() {
            continue;
        }
        let _ = request.reply.send(reader.hotspot_temp(request.pci_bus));
    }
    debug!("nvapi worker stopped");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "gated-tests")]
    use crate::repositories::hwmon::device_io::UNREACHABLE_AFTER_TIMEOUTS;
    use std::sync::mpsc as std_mpsc;
    use std::sync::{Arc, Mutex};

    const REPLY_TIMEOUT: Duration = Duration::from_millis(50);

    /// Answers a fixed hotspot for bus 1, an impossible one for bus 3, and records the thread
    /// each read runs on.
    struct FakeNvApi {
        threads: Arc<Mutex<Vec<String>>>,
    }

    impl HotspotReader for FakeNvApi {
        fn hotspot_temp(&self, pci_bus: u32) -> Option<f64> {
            let name = thread::current().name().unwrap_or_default().to_owned();
            self.threads.lock().unwrap().push(name);
            match pci_bus {
                1 => Some(61.),
                3 => Some(f64::NAN),
                _ => None,
            }
        }
    }

    /// Answers whatever hotspot the test last set, for every bus.
    struct SettableNvApi {
        temp: Arc<Mutex<f64>>,
    }

    impl HotspotReader for SettableNvApi {
        fn hotspot_temp(&self, _pci_bus: u32) -> Option<f64> {
            Some(*self.temp.lock().unwrap())
        }
    }

    /// Blocks every read until the test lets it go, like a driver call that does not return.
    #[cfg(feature = "gated-tests")]
    struct StalledNvApi {
        release: std_mpsc::Receiver<()>,
    }

    #[cfg(feature = "gated-tests")]
    impl HotspotReader for StalledNvApi {
        fn hotspot_temp(&self, _pci_bus: u32) -> Option<f64> {
            let _ = self.release.recv();
            Some(61.)
        }
    }

    #[test]
    fn a_failed_init_leaves_nvapi_off() {
        // Goal: nvapi that cannot load or initialize is simply off, with no handle to read
        // through. Method: an init that reports failure.
        crate::rt::test_runtime(async {
            let io = NvApiIo::start_with(|| None::<FakeNvApi>, INIT_TIMEOUT, REPLY_TIMEOUT).await;

            assert!(io.is_none());
        });
    }

    #[test]
    fn hotspots_are_read_on_the_nvapi_thread() {
        // Goal: init and every read run on the nvapi thread, and a GPU nvapi has no hotspot for,
        // or an impossible one, reads as none. Method: a fake library that records the thread of
        // its init and of each read.
        crate::rt::test_runtime(async {
            let threads = Arc::new(Mutex::new(Vec::new()));
            let reader_threads = Arc::clone(&threads);
            let init = move || {
                let name = thread::current().name().unwrap_or_default().to_owned();
                reader_threads.lock().unwrap().push(name);
                Some(FakeNvApi {
                    threads: reader_threads,
                })
            };
            let io = NvApiIo::start_with(init, INIT_TIMEOUT, Duration::from_secs(5))
                .await
                .expect("the fake initializes");

            assert_eq!(io.hotspot_temp(1).await, Some(61.));
            assert_eq!(io.hotspot_temp(2).await, None);
            assert_eq!(io.hotspot_temp(3).await, None);
            assert_eq!(*threads.lock().unwrap(), [THREAD_NAME; 4]);
            assert_eq!(io.health(), DeviceHealth::Healthy);
        });
    }

    #[test]
    fn an_impossible_hotspot_latches_until_a_valid_one() {
        // Goal: an impossible hotspot is omitted and latched per bus, so it is logged once, and a
        // valid reading re-arms the latch. Method: a fake whose answer the test switches.
        crate::rt::test_runtime(async {
            let temp = Arc::new(Mutex::new(f64::NAN));
            let reader_temp = Arc::clone(&temp);
            let init = move || Some(SettableNvApi { temp: reader_temp });
            let io = NvApiIo::start_with(init, INIT_TIMEOUT, Duration::from_secs(5))
                .await
                .expect("the fake initializes");

            assert_eq!(io.hotspot_temp(1).await, None);
            assert!(io.non_finite_latched(1));
            assert!(io.non_finite_latched(2).not());
            assert_eq!(io.hotspot_temp(1).await, None);
            assert!(io.non_finite_latched(1));

            *temp.lock().unwrap() = 61.;
            assert_eq!(io.hotspot_temp(1).await, Some(61.));
            assert!(io.non_finite_latched(1).not());
        });
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_stalled_read_times_out_and_omits_the_hotspot() {
        // Goal: a read the driver never returns from costs one reply budget and no hotspot, and
        // enough of them stop dispatch until the probe. Method: a fake library that blocks every
        // read until released. Gated: it bounds a real-time wait from above.
        crate::rt::test_runtime(async {
            let (release_tx, release_rx) = std_mpsc::channel();
            let init = move || {
                Some(StalledNvApi {
                    release: release_rx,
                })
            };
            let io = NvApiIo::start_with(init, INIT_TIMEOUT, REPLY_TIMEOUT)
                .await
                .expect("the fake initializes");

            let started = std::time::Instant::now();
            assert_eq!(io.hotspot_temp(1).await, None);
            assert!(started.elapsed() >= REPLY_TIMEOUT);
            for _ in 1..UNREACHABLE_AFTER_TIMEOUTS {
                assert_eq!(io.hotspot_temp(1).await, None);
            }
            assert!(matches!(io.health(), DeviceHealth::Unreachable { .. }));
            let skipped = std::time::Instant::now();
            assert_eq!(io.hotspot_temp(1).await, None);
            assert!(
                skipped.elapsed() < REPLY_TIMEOUT,
                "an unreachable nvapi is not asked"
            );

            drop(release_tx);
        });
    }

    #[test]
    fn an_init_past_its_budget_leaves_nvapi_off() {
        // Goal: a library that hangs while initializing cannot hold startup past its budget.
        // Method: an init that blocks until released, with a short budget.
        crate::rt::test_runtime(async {
            let (release_tx, release_rx) = std_mpsc::channel::<()>();
            let init = move || {
                let _ = release_rx.recv();
                Some(FakeNvApi {
                    threads: Arc::new(Mutex::new(Vec::new())),
                })
            };

            let io = NvApiIo::start_with(init, REPLY_TIMEOUT, REPLY_TIMEOUT).await;

            assert!(io.is_none());
            drop(release_tx);
        });
    }
}
