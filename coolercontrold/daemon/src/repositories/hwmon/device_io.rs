// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-device sysfs IO, isolated on a thread of its own.
//!
//! kernfs advertises a `.poll`, so `io_uring` issues sysfs reads inline on the submitting thread
//! and kernfs then blocks anyway. A driver that does a USB round trip inside its read therefore
//! parks the whole single-threaded runtime, and with it the snapshot cap, the permit timeouts and
//! the failsafe's staleness counting: every protection the daemon has is a timer on the runtime
//! the stall is holding.
//!
//! One thread, one ring and one `SysfsFdCache` per device makes that machinery able to run. It
//! adds no safety of its own.
use std::cell::{Cell, RefCell};
use std::fmt::Display;
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use anyhow::Result;
use log::{debug, error, warn};
use tokio::sync::{mpsc, oneshot};

use crate::cc_fs::{self, ReadIndex, SysfsValue};
use crate::repositories::failsafe::MISSING_STATUS_THRESHOLD;
use crate::rt;

/// Consecutive reply timeouts before a device is declared unreachable and taken out of the
/// per-tick rotation.
///
/// Deliberately the same count as `MISSING_STATUS_THRESHOLD`, which the failsafe already uses to
/// decide a channel's readings are stale. One staleness concept is easier to reason about than two
/// that can drift apart.
pub const UNREACHABLE_AFTER_TIMEOUTS: u8 = 8;
const _: () = assert!(UNREACHABLE_AFTER_TIMEOUTS > 0);

/// How long an unreachable device is left alone before one probe is allowed through.
///
/// Cheap enough to ignore next to a 1 s poll rate, frequent enough that a device which comes back
/// is noticed within a minute rather than needing a daemon restart.
pub const UNREACHABLE_PROBE_INTERVAL: Duration = Duration::from_secs(30);

/// Requests a worker will accept before the queue applies backpressure.
///
/// This bounds memory, not correctness: a send waits for room inside the caller's timeout, so any
/// depth of at least one behaves the same. Sized past the widest per-device concurrent read fan-out
/// (`temps::extract_temp_statuses` spawns one task per temp channel) so a healthy device with many
/// channels never waits here.
pub const QUEUE_DEPTH: usize = 128;
const _: () = assert!(QUEUE_DEPTH > 0);

/// Upper bound on one batched read.
///
/// A batch is one device's attributes of one kind, so it is bounded by that device's channel
/// count. The cap is the descriptor cache's, since a batch can hold at most one descriptor per
/// entry; past it the caller splits rather than growing a request without limit.
pub const READ_BATCH_MAX: usize = 512;
const _: () = assert!(READ_BATCH_MAX > 0);

/// Stack for a worker thread. It runs one `async fn` with a fixed buffer and no recursion, so the
/// default 8 MiB is wasted address space once there is one per device.
const WORKER_STACK_BYTES: usize = 256 * 1024;

/// How long one operation may take before it counts against the device's health.
///
/// Deliberately generous, and matched to the hwmon read permit budget: `asus_rog_ryujin` waits up
/// to 1.5 s per round trip and does four on its first read, and timing that out would mark a
/// working device unreachable. Being generous costs nothing now that the wait no longer parks the
/// runtime.
#[allow(clippy::cast_precision_loss)]
#[must_use]
pub fn reply_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * MISSING_STATUS_THRESHOLD as f64)
}

/// What the caller can observe about a device's IO.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceHealth {
    /// Answering normally, or not isolated at all (`DeviceIo::Inline`).
    Healthy,
    /// At least one reply timed out, but not yet enough to give up on it.
    Degraded { consecutive_timeouts: u8 },
    /// Not answering. Out of the per-tick rotation until the next probe is due.
    Unreachable { consecutive_timeouts: u8 },
}

/// One device's sysfs IO.
///
/// `Inline` is the historical behaviour and what tests use: reads run on the caller's runtime
/// against a shared descriptor cache. `Threaded` moves them to a thread of the device's own.
/// A timeout only exists on the `Threaded` path, because a timer cannot fire while an inline read
/// holds the runtime it lives on.
#[derive(Clone, Debug)]
pub enum DeviceIo {
    /// Reads run on the caller's runtime against a descriptor cache of their own, exactly as
    /// before this type existed.
    Inline(cc_fs::SysfsFdCache),
    /// `Rc` because a device's `HwmonDriverInfo` is cloned around the main thread; every clone
    /// must talk to the same worker.
    Threaded(Rc<Worker>),
}

impl Default for DeviceIo {
    /// Not isolated. Every device starts here and the hwmon repo promotes the ones it can, so a
    /// failure to start a worker degrades to the historical behaviour instead of losing a device.
    fn default() -> Self {
        Self::Inline(cc_fs::SysfsFdCache::default())
    }
}

impl DeviceIo {
    /// Start a worker thread for one device, so a driver that blocks inside its sysfs read parks
    /// only its own thread.
    ///
    /// # Errors
    ///
    /// When the OS refuses the thread. The caller should fall back to `Inline`, losing the
    /// isolation rather than the device.
    pub fn threaded(device_name: &str, reply_timeout: Duration) -> Result<Self> {
        debug_assert!(reply_timeout > Duration::ZERO);
        let (tx, rx) = mpsc::channel::<Request>(QUEUE_DEPTH);
        let thread_name = worker_thread_name(device_name);
        debug!("device IO worker starting: {thread_name}");
        // Detached deliberately: a wedged worker never returns, so there is no handle worth
        // keeping. Holding one would only invite a `join` that cannot be bounded.
        std::thread::Builder::new()
            .name(thread_name)
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || run_worker(rx))?;
        Ok(Self::Threaded(Rc::new(Worker {
            state: HealthState::new(device_name.to_owned()),
            tx,
            reply_timeout,
            pending_registry: RefCell::new(None),
        })))
    }

    /// Starts an isolated worker, falling back to inline IO if the OS refuses the thread.
    ///
    /// The fallback loses the isolation, not the device, which is the right way round: a machine
    /// short on threads should still report its temperatures.
    #[must_use]
    pub fn isolated_or_inline(device_name: &str, reply_timeout: Duration) -> Self {
        match Self::threaded(device_name, reply_timeout) {
            Ok(io) => io,
            Err(err) => {
                warn!(
                    "Could not start an IO thread for device {device_name}: {err}. Falling back \
                     to shared IO, where a stall in this device's driver delays others."
                );
                Self::default()
            }
        }
    }

    /// Reads one numeric attribute.
    /// Reads one attribute by path, without caching its descriptor.
    ///
    /// For detection probes, labels and read-before-write checks: none repeat per tick, so a held
    /// descriptor buys nothing. The per-tick set goes through `read_many`.
    pub async fn read_value(&self, path: &Path) -> Result<SysfsValue> {
        match self {
            Self::Inline(_) => cc_fs::read_sysfs_value(path).await,
            Self::Threaded(worker) => {
                worker
                    .dispatch(&path.display(), |reply| Request::Read {
                        path: path.to_path_buf(),
                        reply,
                    })
                    .await
            }
        }
    }

    /// Hands the device its read table, covering every attribute the per-tick pass reads.
    ///
    /// Called once, after detection settles the channel set.
    ///
    /// # Errors
    ///
    /// When the worker is gone or does not answer.
    pub async fn install_registry(&self, paths: Vec<PathBuf>) -> Result<()> {
        match self {
            Self::Inline(fds) => {
                fds.install(paths);
                Ok(())
            }
            Self::Threaded(worker) => {
                *worker.pending_registry.borrow_mut() = Some(paths);
                worker.ensure_registry().await;
                Ok(())
            }
        }
    }

    /// The path a slot was registered with, for tests that assert the mapping. `Inline` only: the
    /// threaded table lives on the worker and reading it would cost a round trip per read.
    #[cfg(test)]
    #[must_use]
    pub fn registered_path(&self, index: ReadIndex) -> Option<PathBuf> {
        match self {
            Self::Inline(fds) => fds.path_of(index),
            Self::Threaded(_) => None,
        }
    }

    /// Debug-only check that a slot addresses the attribute the caller meant.
    ///
    /// Slots trade a loud failure for a quiet one: a wrong path fails `ENOENT`, but a wrong slot
    /// reads a different sensor and reports it as this channel's. Only `Inline` can answer without
    /// a round trip, which is the configuration tests run in, and tests are where a mismatch would
    /// be introduced.
    pub fn debug_assert_slot(&self, index: ReadIndex, expected: &Path) {
        let Self::Inline(fds) = self else {
            return;
        };
        if let Some(registered) = fds.path_of(index) {
            debug_assert_eq!(
                registered, expected,
                "read slot {index} addresses the wrong attribute"
            );
        }
    }

    /// Reads several attributes in one hop, so a device's whole attribute set costs one round
    /// trip rather than one each. Results are positional, with per-path errors preserved.
    ///
    /// # Panics
    ///
    /// Debug builds assert the batch is within `READ_BATCH_MAX` and that the reply is positional.
    pub async fn read_many(&self, indices: &[ReadIndex]) -> Vec<Result<SysfsValue>> {
        debug_assert!(indices.len() <= READ_BATCH_MAX);
        if indices.is_empty() {
            return Vec::new();
        }
        match self {
            Self::Inline(fds) => {
                let mut out = Vec::with_capacity(indices.len());
                for index in indices {
                    out.push(fds.read_index(*index).await);
                }
                out
            }
            Self::Threaded(worker) => {
                worker.ensure_registry().await;
                let batch = indices.to_vec();
                let expected = batch.len();
                let dispatched = worker
                    .dispatch(&BatchLabel(indices[0], expected), |reply| {
                        Request::ReadMany {
                            indices: batch,
                            reply,
                        }
                    })
                    .await;
                match dispatched {
                    Ok(values) => {
                        debug_assert_eq!(values.len(), expected);
                        values
                    }
                    // The device did not answer at all, so every path in the batch failed the
                    // same way. Fanning the one error out keeps the result positional.
                    Err(err) => {
                        let message = err.to_string();
                        (0..expected)
                            .map(|_| Err(anyhow::anyhow!("{message}")))
                            .collect()
                    }
                }
            }
        }
    }

    /// Writes one attribute, replacing its contents.
    pub async fn write_value(&self, path: &Path, data: Vec<u8>) -> Result<()> {
        match self {
            // Writes are open-write-close on both paths: a held descriptor buys nothing when the
            // kernel regenerates the attribute anyway, and sysfs writes are not on the hot path.
            Self::Inline(_) => cc_fs::write(path, data).await,
            Self::Threaded(worker) => {
                worker
                    .dispatch(&path.display(), |reply| Request::Write {
                        path: path.to_path_buf(),
                        data,
                        reply,
                    })
                    .await
            }
        }
    }

    /// Releases every held descriptor, for when the devices behind them may be gone (suspend).
    ///
    /// Best effort on the threaded path: a wedged worker never gets to act on it, which is
    /// harmless because its descriptors are already unusable.
    pub fn clear_descriptors(&self) {
        match self {
            Self::Inline(fds) => fds.clear(),
            Self::Threaded(worker) => {
                if worker.tx.try_send(Request::ClearDescriptors).is_err() {
                    debug!(
                        "could not clear descriptors for {}: worker busy or gone",
                        worker.state.device_name()
                    );
                }
            }
        }
    }

    /// A `DeviceIo` whose worker never answers, for tests that need a wedged device.
    ///
    /// Nothing drains the returned receiver, so every dispatch times out. That is what a wedged
    /// driver looks like from the main thread, without needing a driver that wedges or a thread
    /// that never returns. The caller must keep the receiver alive: dropping it closes the
    /// channel, which is the distinct "worker gone" path.
    #[cfg(test)]
    #[must_use]
    pub fn wedged_for_test(reply_timeout: Duration) -> (Self, mpsc::Receiver<Request>) {
        let (tx, rx) = mpsc::channel::<Request>(QUEUE_DEPTH);
        let worker = Worker {
            state: HealthState::new("wedged".to_owned()),
            tx,
            reply_timeout,
            pending_registry: RefCell::new(None),
        };
        (Self::Threaded(Rc::new(worker)), rx)
    }

    /// Lets the next dispatch through even if the device is currently unreachable.
    ///
    /// Shutdown uses it: resetting a channel to its firmware default is the most safety-relevant
    /// write the daemon makes, and a device that stopped answering may well answer now.
    pub fn allow_probe_now(&self) {
        match self {
            Self::Inline(_) => {}
            Self::Threaded(worker) => worker.state.allow_next_dispatch(),
        }
    }

    /// Whether this device is answering. Always `Healthy` when not isolated.
    #[must_use]
    pub fn health(&self) -> DeviceHealth {
        match self {
            Self::Inline(_) => DeviceHealth::Healthy,
            Self::Threaded(worker) => worker.health(),
        }
    }

    /// Descriptors currently held open, asked of the worker thread on the threaded path.
    ///
    /// The descriptor cache has no observable effect other than not reopening, so a test has to
    /// count them to prove it. The round trip is why this is test-only: it is the only reason the
    /// main thread ever needs to see inside a worker's cache.
    #[cfg(test)]
    pub async fn descriptor_count(&self) -> usize {
        match self {
            Self::Inline(fds) => fds.len(),
            Self::Threaded(worker) => worker
                .dispatch(&"descriptor-count", |reply| Request::DescriptorCount {
                    reply,
                })
                .await
                .unwrap_or_default(),
        }
    }

    /// Whether the per-tick rotation should skip this device for now.
    #[must_use]
    pub fn is_unreachable(&self) -> bool {
        matches!(self.health(), DeviceHealth::Unreachable { .. })
    }
}

/// What we have observed about one device's responsiveness.
///
/// Split from `Worker` because it says nothing about sysfs: the NVML worker counts timeouts,
/// goes unreachable and probes back on exactly the same rules.
#[derive(Debug)]
pub struct HealthState {
    device_name: String,
    consecutive_timeouts: Cell<u8>,
    /// `Some` while the device is unreachable: no request is dispatched until this instant.
    probe_after: Cell<Option<Instant>>,
}

impl HealthState {
    #[must_use]
    pub fn new(device_name: String) -> Self {
        Self {
            device_name,
            consecutive_timeouts: Cell::new(0),
            probe_after: Cell::new(None),
        }
    }

    #[must_use]
    pub fn device_name(&self) -> &str {
        &self.device_name
    }

    /// Whether a device that is not answering should be given its next probe yet.
    #[must_use]
    pub fn dispatchable(&self) -> bool {
        self.probe_after
            .get()
            .is_none_or(|probe_after| Instant::now() >= probe_after)
    }

    /// The device answered, so whatever it said, it is alive.
    pub fn record_answered(&self) {
        if self.consecutive_timeouts.get() > 0 {
            debug!("device IO recovered: {}", self.device_name);
        }
        if self.probe_after.get().is_some() {
            warn!(
                "Device {} is answering again after being unreachable.",
                self.device_name
            );
        }
        self.consecutive_timeouts.set(0);
        self.probe_after.set(None);
    }

    /// The device did not answer in time. `what` names the operation, for the log only.
    pub fn record_timeout(&self, what: &dyn std::fmt::Display) {
        let timeouts = self.consecutive_timeouts.get().saturating_add(1);
        self.consecutive_timeouts.set(timeouts);
        let was_unreachable = self.probe_after.get().is_some();
        if timeouts < UNREACHABLE_AFTER_TIMEOUTS {
            debug!(
                "device IO timeout {timeouts} of {UNREACHABLE_AFTER_TIMEOUTS} for {} at {what}",
                self.device_name
            );
            return;
        }
        self.probe_after
            .set(Some(Instant::now() + UNREACHABLE_PROBE_INTERVAL));
        // Once, on the way in. A device that stays wedged must not log every probe.
        if was_unreachable.not() {
            warn!(
                "Device {} stopped answering after {timeouts} timed out reads. Its readings and \
                 fan control are suspended; retrying every {} seconds.",
                self.device_name,
                UNREACHABLE_PROBE_INTERVAL.as_secs()
            );
        }
    }

    /// Lets the next dispatch through even if the device is currently unreachable.
    pub fn allow_next_dispatch(&self) {
        self.probe_after.set(None);
    }

    /// Puts the device into the state `record_timeout` would have left it in.
    #[cfg(test)]
    pub fn force_unreachable_at(&self, timeouts: u8, probe_after: Instant) {
        self.consecutive_timeouts.set(timeouts);
        self.probe_after.set(Some(probe_after));
    }

    #[must_use]
    pub fn health(&self) -> DeviceHealth {
        let consecutive_timeouts = self.consecutive_timeouts.get();
        if self.probe_after.get().is_some() {
            return DeviceHealth::Unreachable {
                consecutive_timeouts,
            };
        }
        if consecutive_timeouts == 0 {
            return DeviceHealth::Healthy;
        }
        DeviceHealth::Degraded {
            consecutive_timeouts,
        }
    }
}

/// Splits a channel set into the slots to read and a per-channel flag saying whether it has one.
///
/// The flags are what keep a positional reply aligned: a channel with no slot contributes nothing
/// to the batch, so zipping the reply onto the channels directly would hand it the next channel's
/// reading. Returning the flags alongside lets the caller skip it instead.
pub fn slots_for<T>(
    channels: &[T],
    slot_of: impl Fn(&T) -> Option<ReadIndex>,
) -> (Vec<ReadIndex>, Vec<bool>) {
    let mut slots = Vec::with_capacity(channels.len());
    let mut slotted = Vec::with_capacity(channels.len());
    for channel in channels {
        match slot_of(channel) {
            Some(slot) => {
                slots.push(slot);
                slotted.push(true);
            }
            None => slotted.push(false),
        }
    }
    debug_assert_eq!(slotted.len(), channels.len());
    (slots, slotted)
}

/// The main-thread half of a worker: the queue into it, and what we have observed about it.
#[derive(Debug)]
pub struct Worker {
    state: HealthState,
    tx: mpsc::Sender<Request>,
    reply_timeout: Duration,
    /// The read table, kept until the worker acknowledges it.
    ///
    /// A device that is already wedged when detection ends cannot take delivery: the install
    /// times out like any other dispatch. Holding the table and retrying on the next batch means
    /// such a device still reports timeouts and still goes unreachable, rather than failing every
    /// read as unregistered and looking healthy while doing it.
    pending_registry: RefCell<Option<Vec<PathBuf>>>,
}

impl Worker {
    /// Send one request and wait for its reply, within the device's budget.
    ///
    /// Only a timeout counts against the device's health. An `io::Error` coming back means the
    /// device answered and the answer was an error, which says nothing about whether it is wedged.
    async fn dispatch<T, F>(&self, what: &dyn Display, make_request: F) -> Result<T>
    where
        F: FnOnce(oneshot::Sender<Result<T>>) -> Request,
    {
        // Refuse to queue for a device that is not answering, until its next probe is due.
        // Otherwise every tick adds a request to a queue nobody drains and pays a full timeout
        // to learn what it already knew.
        if self.state.dispatchable().not() {
            return Err(unreachable(self.state.device_name(), what));
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = make_request(reply_tx);
        let exchange = async {
            self.tx
                .send(request)
                .await
                .map_err(|_| worker_gone(self.state.device_name()))?;
            reply_rx
                .await
                .map_err(|_| worker_gone(self.state.device_name()))?
        };
        match rt::timeout(self.reply_timeout, exchange).await {
            Ok(result) => {
                self.state.record_answered();
                result
            }
            Err(_elapsed) => {
                self.state.record_timeout(what);
                Err(timed_out(
                    self.state.device_name(),
                    what,
                    self.reply_timeout,
                ))
            }
        }
    }

    fn health(&self) -> DeviceHealth {
        self.state.health()
    }

    /// Delivers the read table if the worker has not taken it yet.
    async fn ensure_registry(&self) {
        let Some(paths) = self.pending_registry.borrow().clone() else {
            return;
        };
        if self
            .dispatch(&"install", |reply| Request::Install { paths, reply })
            .await
            .is_ok()
        {
            *self.pending_registry.borrow_mut() = None;
        }
    }
}

/// Work for a device's worker. Every payload is plain data, so the thread boundary needs no shared
/// ownership and no lock.
#[derive(Debug)]
pub enum Request {
    Read {
        path: PathBuf,
        reply: oneshot::Sender<Result<SysfsValue>>,
    },
    ReadMany {
        indices: Vec<ReadIndex>,
        reply: oneshot::Sender<Result<Vec<Result<SysfsValue>>>>,
    },
    /// Hands the worker its read table. Sent once, after detection settles the channel set.
    Install {
        paths: Vec<PathBuf>,
        reply: oneshot::Sender<Result<()>>,
    },
    Write {
        path: PathBuf,
        data: Vec<u8>,
        reply: oneshot::Sender<Result<()>>,
    },
    /// Counts the worker's own descriptor cache, so a test can prove it is not reopening.
    #[cfg(test)]
    DescriptorCount {
        reply: oneshot::Sender<Result<usize>>,
    },
    ClearDescriptors,
}

/// The worker thread body: its own runtime, its own descriptor cache, one request at a time.
fn run_worker(rx: mpsc::Receiver<Request>) {
    match rt::worker_runtime(serve(rx)) {
        Ok(()) => debug!("device IO worker stopped"),
        // The device loses isolation, not its readings: every dispatch then times out and the
        // device is marked unreachable, which is the same path a wedged driver takes.
        Err(err) => error!("Device IO worker could not start its runtime: {err}"),
    }
}

/// Serve requests until the last `DeviceIo` handle is dropped and the queue closes.
async fn serve(mut rx: mpsc::Receiver<Request>) {
    let fds = cc_fs::SysfsFdCache::default();
    while let Some(request) = rx.recv().await {
        match request {
            Request::Read { path, reply } => {
                // Uncached: one-shots are detection probes, labels and pre-write reads, none of
                // which repeat per tick. The table is for the per-tick set alone.
                let result = cc_fs::read_sysfs_value(&path).await;
                // A dropped receiver means the caller already timed out. Expected, not an error.
                let _ = reply.send(result);
            }
            Request::ReadMany { indices, reply } => {
                let mut values = Vec::with_capacity(indices.len());
                for index in &indices {
                    values.push(fds.read_index(*index).await);
                }
                let _ = reply.send(Ok(values));
            }
            Request::Install { paths, reply } => {
                fds.install(paths);
                let _ = reply.send(Ok(()));
            }
            #[cfg(test)]
            Request::DescriptorCount { reply } => {
                let _ = reply.send(Ok(fds.len()));
            }
            Request::Write { path, data, reply } => {
                let result = cc_fs::write(&path, data).await;
                let _ = reply.send(result);
            }
            Request::ClearDescriptors => fds.clear(),
        }
    }
}

/// Thread names are capped at 15 bytes by the kernel, so the device name is truncated rather than
/// silently rejected.
fn worker_thread_name(device_name: &str) -> String {
    const PREFIX: &str = "ccio-";
    const NAME_MAX_BYTES: usize = 15;
    const _: () = assert!(PREFIX.len() < NAME_MAX_BYTES);
    let mut name = String::with_capacity(NAME_MAX_BYTES);
    name.push_str(PREFIX);
    for ch in device_name.chars() {
        if name.len() + ch.len_utf8() > NAME_MAX_BYTES {
            break;
        }
        name.push(ch);
    }
    debug_assert!(name.len() <= NAME_MAX_BYTES);
    debug_assert!(name.starts_with(PREFIX));
    name
}

fn timed_out(device_name: &str, what: &dyn Display, budget: Duration) -> anyhow::Error {
    Error::new(
        ErrorKind::TimedOut,
        format!(
            "device {device_name} did not answer within {} ms reading {what}",
            budget.as_millis()
        ),
    )
    .into()
}

fn unreachable(device_name: &str, what: &dyn Display) -> anyhow::Error {
    Error::new(
        ErrorKind::HostUnreachable,
        format!("device {device_name} is unreachable, skipping {what}"),
    )
    .into()
}

/// Names a batch in a timeout message without carrying a path across the boundary.
struct BatchLabel(ReadIndex, usize);

impl Display for BatchLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} attribute(s) from slot {}", self.1, self.0)
    }
}

fn worker_gone(device_name: &str) -> anyhow::Error {
    Error::new(
        ErrorKind::BrokenPipe,
        format!("device {device_name} IO worker is gone"),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: the descriptor cache is the reason a device thread is cheaper than a pool, but it
    /// lives on the worker where nothing could see it, so the no-leak claim was only ever proven
    /// for `Inline`. Method: batch the same paths twice on a real worker and count its cache.
    #[test]
    fn a_worker_holds_its_descriptors_across_batches() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let first = dir.path().join("temp1_input");
            let second = dir.path().join("temp2_input");
            std::fs::write(&first, "41000\n").unwrap();
            std::fs::write(&second, "52000\n").unwrap();
            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();
            io.install_registry(vec![first, second]).await.unwrap();
            let slots = [0, 1];
            assert_eq!(
                io.descriptor_count().await,
                0,
                "nothing open before the first read"
            );

            io.read_many(&slots).await;
            assert_eq!(io.descriptor_count().await, 2);

            io.read_many(&slots).await;
            assert_eq!(
                io.descriptor_count().await,
                2,
                "a second batch reopened instead of reusing the cache"
            );
        });
    }

    /// Goal: a device already wedged when detection ends cannot take delivery of its read table,
    /// and it must still report timeouts rather than failing every read as unregistered and
    /// looking healthy while it does. Method: install against a worker nobody drains, then batch.
    #[test]
    fn a_wedged_device_still_times_out_when_it_never_took_its_table() {
        crate::rt::test_runtime(async {
            let (io, _rx) = DeviceIo::wedged_for_test(Duration::from_millis(10));
            io.install_registry(vec![PathBuf::from("/sys/class/hwmon/hwmon0/temp1_input")])
                .await
                .expect("install never reports failure, it retries");

            let results = io.read_many(&[0]).await;

            assert_eq!(results.len(), 1);
            let message = results[0].as_ref().unwrap_err().to_string();
            assert!(
                message.contains("did not answer"),
                "an undelivered table must not turn a wedged device into an unregistered slot, \
                 got: {message}"
            );
            assert!(matches!(io.health(), DeviceHealth::Degraded { .. }));
        });
    }

    // --- slots_for: keeping a positional reply aligned ---

    /// Goal: slots are positional, so a channel with no slot must be marked, not skipped silently.
    /// Zipping a short reply onto the full channel list reports one sensor's value under another
    /// sensor's name, which a fan curve then follows with nothing logged. The debug assertion in
    /// the readers catches a missing slot in tests; this is what keeps release builds correct.
    ///
    /// Method: gaps at both ends and in the middle, then walk the reply back onto the channels the
    /// way the readers do and check each value lands on its own channel.
    #[test]
    fn an_unslotted_channel_is_flagged_so_the_reply_stays_aligned() {
        let channels = [None, Some(7), None, Some(3), None];
        let (slots, slotted) = slots_for(&channels, |slot| *slot);

        assert_eq!(slots, vec![7, 3], "only real slots go into the batch");
        assert_eq!(slotted, vec![false, true, false, true, false]);

        // The reply comes back in slot order; walk it back the way the readers do.
        let reply = ["value-for-7", "value-for-3"];
        let mut reply = reply.into_iter();
        let landed: Vec<Option<&str>> = slotted
            .into_iter()
            .map(|has_slot| has_slot.then(|| reply.next()).flatten())
            .collect();

        assert_eq!(
            landed,
            vec![None, Some("value-for-7"), None, Some("value-for-3"), None],
            "each value must land on the channel that asked for it"
        );
    }

    // --- HealthState: the rules both workers share ---

    /// Goal: a device is only given up on after the full budget, or one slow read would suspend
    /// a working device. Method: count timeouts up to the threshold and check each verdict.
    #[test]
    fn health_degrades_before_it_gives_up() {
        let state = HealthState::new("gpu0".to_owned());
        assert_eq!(state.health(), DeviceHealth::Healthy);
        for expected in 1..UNREACHABLE_AFTER_TIMEOUTS {
            state.record_timeout(&"read");
            assert_eq!(
                state.health(),
                DeviceHealth::Degraded {
                    consecutive_timeouts: expected
                }
            );
            assert!(state.dispatchable(), "a degraded device still gets work");
        }
        state.record_timeout(&"read");
        assert_eq!(
            state.health(),
            DeviceHealth::Unreachable {
                consecutive_timeouts: UNREACHABLE_AFTER_TIMEOUTS
            }
        );
    }

    /// Goal: an unreachable device must stop being dispatched to, or every tick queues work
    /// nobody drains and pays a timeout to learn what it already knew.
    #[test]
    fn an_unreachable_device_is_not_dispatched_to_until_its_probe() {
        let state = HealthState::new("gpu0".to_owned());
        for _ in 0..UNREACHABLE_AFTER_TIMEOUTS {
            state.record_timeout(&"read");
        }
        assert!(state.dispatchable().not(), "work must not be queued");

        state.force_unreachable_at(UNREACHABLE_AFTER_TIMEOUTS, Instant::now());
        assert!(state.dispatchable(), "the probe is due, let one through");
    }

    /// Goal: a device that answers again must clear completely, so a later blip starts from zero
    /// rather than tipping it straight back to unreachable.
    #[test]
    fn answering_clears_the_whole_history() {
        let state = HealthState::new("gpu0".to_owned());
        for _ in 0..UNREACHABLE_AFTER_TIMEOUTS {
            state.record_timeout(&"read");
        }
        state.record_answered();
        assert_eq!(state.health(), DeviceHealth::Healthy);
        assert!(state.dispatchable());
    }

    /// Short enough that a wedge test finishes quickly, long enough that a real worker on a busy
    /// build machine answers well inside it.
    const TEST_TIMEOUT: Duration = Duration::from_millis(80);

    fn worker_of(io: &DeviceIo) -> &Rc<Worker> {
        match io {
            DeviceIo::Threaded(worker) => worker,
            DeviceIo::Inline(_) => panic!("expected a threaded DeviceIo"),
        }
    }

    /// Goal: a batch must be positional and per-path, because every caller zips the results back
    /// against the channels that asked for them. A shifted or collapsed result would silently
    /// report one channel's value under another channel's name.
    /// Method: mix readable and missing paths and assert each slot carries its own outcome.
    #[test]
    fn a_batch_returns_one_result_per_path_in_order() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let good = dir.path().join("temp1_input");
            let absent = dir.path().join("temp2_input");
            let other = dir.path().join("temp3_input");
            std::fs::write(&good, "41000\n").unwrap();
            std::fs::write(&other, "52000\n").unwrap();

            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();
            io.install_registry(vec![good.clone(), absent.clone(), other.clone()])
                .await
                .unwrap();
            let results = io.read_many(&[0, 1, 2]).await;

            assert_eq!(results.len(), 3);
            assert_eq!(results[0].as_ref().unwrap().trimmed_str().unwrap(), "41000");
            let err = results[1].as_ref().unwrap_err();
            assert_eq!(
                err.downcast_ref::<Error>().unwrap().kind(),
                ErrorKind::NotFound,
                "a missing path fails only its own slot"
            );
            assert_eq!(results[2].as_ref().unwrap().trimmed_str().unwrap(), "52000");
        });
    }

    /// Goal: batching is the whole point of `read_many`, so it must cost one message rather than
    /// one per path. Method: drain the queue after a batch and count what was actually sent.
    #[test]
    fn a_batch_costs_one_message_regardless_of_size() {
        crate::rt::test_runtime(async {
            let (io, mut rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            let slots: Vec<ReadIndex> = (0..12).collect();

            let results = io.read_many(&slots).await;

            assert_eq!(results.len(), 12, "every slot still gets a result");
            let mut queued = 0;
            while let Ok(request) = rx.try_recv() {
                assert!(matches!(request, Request::ReadMany { .. }));
                queued += 1;
            }
            assert_eq!(queued, 1, "12 paths must cost one message, not 12");
        });
    }

    /// Goal: when the device does not answer at all, every path in the batch has to fail, or a
    /// caller zipping results would read a stale slot as a fresh value.
    #[test]
    fn a_batch_that_times_out_fails_every_path() {
        crate::rt::test_runtime(async {
            let (io, _rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            let slots: Vec<ReadIndex> = (0..4).collect();

            let results = io.read_many(&slots).await;

            assert_eq!(results.len(), 4);
            assert!(results.iter().all(Result::is_err), "no slot may look fresh");
        });
    }

    /// Goal: an empty batch must not dispatch. A device with no channels of a given kind calls
    /// this every tick, and a round trip for nothing is exactly the cost batching exists to remove.
    #[test]
    fn an_empty_batch_dispatches_nothing() {
        crate::rt::test_runtime(async {
            let (io, mut rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            assert!(io.read_many(&[]).await.is_empty());
            assert!(rx.try_recv().is_err(), "an empty batch must not be sent");
        });
    }

    /// Goal: the inline variant must agree with the threaded one, since tests and any device
    /// without a worker take that path.
    #[test]
    fn inline_batches_match_the_threaded_results() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let good = dir.path().join("temp1_input");
            std::fs::write(&good, "33000\n").unwrap();
            let absent = dir.path().join("nope_input");
            let inline_io = DeviceIo::default();
            inline_io
                .install_registry(vec![good.clone(), absent.clone()])
                .await
                .unwrap();
            let inline = inline_io.read_many(&[0, 1]).await;
            let threaded_io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();
            threaded_io
                .install_registry(vec![good, absent])
                .await
                .unwrap();
            let threaded = threaded_io.read_many(&[0, 1]).await;

            assert_eq!(inline.len(), threaded.len());
            assert_eq!(
                inline[0].as_ref().unwrap().trimmed_str().unwrap(),
                threaded[0].as_ref().unwrap().trimmed_str().unwrap()
            );
            assert!(inline[1].is_err());
            assert!(threaded[1].is_err());
        });
    }

    /// Goal: thread names must fit the kernel's 15-byte cap, whatever the device is called.
    /// Method: check a short name is kept whole (positive space) and an over-long one is truncated
    /// rather than rejected (negative space), with the prefix intact in both.
    #[test]
    fn worker_thread_name_fits_the_kernel_limit() {
        let short = worker_thread_name("octo");
        assert_eq!(short, "ccio-octo");
        assert!(short.len() <= 15);

        let long = worker_thread_name("a-very-long-hwmon-device-name");
        assert!(long.len() <= 15, "got {} bytes: {long}", long.len());
        assert!(long.starts_with("ccio-"));
        assert_ne!(long, "ccio-a-very-long-hwmon-device-name");

        // Multi-byte characters must not be split across the boundary.
        let wide = worker_thread_name("日本語のデバイス名");
        assert!(wide.len() <= 15, "got {} bytes: {wide}", wide.len());
        assert!(std::str::from_utf8(wide.as_bytes()).is_ok());
    }

    /// Goal: the inline variant must behave exactly as the historical path did, since every test
    /// in the daemon and every non-isolated device relies on it. Method: read a real file through
    /// `DeviceIo::default()` and compare with the expected contents.
    #[test]
    fn inline_reads_the_file_contents() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("temp1_input");
            std::fs::write(&path, "45000\n").unwrap();

            let io = DeviceIo::default();
            assert!(matches!(io, DeviceIo::Inline(_)));
            let value = io.read_value(&path).await.unwrap();
            assert_eq!(value.trimmed_str().unwrap(), "45000");
            assert_eq!(io.health(), DeviceHealth::Healthy);
            assert!(io.is_unreachable().not());
        });
    }

    /// Goal: a real worker thread must return the same bytes the inline path would, so isolating a
    /// device changes only where the read runs. Method: start a worker, read a file written from
    /// the test thread, and read it again after a rewrite to prove the descriptor stays live.
    #[test]
    fn threaded_reads_the_file_contents() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("fan1_input");
            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();

            for expected in ["1200", "0", "2400"] {
                std::fs::write(&path, format!("{expected}\n")).unwrap();
                let value = io.read_value(&path).await.unwrap();
                assert_eq!(value.trimmed_str().unwrap(), expected);
            }
            assert_eq!(io.health(), DeviceHealth::Healthy);
        });
    }

    /// Goal: writes must cross the thread boundary too, because the failsafe's job is to write to
    /// healthy devices while another is wedged. Method: write through a worker and read the file
    /// back from the test thread.
    #[test]
    fn threaded_writes_the_file_contents() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("pwm1");
            std::fs::write(&path, "0\n").unwrap();

            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();
            io.write_value(&path, b"128".to_vec()).await.unwrap();

            let written = std::fs::read_to_string(&path).unwrap();
            assert_eq!(written, "128");
            assert_eq!(io.health(), DeviceHealth::Healthy);
        });
    }

    /// Goal: a device that answers with an error is alive, and must not be counted as wedged.
    /// Otherwise a board full of unreadable attributes would mark its device unreachable. Method:
    /// read a missing file through a real worker and assert the error surfaces while health does
    /// not move.
    #[test]
    fn an_io_error_does_not_count_against_health() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();

            let result = io.read_value(&dir.path().join("absent_input")).await;
            assert!(result.is_err());
            let err = result.unwrap_err();
            let io_err = err.downcast_ref::<Error>().unwrap();
            assert_eq!(io_err.kind(), ErrorKind::NotFound);
            assert_eq!(io.health(), DeviceHealth::Healthy);
        });
    }

    /// Goal: a read that never comes back must return control to the caller, which is the entire
    /// point of the work. Method: dispatch to a queue nobody drains and assert the call returns a
    /// timeout inside a bounded wall-clock window, and that the device is recorded as degraded.
    #[test]
    fn a_wedged_device_times_out_instead_of_blocking() {
        crate::rt::test_runtime(async {
            let (io, _rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            assert_eq!(io.health(), DeviceHealth::Healthy);

            let started = Instant::now();
            let result = io
                .read_value(Path::new("/sys/class/hwmon/hwmon0/temp1_input"))
                .await;
            let elapsed = started.elapsed();

            assert!(result.is_err());
            let err = result.unwrap_err();
            let io_err = err.downcast_ref::<Error>().unwrap();
            assert_eq!(io_err.kind(), ErrorKind::TimedOut);
            // Lower bound only: an upper bound would flake on a busy build host.
            assert!(elapsed >= TEST_TIMEOUT, "returned after {elapsed:?}");
            assert_eq!(
                io.health(),
                DeviceHealth::Degraded {
                    consecutive_timeouts: 1
                }
            );
        });
    }

    /// Goal: a device that stays wedged must leave the per-tick rotation, or every tick pays a
    /// full timeout to learn what the last one already knew, and the queue grows without bound.
    /// Method: time out exactly the threshold number of times, then assert the next dispatch is
    /// refused immediately rather than after another timeout.
    #[test]
    fn a_wedged_device_becomes_unreachable_and_stops_dispatching() {
        crate::rt::test_runtime(async {
            let (io, _rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            let path = Path::new("/sys/class/hwmon/hwmon0/temp1_input");

            for _ in 0..UNREACHABLE_AFTER_TIMEOUTS {
                assert!(io.read_value(path).await.is_err());
            }
            assert_eq!(
                io.health(),
                DeviceHealth::Unreachable {
                    consecutive_timeouts: UNREACHABLE_AFTER_TIMEOUTS
                }
            );
            assert!(io.is_unreachable());

            // The gate must be cheap: no dispatch, no timeout, no queue growth.
            let started = Instant::now();
            let result = io.read_value(path).await;
            let elapsed = started.elapsed();
            assert!(result.is_err());
            let err = result.unwrap_err();
            let io_err = err.downcast_ref::<Error>().unwrap();
            assert_eq!(io_err.kind(), ErrorKind::HostUnreachable);
            assert!(
                elapsed < TEST_TIMEOUT,
                "gate took {elapsed:?}, so it dispatched"
            );
        });
    }

    /// Goal: the queue must not grow while a device is unreachable, which is what bounds memory
    /// for a device that never comes back. Method: wedge the device past the threshold, then
    /// dispatch many more times and assert nothing further was enqueued.
    #[test]
    fn an_unreachable_device_enqueues_nothing_further() {
        crate::rt::test_runtime(async {
            let (io, mut rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            let path = Path::new("/sys/class/hwmon/hwmon0/temp1_input");

            for _ in 0..UNREACHABLE_AFTER_TIMEOUTS {
                let _ = io.read_value(path).await;
            }
            let queued_at_threshold = drain(&mut rx);
            assert_eq!(queued_at_threshold, usize::from(UNREACHABLE_AFTER_TIMEOUTS));

            for _ in 0..50 {
                let _ = io.read_value(path).await;
            }
            assert_eq!(drain(&mut rx), 0, "unreachable device still queued work");
        });
    }

    /// Goal: a device that recovers must come back without a daemon restart, so the unreachable
    /// state has to be a pause and not a death sentence. Method: mark a real worker unreachable
    /// with its probe already due, then read a real file and assert health resets.
    #[test]
    fn an_unreachable_device_recovers_on_its_next_probe() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("temp1_input");
            std::fs::write(&path, "41000\n").unwrap();

            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();
            let worker = worker_of(&io);
            worker
                .state
                .force_unreachable_at(UNREACHABLE_AFTER_TIMEOUTS, Instant::now());
            assert_eq!(
                io.health(),
                DeviceHealth::Unreachable {
                    consecutive_timeouts: UNREACHABLE_AFTER_TIMEOUTS
                }
            );

            let value = io.read_value(&path).await.unwrap();
            assert_eq!(value.trimmed_str().unwrap(), "41000");
            assert_eq!(io.health(), DeviceHealth::Healthy);
            assert!(io.is_unreachable().not());
        });
    }

    /// Goal: a probe that is not yet due must not dispatch, or the backoff does nothing. Method:
    /// mark the device unreachable with a probe well in the future and assert the gate holds.
    #[test]
    fn an_unreachable_device_waits_for_its_probe_interval() {
        crate::rt::test_runtime(async {
            let (io, mut rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            let worker = worker_of(&io);
            worker.state.force_unreachable_at(
                UNREACHABLE_AFTER_TIMEOUTS,
                Instant::now() + UNREACHABLE_PROBE_INTERVAL,
            );

            let result = io
                .read_value(Path::new("/sys/class/hwmon/hwmon0/temp1_input"))
                .await;
            assert!(result.is_err());
            assert_eq!(drain(&mut rx), 0);
            assert_eq!(
                io.health(),
                DeviceHealth::Unreachable {
                    consecutive_timeouts: UNREACHABLE_AFTER_TIMEOUTS
                }
            );
        });
    }

    /// Goal: a dead worker must be distinguishable from a slow one, and must not take a full
    /// timeout to report. Method: drop the receiver so the channel closes, then dispatch.
    #[test]
    fn a_gone_worker_reports_immediately() {
        crate::rt::test_runtime(async {
            let (io, rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            drop(rx);

            let started = Instant::now();
            let result = io
                .read_value(Path::new("/sys/class/hwmon/hwmon0/temp1_input"))
                .await;
            let elapsed = started.elapsed();

            assert!(result.is_err());
            let err = result.unwrap_err();
            let io_err = err.downcast_ref::<Error>().unwrap();
            assert_eq!(io_err.kind(), ErrorKind::BrokenPipe);
            assert!(
                elapsed < TEST_TIMEOUT,
                "took {elapsed:?}, so it waited on a dead worker"
            );
        });
    }

    /// Goal: clearing descriptors must be safe on both variants and on a wedged device, because
    /// suspend calls it without knowing a device's state. Method: call it on each and assert no
    /// panic and no health change.
    #[test]
    fn clearing_descriptors_is_harmless() {
        crate::rt::test_runtime(async {
            DeviceIo::default().clear_descriptors();

            let io = DeviceIo::threaded("testdev", TEST_TIMEOUT).unwrap();
            io.clear_descriptors();
            assert_eq!(io.health(), DeviceHealth::Healthy);

            let (wedged, _rx) = DeviceIo::wedged_for_test(TEST_TIMEOUT);
            wedged.clear_descriptors();
            assert_eq!(wedged.health(), DeviceHealth::Healthy);
        });
    }

    /// Count and discard whatever is sitting in a worker's queue.
    fn drain(rx: &mut mpsc::Receiver<Request>) -> usize {
        let mut count = 0;
        while rx.try_recv().is_ok() {
            count += 1;
        }
        count
    }
}
