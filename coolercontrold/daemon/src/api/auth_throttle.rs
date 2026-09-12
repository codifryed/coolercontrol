// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-peer backoff for failed authentication attempts.
//!
//! The daemon has no general rate limiter by design: the single-threaded sidecar and the
//! actor channels' backpressure already bound how fast external work reaches the main
//! loop. Authentication is the exception, because it runs upstream of every channel.
//! `AuthActor` already locks out password guessing, but globally rather than per peer,
//! and bearer-token failures were not throttled at all.
//!
//! Only requests that actually present credentials are counted. A 401 from an expired
//! session cookie is not a guessing attempt, costs no hashing, and counting it would let
//! a UI with a stale session lock its own user out of logging back in.
//!
//! Blocked peers are rejected immediately rather than delayed. On a single-threaded
//! reactor a sleeping request would stall every other client, handing an attacker the
//! outage the throttle exists to prevent.
//!
//! The outcome is never inferred from the response status. `/handshake` answers 200 with
//! or without an `Authorization` header, so a status-based reading would let an attacker
//! clear their own streak between guesses. Only a layer that actually adjudicated the
//! presented credentials marks the response, and only a marked response is counted.

use crate::api::CCError;
use axum::extract::{ConnectInfo, Request};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::ops::Not;
use std::sync::{LazyLock, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// Failures a peer may accumulate before backoff begins. Generous enough that a human
/// mistyping a password never notices it.
const FAILURE_THRESHOLD: u32 = 5;
const BASE_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(300);
/// A peer idle this long is forgotten, so an honest client always recovers on its own.
const ENTRY_TTL: Duration = Duration::from_mins(15);
/// Hard cap on tracked peers, so the throttle's own map cannot become the memory
/// exhaustion vector it exists to prevent.
const MAX_TRACKED_PEERS: usize = 1024;

const _: () = assert!(FAILURE_THRESHOLD > 0);
const _: () = assert!(MAX_TRACKED_PEERS > 0);

/// Process-wide throttle. The daemon presents one authentication surface no matter how
/// many listeners serve it, so per-router state would let a peer double its budget by
/// alternating between the IPv4 and IPv6 servers, which build their routers separately.
///
/// Deliberately a static rather than an actor handle, unlike `AuthActor` next door. This
/// runs in a `from_fn` middleware that carries no state, upstream of every channel, and
/// on the reject path it must answer without awaiting anything. The counters are two
/// integers and a timestamp behind an uncontended lock; a channel round trip to own them
/// would put a queue in front of the very path the throttle exists to keep cheap.
static AUTH_THROTTLE: LazyLock<AuthThrottle> = LazyLock::new(AuthThrottle::new);

#[derive(Debug)]
struct PeerFailures {
    count: u32,
    /// When the current backoff expires. `None` while still under the threshold.
    blocked_until: Option<Instant>,
    last_seen: Instant,
}

/// Invariant: `peers` never holds more than `MAX_TRACKED_PEERS` entries. `evict` runs
/// before every insert and leaves room for exactly one.
#[derive(Debug)]
pub struct AuthThrottle {
    peers: Mutex<HashMap<IpAddr, PeerFailures>>,
}

impl AuthThrottle {
    pub fn new() -> Self {
        Self {
            peers: Mutex::new(HashMap::with_capacity(MAX_TRACKED_PEERS)),
        }
    }

    /// Remaining backoff for `peer`, or `None` if it may attempt authentication now.
    ///
    /// A backoff that expires exactly at `now` counts as lapsed: reporting zero
    /// remaining would reject the request while telling the caller to retry immediately.
    pub fn blocked_for(&self, peer: IpAddr, now: Instant) -> Option<Duration> {
        let peers = self.lock();
        let entry = peers.get(&peer)?;
        let remaining = entry.blocked_until?.checked_duration_since(now)?;
        if remaining.is_zero() {
            return None;
        }
        Some(remaining)
    }

    pub fn record_failure(&self, peer: IpAddr, now: Instant) {
        let mut peers = self.lock();
        Self::evict(&mut peers, now);
        let entry = peers.entry(peer).or_insert(PeerFailures {
            count: 0,
            blocked_until: None,
            last_seen: now,
        });
        // A peer that went quiet long enough starts over rather than resuming a stale
        // streak from days ago.
        if now.duration_since(entry.last_seen) >= ENTRY_TTL {
            entry.count = 0;
            entry.blocked_until = None;
        }
        entry.count = entry.count.saturating_add(1);
        entry.last_seen = now;
        entry.blocked_until = backoff_for(entry.count).map(|backoff| now + backoff);
        debug_assert!(entry.count > 0);
        debug_assert!(peers.len() <= MAX_TRACKED_PEERS);
    }

    pub fn record_success(&self, peer: IpAddr) {
        self.lock().remove(&peer);
    }

    /// Reclaims space only under capacity pressure, leaving room for one insert.
    ///
    /// Expired entries are harmless until then: `blocked_for` already reads a lapsed
    /// backoff as unblocked, and `record_failure` resets a stale streak before counting.
    fn evict(peers: &mut HashMap<IpAddr, PeerFailures>, now: Instant) {
        if peers.len() < MAX_TRACKED_PEERS {
            return;
        }
        peers.retain(|_, entry| now.duration_since(entry.last_seen) < ENTRY_TTL);
        while peers.len() >= MAX_TRACKED_PEERS {
            let Some(oldest) = peers
                .iter()
                .min_by_key(|(_, entry)| entry.last_seen)
                .map(|(peer, _)| *peer)
            else {
                break;
            };
            peers.remove(&oldest);
        }
        assert!(peers.len() < MAX_TRACKED_PEERS);
    }

    /// A poisoned throttle must not take authentication down with it. The map holds only
    /// failure counters, so continuing with whatever state survived is strictly better
    /// than rejecting every subsequent request.
    fn lock(&self) -> MutexGuard<'_, HashMap<IpAddr, PeerFailures>> {
        self.peers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for AuthThrottle {
    fn default() -> Self {
        Self::new()
    }
}

/// Backoff owed after `count` consecutive failures, or `None` while under the threshold.
/// Doubles per failure past the threshold and saturates at `MAX_BACKOFF`.
fn backoff_for(count: u32) -> Option<Duration> {
    let over_threshold = count.checked_sub(FAILURE_THRESHOLD)?;
    if over_threshold == 0 {
        return None;
    }
    // Cap the shift before it can overflow the multiplier. `MAX_BACKOFF` clamps the
    // result long before the cap is reachable in practice.
    let shift = (over_threshold - 1).min(u32::BITS - 1);
    let backoff = BASE_BACKOFF.saturating_mul(1_u32 << shift);
    Some(backoff.min(MAX_BACKOFF))
}

/// The peer address as the kernel reports it.
///
/// `X-Forwarded-For` is deliberately ignored: it is attacker-controlled unless every hop
/// is trusted, and honouring it would let one peer spend another's budget. Behind a
/// reverse proxy this collapses to the proxy's own address, throttling all proxied
/// clients together, which is the safe direction to fail.
fn peer_ip(request: &Request) -> Option<IpAddr> {
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| address.ip())
}

fn presents_credentials(request: &Request) -> bool {
    request.headers().contains_key(header::AUTHORIZATION)
}

/// A verdict on credentials a request actually presented, attached to the response by
/// the layer that checked them. An unmarked response is not a guessing signal: it was
/// produced without the credentials ever being adjudicated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialOutcome {
    /// The credentials authenticated. Scope is a separate question: a valid token
    /// refused for insufficient scope still authenticated.
    Accepted,
    Rejected,
}

/// Records a verdict on the response, for the throttle to read once it unwinds.
pub fn mark(mut response: Response, outcome: CredentialOutcome) -> Response {
    response.extensions_mut().insert(outcome);
    response
}

/// Verdict for a route whose handler consumes credentials directly rather than behind an
/// auth layer, such as `/login`.
///
/// A 429 here is `AuthActor`'s global password lockout, never this middleware's own
/// rejection: that one short-circuits above and never reaches a handler. Counting it
/// keeps the per-peer backoff growing behind the global lockout instead of freezing.
fn outcome_for(status: StatusCode) -> Option<CredentialOutcome> {
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::TOO_MANY_REQUESTS {
        Some(CredentialOutcome::Rejected)
    } else if status.is_success() {
        Some(CredentialOutcome::Accepted)
    } else {
        None
    }
}

/// Marks responses from a route that adjudicates credentials in its handler.
pub async fn credential_route_middleware(request: Request, next: Next) -> Response {
    let response = next.run(request).await;
    match outcome_for(response.status()) {
        Some(outcome) => mark(response, outcome),
        None => response,
    }
}

/// Applies a response's verdict, if it carries one, to the peer that produced it.
fn record(throttle: &AuthThrottle, peer: IpAddr, response: &Response, now: Instant) {
    match response.extensions().get::<CredentialOutcome>() {
        Some(CredentialOutcome::Rejected) => throttle.record_failure(peer, now),
        Some(CredentialOutcome::Accepted) => throttle.record_success(peer),
        None => {}
    }
}

/// Rejects peers in backoff before any credential work runs, then records the outcome of
/// everything downstream.
pub async fn throttle_middleware(request: Request, next: Next) -> Response {
    if presents_credentials(&request).not() {
        return next.run(request).await;
    }
    let Some(peer) = peer_ip(&request) else {
        return next.run(request).await;
    };
    if let Some(remaining) = AUTH_THROTTLE.blocked_for(peer, Instant::now()) {
        return CCError::TooManyAttempts {
            msg: format!(
                "Too many failed authentication attempts. Try again in {}s.",
                remaining.as_secs().saturating_add(1)
            ),
        }
        .into_response();
    }
    let response = next.run(request).await;
    record(&AUTH_THROTTLE, peer, &response, Instant::now());
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(last_octet: u8) -> IpAddr {
        IpAddr::from([127, 0, 0, last_octet])
    }

    fn request_with(header_value: Option<&str>, connect_info: Option<SocketAddr>) -> Request {
        let mut builder = Request::builder().uri("/devices");
        if let Some(value) = header_value {
            builder = builder.header(header::AUTHORIZATION, value);
        }
        let mut request = builder.body(axum::body::Body::empty()).unwrap();
        if let Some(address) = connect_info {
            request.extensions_mut().insert(ConnectInfo(address));
        }
        request
    }

    /// Goal: only requests that actually present credentials are throttled, so a UI
    /// polling with an expired session cookie cannot lock its own user out.
    #[test]
    fn only_credentialed_requests_are_throttled() {
        assert!(presents_credentials(&request_with(
            Some("Bearer cc_x"),
            None
        )));
        assert!(presents_credentials(&request_with(Some("Basic abc"), None)));
        assert!(presents_credentials(&request_with(None, None)).not());
    }

    /// Goal: the peer key comes from the kernel-reported address, and is absent rather
    /// than guessed when the server was built without connect info.
    #[test]
    fn peer_ip_reads_connect_info_only() {
        let address = SocketAddr::from(([192, 168, 1, 50], 40000));
        assert_eq!(
            peer_ip(&request_with(Some("Bearer cc_x"), Some(address))),
            Some(IpAddr::from([192, 168, 1, 50]))
        );
        assert_eq!(peer_ip(&request_with(Some("Bearer cc_x"), None)), None);
    }

    /// Goal: `X-Forwarded-For` must never become the key, since a peer could then spend
    /// another client's budget or dodge its own.
    #[test]
    fn forwarded_for_header_is_ignored() {
        let address = SocketAddr::from(([10, 0, 0, 1], 40000));
        let mut request = request_with(Some("Bearer cc_x"), Some(address));
        request
            .headers_mut()
            .insert("x-forwarded-for", "203.0.113.9".parse().unwrap());
        assert_eq!(peer_ip(&request), Some(IpAddr::from([10, 0, 0, 1])));
    }

    /// Goal: on a credential-consuming route, a rejection and an acceptance are both
    /// recognised, and a server fault counts as neither.
    #[test]
    fn credential_route_outcomes_follow_the_status() {
        assert_eq!(
            outcome_for(StatusCode::UNAUTHORIZED),
            Some(CredentialOutcome::Rejected)
        );
        assert_eq!(
            outcome_for(StatusCode::OK),
            Some(CredentialOutcome::Accepted)
        );
        assert_eq!(
            outcome_for(StatusCode::NO_CONTENT),
            Some(CredentialOutcome::Accepted)
        );
        assert_eq!(outcome_for(StatusCode::INTERNAL_SERVER_ERROR), None);
    }

    /// Goal: the global password lockout keeps the per-peer backoff growing. Its 429 is
    /// the only 429 a handler can produce, since the throttle's own never reaches one.
    #[test]
    fn global_lockout_still_counts_against_the_peer() {
        assert_eq!(
            outcome_for(StatusCode::TOO_MANY_REQUESTS),
            Some(CredentialOutcome::Rejected)
        );
    }

    /// Goal: an unmarked response never moves the counter. `/handshake` answers 200
    /// whatever the `Authorization` header holds, so reading its status as an acceptance
    /// would let an attacker clear their streak between guesses.
    #[test]
    fn unmarked_responses_are_not_counted() {
        let ok = StatusCode::OK.into_response();
        assert_eq!(ok.extensions().get::<CredentialOutcome>(), None);

        let marked = mark(StatusCode::OK.into_response(), CredentialOutcome::Accepted);
        assert_eq!(
            marked.extensions().get::<CredentialOutcome>(),
            Some(&CredentialOutcome::Accepted)
        );
    }

    /// Goal: a 403 for an under-scoped but valid token clears the streak rather than
    /// counting, since the credential itself authenticated.
    #[test]
    fn insufficient_scope_is_an_acceptance() {
        let response = mark(
            CCError::InsufficientScope {
                msg: "no write access".to_string(),
            }
            .into_response(),
            CredentialOutcome::Accepted,
        );
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(
            response.extensions().get::<CredentialOutcome>(),
            Some(&CredentialOutcome::Accepted)
        );
    }

    /// Goal: the first `FAILURE_THRESHOLD` failures are free, so an honest client that
    /// mistypes a password a few times is never delayed.
    #[test]
    fn failures_under_the_threshold_do_not_block() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        assert_eq!(throttle.blocked_for(peer(1), now), None);
    }

    /// Goal: the failure past the threshold starts backoff at the base duration.
    #[test]
    fn first_failure_past_the_threshold_blocks() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        assert_eq!(throttle.blocked_for(peer(1), now), Some(BASE_BACKOFF));
    }

    /// Goal: backoff doubles per further failure and saturates, so a persistent guesser
    /// is silenced quickly without the duration ever overflowing.
    #[test]
    fn backoff_doubles_then_saturates() {
        assert_eq!(backoff_for(FAILURE_THRESHOLD), None);
        assert_eq!(backoff_for(FAILURE_THRESHOLD + 1), Some(BASE_BACKOFF));
        assert_eq!(backoff_for(FAILURE_THRESHOLD + 2), Some(BASE_BACKOFF * 2));
        assert_eq!(backoff_for(FAILURE_THRESHOLD + 3), Some(BASE_BACKOFF * 4));
        assert_eq!(backoff_for(u32::MAX), Some(MAX_BACKOFF));
        assert!(backoff_for(0).is_none());
    }

    /// Goal: the block lapses on its own, so a throttled peer is never locked out
    /// permanently.
    #[test]
    fn block_expires_after_its_backoff() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        assert!(throttle.blocked_for(peer(1), now).is_some());
        assert_eq!(throttle.blocked_for(peer(1), now + BASE_BACKOFF), None);
    }

    /// Goal: authenticating successfully clears the streak immediately.
    #[test]
    fn success_clears_the_streak() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        throttle.record_success(peer(1));
        assert_eq!(throttle.blocked_for(peer(1), now), None);
    }

    /// Goal: one peer's failures never throttle another, which is the whole reason this
    /// is keyed per peer rather than global.
    #[test]
    fn peers_are_tracked_independently() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        assert!(throttle.blocked_for(peer(1), now).is_some());
        assert_eq!(throttle.blocked_for(peer(2), now), None);
    }

    /// Goal: a stale streak does not resume days later and block an honest client on its
    /// first attempt.
    #[test]
    fn stale_streak_resets_after_the_entry_ttl() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        let later = now + ENTRY_TTL;
        throttle.record_failure(peer(1), later);
        assert_eq!(throttle.blocked_for(peer(1), later), None);
    }

    /// Goal: the map stays bounded no matter how many distinct peers attack, so the
    /// throttle cannot be turned into the memory vector it guards against.
    #[test]
    fn tracked_peers_stay_bounded() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for index in 0..(MAX_TRACKED_PEERS * 2) {
            let octets = u32::try_from(index).unwrap().to_be_bytes();
            throttle.record_failure(IpAddr::from(octets), now);
        }
        assert!(throttle.lock().len() <= MAX_TRACKED_PEERS);
    }

    /// Goal: eviction under pressure never drops the peer being recorded, so an attacker
    /// cannot clear their own streak by flooding the map with fresh addresses.
    #[test]
    fn eviction_keeps_the_peer_being_recorded() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for index in 0..(MAX_TRACKED_PEERS * 2) {
            let octets = u32::try_from(index).unwrap().to_be_bytes();
            throttle.record_failure(IpAddr::from(octets), now);
        }
        let last = IpAddr::from(
            u32::try_from(MAX_TRACKED_PEERS * 2 - 1)
                .unwrap()
                .to_be_bytes(),
        );
        assert!(throttle.lock().contains_key(&last));
    }

    /// Goal: an open route cannot be used to clear a streak. `/handshake` answers 200
    /// whatever the `Authorization` header holds, so a peer mid-backoff must stay blocked
    /// across any number of them.
    #[test]
    fn unadjudicated_success_does_not_clear_the_streak() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        let blocked = throttle.blocked_for(peer(1), now);
        assert!(blocked.is_some());

        for _ in 0..10 {
            record(&throttle, peer(1), &StatusCode::OK.into_response(), now);
        }
        assert_eq!(throttle.blocked_for(peer(1), now), blocked);
    }

    /// Goal: an adjudicated success still clears the streak, so the guard above does not
    /// leave an honest client throttled after it authenticates.
    #[test]
    fn adjudicated_success_clears_the_streak() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        assert!(throttle.blocked_for(peer(1), now).is_some());

        let response = mark(StatusCode::OK.into_response(), CredentialOutcome::Accepted);
        record(&throttle, peer(1), &response, now);
        assert_eq!(throttle.blocked_for(peer(1), now), None);
    }

    /// Goal: a rejection recorded through the same seam still counts, so the backoff is
    /// driven by the marker rather than by the status the throttle used to read.
    #[test]
    fn adjudicated_rejection_counts() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        let response = mark(
            StatusCode::UNAUTHORIZED.into_response(),
            CredentialOutcome::Rejected,
        );
        for _ in 0..=FAILURE_THRESHOLD {
            record(&throttle, peer(1), &response, now);
        }
        assert_eq!(throttle.blocked_for(peer(1), now), Some(BASE_BACKOFF));
    }

    /// Goal: a poisoned mutex degrades to "throttle still works" rather than taking
    /// every subsequent authentication down with it.
    #[test]
    fn poisoned_lock_still_serves() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        throttle.record_failure(peer(1), now);

        let poisoned = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = throttle.lock();
            panic!("poison the throttle");
        }));
        assert!(poisoned.is_err());

        throttle.record_failure(peer(1), now);
        assert_eq!(
            throttle.lock().get(&peer(1)).map(|entry| entry.count),
            Some(2)
        );
    }
}
