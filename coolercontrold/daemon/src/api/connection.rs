// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounds on API connections before any request is authenticated.
//!
//! Everything here runs for peers that have proven nothing, so each connection must end on
//! its own if the peer stalls. A remote connection starts in a small pre-auth pool and moves
//! to the larger remote pool once a request on it authenticates. A stream-less HTTP/2
//! connection, or a response the peer never reads, can outlive every timeout, so peers that
//! never authenticate can fill only the pre-auth pool. Connections that already authenticated
//! keep working while it is full, but a new one, even from a signed-in client, needs a slot.

use crate::api::peer::{PeerKey, TrustedProxies};
use axum::extract::Request;
use axum::http::Extensions;
use axum::middleware::{AddExtension, Next};
use axum::response::Response;
use axum::Extension;
use axum_server::accept::Accept;
use futures_util::future::BoxFuture;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use log::{debug, info};
use nix::sys::socket::{
    bind, listen, setsockopt, socket, sockopt, AddressFamily, Backlog, SockFlag, SockProtocol,
    SockType, SockaddrStorage,
};
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::future::Future;
use std::io::{self, ErrorKind, IoSlice};
use std::net::{IpAddr, SocketAddr};
use std::ops::Not;
use std::os::fd::AsRawFd;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::{Instant, Sleep};
use tower::Layer;

/// Plaintext bytes after which a connection has committed to a protocol: the length of the
/// HTTP/2 preface, the most hyper-util's version sniff ever waits for.
const FIRST_BYTES_COUNT: usize = 24;

/// Concurrent connections one remote peer may hold across both remote pools. A browser
/// opens at most six per host over HTTP/1.1, and a whole IPv6 /64 shares this, so it leaves
/// room for a household.
const PER_PEER_CONNECTIONS: usize = 64;
/// Of those, connections on which no request has authenticated yet: room for a few
/// browsers loading the web app at once, while no one peer fills the pre-auth pool.
const PER_PEER_PRE_AUTH_CONNECTIONS: usize = 16;
/// Concurrent authenticated connections all remote peers may hold together.
const REMOTE_CONNECTIONS: usize = 256;
/// Concurrent remote connections on which no request has authenticated yet. Every new
/// remote connection, a signed-in client's included, is admitted against this pool alone.
const REMOTE_PRE_AUTH_CONNECTIONS: usize = 64;
/// Connections from this host, pooled apart so a remote flood cannot lock out the local app.
/// No pre-auth pool: a local peer can already reach the hardware it would starve.
const LOOPBACK_CONNECTIONS: usize = 128;
/// Connections may hold at most one part in this many of the open file limit. The rest is
/// for the hardware, whose descriptor use grows with the number of devices.
const OPEN_FILES_SHARE: u64 = 4;
/// Pools that each keep at least one connection however low the open file limit is.
const POOL_COUNT: usize = 3;

const _: () = assert!(PER_PEER_CONNECTIONS <= REMOTE_CONNECTIONS);
const _: () = assert!(PER_PEER_PRE_AUTH_CONNECTIONS <= PER_PEER_CONNECTIONS);
const _: () = assert!(PER_PEER_PRE_AUTH_CONNECTIONS <= REMOTE_PRE_AUTH_CONNECTIONS);
const _: () = assert!(REMOTE_PRE_AUTH_CONNECTIONS <= REMOTE_CONNECTIONS);
const _: () = assert!(OPEN_FILES_SHARE > 1);

#[derive(Debug, Clone, Copy)]
struct ConnectionTimeouts {
    /// How long a new connection has to finish any TLS handshake and send its first
    /// `FIRST_BYTES_COUNT` bytes. Covers what hyper's header timeout cannot reach: the TLS
    /// peek and hyper-util's version sniff, neither of which has a deadline of its own.
    first_bytes_timeout: Duration,
    /// How long hyper waits for a complete request head, and for the next one on an idle
    /// keep-alive connection.
    header_read: Duration,
    /// How often an idle HTTP/2 connection is pinged. A peer that stops answering, such as
    /// one behind an expired NAT mapping, is dropped after hyper's 20 s ping timeout.
    h2_keep_alive_interval: Duration,
}

/// The header timeout is hyper's own default, which it silently drops without a timer.
const TIMEOUTS: ConnectionTimeouts = ConnectionTimeouts {
    first_bytes_timeout: Duration::from_secs(10),
    header_read: Duration::from_secs(30),
    h2_keep_alive_interval: Duration::from_secs(20),
};

const _: () = assert!(TIMEOUTS.first_bytes_timeout.as_secs() <= TIMEOUTS.header_read.as_secs());

/// Pending connections the kernel queues per listener. The value std and tokio use.
const LISTEN_BACKLOG: i32 = 128;

/// A non-blocking listener on `address`, ready for `server`. Needs no reactor.
///
/// An IPv6 listener takes IPv6 traffic only. Linux otherwise makes `::` dual-stack, which
/// claims the port for IPv4 too, so it and a `0.0.0.0` listener cannot share a port:
/// whichever binds second gets `EADDRINUSE`. Each family has its own setting and listener.
///
/// The exception is an IPv4-mapped address such as `::ffff:192.0.2.1`: it names an IPv4
/// address, which an IPv6-only socket cannot bind, so its socket stays dual-stack.
pub fn listener(address: SocketAddr) -> io::Result<std::net::TcpListener> {
    let (family, is_ipv6_only) = match address {
        SocketAddr::V4(_) => (AddressFamily::Inet, false),
        SocketAddr::V6(ipv6) => (AddressFamily::Inet6, ipv6.ip().to_ipv4_mapped().is_none()),
    };
    let socket_fd = socket(
        family,
        SockType::Stream,
        SockFlag::SOCK_CLOEXEC | SockFlag::SOCK_NONBLOCK,
        SockProtocol::Tcp,
    )?;
    if is_ipv6_only {
        setsockopt(&socket_fd, sockopt::Ipv6V6Only, &true)?;
    }
    // A restarted daemon rebinds while its old connections are still in TIME_WAIT.
    setsockopt(&socket_fd, sockopt::ReuseAddr, &true)?;
    bind(socket_fd.as_raw_fd(), &SockaddrStorage::from(address))?;
    listen(&socket_fd, Backlog::new(LISTEN_BACKLOG)?)?;
    Ok(std::net::TcpListener::from(socket_fd))
}

/// The API server for `listener`, with every connection admitted by `limiter`, accepted
/// through `acceptor`, then bounded by the guard. Taking both here keeps the guard on every
/// listener.
pub fn server<A>(
    listener: std::net::TcpListener,
    acceptor: A,
    limiter: Arc<ConnectionLimiter>,
) -> io::Result<axum_server::Server<SocketAddr, ConnectionGuardAcceptor<A>>> {
    server_with(listener, acceptor, limiter, TIMEOUTS, tcp_peer_ip)
}

/// `peer_ip` picks the address a connection is charged to. Tests substitute a remote one,
/// since every test connection comes from loopback.
fn server_with<A>(
    listener: std::net::TcpListener,
    acceptor: A,
    limiter: Arc<ConnectionLimiter>,
    timeouts: ConnectionTimeouts,
    peer_ip: fn(SocketAddr) -> IpAddr,
) -> io::Result<axum_server::Server<SocketAddr, ConnectionGuardAcceptor<A>>> {
    let mut server = axum_server::from_tcp(listener)?.acceptor(ConnectionGuardAcceptor {
        inner: acceptor,
        limiter,
        first_bytes_timeout: timeouts.first_bytes_timeout,
        peer_ip,
    });
    configure_http(server.http_builder(), timeouts);
    Ok(server)
}

/// The connection limits are properties of the connection, so they charge its TCP peer.
fn tcp_peer_ip(address: SocketAddr) -> IpAddr {
    address.ip()
}

/// The limiter every listener shares, sized to this process's open file limit.
pub fn process_limiter(trusted_proxies: Arc<TrustedProxies>) -> Arc<ConnectionLimiter> {
    let limits = ConnectionLimits::for_open_file_limit(crate::open_files::limit());
    if limits != ConnectionLimits::FULL {
        info!(
            "API connections are limited to {} by the open file limit.",
            limits.total()
        );
    }
    ConnectionLimiter::new(limits, trusted_proxies)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ConnectionLimits {
    per_peer: usize,
    per_peer_pre_auth: usize,
    remote: usize,
    remote_pre_auth: usize,
    loopback: usize,
}

impl ConnectionLimits {
    const FULL: Self = Self {
        per_peer: PER_PEER_CONNECTIONS,
        per_peer_pre_auth: PER_PEER_PRE_AUTH_CONNECTIONS,
        remote: REMOTE_CONNECTIONS,
        remote_pre_auth: REMOTE_PRE_AUTH_CONNECTIONS,
        loopback: LOOPBACK_CONNECTIONS,
    };

    /// The full limits, scaled down in proportion when they would hold more than their
    /// share of `open_files`. Each pool keeps at least one connection.
    fn for_open_file_limit(open_files: u64) -> Self {
        let budget = usize::try_from(open_files / OPEN_FILES_SHARE).unwrap_or(usize::MAX);
        let full_total = Self::FULL.total();
        if budget >= full_total {
            return Self::FULL;
        }
        let scaled = |full: usize| (full * budget / full_total).max(1);
        let remote = scaled(REMOTE_CONNECTIONS);
        let remote_pre_auth = scaled(REMOTE_PRE_AUTH_CONNECTIONS);
        let per_peer = PER_PEER_CONNECTIONS.min(remote);
        let limits = Self {
            per_peer,
            per_peer_pre_auth: PER_PEER_PRE_AUTH_CONNECTIONS
                .min(remote_pre_auth)
                .min(per_peer),
            remote,
            remote_pre_auth,
            loopback: scaled(LOOPBACK_CONNECTIONS),
        };
        debug_assert!(limits.total() <= budget.max(POOL_COUNT));
        debug_assert!(limits.per_peer_pre_auth <= limits.remote_pre_auth);
        limits
    }

    fn total(self) -> usize {
        self.remote + self.remote_pre_auth + self.loopback
    }
}

/// One remote peer's connections, by pool.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PeerConnections {
    pre_auth: usize,
    authenticated: usize,
}

impl PeerConnections {
    fn total(self) -> usize {
        self.pre_auth + self.authenticated
    }
}

#[derive(Debug)]
struct ConnectionCounts {
    remote_by_peer: HashMap<PeerKey, PeerConnections>,
    /// Authenticated remote connections.
    remote: usize,
    remote_pre_auth: usize,
    loopback: usize,
}

impl ConnectionCounts {
    /// Asserts the `ConnectionLimiter` invariant. Walks the map, so debug builds only.
    fn assert_consistent(&self) {
        if cfg!(debug_assertions).not() {
            return;
        }
        let peers = self.remote_by_peer.values();
        let authenticated: usize = peers.clone().map(|held| held.authenticated).sum();
        let pre_auth: usize = peers.clone().map(|held| held.pre_auth).sum();
        debug_assert_eq!(self.remote, authenticated);
        debug_assert_eq!(self.remote_pre_auth, pre_auth);
        debug_assert!(peers.clone().all(|held| held.total() > 0));
    }
}

/// Counts open connections against `ConnectionLimits`, shared by every listener.
///
/// Invariants: each count stays within its limit; `remote` and `remote_pre_auth` are the
/// sums of `remote_by_peer`, which holds no empty entries, so it never has more keys than
/// the two remote pools hold connections.
#[derive(Debug)]
pub struct ConnectionLimiter {
    limits: ConnectionLimits,
    /// Exempt from the per-peer limits, since every client behind one shares its address.
    /// Still counted in their pool.
    trusted_proxies: Arc<TrustedProxies>,
    counts: Mutex<ConnectionCounts>,
}

impl ConnectionLimiter {
    fn new(limits: ConnectionLimits, trusted_proxies: Arc<TrustedProxies>) -> Arc<Self> {
        Arc::new(Self {
            limits,
            trusted_proxies,
            counts: Mutex::new(ConnectionCounts {
                remote_by_peer: HashMap::with_capacity(limits.remote + limits.remote_pre_auth),
                remote: 0,
                remote_pre_auth: 0,
                loopback: 0,
            }),
        })
    }

    /// A permit for one more connection from `peer`, or `None` when a limit is reached. A
    /// remote connection is admitted into the pre-auth pool, and `promote` moves it on.
    pub fn try_acquire(self: &Arc<Self>, peer: IpAddr) -> Option<ConnectionPermit> {
        let key = PeerKey::from_ip(peer);
        let mut counts = self.lock();
        if key == PeerKey::Loopback {
            if counts.loopback < self.limits.loopback {
                counts.loopback += 1;
                return Some(self.permit(key));
            }
            return None;
        }
        if counts.remote_pre_auth >= self.limits.remote_pre_auth {
            return None;
        }
        let held = counts.remote_by_peer.get(&key).copied().unwrap_or_default();
        if self.trusted_proxies.contains(peer).not() {
            if held.pre_auth >= self.limits.per_peer_pre_auth {
                return None;
            }
            if held.total() >= self.limits.per_peer {
                return None;
            }
        }
        counts.remote_by_peer.entry(key).or_default().pre_auth += 1;
        counts.remote_pre_auth += 1;
        debug_assert!(
            counts.remote_by_peer.len() <= self.limits.remote + self.limits.remote_pre_auth
        );
        counts.assert_consistent();
        Some(self.permit(key))
    }

    fn permit(self: &Arc<Self>, key: PeerKey) -> ConnectionPermit {
        ConnectionPermit {
            limiter: Arc::clone(self),
            key,
            authenticated: AtomicBool::new(false),
        }
    }

    /// Moves `permit`'s connection from the pre-auth pool to the remote pool. A full remote
    /// pool leaves it where it is: its request already authenticated, so refusing it
    /// would only punish a client that did everything right.
    fn promote(&self, permit: &ConnectionPermit) {
        debug_assert_ne!(permit.key, PeerKey::Loopback);
        let mut counts = self.lock();
        // Re-read under the lock: another request on this connection may have won the race.
        if permit.authenticated.load(Ordering::Relaxed) {
            return;
        }
        if counts.remote >= self.limits.remote {
            return;
        }
        let Some(held) = counts.remote_by_peer.get_mut(&permit.key) else {
            // Only after a poisoned update lost this connection's entry.
            return;
        };
        debug_assert!(held.pre_auth > 0);
        held.pre_auth = held.pre_auth.saturating_sub(1);
        held.authenticated += 1;
        counts.remote_pre_auth = counts.remote_pre_auth.saturating_sub(1);
        counts.remote += 1;
        permit.authenticated.store(true, Ordering::Relaxed);
        debug_assert!(counts.remote <= self.limits.remote);
        counts.assert_consistent();
    }

    fn release(&self, key: PeerKey, authenticated: bool) {
        let mut counts = self.lock();
        if key == PeerKey::Loopback {
            debug_assert!(counts.loopback > 0);
            counts.loopback = counts.loopback.saturating_sub(1);
            return;
        }
        counts.assert_consistent();
        if authenticated {
            debug_assert!(counts.remote > 0);
            counts.remote = counts.remote.saturating_sub(1);
        } else {
            debug_assert!(counts.remote_pre_auth > 0);
            counts.remote_pre_auth = counts.remote_pre_auth.saturating_sub(1);
        }
        if let Some(held) = counts.remote_by_peer.get_mut(&key) {
            if authenticated {
                held.authenticated = held.authenticated.saturating_sub(1);
            } else {
                held.pre_auth = held.pre_auth.saturating_sub(1);
            }
            if held.total() == 0 {
                counts.remote_by_peer.remove(&key);
            }
        }
        counts.assert_consistent();
    }

    /// A poisoned limiter must not refuse every connection from then on. The counts may be
    /// off by the one update in flight when it poisoned, which is far better than an outage.
    fn lock(&self) -> MutexGuard<'_, ConnectionCounts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One admitted connection. Dropping it, when the connection ends, frees its slot in
/// whichever pool it is in.
#[derive(Debug)]
pub struct ConnectionPermit {
    limiter: Arc<ConnectionLimiter>,
    key: PeerKey,
    /// One way: set, under the limiter's lock, when the connection moves to the remote pool.
    authenticated: AtomicBool,
}

impl ConnectionPermit {
    /// Counts this connection as authenticated from now on. Idempotent, and free once done,
    /// since every authenticated request calls it.
    fn promote(&self) {
        if self.key == PeerKey::Loopback {
            return;
        }
        if self.authenticated.load(Ordering::Relaxed) {
            return;
        }
        self.limiter.promote(self);
    }
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        let authenticated = *self.authenticated.get_mut();
        self.limiter.release(self.key, authenticated);
    }
}

/// The permit of the connection a request arrived on, shared by every request on it.
#[derive(Debug, Clone)]
pub struct AdmittedConnection(Arc<ConnectionPermit>);

#[cfg(test)]
impl AdmittedConnection {
    /// A remote connection with a limiter of its own, for tests of the layers that promote.
    pub fn remote_for_test() -> Self {
        let limiter = ConnectionLimiter::new(ConnectionLimits::FULL, Arc::default());
        let permit = limiter.try_acquire(IpAddr::from([192, 0, 2, 60]));
        Self(Arc::new(permit.expect("an empty limiter admits")))
    }

    pub fn is_authenticated(&self) -> bool {
        self.0.authenticated.load(Ordering::Relaxed)
    }
}

/// Records that a request on this connection authenticated, moving the connection out of
/// the pre-auth pool. A request that came through no guard, as in tests, carries no
/// connection, and nothing happens.
pub fn promote_connection(extensions: &Extensions) {
    if let Some(AdmittedConnection(permit)) = extensions.get::<AdmittedConnection>() {
        permit.promote();
    }
}

/// Promotes the connection of a request its handler authenticated itself, such as
/// `/login`, whose success status is the verdict.
pub async fn promote_on_success_middleware(request: Request, next: Next) -> Response {
    let admitted = request.extensions().get::<AdmittedConnection>().cloned();
    let response = next.run(request).await;
    if response.status().is_success() {
        if let Some(AdmittedConnection(permit)) = admitted {
            permit.promote();
        }
    }
    response
}

/// axum-server builds hyper's builder without a timer, and hyper ignores a default timeout
/// that has no timer to run it. So until both timers are set, a peer sending headers a
/// byte at a time, or an idle keep-alive connection, holds its slot forever. The timeouts
/// only run while hyper waits for a request head, never while a response streams, so SSE
/// and long LCD uploads are unaffected. Hyper panics on a configured timeout without a
/// timer, hence the timer on each protocol it is configured for.
fn configure_http(builder: &mut Builder<TokioExecutor>, timeouts: ConnectionTimeouts) {
    debug_assert!(timeouts.header_read.is_zero().not());
    debug_assert!(timeouts.h2_keep_alive_interval.is_zero().not());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(timeouts.header_read);
    builder
        .http2()
        .timer(TokioTimer::new())
        .keep_alive_interval(timeouts.h2_keep_alive_interval);
}

/// Wraps the protocol acceptor with the connection limits and a deadline for the
/// connection's first bytes.
#[derive(Debug, Clone)]
pub struct ConnectionGuardAcceptor<A> {
    inner: A,
    limiter: Arc<ConnectionLimiter>,
    first_bytes_timeout: Duration,
    peer_ip: fn(SocketAddr) -> IpAddr,
}

impl<A, S> Accept<TcpStream, S> for ConnectionGuardAcceptor<A>
where
    A: Accept<TcpStream, S>,
    A::Future: Send + 'static,
    A::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    A::Service: Send + 'static,
{
    type Stream = GuardedStream<A::Stream>;
    type Service = AddExtension<A::Service, AdmittedConnection>;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    /// Over-limit connections are dropped here, before any TLS or HTTP work, which closes
    /// them at once. The permit then travels with the stream, so the slot frees whenever the
    /// connection ends, however it ends, and with each request, so auth can promote it.
    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let permit = stream
            .peer_addr()
            .ok()
            .and_then(|address| self.limiter.try_acquire((self.peer_ip)(address)));
        let Some(permit) = permit else {
            debug!("Refused an API connection over the connection limit.");
            return Box::pin(std::future::ready(Err(io::Error::other(
                "over the connection limit",
            ))));
        };
        let permit = Arc::new(permit);
        let deadline = Instant::now() + self.first_bytes_timeout;
        let accepting = self.inner.accept(stream, service);
        Box::pin(async move {
            let Ok(accepted) = tokio::time::timeout_at(deadline, accepting).await else {
                return Err(first_bytes_timeout());
            };
            let (stream, service) = accepted?;
            let service = Extension(AdmittedConnection(Arc::clone(&permit))).layer(service);
            Ok((GuardedStream::new(stream, deadline, permit), service))
        })
    }
}

fn first_bytes_timeout() -> io::Error {
    io::Error::new(
        ErrorKind::TimedOut,
        "no request before the connection deadline",
    )
}

pin_project! {
    /// A connection stream that fails its reads once the first-bytes deadline passes
    /// without `FIRST_BYTES_COUNT` plaintext bytes having arrived.
    ///
    /// Every real HTTP/1.1 request head is longer than that, since it must carry a `Host`,
    /// and so is the HTTP/2 preface. A bare HTTP/1.0 request can be shorter, and then the
    /// deadline closes its connection, which such a client survives by reconnecting.
    #[derive(Debug)]
    pub struct GuardedStream<S> {
        #[pin]
        inner: S,
        // Dropped once the connection has committed to a protocol.
        deadline: Option<Pin<Box<Sleep>>>,
        bytes_read: usize,
        permit: Arc<ConnectionPermit>,
    }
}

impl<S> GuardedStream<S> {
    fn new(inner: S, deadline: Instant, permit: Arc<ConnectionPermit>) -> Self {
        Self {
            inner,
            deadline: Some(Box::pin(tokio::time::sleep_until(deadline))),
            bytes_read: 0,
            permit,
        }
    }
}

impl<S: AsyncRead> AsyncRead for GuardedStream<S> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.project();
        let Some(deadline) = this.deadline.as_mut() else {
            return this.inner.poll_read(cx, buf);
        };
        if deadline.as_mut().poll(cx).is_ready() {
            return Poll::Ready(Err(first_bytes_timeout()));
        }
        let filled_before = buf.filled().len();
        let polled = this.inner.poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = polled {
            debug_assert!(buf.filled().len() >= filled_before);
            *this.bytes_read += buf.filled().len() - filled_before;
            if *this.bytes_read >= FIRST_BYTES_COUNT {
                *this.deadline = None;
            }
        }
        polled
    }
}

impl<S: AsyncWrite> AsyncWrite for GuardedStream<S> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write_vectored(cx, bufs)
    }

    fn is_write_vectored(&self) -> bool {
        self.inner.is_write_vectored()
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::dual_protocol::DualProtocolAcceptor;
    use crate::grpc_api;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use axum_server::accept::DefaultAcceptor;
    use axum_server::tls_rustls::RustlsConfig;
    use std::net::Ipv6Addr;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::time::timeout;
    use tonic_health::pb::health_client::HealthClient;
    use tonic_health::pb::HealthCheckRequest;

    /// Real time with short timeouts. A paused clock cannot measure these: it advances to
    /// the next timer before the runtime reads socket events, so a close is seen late.
    const TEST_TIMEOUTS: ConnectionTimeouts = ConnectionTimeouts {
        first_bytes_timeout: Duration::from_millis(300),
        header_read: Duration::from_millis(300),
        h2_keep_alive_interval: Duration::from_secs(20),
    };
    /// Far beyond any timeout under test, so a regression fails instead of hanging.
    const NEVER: Duration = Duration::from_secs(10);
    const STREAMED_CHUNKS: u32 = 5;
    const CHUNK_INTERVAL: Duration = Duration::from_millis(200);

    const _: () = assert!(
        CHUNK_INTERVAL.as_millis() * STREAMED_CHUNKS as u128
            > 2 * TEST_TIMEOUTS.header_read.as_millis()
    );

    /// Attempts at a port free for both families. The kernel picks one that is free for
    /// IPv4, which another process may still hold for IPv6.
    const SHARED_PORT_ATTEMPTS: usize = 8;

    fn socket_address(text: &str) -> SocketAddr {
        text.parse().unwrap()
    }

    /// Goal: `0.0.0.0` and `::` listen on one port together, as a config that sets both
    /// expects. A dual-stack `::` claims the IPv4 port as well and fails with `EADDRINUSE`.
    #[test]
    fn ipv4_and_ipv6_wildcards_share_a_port() {
        for _ in 0..SHARED_PORT_ATTEMPTS {
            let ipv4 = listener(socket_address("0.0.0.0:0")).unwrap();
            let port = ipv4.local_addr().unwrap().port();
            let ipv6_address = SocketAddr::from((Ipv6Addr::UNSPECIFIED, port));
            match listener(ipv6_address) {
                Ok(ipv6) => {
                    assert_eq!(ipv6.local_addr().unwrap(), ipv6_address);
                    return;
                }
                Err(err) => assert_eq!(err.kind(), ErrorKind::AddrInUse, "{err}"),
            }
        }
        panic!("`::` never shared a port with `0.0.0.0`");
    }

    /// Goal: an IPv4-mapped `ipv6_address` still binds, as it did before IPv6 listeners
    /// became IPv6-only. The kernel rejects it on an IPv6-only socket with `EINVAL`.
    #[test]
    fn ipv4_mapped_ipv6_address_binds() {
        let mapped = listener(socket_address("[::ffff:127.0.0.1]:0")).unwrap();
        let bound = mapped.local_addr().unwrap();
        assert_eq!(bound.ip(), socket_address("[::ffff:127.0.0.1]:0").ip());
        assert_ne!(bound.port(), 0);
    }

    /// Goal: the port still belongs to one listener per family. `SO_REUSEADDR` must not let
    /// a second daemon listen on an address the first already serves.
    #[test]
    fn listening_address_cannot_be_bound_twice() {
        let first = listener(socket_address("127.0.0.1:0")).unwrap();
        let taken = first.local_addr().unwrap();
        let error = listener(taken).unwrap_err();
        assert_eq!(error.kind(), ErrorKind::AddrInUse);
    }

    /// Serves `router` through the production server, the way `create_api_server` does.
    async fn serve(router: Router) -> SocketAddr {
        let limiter = ConnectionLimiter::new(ConnectionLimits::FULL, Arc::default());
        serve_limited(router, limiter).await
    }

    async fn serve_limited(router: Router, limiter: Arc<ConnectionLimiter>) -> SocketAddr {
        serve_as(router, limiter, TEST_TIMEOUTS, tcp_peer_ip).await
    }

    /// Serves `router` with every connection charged to the address `peer_ip` picks.
    async fn serve_as(
        router: Router,
        limiter: Arc<ConnectionLimiter>,
        timeouts: ConnectionTimeouts,
        peer_ip: fn(SocketAddr) -> IpAddr,
    ) -> SocketAddr {
        let tcp_listener = listener(socket_address("127.0.0.1:0")).unwrap();
        let address = tcp_listener.local_addr().unwrap();
        let acceptor = DefaultAcceptor::new();
        let server = server_with(tcp_listener, acceptor, limiter, timeouts, peer_ip).unwrap();
        tokio::spawn(async move {
            server
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        });
        address
    }

    /// Serves `router` behind TLS, the way `create_api_server` does when TLS is enabled.
    async fn serve_tls(router: Router) -> SocketAddr {
        let certified = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
        let config = RustlsConfig::from_pem(
            certified.cert.pem().into_bytes(),
            certified.signing_key.serialize_pem().into_bytes(),
        )
        .await
        .unwrap();
        let tcp_listener = listener(socket_address("127.0.0.1:0")).unwrap();
        let address = tcp_listener.local_addr().unwrap();
        let acceptor = DualProtocolAcceptor::new(config);
        let limiter = ConnectionLimiter::new(ConnectionLimits::FULL, Arc::default());
        let server =
            server_with(tcp_listener, acceptor, limiter, TEST_TIMEOUTS, tcp_peer_ip).unwrap();
        tokio::spawn(async move {
            server
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        });
        address
    }

    /// Writes `bytes`, then waits for the server to close the connection.
    async fn closed_after(address: SocketAddr, bytes: &[u8]) {
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(bytes).await.unwrap();
        let started = std::time::Instant::now();
        let mut received = Vec::new();
        // A reset is as good as a close here: either way the slot is freed.
        let _ = timeout(NEVER, stream.read_to_end(&mut received))
            .await
            .expect("the stalled connection must be closed");
        assert!(
            started.elapsed()
                >= TEST_TIMEOUTS
                    .first_bytes_timeout
                    .min(TEST_TIMEOUTS.header_read)
        );
    }

    fn app() -> Router {
        Router::new().route("/", get(|| async { "ok" })).route(
            "/stream",
            get(|| async { Body::from_stream(slow_chunks()) }),
        )
    }

    /// A response that keeps streaming well past the header timeout.
    fn slow_chunks() -> impl futures_util::Stream<Item = Result<String, io::Error>> {
        futures_util::stream::unfold(0, |sent| async move {
            if sent == STREAMED_CHUNKS {
                return None;
            }
            tokio::time::sleep(CHUNK_INTERVAL).await;
            Some((Ok(format!("chunk{sent};")), sent + 1))
        })
    }

    async fn request(address: SocketAddr, path: &str) -> TcpStream {
        let mut stream = TcpStream::connect(address).await.unwrap();
        let head = format!("GET {path} HTTP/1.1\r\nhost: cc.lan\r\n\r\n");
        stream.write_all(head.as_bytes()).await.unwrap();
        stream
    }

    /// Goal: a keep-alive connection left idle after its response is closed by the header
    /// timeout. Without a timer on the builder it would stay open forever. Method: read to
    /// EOF; the close must come, and not before the timeout.
    #[tokio::test]
    async fn idle_keep_alive_connection_is_closed() {
        let address = serve(app()).await;
        let mut stream = request(address, "/").await;
        let started = std::time::Instant::now();
        let mut received = Vec::new();
        timeout(NEVER, stream.read_to_end(&mut received))
            .await
            .expect("the idle connection must be closed")
            .unwrap();

        assert!(String::from_utf8_lossy(&received).starts_with("HTTP/1.1 200 OK"));
        assert!(started.elapsed() >= TEST_TIMEOUTS.header_read);
    }

    /// Goal: a connection that never finishes its request head is closed too, which is the
    /// slowloris shape. Method: half a request line, then silence.
    #[tokio::test]
    async fn partial_request_head_is_closed() {
        let address = serve(app()).await;
        let mut stream = TcpStream::connect(address).await.unwrap();
        stream.write_all(b"GET / HTTP/1.1\r\nhost: ").await.unwrap();
        let mut received = Vec::new();
        timeout(NEVER, stream.read_to_end(&mut received))
            .await
            .expect("the stalled connection must be closed")
            .unwrap();
    }

    /// Goal: a peer that connects and says nothing is closed. Hyper's header timeout never
    /// starts, because hyper-util is still waiting to learn the HTTP version.
    #[tokio::test]
    async fn silent_plain_connection_is_closed() {
        closed_after(serve(app()).await, b"").await;
    }

    /// Goal: a peer that sends part of the HTTP/2 preface and stops is closed, since that
    /// also stalls the version sniff before hyper takes over.
    #[tokio::test]
    async fn partial_h2_preface_is_closed() {
        closed_after(serve(app()).await, b"PRI * HTTP/2.0\r\n").await;
    }

    /// Goal: a silent peer on the TLS listener is closed too. There the TLS first-byte peek
    /// waits before any handshake timeout applies.
    #[tokio::test]
    async fn silent_tls_connection_is_closed() {
        closed_after(serve_tls(app()).await, b"").await;
    }

    /// Goal: the guard lets a real HTTP/2 client through: gRPC over h2c prior knowledge, whose
    /// preface is exactly the byte count that disarms the deadline. Method: the health service
    /// through the guarded server, called well after the first-bytes deadline would have fired.
    #[tokio::test]
    async fn h2c_grpc_is_served_through_the_guard() {
        let router = Router::new().route_service(
            &grpc_api::health_service_route(),
            grpc_api::health_service().await,
        );
        let address = serve(router).await;
        let channel = tonic::transport::Endpoint::new(format!("http://{address}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let mut client = HealthClient::new(channel);
        let request = || {
            tonic::Request::new(HealthCheckRequest {
                service: String::new(),
            })
        };
        assert_eq!(
            client.check(request()).await.unwrap().into_inner().status,
            1
        );
        tokio::time::sleep(TEST_TIMEOUTS.first_bytes_timeout * 2).await;
        assert_eq!(
            client.check(request()).await.unwrap().into_inner().status,
            1
        );
    }

    fn limiter(limits: ConnectionLimits) -> Arc<ConnectionLimiter> {
        ConnectionLimiter::new(limits, Arc::default())
    }

    /// Only the loopback pool is reachable from a test's own connections.
    fn loopback_limiter(loopback: usize) -> Arc<ConnectionLimiter> {
        limiter(ConnectionLimits { loopback, ..SMALL })
    }

    /// Limits small enough to reach in a test.
    const SMALL: ConnectionLimits = ConnectionLimits {
        per_peer: 3,
        per_peer_pre_auth: 2,
        remote: 4,
        remote_pre_auth: 3,
        loopback: 1,
    };

    fn ip(address: &str) -> IpAddr {
        address.parse().unwrap()
    }

    /// Open connections as (pre-auth, authenticated remote, loopback).
    fn pools(limiter: &ConnectionLimiter) -> (usize, usize, usize) {
        let counts = limiter.lock();
        (counts.remote_pre_auth, counts.remote, counts.loopback)
    }

    /// Goal: at a normal open file limit the full limits apply, and at a low one every pool,
    /// the pre-auth one included, shrinks in proportion to stay within its share, never
    /// below one each. Method: limits just under what the full pools need must scale.
    #[test]
    fn limits_scale_with_the_open_file_limit() {
        assert_eq!(
            ConnectionLimits::for_open_file_limit(524_288),
            ConnectionLimits::FULL
        );
        let full_total = ConnectionLimits::FULL.total();
        let open_files = u64::try_from(full_total - 1).unwrap() * OPEN_FILES_SHARE;
        assert!(ConnectionLimits::for_open_file_limit(open_files).total() < full_total);
        let tight = ConnectionLimits::for_open_file_limit(1024);
        assert!(tight.total() <= 1024 / 4);
        assert!(tight.remote > tight.remote_pre_auth);
        assert!(tight.remote > tight.loopback);
        assert_eq!(tight.per_peer, PER_PEER_CONNECTIONS);
        assert_eq!(tight.per_peer_pre_auth, PER_PEER_PRE_AUTH_CONNECTIONS);
        let tiny = ConnectionLimits::for_open_file_limit(40);
        assert!(tiny.total() <= 10);
        assert!(tiny.per_peer <= tiny.remote);
        assert!(tiny.per_peer_pre_auth <= tiny.remote_pre_auth);
        let starved = ConnectionLimits::for_open_file_limit(0);
        assert_eq!(
            (starved.remote, starved.remote_pre_auth, starved.loopback),
            (1, 1, 1)
        );
        assert_eq!((starved.per_peer, starved.per_peer_pre_auth), (1, 1));
    }

    /// Goal: one peer is held to its own pre-auth limit, and an IPv6 /64 counts as one peer.
    /// Method: two addresses in one /64 reach a limit of two; a third is refused, while
    /// another peer is admitted.
    #[test]
    fn per_peer_pre_auth_limit_holds() {
        let limiter = limiter(SMALL);
        let held = [
            limiter.try_acquire(ip("2001:db8::1")).unwrap(),
            limiter.try_acquire(ip("2001:db8::2")).unwrap(),
        ];
        assert!(limiter.try_acquire(ip("2001:db8::3")).is_none());
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_some());
        drop(held);
    }

    /// Goal: the per-peer limit counts a peer's connections in both pools, so promoting
    /// connections never lets one peer exceed it. Method: two promoted and one pre-auth
    /// connection reach a limit of three while the pre-auth limit still has room.
    #[test]
    fn per_peer_limit_spans_both_pools() {
        let limiter = limiter(SMALL);
        let first = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        let second = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        first.promote();
        let third = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        second.promote();
        assert_eq!(pools(&limiter), (1, 2, 0));
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_none());
        drop([first, second, third]);
    }

    /// Goal: new remote connections are admitted against the pre-auth pool alone, which
    /// caps all remote peers together, while loopback keeps its own pool. Method: two peers
    /// fill a pre-auth pool of two; promoting one connection lets a third peer in.
    #[test]
    fn pre_auth_pool_caps_all_remote_peers() {
        let limiter = limiter(ConnectionLimits {
            per_peer_pre_auth: 1,
            remote_pre_auth: 2,
            ..SMALL
        });
        let remote = [
            limiter.try_acquire(ip("192.0.2.1")).unwrap(),
            limiter.try_acquire(ip("192.0.2.2")).unwrap(),
        ];
        assert!(limiter.try_acquire(ip("192.0.2.3")).is_none());
        let local = limiter.try_acquire(ip("127.0.0.1")).unwrap();
        assert!(limiter.try_acquire(ip("::1")).is_none());
        drop(local);
        assert!(limiter.try_acquire(ip("::ffff:127.0.0.1")).is_some());
        remote[0].promote();
        assert!(limiter.try_acquire(ip("192.0.2.3")).is_some());
        drop(remote);
    }

    /// Goal: promotion moves a connection from the pre-auth pool to the remote pool once,
    /// freeing its pre-auth slot, and a second promotion changes nothing. Method: a pre-auth
    /// pool of one refuses a second peer until the first is promoted, then again.
    #[test]
    fn promotion_moves_a_connection_once() {
        let limiter = limiter(ConnectionLimits {
            remote_pre_auth: 1,
            ..SMALL
        });
        let first = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        assert!(limiter.try_acquire(ip("192.0.2.2")).is_none());
        first.promote();
        assert_eq!(pools(&limiter), (0, 1, 0));
        let second = limiter.try_acquire(ip("192.0.2.2")).unwrap();
        first.promote();
        assert_eq!(pools(&limiter), (1, 1, 0));
        drop([first, second]);
    }

    /// Goal: promotion into a full remote pool leaves the connection pre-auth rather than
    /// failing it, and a later promotion succeeds once the pool has room. Method: a remote
    /// pool of one, filled by the first of two promotions, then freed by a drop.
    #[test]
    fn promotion_into_a_full_pool_stays_pre_auth() {
        let limiter = limiter(ConnectionLimits { remote: 1, ..SMALL });
        let first = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        let second = limiter.try_acquire(ip("192.0.2.2")).unwrap();
        first.promote();
        second.promote();
        assert_eq!(pools(&limiter), (1, 1, 0));
        assert!(second.authenticated.load(Ordering::Relaxed).not());
        drop(first);
        assert_eq!(pools(&limiter), (1, 0, 0));
        second.promote();
        assert_eq!(pools(&limiter), (0, 1, 0));
        drop(second);
    }

    /// Goal: loopback has no pre-auth distinction: promoting a local connection is a no-op,
    /// and a full pre-auth pool never refuses one. Method: a remote peer fills a pre-auth
    /// pool of one, then a local connection is admitted and promoted.
    #[test]
    fn loopback_is_never_pre_auth() {
        let limiter = limiter(ConnectionLimits {
            per_peer_pre_auth: 1,
            remote_pre_auth: 1,
            ..SMALL
        });
        let remote = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        let local = limiter.try_acquire(ip("127.0.0.1")).unwrap();
        local.promote();
        assert_eq!(pools(&limiter), (1, 0, 1));
        drop([remote, local]);
    }

    /// Goal: a trusted proxy carries many clients on one address, so neither per-peer limit
    /// applies to it, in either pool, while both pools still do. Method: per-peer limits of
    /// one; the proxy fills both pools alone, while an untrusted peer stops at one.
    #[test]
    fn trusted_proxy_is_exempt_from_the_per_peer_limits() {
        let limits = ConnectionLimits {
            per_peer: 1,
            per_peer_pre_auth: 1,
            remote: 2,
            remote_pre_auth: 3,
            loopback: 1,
        };
        let trusted = Arc::new(TrustedProxies::from_config(&["192.0.2.1".to_string()]));
        let limiter = ConnectionLimiter::new(limits, trusted);
        let proxied: Vec<_> = (0..3)
            .map(|_| limiter.try_acquire(ip("192.0.2.1")).unwrap())
            .collect();
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_none());
        proxied.iter().for_each(ConnectionPermit::promote);
        assert_eq!(pools(&limiter), (1, 2, 0));
        let more: Vec<_> = (0..2)
            .map(|_| limiter.try_acquire(ip("192.0.2.1")).unwrap())
            .collect();
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_none());
        drop(more);
        let untrusted = limiter.try_acquire(ip("192.0.2.2")).unwrap();
        assert!(limiter.try_acquire(ip("192.0.2.2")).is_none());
        drop(untrusted);
        drop(proxied);
    }

    /// Goal: a slot frees from whichever pool its connection is in when it ends, and
    /// nothing is left behind once every connection has ended. Method: one connection per
    /// pool, dropped in turn, with the pools checked after each.
    #[test]
    fn permits_release_on_drop() {
        let limiter = limiter(SMALL);
        let pre_auth = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        let promoted = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        promoted.promote();
        let local = limiter.try_acquire(ip("127.0.0.1")).unwrap();
        assert_eq!(pools(&limiter), (1, 1, 1));
        drop(promoted);
        assert_eq!(pools(&limiter), (1, 0, 1));
        drop(pre_auth);
        assert_eq!(pools(&limiter), (0, 0, 1));
        drop(local);
        assert_eq!(pools(&limiter), (0, 0, 0));
        assert!(limiter.lock().remote_by_peer.is_empty());
    }

    /// Goal: `promote_connection` promotes the connection a request carries, and is a no-op
    /// for a request that came through no guard. Method: empty extensions, then extensions
    /// holding a pre-auth permit.
    #[test]
    fn promote_connection_reads_the_request_extension() {
        let limiter = limiter(SMALL);
        promote_connection(&Extensions::new());
        let mut extensions = Extensions::new();
        let permit = Arc::new(limiter.try_acquire(ip("192.0.2.1")).unwrap());
        extensions.insert(AdmittedConnection(Arc::clone(&permit)));
        assert_eq!(pools(&limiter), (1, 0, 0));
        promote_connection(&extensions);
        assert_eq!(pools(&limiter), (0, 1, 0));
        drop(extensions);
        drop(permit);
        assert_eq!(pools(&limiter), (0, 0, 0));
    }

    /// Goal: a route that authenticates in its handler promotes its connection only on
    /// success. Method: `/login` stand-ins answering 401 then 200 on one connection.
    #[tokio::test]
    async fn only_a_successful_login_promotes() {
        use axum::http::StatusCode;
        use tower::ServiceExt as _;
        let limiter = limiter(SMALL);
        let permit = Arc::new(limiter.try_acquire(ip("192.0.2.1")).unwrap());
        let app = Router::new()
            .route("/rejected", get(|| async { StatusCode::UNAUTHORIZED }))
            .route("/accepted", get(|| async { StatusCode::OK }))
            .layer(axum::middleware::from_fn(promote_on_success_middleware));
        for (uri, pools_after) in [("/rejected", (1, 0, 0)), ("/accepted", (0, 1, 0))] {
            let mut request = Request::get(uri).body(Body::empty()).unwrap();
            request
                .extensions_mut()
                .insert(AdmittedConnection(Arc::clone(&permit)));
            app.clone().oneshot(request).await.unwrap();
            assert_eq!(pools(&limiter), pools_after);
        }
    }

    /// Sends a request and returns whatever arrives before the server closes. A refused
    /// connection may also be reset, which reads as nothing received.
    async fn exchange(stream: &mut TcpStream) -> Vec<u8> {
        let _ = stream
            .write_all(b"GET / HTTP/1.1\r\nhost: cc.lan\r\nconnection: close\r\n\r\n")
            .await;
        let mut received = Vec::new();
        let _ = timeout(NEVER, stream.read_to_end(&mut received))
            .await
            .expect("the connection must end");
        received
    }

    /// Goal: through the real server, a connection over the limit is closed without being
    /// served, and the slot is reusable once the holder disconnects. Method: a loopback pool
    /// of one; the held connection sends nothing until the second has been refused.
    #[tokio::test]
    async fn over_limit_connection_is_refused_until_a_slot_frees() {
        let address = serve_limited(app(), loopback_limiter(1)).await;
        let mut held = TcpStream::connect(address).await.unwrap();
        let mut refused = TcpStream::connect(address).await.unwrap();
        assert!(exchange(&mut refused).await.is_empty());
        assert!(String::from_utf8_lossy(&exchange(&mut held).await).starts_with("HTTP/1.1 200"));
        drop(held);

        let started = std::time::Instant::now();
        loop {
            let mut next = TcpStream::connect(address).await.unwrap();
            if exchange(&mut next).await.is_empty().not() {
                break;
            }
            assert!(started.elapsed() < NEVER, "the slot was never freed");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Goal: the header timeout never cuts a response that is still streaming, which is what
    /// SSE and slow LCD work look like on the wire. Method: a body that trickles out for over
    /// twice the timeout arrives whole.
    #[tokio::test]
    async fn streaming_response_outlives_the_header_timeout() {
        let address = serve(app()).await;
        let mut stream = request(address, "/stream").await;
        let mut received = Vec::new();
        let mut buffer = [0_u8; 1024];
        let last_chunk = format!("chunk{};", STREAMED_CHUNKS - 1);
        let started = std::time::Instant::now();
        while String::from_utf8_lossy(&received)
            .contains(&last_chunk)
            .not()
        {
            let read = timeout(NEVER, stream.read(&mut buffer))
                .await
                .unwrap()
                .unwrap();
            assert!(read > 0, "the stream was cut after {:?}", started.elapsed());
            received.extend_from_slice(&buffer[..read]);
        }
        assert!(started.elapsed() > TEST_TIMEOUTS.header_read * 2);
    }

    /// Long enough that an idle connection holds its slot for the whole test.
    const HOLDING_TIMEOUTS: ConnectionTimeouts = ConnectionTimeouts {
        first_bytes_timeout: Duration::from_secs(5),
        header_read: Duration::from_secs(5),
        h2_keep_alive_interval: Duration::from_secs(20),
    };

    const _: () = assert!(HOLDING_TIMEOUTS.header_read.as_secs() < NEVER.as_secs());

    /// Every connection in the remote tests comes from this one TEST-NET peer.
    fn remote_peer(_: SocketAddr) -> IpAddr {
        IpAddr::from([192, 0, 2, 50])
    }

    /// Serves `app()` through the production server with each connection charged to
    /// `remote_peer`, plus `/auth`, which authenticates every request the way the auth
    /// layers do.
    async fn serve_remote(limiter: Arc<ConnectionLimiter>) -> SocketAddr {
        let router = app().route(
            "/auth",
            get(|request: Request| async move {
                promote_connection(request.extensions());
                "ok"
            }),
        );
        serve_as(router, limiter, HOLDING_TIMEOUTS, remote_peer).await
    }

    /// Sends an authenticated request on a kept-alive connection and returns its response.
    async fn authenticate(stream: &mut TcpStream) -> String {
        stream
            .write_all(b"GET /auth HTTP/1.1\r\nhost: cc.lan\r\n\r\n")
            .await
            .unwrap();
        let mut received = Vec::new();
        let mut buffer = [0_u8; 1024];
        while String::from_utf8_lossy(&received)
            .contains("\r\n\r\nok")
            .not()
        {
            let read = timeout(NEVER, stream.read(&mut buffer))
                .await
                .expect("the response must arrive")
                .unwrap();
            assert!(read > 0, "the connection was closed");
            received.extend_from_slice(&buffer[..read]);
        }
        String::from_utf8_lossy(&received).into_owned()
    }

    /// Waits for the server side of connect, close and promotion to reach the limiter.
    async fn pools_reach(limiter: &ConnectionLimiter, expected: (usize, usize, usize)) {
        let started = std::time::Instant::now();
        while pools(limiter) != expected {
            assert!(
                started.elapsed() < NEVER,
                "pools stayed at {:?}",
                pools(limiter)
            );
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    /// Goal: through the real server, idle peers that never authenticate fill only the
    /// pre-auth pool, while a connection that authenticated keeps being served. Method:
    /// one connection authenticates, two idle ones fill a pre-auth pool of two, a third is
    /// refused, then the first is served again.
    #[tokio::test]
    async fn authenticated_connection_outlives_a_full_pre_auth_pool() {
        let limiter = limiter(ConnectionLimits {
            per_peer: 8,
            per_peer_pre_auth: 2,
            remote: 2,
            remote_pre_auth: 2,
            loopback: 1,
        });
        let address = serve_remote(Arc::clone(&limiter)).await;
        let mut signed_in = TcpStream::connect(address).await.unwrap();
        assert!(authenticate(&mut signed_in)
            .await
            .starts_with("HTTP/1.1 200"));
        pools_reach(&limiter, (0, 1, 0)).await;

        let idle = [
            TcpStream::connect(address).await.unwrap(),
            TcpStream::connect(address).await.unwrap(),
        ];
        pools_reach(&limiter, (2, 1, 0)).await;
        let mut refused = TcpStream::connect(address).await.unwrap();
        assert!(exchange(&mut refused).await.is_empty());
        assert!(authenticate(&mut signed_in)
            .await
            .starts_with("HTTP/1.1 200"));
        assert_eq!(pools(&limiter), (2, 1, 0));
        drop(idle);
    }

    /// Goal: through the real server, a connection that authenticates while the remote pool
    /// is full keeps being served from the pre-auth pool, and is promoted once a slot frees.
    /// Slots free from either pool when their connections close. Method: two connections
    /// authenticate into a remote pool of one; the first closes and the second tries again.
    #[tokio::test]
    async fn promotion_waits_for_room_in_the_remote_pool() {
        let limiter = limiter(ConnectionLimits {
            per_peer: 8,
            per_peer_pre_auth: 4,
            remote: 1,
            remote_pre_auth: 4,
            loopback: 1,
        });
        let address = serve_remote(Arc::clone(&limiter)).await;
        let mut first = TcpStream::connect(address).await.unwrap();
        assert!(authenticate(&mut first).await.starts_with("HTTP/1.1 200"));
        let mut second = TcpStream::connect(address).await.unwrap();
        assert!(authenticate(&mut second).await.starts_with("HTTP/1.1 200"));
        pools_reach(&limiter, (1, 1, 0)).await;

        drop(first);
        pools_reach(&limiter, (1, 0, 0)).await;
        assert!(authenticate(&mut second).await.starts_with("HTTP/1.1 200"));
        assert_eq!(pools(&limiter), (0, 1, 0));
        drop(second);
        pools_reach(&limiter, (0, 0, 0)).await;
        assert!(limiter.lock().remote_by_peer.is_empty());
    }
}
