// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::ENV_DBUS;
use futures_util::{Stream, StreamExt};
use log::{info, warn};
use nix::fcntl::{fcntl, FcntlArg, FdFlag};
use std::cell::Cell;
use std::env;
use std::future::Future;
use std::ops::Not;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use zbus::{zvariant, Connection, Proxy};

const SLEEP_DEFAULT_BUS_NAME: &str = "org.freedesktop.login1";
const SLEEP_OBJECTPATH: &str = "/org/freedesktop/login1";
const SLEEP_INTERFACE: &str = "org.freedesktop.login1.Manager";
const SIGNAL_PREPARE_FOR_SLEEP: &str = "PrepareForSleep";
/// A logind delay lock on sleep: logind waits for it to be released before sleeping, for at most
/// its `InhibitDelayMaxSec`.
const METHOD_INHIBIT: &str = "Inhibit";
const INHIBIT_WHAT: &str = "sleep";
const INHIBIT_WHO: &str = "CoolerControl";
const INHIBIT_WHY: &str = "Prepare devices and the watchdog for sleep";
const INHIBIT_MODE: &str = "delay";
const LOGIND_OPERATION_IN_PROGRESS: &str = "org.freedesktop.login1.OperationInProgress";
// Bounds the DBus handshake (connect + proxy + AddMatch for PrepareForSleep) and each Inhibit
// call, which take tens of milliseconds on a healthy system. A wedged logind/dbus-broker then
// cannot block the listener: a setup that times out runs deaf (nothing is recorded), and an
// Inhibit that times out leaves the next sleep without a lock.
const DBUS_CALL_TIMEOUT_S: u64 = 5;
/// The low bit of `SleepSignals::state`; the bits above it are the wake count.
const PREPARING_BIT: u64 = 1;

/// What the main loop should do this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepPhase {
    /// No sleep in progress: poll as usual.
    Awake,
    /// Going to sleep, or not woken yet. Pass `wake_count` to `release_inhibitor`.
    Sleeping { wake_count: u64 },
    /// Woken since the last handled wake. Pass `wake_count` to `wake_handled` once done.
    Woken { wake_count: u64 },
}

/// The `PrepareForSleep` signals seen so far. Only the listener writes them, so a signal that
/// arrives while the main loop is handling an earlier one is never overwritten.
#[derive(Default)]
struct SleepSignals {
    /// The wake count shifted up one bit, with `PREPARING_BIT` following the latest signal. One
    /// word, so the main loop never pairs a new count with a stale `preparing`. Every wake counts,
    /// even one with no sleep seen before it, as when the daemon started mid-sleep: the devices
    /// went through the sleep all the same.
    state: AtomicU64,
    /// The delay lock, held while awake so logind waits for the sleep arm before it sleeps.
    inhibitor: Mutex<Option<HeldInhibitor>>,
    /// Wakes the main loop on each signal, so a sleep is not left waiting on the next tick.
    changed: Notify,
}

/// A delay lock, and the wake after which it was taken or last renewed: the sleep it is for.
struct HeldInhibitor {
    /// Held only to be dropped: closing it releases the lock.
    _fd: OwnedFd,
    wake_count: u64,
}

impl SleepSignals {
    fn record(&self, going_to_sleep: bool) {
        let next = |state: u64| {
            let wake_count = state >> 1;
            if going_to_sleep {
                (wake_count << 1) | PREPARING_BIT
            } else {
                debug_assert!(wake_count < u64::MAX >> 1);
                (wake_count + 1) << 1
            }
        };
        // One RMW: the count and `preparing` change together.
        let previous = self
            .state
            .update(Ordering::Release, Ordering::Relaxed, next);
        debug_assert_eq!(self.preparing(), going_to_sleep);
        debug_assert!(self.wake_count() >= previous >> 1);
        self.changed.notify_one();
    }

    /// Both halves of one load: whether going to sleep, and the wake count.
    fn load(&self) -> (bool, u64) {
        let state = self.state.load(Ordering::Acquire);
        (state & PREPARING_BIT == PREPARING_BIT, state >> 1)
    }

    fn preparing(&self) -> bool {
        self.load().0
    }

    fn wake_count(&self) -> u64 {
        self.load().1
    }

    fn holds_inhibitor(&self) -> bool {
        self.inhibitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .is_some()
    }

    fn hold_inhibitor(&self, inhibitor: OwnedFd) {
        let mut held = self
            .inhibitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        // The listener is the only writer, so this is the latest wake.
        let wake_count = self.wake_count();
        // A second lock would only hold logind up for longer; dropping it releases it.
        match held.as_mut() {
            Some(held) => held.wake_count = wake_count,
            None => {
                *held = Some(HeldInhibitor {
                    _fd: inhibitor,
                    wake_count,
                });
            }
        }
        debug_assert!(held
            .as_ref()
            .is_some_and(|held| held.wake_count == wake_count));
    }

    /// A lock still held at a wake, as when logind slept once its delay ran out, now serves the
    /// next sleep. Retagged, the sleep arm still preparing for the last sleep leaves it held.
    /// False when no lock is held.
    fn renew_inhibitor(&self) -> bool {
        let mut held = self
            .inhibitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let Some(held) = held.as_mut() else {
            return false;
        };
        let wake_count = self.wake_count();
        debug_assert!(held.wake_count <= wake_count);
        held.wake_count = wake_count;
        true
    }

    /// The listener stopped while the daemon runs on. A sleep still recorded would hold the main
    /// loop in the sleep arm with nothing left to end it, so it counts as woken: controlling the
    /// fans again beats idling. A held lock would hold up every later sleep, so it goes too.
    fn listener_lost(&self) {
        self.drop_inhibitor();
        if self.preparing() {
            self.record(false);
        }
        debug_assert!(self.preparing().not());
    }

    /// Releases a lock taken for the sleep after `wake_count` or an earlier one. A lock taken
    /// since is for a later sleep, which the sleep arm has not prepared for.
    fn release_inhibitor(&self, wake_count: u64) {
        let mut held = self
            .inhibitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if held
            .as_ref()
            .is_some_and(|held| held.wake_count <= wake_count)
        {
            // Closing the descriptor is what releases the lock.
            drop(held.take());
        }
        // Checked under the lock: the listener may take a fresh one as soon as it is free.
        debug_assert!(held
            .as_ref()
            .is_none_or(|held| held.wake_count > wake_count));
    }

    fn drop_inhibitor(&self) {
        let mut held = self
            .inhibitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        drop(held.take());
        debug_assert!(held.is_none());
    }
}

/// Tracks system sleep/resume state for the main loop. The dbus connection and signal loop run on
/// the Tokio sidecar (zbus needs a Tokio reactor); the signals are shared back to the main thread
/// and polled each tick. If dbus is disabled or fails to connect, nothing is recorded and the
/// listener is effectively deaf.
pub struct SleepListener {
    signals: Arc<SleepSignals>,
    /// Main-thread only: the wakes the main loop has finished handling.
    handled_wake_count: Cell<u64>,
}

impl SleepListener {
    pub fn new(run_token: CancellationToken) -> Self {
        let listener = Self::from_signals(Arc::default());
        if dbus_listener_enabled().not() {
            info!("DBUS sleep listener disabled.");
            return listener;
        }
        let signals = Arc::clone(&listener.signals);
        crate::sidecar::handle().spawn(move || run_listener(run_token, signals));
        listener
    }

    fn from_signals(signals: Arc<SleepSignals>) -> Self {
        Self {
            signals,
            handled_wake_count: Cell::new(0),
        }
    }

    /// Sleeping comes first: a wake followed by another sleep before this tick is handled once,
    /// after the second wake, rather than once while going down and again after.
    pub fn phase(&self) -> SleepPhase {
        // One load: a count and `preparing` read apart could pair a wake with the sleep before
        // it, and the lock renewed at that wake would be released unprepared.
        let (preparing, wake_count) = self.signals.load();
        if preparing {
            return SleepPhase::Sleeping { wake_count };
        }
        let handled_wake_count = self.handled_wake_count.get();
        debug_assert!(handled_wake_count <= wake_count);
        if wake_count == handled_wake_count {
            SleepPhase::Awake
        } else {
            debug_assert!(wake_count > handled_wake_count);
            SleepPhase::Woken { wake_count }
        }
    }

    /// Marks the wakes up to `wake_count` handled. Any that arrived since stay pending, so the
    /// main loop handles them on a later tick.
    pub fn wake_handled(&self, wake_count: u64) {
        debug_assert!(wake_count >= self.handled_wake_count.get());
        debug_assert!(wake_count <= self.signals.wake_count());
        self.handled_wake_count.set(wake_count);
    }

    /// Lets logind go ahead with the sleep the `Sleeping` phase's `wake_count` names, once the
    /// sleep arm has done its part. A no-op when no lock for it is held.
    pub fn release_inhibitor(&self, wake_count: u64) {
        self.signals.release_inhibitor(wake_count);
    }

    /// Completes when a sleep signal has arrived since the last call.
    pub fn changed(&self) -> impl Future<Output = ()> + '_ {
        self.signals.changed.notified()
    }

    #[cfg(test)]
    pub fn deaf() -> Self {
        Self::from_signals(Arc::default())
    }

    /// Stands in for a `PrepareForSleep` signal.
    #[cfg(test)]
    pub fn record(&self, going_to_sleep: bool) {
        self.signals.record(going_to_sleep);
    }

    #[cfg(all(test, feature = "gated-tests"))]
    pub fn hold_inhibitor(&self, inhibitor: OwnedFd) {
        self.signals.hold_inhibitor(inhibitor);
    }

    #[cfg(all(test, feature = "gated-tests"))]
    pub fn holds_inhibitor(&self) -> bool {
        self.signals.holds_inhibitor()
    }
}

fn dbus_listener_enabled() -> bool {
    env::var(ENV_DBUS)
        .ok()
        .and_then(|env_dbus| {
            env_dbus
                .parse::<u8>()
                .ok()
                .map(|bb| bb != 0)
                .or_else(|| Some(env_dbus.trim().to_lowercase() != "off"))
        })
        .unwrap_or(true)
}

/// Runs on the sidecar: connects to dbus, then records `PrepareForSleep` signals as they arrive.
/// Returns (running deaf) if the connection cannot be established within the timeout.
async fn run_listener(run_token: CancellationToken, signals: Arc<SleepSignals>) {
    // We wrap the full setup (not just the connect) because the hang has been seen on the AddMatch
    // inside `receive_signal` as well, not only on `Connection::system`.
    // See https://gitlab.com/coolercontrol/coolercontrol/-/issues/264.
    let setup = async {
        let conn = Connection::system().await?;
        let proxy = Proxy::new(
            &conn,
            SLEEP_DEFAULT_BUS_NAME,
            SLEEP_OBJECTPATH,
            SLEEP_INTERFACE,
        )
        .await?;
        let signal = proxy.receive_signal(SIGNAL_PREPARE_FOR_SLEEP).await?;
        Ok::<_, zbus::Error>((conn, proxy, signal))
    };
    let (conn, proxy, sleep_signal) =
        match timeout(Duration::from_secs(DBUS_CALL_TIMEOUT_S), setup).await {
            Ok(Ok(connected)) => connected,
            Ok(Err(err)) => {
                warn!("Could not connect to DBUS, sleep listener will not work: {err}");
                return;
            }
            Err(_) => {
                warn!(
                    "DBUS sleep listener setup timed out after {DBUS_CALL_TIMEOUT_S}s; \
                     continuing without it. Sleep/resume events will not be handled on this run."
                );
                return;
            }
        };
    info!("DBUS sleep listener connected.");
    take_inhibitor(&proxy, &signals).await;
    // true when entering sleep, false when waking.
    let sleep_signal = sleep_signal.map(|msg| msg.body().deserialize::<bool>());
    // The sleep arm released the last lock; the next sleep needs its own.
    let on_wake = async || take_inhibitor(&proxy, &signals).await;
    record_signals(&run_token, &signals, sleep_signal, on_wake).await;
    let _ = conn.close().await;
}

/// Records each signal until the run is cancelled or the stream ends. A stream that ends while
/// the daemon runs on is a lost listener.
async fn record_signals(
    run_token: &CancellationToken,
    signals: &SleepSignals,
    mut sleep_signal: impl Stream<Item = zbus::Result<bool>> + Unpin,
    mut on_wake: impl AsyncFnMut(),
) {
    loop {
        // Matched inside the arm: a `Some(..)` pattern would only disable the branch on `None`,
        // and the cancellation branch never is, so a lost stream would never break the loop.
        tokio::select! {
            () = run_token.cancelled() => break,
            msg = sleep_signal.next() => match msg {
                None => break,
                Some(Ok(true)) => {
                    info!("Received message that system is going to sleep.");
                    signals.record(true);
                }
                Some(Ok(false)) => {
                    info!("Received message that system is waking from sleep");
                    signals.record(false);
                    on_wake().await;
                }
                Some(Err(err)) => warn!("Failed to read PrepareForSleep signal body: {err}"),
            },
        }
    }
    if run_token.is_cancelled().not() {
        warn!(
            "DBUS sleep listener stopped; sleep and resume will not be handled for the rest of \
             this run."
        );
        signals.listener_lost();
    }
}

/// Without a delay lock logind sleeps at once, and the freeze can land before the sleep arm has
/// widened the watchdog deadline or prepared the devices. Bounded by `DBUS_CALL_TIMEOUT_S`, for the
/// same wedged-logind reason as the setup. Without one the daemon runs on, as before.
async fn take_inhibitor(proxy: &Proxy<'_>, signals: &SleepSignals) {
    if signals.renew_inhibitor() {
        return;
    }
    // Only the listener takes locks, so none can appear before the call below returns.
    debug_assert!(signals.holds_inhibitor().not());
    let call = proxy.call::<_, _, zvariant::OwnedFd>(
        METHOD_INHIBIT,
        &(INHIBIT_WHAT, INHIBIT_WHO, INHIBIT_WHY, INHIBIT_MODE),
    );
    match timeout(Duration::from_secs(DBUS_CALL_TIMEOUT_S), call).await {
        Ok(Ok(inhibitor)) => hold_new_inhibitor(signals, inhibitor.into()),
        Ok(Err(zbus::Error::MethodError(name, _, _))) if is_sleep_in_progress(name.as_str()) => {
            info!("Sleep already in progress; taking the sleep delay lock after the wake.");
        }
        Ok(Err(err)) => warn!("Could not take a sleep delay lock from logind: {err}"),
        Err(_) => {
            warn!("Taking a sleep delay lock from logind timed out after {DBUS_CALL_TIMEOUT_S}s.");
        }
    }
}

/// zbus receives the descriptor without close-on-exec, so every child the daemon spawns would
/// inherit the lock and keep it alive past its release. One that cannot be fixed is dropped.
fn hold_new_inhibitor(signals: &SleepSignals, inhibitor: OwnedFd) {
    if let Err(err) = fcntl(&inhibitor, FcntlArg::F_SETFD(FdFlag::FD_CLOEXEC)) {
        warn!("Could not keep the sleep delay lock from child processes, releasing it: {err}");
        return;
    }
    debug_assert!(is_close_on_exec(&inhibitor));
    signals.hold_inhibitor(inhibitor);
    debug_assert!(signals.holds_inhibitor());
}

fn is_close_on_exec(fd: &OwnedFd) -> bool {
    fcntl(fd, FcntlArg::F_GETFD)
        .is_ok_and(|bits| FdFlag::from_bits_truncate(bits).contains(FdFlag::FD_CLOEXEC))
}

/// logind refuses new delay locks once a sleep has begun, as when the daemon starts mid-sleep.
/// That is expected: the next wake takes the lock.
fn is_sleep_in_progress(error_name: &str) -> bool {
    debug_assert!(error_name.is_empty().not());
    error_name == LOGIND_OPERATION_IN_PROGRESS
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener() -> SleepListener {
        SleepListener::from_signals(Arc::default())
    }

    /// The count and `preparing` share one word, so each signal must move both and leave the
    /// other half intact. Sleep, wake, sleep again, then a wake with no sleep seen before it.
    #[test]
    fn each_signal_sets_both_halves_of_the_state() {
        let signals = SleepSignals::default();
        assert_eq!(signals.load(), (false, 0));
        signals.record(true);
        assert_eq!(signals.load(), (true, 0));
        signals.record(false);
        assert_eq!(signals.load(), (false, 1));
        signals.record(true);
        assert_eq!(signals.load(), (true, 1));
        signals.record(false);
        assert_eq!(signals.load(), (false, 2));
        signals.record(false);
        assert_eq!(signals.load(), (false, 3));
    }

    /// A deaf listener, dbus disabled or never connected, must leave the main loop polling.
    #[test]
    fn a_deaf_listener_stays_awake() {
        let listener = listener();
        assert_eq!(listener.phase(), SleepPhase::Awake);
        assert_eq!(listener.phase(), SleepPhase::Awake);
    }

    /// The common path: sleep, wake, handle the wake, back to polling. Feed one signal of each
    /// and check the phase after every step.
    #[test]
    fn a_sleep_and_wake_is_handled_once() {
        let listener = listener();
        listener.signals.record(true);
        assert_eq!(listener.phase(), SleepPhase::Sleeping { wake_count: 0 });

        listener.signals.record(false);
        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 1 });
        // Unhandled, the wake stays pending.
        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 1 });

        listener.wake_handled(1);
        assert_eq!(listener.phase(), SleepPhase::Awake);
    }

    /// A second sleep and wake while the first wake is being handled must not be wiped when
    /// that handling finishes. Record both mid-handling, then mark only the first handled.
    #[test]
    fn a_wake_during_wake_handling_stays_pending() {
        let listener = listener();
        listener.signals.record(true);
        listener.signals.record(false);
        let SleepPhase::Woken { wake_count } = listener.phase() else {
            panic!("expected a wake");
        };

        listener.signals.record(true);
        listener.signals.record(false);
        listener.wake_handled(wake_count);

        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 2 });
        listener.wake_handled(2);
        assert_eq!(listener.phase(), SleepPhase::Awake);
    }

    /// A second sleep that has begun but not woken by the time wake handling finishes must read
    /// as sleeping, so the sleep arm prepares for it. Then its wake must still be handled.
    #[test]
    fn a_sleep_during_wake_handling_reads_as_sleeping() {
        let listener = listener();
        listener.signals.record(true);
        listener.signals.record(false);
        listener.signals.record(true);
        listener.wake_handled(1);

        assert_eq!(listener.phase(), SleepPhase::Sleeping { wake_count: 1 });
        listener.signals.record(false);
        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 2 });
    }

    /// A wake followed by another sleep before the main loop looks is one sleep to the loop:
    /// prepare once, wake once. Record wake then sleep unseen, and check sleeping wins.
    #[test]
    fn sleeping_takes_precedence_over_a_pending_wake() {
        let listener = listener();
        listener.signals.record(true);
        listener.signals.record(false);
        listener.signals.record(true);

        assert_eq!(listener.phase(), SleepPhase::Sleeping { wake_count: 1 });
        listener.signals.record(false);
        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 2 });
    }

    /// A daemon started mid-sleep sees only the wake, yet its devices still went through the
    /// sleep. Record a lone wake and check it is handled like any other.
    #[test]
    fn a_wake_with_no_sleep_seen_is_still_handled() {
        let listener = listener();
        listener.signals.record(false);
        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 1 });
        listener.wake_handled(1);
        assert_eq!(listener.phase(), SleepPhase::Awake);
    }

    /// logind waits on the delay lock for only a few seconds, so a signal must wake the main
    /// loop rather than wait for its next tick. Poll the change once before and after a signal.
    #[test]
    fn a_signal_wakes_the_main_loop() {
        use futures_util::FutureExt;
        let listener = listener();
        assert_eq!(listener.changed().now_or_never(), None);

        listener.signals.record(true);

        assert_eq!(listener.changed().now_or_never(), Some(()));
        assert_eq!(
            listener.changed().now_or_never(),
            None,
            "one wake per signal"
        );
    }

    /// A second lock would hold logind up for longer, and a released one must be gone. Hold two
    /// pipes as locks, check only one is kept, then release it.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn one_delay_lock_is_held_and_released() {
        let listener = listener();
        let (first, _first_writer) = std::io::pipe().unwrap();
        let (second, _second_writer) = std::io::pipe().unwrap();
        listener.hold_inhibitor(first.into());
        listener.hold_inhibitor(second.into());
        assert!(listener.holds_inhibitor());

        listener.release_inhibitor(0);
        assert!(listener.holds_inhibitor().not());
        listener.release_inhibitor(0);
        assert!(listener.holds_inhibitor().not());
    }

    /// A lock a child inherits outlives its release and holds up every later sleep. Clear
    /// close-on-exec on a pipe, as zbus hands the lock over, hold it, and check the held copy.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_held_lock_is_closed_on_exec() {
        let listener = listener();
        let (reader, _writer) = std::io::pipe().unwrap();
        let reader = OwnedFd::from(reader);
        fcntl(&reader, FcntlArg::F_SETFD(FdFlag::empty())).unwrap();
        assert!(is_close_on_exec(&reader).not());

        hold_new_inhibitor(&listener.signals, reader);

        let held = listener.signals.inhibitor.lock().unwrap();
        assert!(is_close_on_exec(
            &held.as_ref().expect("the lock is held")._fd
        ));
    }

    /// A lock that serves a later sleep must outlast the sleep arm still preparing for this
    /// one, or that later sleep runs unlocked. Read the sleep's wake count, then sleep and wake
    /// mid-preparation: a lock renewed at that wake, or a fresh one, must survive the release.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_lock_for_a_later_sleep_outlasts_the_release() {
        let listener = listener();
        let (first, _first_writer) = std::io::pipe().unwrap();
        listener.hold_inhibitor(first.into());
        listener.signals.record(true);
        let SleepPhase::Sleeping { wake_count } = listener.phase() else {
            panic!("expected a sleep");
        };

        listener.signals.record(false);
        assert!(listener.signals.renew_inhibitor());
        listener.release_inhibitor(wake_count);
        assert!(listener.holds_inhibitor(), "a renewed lock is kept");

        listener.signals.drop_inhibitor();
        let (second, _second_writer) = std::io::pipe().unwrap();
        listener.hold_inhibitor(second.into());
        listener.release_inhibitor(wake_count);
        assert!(listener.holds_inhibitor(), "a fresh lock is kept");

        listener.signals.record(true);
        let SleepPhase::Sleeping { wake_count } = listener.phase() else {
            panic!("expected a sleep");
        };
        listener.release_inhibitor(wake_count);
        assert!(listener.holds_inhibitor().not());
    }

    /// A listener lost mid-sleep would leave the loop in the sleep arm for good, with no
    /// watchdog to end it outside systemd. Lose it after a sleep signal: that counts as a wake.
    #[test]
    fn a_listener_lost_while_sleeping_counts_as_a_wake() {
        let listener = listener();
        listener.signals.record(true);

        listener.signals.listener_lost();

        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 1 });
    }

    /// A stream that ends mid-sleep must end the loop and count as a wake, or the main loop
    /// sits in the sleep arm for good. Feed one sleep, then end the stream, and poll once.
    #[test]
    fn an_ended_signal_stream_counts_as_a_wake() {
        use futures_util::FutureExt;
        let listener = listener();
        let run_token = CancellationToken::new();
        let sleep_signal = futures_util::stream::iter([Ok(true)]);

        let recording = record_signals(&run_token, &listener.signals, sleep_signal, async || {});

        assert_eq!(recording.now_or_never(), Some(()));
        assert_eq!(listener.phase(), SleepPhase::Woken { wake_count: 1 });
    }

    /// Lost while awake there is nothing to wake from, so no wake may be invented. Lose it with
    /// no sleep recorded.
    #[test]
    fn a_listener_lost_while_awake_changes_nothing() {
        let listener = listener();
        listener.signals.listener_lost();
        assert_eq!(listener.phase(), SleepPhase::Awake);
        assert_eq!(listener.signals.wake_count(), 0);
    }

    /// A lock left held would hold up every later sleep once nobody can release it. Hold a pipe
    /// as the lock, then lose the listener.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_listener_lost_releases_its_lock() {
        let listener = listener();
        let (reader, _writer) = std::io::pipe().unwrap();
        listener.hold_inhibitor(reader.into());

        listener.signals.listener_lost();

        assert!(listener.holds_inhibitor().not());
    }

    /// A daemon started mid-sleep is refused the lock, which is expected and must not read as
    /// a fault; anything else still must. Check logind's refusal and two other errors.
    #[test]
    fn only_a_sleep_in_progress_counts_as_expected() {
        assert!(is_sleep_in_progress(
            "org.freedesktop.login1.OperationInProgress"
        ));
        assert!(is_sleep_in_progress("org.freedesktop.DBus.Error.AccessDenied").not());
        assert!(is_sleep_in_progress("org.freedesktop.login1.OperationInProgressX").not());
    }

    /// A wake that never arrives leaves the loop in the sleep arm; the watchdog's sleep budget
    /// bounds that. Record a lone sleep and check nothing else turns it into a wake.
    #[test]
    fn a_sleep_with_no_wake_stays_sleeping() {
        let listener = listener();
        listener.signals.record(true);
        listener.signals.record(true);
        assert_eq!(listener.phase(), SleepPhase::Sleeping { wake_count: 0 });
        assert_eq!(listener.signals.wake_count(), 0);
    }
}
