// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::ENV_DBUS;
use log::{info, warn};
use std::cell::Cell;
use std::env;
use std::future::Future;
use std::ops::Not;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;
use tokio::sync::Notify;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use zbus::export::ordered_stream::OrderedStreamExt;
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
// The whole DBus handshake (connect + proxy + AddMatch for PrepareForSleep) should complete in
// tens of milliseconds on a healthy system. We cap it so a wedged logind/dbus-broker can't block
// the listener from ever connecting; on timeout we simply run deaf (nothing is recorded).
const DBUS_SETUP_TIMEOUT_S: u64 = 5;

/// What the main loop should do this tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SleepPhase {
    /// No sleep in progress: poll as usual.
    Awake,
    /// Going to sleep, or not woken yet.
    Sleeping,
    /// Woken since the last handled wake. Pass `wake_count` to `wake_handled` once done.
    Woken { wake_count: u64 },
}

/// The `PrepareForSleep` signals seen so far. Only the listener writes them, so a signal that
/// arrives while the main loop is handling an earlier one is never overwritten.
#[derive(Default)]
struct SleepSignals {
    /// Follows the latest signal.
    preparing: AtomicBool,
    /// Every wake, counted. A wake that arrives with no sleep seen before it, as when the daemon
    /// started mid-sleep, still counts: the devices went through the sleep all the same.
    wake_count: AtomicU64,
    /// The delay lock, held while awake so logind waits for the sleep arm before it sleeps.
    inhibitor: Mutex<Option<OwnedFd>>,
    /// Wakes the main loop on each signal, so a sleep is not left waiting on the next tick.
    changed: Notify,
}

impl SleepSignals {
    fn record(&self, going_to_sleep: bool) {
        if going_to_sleep.not() {
            let previous = self.wake_count.fetch_add(1, Ordering::Relaxed);
            debug_assert!(previous < u64::MAX);
        }
        // Last, with Release: a main loop that sees `preparing` cleared also sees the wake.
        self.preparing.store(going_to_sleep, Ordering::Release);
        // The listener is the only writer, so the store reads back.
        debug_assert_eq!(self.preparing.load(Ordering::Relaxed), going_to_sleep);
        self.changed.notify_one();
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
        // A second lock would only hold logind up for longer; dropping it releases it.
        if held.is_none() {
            *held = Some(inhibitor);
        }
        debug_assert!(held.is_some());
    }

    /// The listener stopped while the daemon runs on. A sleep still recorded would hold the main
    /// loop in the sleep arm with nothing left to end it, so it counts as woken: controlling the
    /// fans again beats idling. A held lock would hold up every later sleep, so it goes too.
    fn listener_lost(&self) {
        self.release_inhibitor();
        if self.preparing.load(Ordering::Acquire) {
            self.record(false);
        }
        debug_assert!(self.preparing.load(Ordering::Relaxed).not());
    }

    /// Closing the descriptor is what releases the lock.
    fn release_inhibitor(&self) {
        let mut held = self
            .inhibitor
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        drop(held.take());
        // Checked under the lock: the listener may take a fresh one as soon as it is free.
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
        if self.signals.preparing.load(Ordering::Acquire) {
            return SleepPhase::Sleeping;
        }
        let wake_count = self.signals.wake_count.load(Ordering::Relaxed);
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
        debug_assert!(wake_count <= self.signals.wake_count.load(Ordering::Relaxed));
        self.handled_wake_count.set(wake_count);
    }

    /// Lets logind go ahead with the sleep, once the sleep arm has done its part. A no-op when
    /// no lock is held.
    pub fn release_inhibitor(&self) {
        self.signals.release_inhibitor();
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
    let (conn, proxy, mut sleep_signal) =
        match timeout(Duration::from_secs(DBUS_SETUP_TIMEOUT_S), setup).await {
            Ok(Ok(connected)) => connected,
            Ok(Err(err)) => {
                warn!("Could not connect to DBUS, sleep listener will not work: {err}");
                return;
            }
            Err(_) => {
                warn!(
                    "DBUS sleep listener setup timed out after {DBUS_SETUP_TIMEOUT_S}s; \
                     continuing without it. Sleep/resume events will not be handled on this run."
                );
                return;
            }
        };
    info!("DBUS sleep listener connected.");
    take_inhibitor(&proxy, &signals).await;
    loop {
        tokio::select! {
            () = run_token.cancelled() => break,
            Some(msg) = sleep_signal.next() => {
                // true when entering sleep, false when waking.
                match msg.body().deserialize::<bool>() {
                    Ok(true) => {
                        info!("Received message that system is going to sleep.");
                        signals.record(true);
                    }
                    Ok(false) => {
                        info!("Received message that system is waking from sleep");
                        signals.record(false);
                        // The sleep arm released the last one; the next sleep needs its own.
                        take_inhibitor(&proxy, &signals).await;
                    }
                    Err(err) => warn!("Failed to read PrepareForSleep signal body: {err}"),
                }
            },
            else => break,
        }
    }
    if run_token.is_cancelled().not() {
        warn!(
            "DBUS sleep listener stopped; sleep and resume will not be handled for the rest of \
             this run."
        );
        signals.listener_lost();
    }
    let _ = conn.close().await;
}

/// Without a delay lock logind sleeps at once, and the freeze can land before the sleep arm has
/// widened the watchdog deadline or prepared the devices. Bounded like the setup, for the same
/// wedged-logind reason. Without one the daemon runs on, as before.
async fn take_inhibitor(proxy: &Proxy<'_>, signals: &SleepSignals) {
    if signals.holds_inhibitor() {
        return;
    }
    let call = proxy.call::<_, _, zvariant::OwnedFd>(
        METHOD_INHIBIT,
        &(INHIBIT_WHAT, INHIBIT_WHO, INHIBIT_WHY, INHIBIT_MODE),
    );
    match timeout(Duration::from_secs(DBUS_SETUP_TIMEOUT_S), call).await {
        Ok(Ok(inhibitor)) => signals.hold_inhibitor(inhibitor.into()),
        Ok(Err(err)) => warn!(
            "Could not take a sleep delay lock from logind, so devices may not be prepared \
             before sleep: {err}"
        ),
        Err(_) => warn!(
            "Taking a sleep delay lock from logind timed out after {DBUS_SETUP_TIMEOUT_S}s, so \
             devices may not be prepared before sleep."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener() -> SleepListener {
        SleepListener::from_signals(Arc::default())
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
        assert_eq!(listener.phase(), SleepPhase::Sleeping);

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

        assert_eq!(listener.phase(), SleepPhase::Sleeping);
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

        assert_eq!(listener.phase(), SleepPhase::Sleeping);
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

        listener.release_inhibitor();
        assert!(listener.holds_inhibitor().not());
        listener.release_inhibitor();
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

    /// Lost while awake there is nothing to wake from, so no wake may be invented. Lose it with
    /// no sleep recorded.
    #[test]
    fn a_listener_lost_while_awake_changes_nothing() {
        let listener = listener();
        listener.signals.listener_lost();
        assert_eq!(listener.phase(), SleepPhase::Awake);
        assert_eq!(listener.signals.wake_count.load(Ordering::Relaxed), 0);
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

    /// A wake that never arrives leaves the loop in the sleep arm; the watchdog's sleep budget
    /// bounds that. Record a lone sleep and check nothing else turns it into a wake.
    #[test]
    fn a_sleep_with_no_wake_stays_sleeping() {
        let listener = listener();
        listener.signals.record(true);
        listener.signals.record(true);
        assert_eq!(listener.phase(), SleepPhase::Sleeping);
        assert_eq!(listener.signals.wake_count.load(Ordering::Relaxed), 0);
    }
}
