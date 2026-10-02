// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The remote password breaker: caps password attempts from all remote peers together,
//! against guessing spread across many addresses. Its threshold is well above one throttled
//! key's reach, so only a distributed attack trips it. Loopback and recently proven keys
//! bypass it, so the admin can still log in; tokens, 122 random bits, never touch it.

use crate::api::auth_throttle::{self, CredentialOutcome};
use crate::api::peer::PeerKey;
use log::warn;
use std::collections::HashMap;
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

/// Remote keys remembered after a password success. Bounded so the map cannot grow with
/// traffic; only a correct password adds to it, so an attacker cannot fill it.
const KNOWN_GOOD_CAPACITY: usize = 32;
/// Session cookies last a year, so a fresh login is rare. A month covers the admin's
/// usual networks without keeping a key that has since changed hands.
const KNOWN_GOOD_TTL: Duration = Duration::from_hours(30 * 24);

const _: () = assert!(KNOWN_GOOD_CAPACITY > 0);

#[derive(Debug)]
struct BreakerState {
    /// Start of the current counting window, `None` before the first attempt.
    window_start: Option<Instant>,
    /// Attempts charged in the current window: failures, plus attempts still in flight.
    charged: u32,
    tripped_until: Option<Instant>,
    /// Each known-good key and when it last proved the password. In memory only: a
    /// restart forgets them, which only costs a remote admin a wait during an attack.
    known_good: HashMap<PeerKey, Instant>,
}

/// Invariants: `charged` never exceeds `THRESHOLD`, as the breaker trips instead of
/// charging, and `known_good` never holds more than `KNOWN_GOOD_CAPACITY` keys.
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
                known_good: HashMap::with_capacity(KNOWN_GOOD_CAPACITY + 1),
            }),
        }
    }

    /// Charges a remote password attempt, or refuses it with the cooldown remaining.
    /// Charged on arrival, so a concurrent burst trips it rather than overshooting, which
    /// also bounds the hashes queued behind the auth actor.
    pub fn admit(&self, now: Instant) -> Result<BreakerCharge<'_>, Duration> {
        let mut state = self.lock();
        if let Some(remaining) = state.tripped_for(now) {
            return Err(remaining);
        }
        let window_start = state.roll_window(now);
        if state.charged < THRESHOLD {
            state.charged += 1;
            debug_assert!(state.charged <= THRESHOLD);
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

    /// Whether `peer` proved the password recently enough to bypass the breaker.
    pub fn is_known_good(&self, peer: PeerKey, now: Instant) -> bool {
        self.lock()
            .known_good
            .get(&peer)
            .is_some_and(|proved_at| now.duration_since(*proved_at) < KNOWN_GOOD_TTL)
    }

    /// Remembers a remote key that just proved the password. Expired keys go first, then
    /// the least recently proven if the set is still full.
    pub fn remember(&self, peer: PeerKey, now: Instant) {
        debug_assert!(
            peer != PeerKey::Loopback,
            "loopback never enters the breaker"
        );
        let mut state = self.lock();
        let known_good = &mut state.known_good;
        known_good.insert(peer, now);
        if known_good.len() > KNOWN_GOOD_CAPACITY {
            known_good.retain(|_, proved_at| now.duration_since(*proved_at) < KNOWN_GOOD_TTL);
        }
        while known_good.len() > KNOWN_GOOD_CAPACITY {
            let Some(oldest) = known_good
                .iter()
                .min_by_key(|(_, proved_at)| **proved_at)
                .map(|(key, _)| *key)
            else {
                break;
            };
            known_good.remove(&oldest);
        }
        assert!(known_good.len() <= KNOWN_GOOD_CAPACITY);
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
    /// attempt's own charge, never anyone else's: an admin login must not reopen the gate.
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
    use std::ops::Not;

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

    fn remote(last_octet: u8) -> PeerKey {
        PeerKey::from_ip(std::net::IpAddr::from([192, 0, 2, last_octet]))
    }

    /// Goal: a key is known-good after proving the password, and only until its entry
    /// expires. Unknown keys never are.
    #[test]
    fn known_good_lasts_for_its_ttl() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        assert!(breaker.is_known_good(remote(1), now).not());
        breaker.remember(remote(1), now);
        assert!(breaker.is_known_good(remote(1), now + KNOWN_GOOD_TTL - Duration::from_secs(1)));
        assert!(breaker.is_known_good(remote(1), now + KNOWN_GOOD_TTL).not());
        assert!(breaker.is_known_good(remote(2), now).not());
    }

    /// Goal: the set stays bounded however many keys prove the password, and eviction takes
    /// the least recently proven key, never the one just added.
    #[test]
    fn known_good_stays_bounded() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        for index in 0..100_u8 {
            breaker.remember(remote(index), now + Duration::from_secs(u64::from(index)));
        }
        let last = now + Duration::from_secs(99);
        assert!(breaker.lock().known_good.len() <= KNOWN_GOOD_CAPACITY);
        assert!(breaker.is_known_good(remote(99), last));
        assert!(breaker.is_known_good(remote(0), last).not());
    }

    /// Goal: proving the password again refreshes a key's entry rather than adding one.
    #[test]
    fn remembering_again_refreshes() {
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        breaker.remember(remote(1), now);
        let later = now + KNOWN_GOOD_TTL - Duration::from_secs(1);
        breaker.remember(remote(1), later);
        assert!(breaker.is_known_good(remote(1), later + Duration::from_secs(60)));
        assert_eq!(breaker.lock().known_good.len(), 1);
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
