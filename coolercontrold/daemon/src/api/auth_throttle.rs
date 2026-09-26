// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Per-peer backoff for failed authentication attempts, the one rate limit the daemon has:
//! authentication runs upstream of the actor channels whose backpressure bounds the rest.
//! Only adjudicated credentials count. Guessing spread across many peers is capped
//! separately, by `auth_breaker`.

use crate::api::auth_breaker::{BreakerCharge, RemoteBreaker};
use crate::api::peer::{ClientAddr, PeerKey};
use crate::api::CCError;
use axum::extract::{ConnectInfo, Request};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::collections::HashMap;
use std::net::SocketAddr;
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
// `max_attempts_within` terminates only if each backoff past the threshold advances time.
const _: () = assert!(BASE_BACKOFF.as_nanos() > 0);
const _: () = assert!(MAX_TRACKED_PEERS > 0);

/// Process-wide, so a peer cannot double its budget across the IPv4 and IPv6 listeners.
/// Statics, not an actor: the reject path must answer without awaiting a channel.
/// Passwords and tokens are budgeted apart: with one budget a valid token would clear a
/// password streak, and password failures would refuse the desktop app's token traffic.
static TOKEN_THROTTLE: LazyLock<AuthThrottle> = LazyLock::new(AuthThrottle::new);
static PASSWORD_THROTTLE: LazyLock<AuthThrottle> = LazyLock::new(AuthThrottle::new);
static REMOTE_BREAKER: LazyLock<RemoteBreaker> = LazyLock::new(RemoteBreaker::new);

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
    peers: Mutex<HashMap<PeerKey, PeerFailures>>,
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
    pub fn blocked_for(&self, peer: PeerKey, now: Instant) -> Option<Duration> {
        Self::remaining(&self.lock(), peer, now)
    }

    /// Admits an attempt and charges it as a failure at once, or refuses it with the
    /// backoff remaining.
    ///
    /// Charging on arrival rather than on the verdict closes two gaps. Concurrent requests
    /// would all pass a check made before any of them finished, and a request whose future
    /// is dropped, by a client reset or the timeout layer, never reaches a verdict at all
    /// while its hash still runs. The returned `Attempt` settles the charge.
    pub fn admit(&self, peer: PeerKey, now: Instant) -> Result<Attempt<'_>, Duration> {
        let mut peers = self.lock();
        if let Some(remaining) = Self::remaining(&peers, peer, now) {
            return Err(remaining);
        }
        Self::charge(&mut peers, peer, now);
        Ok(Attempt {
            throttle: self,
            peer,
        })
    }

    pub fn record_failure(&self, peer: PeerKey, now: Instant) {
        Self::charge(&mut self.lock(), peer, now);
    }

    pub fn record_success(&self, peer: PeerKey) {
        self.lock().remove(&peer);
    }

    /// Takes back one charge for an attempt that reached no verdict.
    ///
    /// Never extends a block: the refunded charge may be the one that set it, and its
    /// replacement deadline is measured from now rather than from the charge.
    fn refund(&self, peer: PeerKey, now: Instant) {
        let mut peers = self.lock();
        // Absent when eviction reclaimed the entry while the attempt was in flight.
        let Some(entry) = peers.get_mut(&peer) else {
            return;
        };
        entry.count = entry.count.saturating_sub(1);
        entry.blocked_until = match (entry.blocked_until, backoff_for(entry.count)) {
            (Some(until), Some(backoff)) => Some(until.min(now + backoff)),
            _ => None,
        };
        if entry.count == 0 {
            peers.remove(&peer);
        }
    }

    fn remaining(
        peers: &HashMap<PeerKey, PeerFailures>,
        peer: PeerKey,
        now: Instant,
    ) -> Option<Duration> {
        let entry = peers.get(&peer)?;
        let remaining = entry.blocked_until?.checked_duration_since(now)?;
        if remaining.is_zero() {
            return None;
        }
        Some(remaining)
    }

    fn charge(peers: &mut HashMap<PeerKey, PeerFailures>, peer: PeerKey, now: Instant) {
        Self::evict(peers, now);
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

    /// Reclaims space only under capacity pressure, leaving room for one insert.
    ///
    /// Expired entries are harmless until then: `blocked_for` already reads a lapsed
    /// backoff as unblocked, and `record_failure` resets a stale streak before counting.
    fn evict(peers: &mut HashMap<PeerKey, PeerFailures>, now: Instant) {
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
    fn lock(&self) -> MutexGuard<'_, HashMap<PeerKey, PeerFailures>> {
        self.peers.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Default for AuthThrottle {
    fn default() -> Self {
        Self::new()
    }
}

/// A charged attempt, settled once its verdict is known. Dropping it unsettled keeps the
/// charge: a request abandoned mid-flight still spent a hash.
#[derive(Debug)]
#[must_use]
pub struct Attempt<'a> {
    throttle: &'a AuthThrottle,
    peer: PeerKey,
}

impl Attempt<'_> {
    /// A rejection keeps the charge already taken, a success clears the streak, and no
    /// verdict at all refunds it.
    pub fn settle(self, outcome: Option<CredentialOutcome>, now: Instant) {
        match outcome {
            Some(CredentialOutcome::Rejected) => {}
            Some(CredentialOutcome::Accepted) => self.throttle.record_success(self.peer),
            None => self.throttle.refund(self.peer, now),
        }
    }
}

/// Backoff owed after `count` consecutive failures, or `None` while under the threshold.
/// Doubles per failure past the threshold and saturates at `MAX_BACKOFF`. `const` so the
/// breaker can size its threshold against it at compile time.
const fn backoff_for(count: u32) -> Option<Duration> {
    let Some(over_threshold) = count.checked_sub(FAILURE_THRESHOLD) else {
        return None;
    };
    if over_threshold == 0 {
        return None;
    }
    // Cap the shift before it can overflow the multiplier. `MAX_BACKOFF` clamps the
    // result long before the cap is reachable in practice.
    let shift = if over_threshold - 1 < u32::BITS - 1 {
        over_threshold - 1
    } else {
        u32::BITS - 1
    };
    let backoff = BASE_BACKOFF.saturating_mul(1_u32 << shift);
    if backoff.as_nanos() < MAX_BACKOFF.as_nanos() {
        Some(backoff)
    } else {
        Some(MAX_BACKOFF)
    }
}

/// The most rejected attempts one key fits in `window`, retrying the moment each backoff
/// lapses. Bounded: past the threshold every attempt adds at least `BASE_BACKOFF`.
pub const fn max_attempts_within(window: Duration) -> u32 {
    let mut elapsed = Duration::ZERO;
    let mut attempts = 0_u32;
    while elapsed.as_nanos() < window.as_nanos() {
        attempts += 1;
        if let Some(backoff) = backoff_for(attempts) {
            elapsed = elapsed.saturating_add(backoff);
        }
    }
    attempts
}

/// The client, keyed by network. See `PeerKey`.
///
/// `X-Forwarded-For` is honoured only through `ClientAddr`, which reads it solely from a
/// configured trusted proxy: from anyone else it is attacker-controlled, and would let one
/// peer spend another's budget. Behind an unlisted proxy this collapses to the proxy's own
/// address, throttling its clients together, which is the safe direction to fail.
fn peer_key(request: &Request) -> Option<PeerKey> {
    if let Some(ClientAddr(address)) = request.extensions().get::<ClientAddr>() {
        return Some(PeerKey::from_ip(*address));
    }
    request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(address)| PeerKey::from_ip(address.ip()))
}

/// A 401 for a stale session cookie presented nothing, so it is not a guess, and counting
/// it would lock a stale UI out of logging back in.
fn presents_credentials(request: &Request) -> bool {
    request.headers().contains_key(header::AUTHORIZATION)
}

/// Matches the scheme `auth::bearer_token` accepts, so the throttle gates exactly the
/// requests that reach token validation.
fn presents_bearer_token(request: &Request) -> bool {
    request
        .headers()
        .get(header::AUTHORIZATION)
        .is_some_and(|value| value.as_bytes().starts_with(b"Bearer "))
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
/// auth layer, such as `/login`. Any other status, such as `/set-passwd` refusing the
/// default password as the new one, says nothing about the password presented.
fn outcome_for(status: StatusCode) -> Option<CredentialOutcome> {
    if status == StatusCode::UNAUTHORIZED {
        Some(CredentialOutcome::Rejected)
    } else if status.is_success() {
        Some(CredentialOutcome::Accepted)
    } else {
        None
    }
}

/// Throttles a route whose handler checks the admin password: `/login`, and `/set-passwd`
/// inside its session check.
///
/// Scoped to these routes rather than to the `Basic` scheme. A reverse proxy doing HTTP
/// Basic auth forwards `Authorization: Basic` on every request, and none of those guess
/// the daemon's password. A request with no `Authorization` at all is refused by the
/// extractor before any hashing, so it is not an attempt either.
pub async fn password_throttle_middleware(request: Request, next: Next) -> Response {
    if presents_credentials(&request).not() {
        return next.run(request).await;
    }
    let Some(peer) = peer_key(&request) else {
        return next.run(request).await;
    };
    let admission =
        match PasswordAdmission::admit(&PASSWORD_THROTTLE, &REMOTE_BREAKER, peer, Instant::now()) {
            Ok(admission) => admission,
            Err(remaining) => return too_many_attempts(remaining),
        };
    let response = next.run(request).await;
    admission.settle(outcome_for(response.status()), Instant::now());
    response
}

/// Everything a password attempt is charged to: its peer's budget, and the remote breaker
/// unless it comes from this host or from a key that recently proved the password.
#[derive(Debug)]
#[must_use]
pub struct PasswordAdmission<'a> {
    attempt: Attempt<'a>,
    breaker: &'a RemoteBreaker,
    breaker_charge: Option<BreakerCharge<'a>>,
    peer: PeerKey,
}

impl<'a> PasswordAdmission<'a> {
    /// The peer's own budget is checked first, so a peer already in backoff is turned
    /// away without spending the breaker. A breaker refusal reaches no verdict, so it
    /// refunds the peer's charge rather than counting against the peer.
    pub fn admit(
        throttle: &'a AuthThrottle,
        breaker: &'a RemoteBreaker,
        peer: PeerKey,
        now: Instant,
    ) -> Result<Self, Duration> {
        let attempt = throttle.admit(peer, now)?;
        if Self::skips_breaker(breaker, peer, now) {
            return Ok(Self {
                attempt,
                breaker,
                breaker_charge: None,
                peer,
            });
        }
        match breaker.admit(now) {
            Ok(charge) => Ok(Self {
                attempt,
                breaker,
                breaker_charge: Some(charge),
                peer,
            }),
            Err(remaining) => {
                attempt.settle(None, now);
                Err(remaining)
            }
        }
    }

    /// Loopback never enters the breaker, and a known-good key bypasses it.
    fn skips_breaker(breaker: &RemoteBreaker, peer: PeerKey, now: Instant) -> bool {
        if peer == PeerKey::Loopback {
            return true;
        }
        breaker.is_known_good(peer, now)
    }

    /// A remote success also makes its key known-good.
    pub fn settle(self, outcome: Option<CredentialOutcome>, now: Instant) {
        self.attempt.settle(outcome, now);
        if let Some(charge) = self.breaker_charge {
            charge.settle(outcome);
        }
        if outcome == Some(CredentialOutcome::Accepted) {
            // Loopback is always exempt, so it has nothing to be remembered for.
            if self.peer != PeerKey::Loopback {
                self.breaker.remember(self.peer, now);
            }
        }
    }
}

/// Rejects peers in token backoff before validation runs, then records the verdict the
/// auth layers marked on the response.
pub async fn token_throttle_middleware(request: Request, next: Next) -> Response {
    if presents_bearer_token(&request).not() {
        return next.run(request).await;
    }
    let Some(peer) = peer_key(&request) else {
        return next.run(request).await;
    };
    if let Some(remaining) = TOKEN_THROTTLE.blocked_for(peer, Instant::now()) {
        return too_many_attempts(remaining);
    }
    let response = next.run(request).await;
    record(&TOKEN_THROTTLE, peer, &response, Instant::now());
    response
}

/// Password failures on record for `peer`, for tests of the router's wiring.
#[cfg(test)]
pub fn password_failures(peer: std::net::IpAddr) -> Option<u32> {
    let key = PeerKey::from_ip(peer);
    PASSWORD_THROTTLE.lock().get(&key).map(|entry| entry.count)
}

/// Applies a response's verdict, if it carries one, to the peer that produced it. Never the
/// status: `/handshake` answers 200 with any token, which would let a guesser clear its streak.
fn record(throttle: &AuthThrottle, peer: PeerKey, response: &Response, now: Instant) {
    match response.extensions().get::<CredentialOutcome>() {
        Some(CredentialOutcome::Rejected) => throttle.record_failure(peer, now),
        Some(CredentialOutcome::Accepted) => throttle.record_success(peer),
        None => {}
    }
}

/// Refused at once, never delayed: a sleeping request would stall the single-threaded reactor.
fn too_many_attempts(remaining: Duration) -> Response {
    debug_assert!(remaining.is_zero().not());
    CCError::TooManyAttempts {
        msg: format!(
            "Too many failed authentication attempts. Try again in {}s.",
            remaining.as_secs().saturating_add(1)
        ),
    }
    .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::auth_breaker;
    use std::net::IpAddr;

    /// TEST-NET-1 addresses: distinct keys, unlike 127/8, which is all one loopback key.
    fn peer(last_octet: u8) -> PeerKey {
        PeerKey::from_ip(IpAddr::from([192, 0, 2, last_octet]))
    }

    fn key_for(index: usize) -> PeerKey {
        let octets = u32::try_from(index).unwrap().to_be_bytes();
        PeerKey::from_ip(IpAddr::from(octets))
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

    /// Goal: the token budget gates exactly the requests that reach token validation. A
    /// `Basic` header is not a token, whatever route it lands on.
    #[test]
    fn only_bearer_requests_are_token_throttled() {
        assert!(presents_bearer_token(&request_with(
            Some("Bearer cc_x"),
            None
        )));
        assert!(presents_bearer_token(&request_with(Some("Basic abc"), None)).not());
        assert!(presents_bearer_token(&request_with(Some("bearer cc_x"), None)).not());
        assert!(presents_bearer_token(&request_with(None, None)).not());
    }

    /// Both middlewares wired as the router wires them, around stub handlers standing in for
    /// a password route and for routes whose auth layer marks a token verdict.
    fn wired_app() -> axum::Router {
        use axum::middleware::from_fn;
        use axum::routing::{get, post};
        let token_verdict = |status: StatusCode, outcome| mark(status.into_response(), outcome);
        axum::Router::new()
            .route(
                "/login",
                // Yields once, like a real hash, so concurrent calls are all in flight
                // before any of them settles.
                post(|| async {
                    tokio::task::yield_now().await;
                    StatusCode::UNAUTHORIZED
                })
                .layer(from_fn(password_throttle_middleware)),
            )
            .route(
                "/token-ok",
                get(move || async move {
                    token_verdict(StatusCode::OK, CredentialOutcome::Accepted)
                }),
            )
            .route(
                "/token-bad",
                get(move || async move {
                    token_verdict(StatusCode::UNAUTHORIZED, CredentialOutcome::Rejected)
                }),
            )
            .route("/other", get(|| async { StatusCode::UNAUTHORIZED }))
            .layer(from_fn(token_throttle_middleware))
    }

    /// The statics are process-wide, so each wiring test owns one TEST-NET-2 address. The
    /// remote breaker is shared too: wiring tests together must stay well under its
    /// threshold, or they would start refusing each other.
    fn wired_peer(last_octet: u8) -> SocketAddr {
        SocketAddr::from(([198, 51, 100, last_octet], 40000))
    }

    async fn call(
        method: &str,
        uri: &str,
        authorization: Option<&str>,
        peer: SocketAddr,
    ) -> StatusCode {
        let mut request = request_with(authorization, Some(peer));
        *request.method_mut() = method.parse().unwrap();
        *request.uri_mut() = uri.parse().unwrap();
        tower::ServiceExt::oneshot(wired_app(), request)
            .await
            .unwrap()
            .status()
    }

    fn failures(throttle: &AuthThrottle, peer: SocketAddr) -> Option<u32> {
        let key = PeerKey::from_ip(peer.ip());
        throttle.lock().get(&key).map(|entry| entry.count)
    }

    /// Goal: password failures block further password attempts but never a valid token from
    /// the same peer, and that token's success leaves the password streak intact.
    #[tokio::test]
    async fn password_failures_leave_token_requests_alone() {
        let peer = wired_peer(10);
        for _ in 0..=FAILURE_THRESHOLD {
            assert_eq!(
                call("POST", "/login", Some("Basic abc"), peer).await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            call("POST", "/login", Some("Basic abc"), peer).await,
            StatusCode::TOO_MANY_REQUESTS
        );

        assert_eq!(
            call("GET", "/token-ok", Some("Bearer cc_x"), peer).await,
            StatusCode::OK
        );
        assert_eq!(
            failures(&PASSWORD_THROTTLE, peer),
            Some(FAILURE_THRESHOLD + 1)
        );
        assert_eq!(failures(&TOKEN_THROTTLE, peer), None);
    }

    /// Goal: token failures are throttled on their own budget, which leaves the peer free to
    /// log in with a password.
    #[tokio::test]
    async fn token_failures_leave_password_attempts_alone() {
        let peer = wired_peer(11);
        for _ in 0..=FAILURE_THRESHOLD {
            assert_eq!(
                call("GET", "/token-bad", Some("Bearer cc_x"), peer).await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(
            call("GET", "/token-bad", Some("Bearer cc_x"), peer).await,
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            call("POST", "/login", Some("Basic abc"), peer).await,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(failures(&PASSWORD_THROTTLE, peer), Some(1));
    }

    /// Goal: a `Basic` header on a route that checks no password is neither counted nor
    /// blocked, so a proxy doing HTTP Basic auth cannot throttle the UI behind it.
    #[tokio::test]
    async fn basic_on_other_routes_is_not_a_password_attempt() {
        let peer = wired_peer(12);
        for _ in 0..(FAILURE_THRESHOLD * 2) {
            assert_eq!(
                call("GET", "/other", Some("Basic abc"), peer).await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(failures(&PASSWORD_THROTTLE, peer), None);
        assert_eq!(failures(&TOKEN_THROTTLE, peer), None);
    }

    /// Goal: a password route called with no credentials at all is not an attempt, since the
    /// extractor refuses it before any hashing.
    #[tokio::test]
    async fn password_route_without_credentials_is_not_counted() {
        let peer = wired_peer(13);
        for _ in 0..(FAILURE_THRESHOLD * 2) {
            assert_eq!(
                call("POST", "/login", None, peer).await,
                StatusCode::UNAUTHORIZED
            );
        }
        assert_eq!(failures(&PASSWORD_THROTTLE, peer), None);
    }

    /// Goal: the peer key comes from the kernel-reported address, and is absent rather
    /// than guessed when the server was built without connect info.
    #[test]
    fn peer_key_reads_connect_info_only() {
        let address = SocketAddr::from(([192, 168, 1, 50], 40000));
        assert_eq!(
            peer_key(&request_with(Some("Bearer cc_x"), Some(address))),
            Some(PeerKey::from_ip(IpAddr::from([192, 168, 1, 50])))
        );
        assert_eq!(peer_key(&request_with(Some("Bearer cc_x"), None)), None);
    }

    /// Goal: a client resolved behind a trusted proxy is the key, not the proxy the TCP
    /// connection came from.
    #[test]
    fn resolved_client_address_is_the_key() {
        let proxy = SocketAddr::from(([127, 0, 0, 1], 40000));
        let mut request = request_with(Some("Bearer cc_x"), Some(proxy));
        let client = IpAddr::from([203, 0, 113, 9]);
        request.extensions_mut().insert(ClientAddr(client));
        assert_eq!(peer_key(&request), Some(PeerKey::from_ip(client)));
    }

    /// Goal: two addresses in one IPv6 /64 spend one budget, so a host cannot dodge its
    /// backoff by walking its own prefix.
    #[test]
    fn one_ipv6_network_shares_a_budget() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        let first = PeerKey::from_ip("2001:db8::1".parse().unwrap());
        let second = PeerKey::from_ip("2001:db8::ffff".parse().unwrap());
        for _ in 0..=FAILURE_THRESHOLD {
            throttle.record_failure(first, now);
        }
        assert_eq!(throttle.blocked_for(second, now), Some(BASE_BACKOFF));
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
        assert_eq!(
            peer_key(&request),
            Some(PeerKey::from_ip(IpAddr::from([10, 0, 0, 1])))
        );
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

    /// Goal: a 429 is no verdict. No handler produces one any more, and one arriving from
    /// elsewhere says nothing about the password.
    #[test]
    fn too_many_requests_is_not_a_verdict() {
        assert_eq!(outcome_for(StatusCode::TOO_MANY_REQUESTS), None);
        assert_eq!(outcome_for(StatusCode::BAD_REQUEST), None);
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
            throttle.record_failure(key_for(index), now);
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
            throttle.record_failure(key_for(index), now);
        }
        let last = key_for(MAX_TRACKED_PEERS * 2 - 1);
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

    /// Goal: concurrent attempts are charged as they arrive, so a burst gets exactly the
    /// free allowance plus the one that trips the backoff, however many are in flight.
    /// Method: admissions held unsettled, as they would be while their hashes run.
    #[test]
    fn unsettled_admissions_are_capped() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        let attempts: Vec<_> = (0..10).map(|_| throttle.admit(peer(1), now)).collect();
        let admitted = attempts.iter().filter(|attempt| attempt.is_ok()).count();
        assert_eq!(admitted, usize::try_from(FAILURE_THRESHOLD + 1).unwrap());
        assert!(attempts.last().unwrap().is_err());
    }

    /// Goal: an attempt dropped before its verdict, as by a client reset or the timeout
    /// layer, stays charged.
    #[test]
    fn dropped_attempt_stays_charged() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        drop(throttle.admit(peer(1), now).unwrap());
        assert_eq!(
            throttle.lock().get(&peer(1)).map(|entry| entry.count),
            Some(1)
        );
    }

    /// Goal: a rejection keeps its charge, so settled failures accumulate into a backoff
    /// exactly as recorded failures do.
    #[test]
    fn rejected_attempts_build_the_backoff() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            let attempt = throttle.admit(peer(1), now).unwrap();
            attempt.settle(Some(CredentialOutcome::Rejected), now);
        }
        assert_eq!(throttle.blocked_for(peer(1), now), Some(BASE_BACKOFF));
    }

    /// Goal: an accepted attempt clears the streak, including charges still in flight.
    #[test]
    fn accepted_attempt_clears_the_streak() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..3 {
            throttle
                .admit(peer(1), now)
                .unwrap()
                .settle(Some(CredentialOutcome::Rejected), now);
        }
        let in_flight = throttle.admit(peer(1), now).unwrap();
        throttle
            .admit(peer(1), now)
            .unwrap()
            .settle(Some(CredentialOutcome::Accepted), now);
        assert!(throttle.lock().get(&peer(1)).is_none());
        in_flight.settle(Some(CredentialOutcome::Rejected), now);
        assert!(throttle.lock().get(&peer(1)).is_none());
    }

    /// Goal: an attempt with no verdict, such as an internal error, gives its charge back,
    /// and the refund lifts a block that charge had set.
    #[test]
    fn refund_returns_the_charge() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..FAILURE_THRESHOLD {
            throttle.record_failure(peer(1), now);
        }
        let attempt = throttle.admit(peer(1), now).unwrap();
        assert_eq!(throttle.blocked_for(peer(1), now), Some(BASE_BACKOFF));
        attempt.settle(None, now);
        assert_eq!(throttle.blocked_for(peer(1), now), None);
        assert_eq!(
            throttle.lock().get(&peer(1)).map(|entry| entry.count),
            Some(FAILURE_THRESHOLD)
        );
    }

    /// Goal: a refund never lengthens a block that is already running, even though its
    /// deadline is recomputed from a later moment.
    #[test]
    fn refund_never_extends_a_block() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        for _ in 0..(FAILURE_THRESHOLD + 3) {
            throttle.record_failure(peer(1), now);
        }
        let blocked = throttle.blocked_for(peer(1), now).unwrap();
        let in_flight = throttle.admit(peer(1), now + blocked).unwrap();
        in_flight.settle(None, now + blocked + BASE_BACKOFF * 100);
        let until = throttle
            .lock()
            .get(&peer(1))
            .unwrap()
            .blocked_until
            .unwrap();
        assert!(until <= now + blocked + backoff_for(FAILURE_THRESHOLD + 4).unwrap());
    }

    /// Goal: a lone refund leaves nothing behind, so a peer that only ever hit internal
    /// errors is not tracked at all.
    #[test]
    fn refunding_the_only_charge_forgets_the_peer() {
        let throttle = AuthThrottle::new();
        let now = Instant::now();
        throttle.admit(peer(1), now).unwrap().settle(None, now);
        assert!(throttle.lock().get(&peer(1)).is_none());
    }

    /// Goal: the check-then-record race is closed end to end. Concurrent `/login` requests
    /// through the real middleware, all in flight at once, admit only the allowance.
    #[tokio::test]
    async fn concurrent_logins_are_capped_through_the_middleware() {
        let peer = wired_peer(14);
        let calls = (0..10).map(|_| call("POST", "/login", Some("Basic abc"), peer));
        let statuses = futures_util::future::join_all(calls).await;
        let refused = statuses
            .iter()
            .filter(|status| **status == StatusCode::TOO_MANY_REQUESTS)
            .count();
        assert_eq!(
            refused,
            10 - usize::try_from(FAILURE_THRESHOLD + 1).unwrap()
        );
    }

    /// Goal: `SINGLE_KEY_MAX_ATTEMPTS_PER_WINDOW` is what one key really reaches under this
    /// backoff, so the breaker's threshold keeps a single attacker from tripping it.
    /// Method: one key retries the moment each backoff lapses for a whole window, every
    /// attempt rejected, charged to a breaker as the middleware charges it.
    #[test]
    fn one_key_through_its_backoff_never_trips_the_breaker() {
        let throttle = AuthThrottle::new();
        let breaker = RemoteBreaker::new();
        let start = Instant::now();
        let mut now = start;
        let mut attempts = 0_u32;
        while now.duration_since(start) < auth_breaker::WINDOW {
            match PasswordAdmission::admit(&throttle, &breaker, peer(1), now) {
                Ok(admission) => {
                    admission.settle(Some(CredentialOutcome::Rejected), now);
                    attempts += 1;
                }
                Err(remaining) => now += remaining,
            }
        }
        assert_eq!(attempts, auth_breaker::SINGLE_KEY_MAX_ATTEMPTS_PER_WINDOW);
        assert!(breaker.admit(now).is_ok());
    }

    /// Admissions from distinct remote keys until the breaker refuses one.
    fn trip(throttle: &AuthThrottle, breaker: &RemoteBreaker, now: Instant) {
        for index in 0..1000 {
            let admitted = PasswordAdmission::admit(throttle, breaker, key_for(index + 1), now);
            match admitted {
                Ok(admission) => admission.settle(Some(CredentialOutcome::Rejected), now),
                Err(_) => return,
            }
        }
        panic!("the breaker never tripped");
    }

    /// Goal: while the breaker is tripped, this host can still log in and a fresh remote
    /// key cannot.
    #[test]
    fn loopback_is_admitted_while_the_breaker_is_tripped() {
        let throttle = AuthThrottle::new();
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        trip(&throttle, &breaker, now);
        assert!(PasswordAdmission::admit(&throttle, &breaker, PeerKey::Loopback, now).is_ok());
        assert!(PasswordAdmission::admit(&throttle, &breaker, peer(200), now).is_err());
    }

    /// Goal: a remote key that proved the password is admitted while the breaker is
    /// tripped, and a key that never did is not.
    #[test]
    fn known_good_key_is_admitted_while_the_breaker_is_tripped() {
        let throttle = AuthThrottle::new();
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        PasswordAdmission::admit(&throttle, &breaker, peer(7), now)
            .unwrap()
            .settle(Some(CredentialOutcome::Accepted), now);
        trip(&throttle, &breaker, now);
        assert!(PasswordAdmission::admit(&throttle, &breaker, peer(7), now).is_ok());
        assert!(PasswordAdmission::admit(&throttle, &breaker, peer(8), now).is_err());
    }

    /// Goal: only a password success makes a key known-good. A rejection or a non-verdict
    /// does not, and loopback is never recorded since it never needs to be.
    #[test]
    fn only_a_remote_password_success_is_remembered() {
        let throttle = AuthThrottle::new();
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        for (octet, outcome) in [(1, Some(CredentialOutcome::Rejected)), (2, None)] {
            PasswordAdmission::admit(&throttle, &breaker, peer(octet), now)
                .unwrap()
                .settle(outcome, now);
            assert!(breaker.is_known_good(peer(octet), now).not());
        }
        PasswordAdmission::admit(&throttle, &breaker, PeerKey::Loopback, now)
            .unwrap()
            .settle(Some(CredentialOutcome::Accepted), now);
        assert!(breaker.is_known_good(PeerKey::Loopback, now).not());
    }

    /// Goal: a valid bearer token never makes its key known-good: tokens can belong to less
    /// trusted clients than the admin. Method: a token success through the real middleware.
    #[tokio::test]
    async fn token_success_is_not_remembered() {
        let peer = wired_peer(15);
        assert_eq!(
            call("GET", "/token-ok", Some("Bearer cc_x"), peer).await,
            StatusCode::OK
        );
        let key = PeerKey::from_ip(peer.ip());
        assert!(REMOTE_BREAKER.is_known_good(key, Instant::now()).not());
    }

    /// Goal: a breaker refusal does not count against the refused peer's own budget, since
    /// no password was checked.
    #[test]
    fn breaker_refusal_refunds_the_peer() {
        let throttle = AuthThrottle::new();
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        trip(&throttle, &breaker, now);
        for _ in 0..(FAILURE_THRESHOLD * 2) {
            assert!(PasswordAdmission::admit(&throttle, &breaker, peer(200), now).is_err());
        }
        assert!(throttle.lock().get(&peer(200)).is_none());
    }

    /// Goal: a peer already in its own backoff is refused before it reaches the breaker,
    /// so it cannot spend the shared budget.
    #[test]
    fn peer_backoff_is_checked_before_the_breaker() {
        let throttle = AuthThrottle::new();
        let breaker = RemoteBreaker::new();
        let now = Instant::now();
        for _ in 0..=FAILURE_THRESHOLD {
            PasswordAdmission::admit(&throttle, &breaker, peer(1), now)
                .unwrap()
                .settle(Some(CredentialOutcome::Rejected), now);
        }
        for _ in 0..100 {
            assert!(PasswordAdmission::admit(&throttle, &breaker, peer(1), now).is_err());
        }
        for index in 0..(auth_breaker::THRESHOLD - FAILURE_THRESHOLD - 1) {
            let key = key_for(usize::try_from(index).unwrap() + 1);
            assert!(PasswordAdmission::admit(&throttle, &breaker, key, now).is_ok());
        }
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
