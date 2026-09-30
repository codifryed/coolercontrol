// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounds on API connections before any request is authenticated.
//!
//! Everything here runs for peers that have proven nothing, so each connection must end on
//! its own if the peer stalls.

use crate::api::peer::{PeerKey, TrustedProxies};
use axum_server::accept::Accept;
use futures_util::future::BoxFuture;
use hyper_util::rt::{TokioExecutor, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use log::{debug, info};
use pin_project_lite::pin_project;
use std::collections::HashMap;
use std::future::Future;
use std::io::{self, ErrorKind, IoSlice};
use std::net::{IpAddr, SocketAddr};
use std::ops::Not;
use std::pin::Pin;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tokio::time::{Instant, Sleep};

/// Plaintext bytes after which a connection has committed to a protocol: the length of the
/// HTTP/2 preface, the most hyper-util's version sniff ever waits for.
const FIRST_BYTES_COUNT: usize = 24;

/// Concurrent connections one remote peer may hold. A browser opens at most six per host
/// over HTTP/1.1, and a whole IPv6 /64 shares this, so it leaves room for a household.
const PER_PEER_CONNECTIONS: usize = 64;
/// Concurrent connections all remote peers may hold together.
const REMOTE_CONNECTIONS: usize = 256;
/// Connections from this host, pooled apart so a remote flood cannot lock out the local app.
const LOOPBACK_CONNECTIONS: usize = 128;
/// Connections may hold at most one part in this many of the open file limit. The rest is
/// for the hardware, whose descriptor use grows with the number of devices.
const OPEN_FILES_SHARE: u64 = 4;

const _: () = assert!(PER_PEER_CONNECTIONS <= REMOTE_CONNECTIONS);
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

/// The API server for `listener`, with every connection admitted by `limiter`, accepted
/// through `acceptor`, then bounded by the guard. Taking both here keeps the guard on every
/// listener.
pub fn server<A>(
    listener: std::net::TcpListener,
    acceptor: A,
    limiter: Arc<ConnectionLimiter>,
) -> io::Result<axum_server::Server<SocketAddr, ConnectionGuardAcceptor<A>>> {
    server_with(listener, acceptor, limiter, TIMEOUTS)
}

fn server_with<A>(
    listener: std::net::TcpListener,
    acceptor: A,
    limiter: Arc<ConnectionLimiter>,
    timeouts: ConnectionTimeouts,
) -> io::Result<axum_server::Server<SocketAddr, ConnectionGuardAcceptor<A>>> {
    let mut server = axum_server::from_tcp(listener)?.acceptor(ConnectionGuardAcceptor {
        inner: acceptor,
        limiter,
        first_bytes_timeout: timeouts.first_bytes_timeout,
    });
    configure_http(server.http_builder(), timeouts);
    Ok(server)
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
    remote: usize,
    loopback: usize,
}

impl ConnectionLimits {
    const FULL: Self = Self {
        per_peer: PER_PEER_CONNECTIONS,
        remote: REMOTE_CONNECTIONS,
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
        let remote = (REMOTE_CONNECTIONS * budget / full_total).max(1);
        let loopback = (LOOPBACK_CONNECTIONS * budget / full_total).max(1);
        let limits = Self {
            per_peer: PER_PEER_CONNECTIONS.min(remote),
            remote,
            loopback,
        };
        debug_assert!(limits.total() <= budget.max(2));
        limits
    }

    fn total(self) -> usize {
        self.remote + self.loopback
    }
}

#[derive(Debug)]
struct ConnectionCounts {
    remote_by_peer: HashMap<PeerKey, usize>,
    remote: usize,
    loopback: usize,
}

impl ConnectionCounts {
    /// The `ConnectionLimiter` invariant. Walks the map, so for debug assertions only.
    fn remote_is_peer_sum(&self) -> bool {
        self.remote == self.remote_by_peer.values().sum::<usize>()
    }
}

/// Counts open connections against `ConnectionLimits`, shared by every listener.
///
/// Invariants: each count stays within its limit; `remote` is the sum of `remote_by_peer`,
/// which holds no zero entries, so it never has more than `limits.remote` keys.
#[derive(Debug)]
pub struct ConnectionLimiter {
    limits: ConnectionLimits,
    /// Exempt from the per-peer limit, since every client behind one shares its address.
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
                remote_by_peer: HashMap::with_capacity(limits.remote),
                remote: 0,
                loopback: 0,
            }),
        })
    }

    /// A permit for one more connection from `peer`, or `None` when a limit is reached.
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
        if counts.remote >= self.limits.remote {
            return None;
        }
        let per_peer_limit = if self.trusted_proxies.contains(peer) {
            self.limits.remote
        } else {
            self.limits.per_peer
        };
        let per_peer = counts.remote_by_peer.entry(key).or_insert(0);
        if *per_peer >= per_peer_limit {
            return None;
        }
        *per_peer += 1;
        counts.remote += 1;
        debug_assert!(counts.remote_by_peer.len() <= self.limits.remote);
        debug_assert!(counts.remote_is_peer_sum());
        Some(self.permit(key))
    }

    fn permit(self: &Arc<Self>, key: PeerKey) -> ConnectionPermit {
        ConnectionPermit {
            limiter: Arc::clone(self),
            key,
        }
    }

    fn release(&self, key: PeerKey) {
        let mut counts = self.lock();
        if key == PeerKey::Loopback {
            debug_assert!(counts.loopback > 0);
            counts.loopback = counts.loopback.saturating_sub(1);
            return;
        }
        debug_assert!(counts.remote > 0);
        debug_assert!(counts.remote_is_peer_sum());
        counts.remote = counts.remote.saturating_sub(1);
        if let Some(per_peer) = counts.remote_by_peer.get_mut(&key) {
            *per_peer = per_peer.saturating_sub(1);
            if *per_peer == 0 {
                counts.remote_by_peer.remove(&key);
            }
        }
        debug_assert!(counts.remote_is_peer_sum());
    }

    /// A poisoned limiter must not refuse every connection from then on. The counts may be
    /// off by the one update in flight when it poisoned, which is far better than an outage.
    fn lock(&self) -> MutexGuard<'_, ConnectionCounts> {
        self.counts.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// One admitted connection. Dropping it, when the connection ends, frees its slot.
#[derive(Debug)]
pub struct ConnectionPermit {
    limiter: Arc<ConnectionLimiter>,
    key: PeerKey,
}

impl Drop for ConnectionPermit {
    fn drop(&mut self) {
        self.limiter.release(self.key);
    }
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
}

impl<A, S> Accept<TcpStream, S> for ConnectionGuardAcceptor<A>
where
    A: Accept<TcpStream, S>,
    A::Future: Send + 'static,
    A::Stream: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    A::Service: Send + 'static,
{
    type Stream = GuardedStream<A::Stream>;
    type Service = A::Service;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    /// Over-limit connections are dropped here, before any TLS or HTTP work, which closes
    /// them at once. The permit then travels with the stream, so the slot frees whenever the
    /// connection ends, however it ends.
    fn accept(&self, stream: TcpStream, service: S) -> Self::Future {
        let permit = stream
            .peer_addr()
            .ok()
            .and_then(|address| self.limiter.try_acquire(address.ip()));
        let Some(permit) = permit else {
            debug!("Refused an API connection over the connection limit.");
            return Box::pin(std::future::ready(Err(io::Error::other(
                "over the connection limit",
            ))));
        };
        let deadline = Instant::now() + self.first_bytes_timeout;
        let accepting = self.inner.accept(stream, service);
        Box::pin(async move {
            let Ok(accepted) = tokio::time::timeout_at(deadline, accepting).await else {
                return Err(first_bytes_timeout());
            };
            let (stream, service) = accepted?;
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
        permit: ConnectionPermit,
    }
}

impl<S> GuardedStream<S> {
    fn new(inner: S, deadline: Instant, permit: ConnectionPermit) -> Self {
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

    /// Serves `router` through the production server, the way `create_api_server` does.
    async fn serve(router: Router) -> SocketAddr {
        let limiter = ConnectionLimiter::new(ConnectionLimits::FULL, Arc::default());
        serve_limited(router, limiter).await
    }

    async fn serve_limited(router: Router, limiter: Arc<ConnectionLimiter>) -> SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let acceptor = DefaultAcceptor::new();
        let server = server_with(
            listener.into_std().unwrap(),
            acceptor,
            limiter,
            TEST_TIMEOUTS,
        )
        .unwrap();
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let acceptor = DualProtocolAcceptor::new(config);
        let limiter = ConnectionLimiter::new(ConnectionLimits::FULL, Arc::default());
        let server = server_with(
            listener.into_std().unwrap(),
            acceptor,
            limiter,
            TEST_TIMEOUTS,
        )
        .unwrap();
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

    fn limiter(per_peer: usize, remote: usize, loopback: usize) -> Arc<ConnectionLimiter> {
        let limits = ConnectionLimits {
            per_peer,
            remote,
            loopback,
        };
        ConnectionLimiter::new(limits, Arc::default())
    }

    fn ip(address: &str) -> IpAddr {
        address.parse().unwrap()
    }

    /// Goal: at a normal open file limit the full limits apply, and at a low one the pools
    /// shrink in proportion to stay within their share, never below one each.
    #[test]
    fn limits_scale_with_the_open_file_limit() {
        assert_eq!(
            ConnectionLimits::for_open_file_limit(524_288),
            ConnectionLimits::FULL
        );
        let tight = ConnectionLimits::for_open_file_limit(1024);
        assert!(tight.total() <= 1024 / 4);
        assert!(tight.remote > tight.loopback);
        assert_eq!(tight.per_peer, PER_PEER_CONNECTIONS);
        let tiny = ConnectionLimits::for_open_file_limit(40);
        assert!(tiny.total() <= 10);
        assert!(tiny.per_peer <= tiny.remote);
        let starved = ConnectionLimits::for_open_file_limit(0);
        assert_eq!(
            (starved.remote, starved.loopback, starved.per_peer),
            (1, 1, 1)
        );
    }

    /// Goal: one peer is held to its own limit, and an IPv6 /64 counts as one peer.
    #[test]
    fn per_peer_limit_holds() {
        let limiter = limiter(2, 10, 1);
        let held = [
            limiter.try_acquire(ip("2001:db8::1")).unwrap(),
            limiter.try_acquire(ip("2001:db8::2")).unwrap(),
        ];
        assert!(limiter.try_acquire(ip("2001:db8::3")).is_none());
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_some());
        drop(held);
    }

    /// Goal: a trusted proxy carries many clients on one address, so the per-peer limit
    /// does not apply to it, while the remote pool still does.
    #[test]
    fn trusted_proxy_is_exempt_from_the_per_peer_limit() {
        let limits = ConnectionLimits {
            per_peer: 1,
            remote: 3,
            loopback: 1,
        };
        let trusted = Arc::new(TrustedProxies::from_config(&["192.0.2.1".to_string()]));
        let limiter = ConnectionLimiter::new(limits, trusted);
        let held: Vec<_> = (0..3)
            .map(|_| limiter.try_acquire(ip("192.0.2.1")).unwrap())
            .collect();
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_none());
        drop(held);
        let untrusted = limiter.try_acquire(ip("192.0.2.2")).unwrap();
        assert!(limiter.try_acquire(ip("192.0.2.2")).is_none());
        drop(untrusted);
    }

    /// Goal: the remote pool caps all remote peers together, and loopback is admitted from
    /// its own pool even when the remote one is full.
    #[test]
    fn pools_are_independent() {
        let limiter = limiter(1, 2, 1);
        let remote = [
            limiter.try_acquire(ip("192.0.2.1")).unwrap(),
            limiter.try_acquire(ip("192.0.2.2")).unwrap(),
        ];
        assert!(limiter.try_acquire(ip("192.0.2.3")).is_none());
        let local = limiter.try_acquire(ip("127.0.0.1")).unwrap();
        assert!(limiter.try_acquire(ip("::1")).is_none());
        drop(local);
        assert!(limiter.try_acquire(ip("::ffff:127.0.0.1")).is_some());
        drop(remote);
    }

    /// Goal: a slot frees when its connection ends, and nothing is left behind once every
    /// connection has ended.
    #[test]
    fn permits_release_on_drop() {
        let limiter = limiter(1, 4, 1);
        let first = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        assert!(limiter.try_acquire(ip("192.0.2.1")).is_none());
        drop(first);
        let again = limiter.try_acquire(ip("192.0.2.1")).unwrap();
        let local = limiter.try_acquire(ip("127.0.0.1")).unwrap();
        drop(again);
        drop(local);
        let counts = limiter.lock();
        assert_eq!((counts.remote, counts.loopback), (0, 0));
        assert!(counts.remote_by_peer.is_empty());
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
        let address = serve_limited(app(), limiter(1, 1, 1)).await;
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
}
