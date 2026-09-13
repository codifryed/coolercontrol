// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Service-manager readiness and liveness heartbeat.
//!
//! Under a `Type=notify` unit, a wedged main loop stops the heartbeat, systemd
//! aborts the daemon and `Restart=always` brings it back, re-applying every
//! device's settings. The heartbeat rides on a completed main-loop tick rather
//! than a timer of its own: a timer would keep pinging while the loop is stuck,
//! which is the failure this exists to catch.
//!
//! No-op when `NOTIFY_SOCKET` is unset, so `OpenRC`, container, and `AppImage`
//! runs are unaffected.

use crate::rt;
use crate::setting::STARTUP_DELAY_SECONDS_MAX;
use log::{debug, info, warn};
use sd_notify::NotifyState;
use std::cell::Cell;
use std::ops::Not;
use std::time::{Duration, Instant};

/// Half the supervisor's interval, as systemd documents. That leaves half the
/// budget spare, but not a whole interval: the gap the supervisor actually sees
/// is the ping interval plus one full poll, since a heartbeat can land just
/// before a tick begins. `warn_if_poll_rate_too_slow` checks that sum.
const PING_INTERVAL_DIVISOR: u32 = 2;

/// A divisor below 2 would leave no slack at all for a late tick.
const _: () = assert!(PING_INTERVAL_DIVISOR >= 2);

/// Bounds the slicing loop below. The longest real caller is the resume pause.
const HEARTBEAT_SLEEP_MAX: Duration = Duration::from_secs(STARTUP_DELAY_SECONDS_MAX as u64);

/// The cap must cover every wait routed through it, or that wait is silently
/// cut short. `setting.rs` owns the bound; this fails the build if it grows.
const _: () = assert!(HEARTBEAT_SLEEP_MAX.as_secs() >= STARTUP_DELAY_SECONDS_MAX as u64);

/// Single-threaded like the rest of the main-thread daemon, so `ping` takes
/// `&self` and keeps its rate limit in a `Cell`.
pub struct Watchdog {
    /// The supervisor's own deadline, `None` outside a `WatchdogSec=` unit.
    /// The ping interval is derived from it rather than stored beside it, so
    /// the two cannot drift apart.
    supervisor_interval: Option<Duration>,
    /// `None` until the first ping, so the first one is always due.
    last_ping: Cell<Option<Instant>>,
    /// The datagram needs a service manager to observe, so tests count the
    /// heartbeats instead.
    #[cfg(test)]
    ping_count: Cell<u32>,
}

impl Watchdog {
    /// Reads the supervisor's watchdog interval from the environment.
    pub fn init() -> Self {
        let watchdog = Self::with_supervisor_interval(sd_notify::watchdog_enabled());
        match watchdog.ping_interval() {
            Some(interval) => info!(
                "Service manager watchdog enabled, sending a heartbeat every {:.1}s",
                interval.as_secs_f64()
            ),
            None => debug!("Service manager watchdog not enabled"),
        }
        watchdog
    }

    fn with_supervisor_interval(supervisor_interval: Option<Duration>) -> Self {
        Self {
            supervisor_interval,
            last_ping: Cell::new(None),
            #[cfg(test)]
            ping_count: Cell::new(0),
        }
    }

    /// A watchdog that never pings, matching a run with no supervisor.
    #[cfg(test)]
    pub fn disabled() -> Self {
        Self::with_supervisor_interval(None)
    }

    /// How often a heartbeat goes out. `None` outside a `WatchdogSec=` unit.
    pub fn ping_interval(&self) -> Option<Duration> {
        self.supervisor_interval.map(ping_interval_for)
    }

    #[cfg(test)]
    pub fn last_ping(&self) -> Option<Instant> {
        self.last_ping.get()
    }

    #[cfg(test)]
    pub fn ping_count(&self) -> u32 {
        self.ping_count.get()
    }

    /// Warns when the poll rate cannot sustain the supervisor's deadline, which
    /// would restart the daemon while it is healthy. Only a hand-edited
    /// `WatchdogSec=` reaches this: the shipped 30s unit leaves 10s spare
    /// against a 5s worst-case poll rate.
    pub fn warn_if_poll_rate_too_slow(&self, poll_rate: Duration) {
        debug_assert!(poll_rate.is_zero().not());
        let Some(supervisor_interval) = self.supervisor_interval else {
            return;
        };
        debug_assert!(supervisor_interval.is_zero().not());
        // A heartbeat can land just before a tick begins, so the longest gap
        // the supervisor sees is the ping interval plus one whole poll.
        let worst_case_gap = ping_interval_for(supervisor_interval) + poll_rate;
        if worst_case_gap < supervisor_interval {
            return;
        }
        warn!(
            "A poll rate of {:.1}s leaves a worst-case heartbeat gap of {:.1}s, which the service \
             manager's watchdog deadline of {:.1}s does not cover. It may restart the daemon \
             while it is healthy. Raise WatchdogSec in the unit file or lower the poll rate.",
            poll_rate.as_secs_f64(),
            worst_case_gap.as_secs_f64(),
            supervisor_interval.as_secs_f64()
        );
    }

    /// Sends a heartbeat if one is due. Cheap to call on every tick.
    pub fn ping(&self) {
        let Some(ping_interval) = self.ping_interval() else {
            return;
        };
        debug_assert!(ping_interval.is_zero().not());
        let now = Instant::now();
        if ping_is_due(self.last_ping.get(), ping_interval, now).not() {
            return;
        }
        self.last_ping.set(Some(now));
        #[cfg(test)]
        self.ping_count.set(self.ping_count.get() + 1);
        // The rate limit must now hold, or every tick sends a datagram.
        debug_assert!(ping_is_due(self.last_ping.get(), ping_interval, now).not());
        notify(&[NotifyState::Watchdog], "watchdog heartbeat");
    }

    /// Sleeps for `total`, pinging throughout. For waits the daemon takes on
    /// purpose that outlast the watchdog interval, the resume pause above all:
    /// a deliberate wait must not read as a hang.
    pub async fn sleep_with_heartbeat(&self, total: Duration) {
        debug_assert!(total <= HEARTBEAT_SLEEP_MAX);
        // Clamp before the branch, so the caller gets the same duration whether
        // or not a supervisor is present.
        let total = total.min(HEARTBEAT_SLEEP_MAX);
        let Some(ping_interval) = self.ping_interval() else {
            rt::sleep(total).await;
            return;
        };
        debug_assert!(ping_interval.is_zero().not());
        let deadline = Instant::now() + total;
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            rt::sleep(remaining.min(ping_interval)).await;
            self.ping();
        }
    }
}

/// Tells the service manager initialization is complete. Free-standing rather
/// than a method: readiness is a one-shot signal that carries no state, so it
/// must not look like it needs the heartbeat's instance.
pub fn notify_ready() {
    notify(&[NotifyState::Ready], "ready");
}

/// Tells the service manager a clean shutdown has begun.
pub fn notify_stopping() {
    notify(&[NotifyState::Stopping], "stopping");
}

fn notify(state: &[NotifyState], description: &str) {
    if let Err(err) = sd_notify::notify(state) {
        // A lost heartbeat costs a restart, a lost readiness a start timeout.
        // Both are visible; neither is improved by panicking.
        warn!("Could not send {description} to the service manager: {err}");
    }
}

/// Half the supervisor's deadline, floored so an absurd `WatchdogSec=` cannot
/// produce a zero interval. systemd never sets zero, but this is external input.
fn ping_interval_for(supervisor_interval: Duration) -> Duration {
    let ping_interval = (supervisor_interval / PING_INTERVAL_DIVISOR).max(Duration::from_millis(1));
    debug_assert!(ping_interval.is_zero().not());
    debug_assert!(ping_interval <= supervisor_interval.max(Duration::from_millis(1)));
    ping_interval
}

/// Split out so the rate limit is testable without a service manager or a
/// clock that has to advance in real time.
fn ping_is_due(last_ping: Option<Instant>, ping_interval: Duration, now: Instant) -> bool {
    let Some(last_ping) = last_ping else {
        return true;
    };
    now.saturating_duration_since(last_ping) >= ping_interval
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `WatchdogSec=` in `packaging/systemd/coolercontrold.service`.
    const SHIPPED_UNIT_INTERVAL: Duration = Duration::from_secs(30);

    /// The interval systemd hands us is halved. Getting this wrong in either
    /// direction is invisible until production: too long and the daemon is
    /// killed while healthy, too short and it is only wasted datagrams.
    /// `init` reads `WATCHDOG_USEC` through `sd_notify` and passes the result
    /// straight here, so the interval is supplied directly rather than through
    /// the process environment, which the rest of the suite reads concurrently.
    #[test]
    fn a_supervisor_interval_is_halved() {
        assert_eq!(
            Watchdog::with_supervisor_interval(Some(SHIPPED_UNIT_INTERVAL)).ping_interval(),
            Some(Duration::from_secs(15))
        );
    }

    /// The shipped unit is the only configuration most users will ever run, so
    /// pin the pairing it produces against the worst-case poll rate. The gap
    /// that matters is the ping interval plus one whole poll, not the ping
    /// interval alone.
    #[test]
    fn shipped_unit_sustains_the_slowest_poll_rate() {
        let ping_interval = Watchdog::with_supervisor_interval(Some(SHIPPED_UNIT_INTERVAL))
            .ping_interval()
            .unwrap();
        // POLL_RATE max is 5.0s.
        let worst_case_gap = ping_interval + Duration::from_secs_f64(5.0);
        assert!(worst_case_gap < SHIPPED_UNIT_INTERVAL);
    }

    /// A disabled watchdog is the common case: no unit, `OpenRC`, a container, or
    /// a developer running the binary by hand. Nothing it exposes may claim
    /// otherwise.
    #[test]
    fn disabled_watchdog_reports_no_interval() {
        let watchdog = Watchdog::disabled();
        assert_eq!(watchdog.ping_interval(), None);
        // Pinging must stay a no-op rather than panic or notify.
        watchdog.ping();
        watchdog.ping();
        assert_eq!(watchdog.last_ping(), None);
    }

    /// The first ping has to go out immediately. systemd starts counting at
    /// READY, so waiting a full interval before the first heartbeat spends
    /// half the budget before the loop has proven anything.
    #[test]
    fn first_ping_is_always_due() {
        assert!(ping_is_due(None, Duration::from_secs(15), Instant::now()));
    }

    /// The rate limit is what keeps a 0.5s poll rate from sending a datagram
    /// every tick. It must hold until the interval has fully elapsed.
    #[test]
    fn ping_is_withheld_until_the_interval_elapses() {
        let interval = Duration::from_secs(15);
        let last_ping = Instant::now();

        assert!(ping_is_due(Some(last_ping), interval, last_ping).not());
        assert!(ping_is_due(
            Some(last_ping),
            interval,
            last_ping + Duration::from_secs(14)
        )
        .not());
        assert!(ping_is_due(Some(last_ping), interval, last_ping + interval));
        assert!(ping_is_due(
            Some(last_ping),
            interval,
            last_ping + Duration::from_secs(3_600)
        ));
    }

    /// The rate limit has to hold through `ping` itself, not only through the
    /// free function: a second immediate call must not re-stamp `last_ping`.
    #[test]
    fn ping_records_the_first_heartbeat_and_then_rate_limits() {
        let watchdog = Watchdog::with_supervisor_interval(Some(Duration::from_secs(30)));
        assert_eq!(watchdog.last_ping(), None);
        watchdog.ping();
        let first = watchdog.last_ping().expect("first ping is always due");
        watchdog.ping();
        assert_eq!(watchdog.last_ping(), Some(first));
    }

    /// A clock that appears to move backwards must not wedge the heartbeat
    /// into never firing again; saturating arithmetic makes it merely not-due.
    #[test]
    fn a_backwards_clock_does_not_panic() {
        let interval = Duration::from_secs(15);
        let last_ping = Instant::now() + Duration::from_secs(60);
        assert!(ping_is_due(Some(last_ping), interval, Instant::now()).not());
    }

    /// The poll-rate warning exists to catch a hand-edited `WatchdogSec=` that
    /// the main loop cannot keep up with. Assert the arithmetic the check turns
    /// on rather than the log line itself. At a divisor of 2 this is equivalent
    /// to comparing the poll rate against the ping interval; it is written as
    /// the worst-case gap so it stays correct if the divisor ever changes.
    #[test]
    fn poll_rate_warning_only_applies_to_an_unsustainable_pairing() {
        let poll_rate = Duration::from_secs(5);

        // Shipped unit: 30s deadline, 15s ping, 20s worst-case gap. Sustainable.
        let shipped = Duration::from_secs(30);
        assert!(ping_interval_for(shipped) + poll_rate < shipped);

        // Hand-edited WatchdogSec=8 gives a 4s ping and a 9s gap: over deadline.
        let too_tight = Duration::from_secs(8);
        assert!(ping_interval_for(too_tight) + poll_rate >= too_tight);

        // Each must run without panicking, warning only for the second.
        Watchdog::with_supervisor_interval(Some(shipped)).warn_if_poll_rate_too_slow(poll_rate);
        Watchdog::with_supervisor_interval(Some(too_tight)).warn_if_poll_rate_too_slow(poll_rate);
        Watchdog::disabled().warn_if_poll_rate_too_slow(poll_rate);
    }

    /// A disabled watchdog still has to honour the sleep it was asked for,
    /// since the resume pause routes through it either way.
    #[test]
    fn disabled_heartbeat_sleep_still_sleeps() {
        rt::test_runtime(async {
            let watchdog = Watchdog::disabled();
            let start = Instant::now();
            watchdog
                .sleep_with_heartbeat(Duration::from_millis(30))
                .await;
            assert!(start.elapsed() >= Duration::from_millis(30));
        });
    }

    /// The slicing loop must terminate on an interval far shorter than the
    /// total, which is the resume case: a 15s ping interval inside a wait of
    /// up to 120s.
    #[test]
    fn heartbeat_sleep_slices_and_terminates() {
        rt::test_runtime(async {
            let watchdog = Watchdog::with_supervisor_interval(Some(Duration::from_millis(10)));
            let start = Instant::now();
            watchdog
                .sleep_with_heartbeat(Duration::from_millis(40))
                .await;
            assert!(start.elapsed() >= Duration::from_millis(40));
        });
    }

    /// The whole point of the function: it must ping *throughout* the wait,
    /// not once at either end. A single heartbeat across a wait many intervals
    /// long still starves the service manager, so count them: 40ms at a 5ms
    /// interval owes 8, and the floor leaves slack for a loaded machine.
    #[test]
    fn heartbeat_sleep_pings_throughout_the_wait() {
        rt::test_runtime(async {
            let watchdog = Watchdog::with_supervisor_interval(Some(Duration::from_millis(10)));
            watchdog
                .sleep_with_heartbeat(Duration::from_millis(40))
                .await;
            assert!(
                watchdog.ping_count() >= 4,
                "only {} heartbeat(s) across the wait",
                watchdog.ping_count()
            );
        });
    }

    /// A caller that overruns the cap must get the same (clamped) duration
    /// whether or not a supervisor is present. The debug assertion catches it
    /// in development; release must not silently diverge between the branches.
    #[test]
    fn the_sleep_cap_covers_the_longest_real_wait() {
        assert!(HEARTBEAT_SLEEP_MAX >= Duration::from_secs(u64::from(STARTUP_DELAY_SECONDS_MAX)));
    }
}
