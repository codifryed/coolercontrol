// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The remote password breaker: a backstop against guessing spread across many peers.
//!
//! The per-peer budget bounds each key, but an attacker holding many addresses gets a fresh
//! budget per address. This caps password attempts from every remote peer together:
//! - Its threshold sits well above what one throttled key can reach in a window, so
//!   tripping it takes a distributed attack. One peer can no longer lock everyone out.
//! - Loopback never enters it, so the local admin can always log in.
//! - A success never resets it: the admin logging in must not reopen the gate mid-attack.
//! - Tokens never touch it. They carry 122 random bits and cost a hash to check.
//!
//! Attempts are charged on arrival, as in the per-peer budget, so a burst of concurrent
//! attempts cannot overshoot the threshold. That also bounds the hashes queued behind the
//! auth actor at about `THRESHOLD` x 8 ms.

use crate::api::auth_throttle::{self, CredentialOutcome};
use log::warn;
use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

pub const WINDOW: Duration = Duration::from_mins(15);
pub const THRESHOLD: u32 = 64;
const COOLDOWN: Duration = Duration::from_mins(15);

/// The most password attempts one key can make within a `WINDOW` under its own backoff.
pub const SINGLE_KEY_MAX_ATTEMPTS_PER_WINDOW: u32 = auth_throttle::max_attempts_within(WINDOW);

const _: () = assert!(THRESHOLD >= 4 * SINGLE_KEY_MAX_ATTEMPTS_PER_WINDOW);
// A cooldown outlasts the window that tripped it, so the breaker reopens on a clean count.
const _: () = assert!(COOLDOWN.as_secs() >= WINDOW.as_secs());

#[derive(Debug)]
struct BreakerState {
    /// Start of the current counting window, `None` before the first attempt.
    window_start: Option<Instant>,
    /// Attempts charged in the current window: failures, plus attempts still in flight.
    charged: u32,
    tripped_until: Option<Instant>,
}

/// Invariant: `charged` never exceeds `THRESHOLD`. The breaker trips instead of charging.
#[derive(Debug)]
pub struct RemoteBreaker {
    state: Mutex<BreakerState>,
}

impl RemoteBreaker {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(BreakerState {
                window_start: None,
                charged: 0,
                tripped_until: None,
            }),
        }
    }

    /// Charges a remote password attempt, or refuses it with the cooldown remaining.
    /// In-flight attempts count toward the trip, so a concurrent burst trips it too.
    pub fn admit(&self, now: Instant) -> Result<BreakerCharge<'_>, Duration> {
        let mut state = self.lock();
        if let Some(remaining) = state.tripped_for(now) {
            return Err(remaining);
        }
        let window_start = state.roll_window(now);
        if state.charged < THRESHOLD {
            state.charged += 1;
            return Ok(BreakerCharge {
                breaker: self,
                window_start,
            });
        }
        state.tripped_until = Some(now + COOLDOWN);
        warn!(
            "{THRESHOLD} failed remote password attempts within {} minutes. Remote password \
             login is paused for {} minutes. Existing sessions, access tokens and local \
             login are unaffected.",
            WINDOW.as_secs() / 60,
            COOLDOWN.as_secs() / 60
        );
        Err(COOLDOWN)
    }

    /// Takes back one charge. A charge from a window that has since rolled over is gone
    /// already, and must not be taken from its successor.
    fn refund(&self, window_start: Instant) {
        let mut state = self.lock();
        if state.window_start == Some(window_start) {
            debug_assert!(state.charged > 0);
            state.charged = state.charged.saturating_sub(1);
        }
    }

    /// A poisoned breaker must not take authentication down with it: the state is a few
    /// counters, and continuing with whatever survived is strictly better.
    fn lock(&self) -> MutexGuard<'_, BreakerState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for RemoteBreaker {
    fn default() -> Self {
        Self::new()
    }
}

impl BreakerState {
    fn tripped_for(&self, now: Instant) -> Option<Duration> {
        let remaining = self.tripped_until?.checked_duration_since(now)?;
        if remaining.is_zero() {
            return None;
        }
        Some(remaining)
    }

    /// Starts a fresh window once the current one has run its length, and returns the
    /// window this moment falls in.
    fn roll_window(&mut self, now: Instant) -> Instant {
        match self.window_start {
            Some(start) if now.duration_since(start) < WINDOW => start,
            _ => {
                self.window_start = Some(now);
                self.charged = 0;
                now
            }
        }
    }
}

/// A remote attempt's charge, settled once its verdict is known. Dropping it unsettled
/// keeps the charge: a request abandoned mid-flight still spent a hash.
#[derive(Debug)]
#[must_use]
pub struct BreakerCharge<'a> {
    breaker: &'a RemoteBreaker,
    window_start: Instant,
}

impl BreakerCharge<'_> {
    /// A rejection keeps the charge. A success or a non-verdict gives back only this
    /// attempt's own charge, never anyone else's.
    pub fn settle(self, outcome: Option<CredentialOutcome>) {
        match outcome {
            Some(CredentialOutcome::Rejected) => {}
            Some(CredentialOutcome::Accepted) | None => self.breaker.refund(self.window_start),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fail(breaker: &RemoteBreaker, times: u32, now: Instant) {
        for _ in 0..times {
            breaker
                .admit(now)
                .unwrap()
                .settle(Some(CredentialOutcome::Rejected));
        }
    }

    /// Goal: exactly `THRESHOLD` remote attempts are admitted per window, and the next one
    /// trips the breaker for the full cooldown.
    #[test]
    fn trips_after_the_threshold() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        fail(&breaker, THRESHOLD, now);
        assert_eq!(breaker.admit(now).unwrap_err(), COOLDOWN);
        assert_eq!(
            breaker.admit(now + Duration::from_secs(1)).unwrap_err(),
            COOLDOWN - Duration::from_secs(1)
        );
    }

    /// Goal: attempts still in flight count, so a concurrent burst from many peers cannot
    /// all slip past the check before any of them fails.
    #[test]
    fn in_flight_attempts_count() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        let held: Vec<_> = (0..THRESHOLD)
            .map(|_| breaker.admit(now).unwrap())
            .collect();
        assert!(breaker.admit(now).is_err());
        drop(held);
    }

    /// Goal: a success returns only its own charge. It neither resets the count mid-attack
    /// nor lifts a trip already in force.
    #[test]
    fn success_does_not_reset() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        fail(&breaker, THRESHOLD - 1, now);
        let in_flight = breaker.admit(now).unwrap();
        assert!(breaker.admit(now).is_err());
        in_flight.settle(Some(CredentialOutcome::Accepted));
        assert!(breaker.admit(now).is_err());
        assert_eq!(breaker.lock().charged, THRESHOLD - 1);
    }

    /// Goal: an attempt with no verdict gives its charge back.
    #[test]
    fn non_verdict_is_refunded() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        breaker.admit(now).unwrap().settle(None);
        assert_eq!(breaker.lock().charged, 0);
    }

    /// Goal: an abandoned attempt stays charged.
    #[test]
    fn dropped_charge_is_kept() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        drop(breaker.admit(now).unwrap());
        assert_eq!(breaker.lock().charged, 1);
    }

    /// Goal: the breaker reopens on its own after the cooldown, with a clean window.
    #[test]
    fn reopens_after_the_cooldown() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        fail(&breaker, THRESHOLD, now);
        assert!(breaker.admit(now).is_err());
        let later = now + COOLDOWN;
        assert!(breaker.admit(later).is_ok());
        assert_eq!(breaker.lock().charged, 1);
    }

    /// Goal: failures from an old window do not carry into the next one.
    #[test]
    fn a_new_window_starts_clean() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        fail(&breaker, THRESHOLD - 1, now);
        let later = now + WINDOW;
        fail(&breaker, THRESHOLD, later);
        assert!(breaker.admit(later).is_err());
    }

    /// Goal: a refund for a charge from a window that has rolled over leaves the new
    /// window's count alone.
    #[test]
    fn stale_refund_is_ignored() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        let old = breaker.admit(now).unwrap();
        let later = now + WINDOW;
        fail(&breaker, 3, later);
        old.settle(None);
        assert_eq!(breaker.lock().charged, 3);
    }

    /// Goal: a poisoned lock degrades to "breaker still works".
    #[test]
    fn poisoned_lock_still_serves() {
        let breaker = RemoteBreaker::new();
        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = breaker.lock();
            panic!("poison the breaker");
        }));
        assert!(poisoned.is_err());
        assert!(breaker.admit(Instant::now()).is_ok());
    }
}
