// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Our custom Dual protocol support for HTTP and HTTPS on the same port.
//!
//! This module provides a custom acceptor that detects whether an incoming
//! connection is TLS or plain HTTP by peeking at the first byte, and handles
//! each protocol appropriately.
//!
//! When TLS is detected, the connection is handled via rustls.
//! When plain HTTP is detected from a non-localhost address, a redirect to HTTPS is sent.
//! Plain HTTP is allowed for:
//! - Requests from localhost (`127.0.0.1`, `::1`)
//! - Requests to the /health endpoint (handled at middleware level)
//!
//! This implementation can possibly be replaced with axum-server-dual-protocol in the future
//! once they support the current axum version and will work with our custom logic.

use crate::api::peer;
use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::uri::{Authority, PathAndQuery};
use axum::http::{header, Request, Response, StatusCode};
use axum::middleware::AddExtension;
use axum::Extension;
use axum_server::accept::Accept;
use axum_server::tls_rustls::{RustlsAcceptor, RustlsConfig};
use futures_util::future::BoxFuture;
use log::trace;
use pin_project_lite::pin_project;
use std::future::Future;
use std::io::{self, ErrorKind};
use std::net::SocketAddr;
use std::ops::Not;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadBuf};
use tower::{Layer, Service};

/// Protocol detected for the connection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// Plain HTTP connection
    Http,
    /// TLS/HTTPS connection
    Https,
}

/// A dual-protocol acceptor that handles both HTTP and HTTPS on the same port.
///
/// It peeks at the first byte of incoming connections to determine if they're
/// TLS (starts with 0x16) or plain HTTP (starts with ASCII letter like 'G' for GET).
#[derive(Clone)]
pub struct DualProtocolAcceptor {
    rustls_acceptor: RustlsAcceptor,
}

impl DualProtocolAcceptor {
    /// Create a new dual-protocol acceptor with the given TLS configuration.
    pub fn new(config: RustlsConfig) -> Self {
        Self {
            rustls_acceptor: RustlsAcceptor::new(config),
        }
    }
}

impl<I, S> Accept<I, S> for DualProtocolAcceptor
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    S: Send + 'static,
{
    type Stream = DualProtocolStream<I>;
    type Service = AddExtension<S, Protocol>;
    type Future = BoxFuture<'static, io::Result<(Self::Stream, Self::Service)>>;

    fn accept(&self, stream: I, service: S) -> Self::Future {
        let acceptor = self.rustls_acceptor.clone();

        Box::pin(async move {
            // Wrap the stream to peek at the first byte
            let mut peekable = PeekableStream::new(stream);

            // Peek at the first byte to detect protocol
            let first_byte = peekable.peek_first_byte().await?;
            let protocol = if is_tls_handshake(first_byte) {
                Protocol::Https
            } else {
                Protocol::Http
            };

            trace!("Detected protocol: {protocol:?} (first byte: 0x{first_byte:02x})");

            match protocol {
                Protocol::Https => {
                    // Handle TLS connection
                    let (tls_stream, service) = acceptor.accept(peekable, service).await?;
                    // Add protocol extension to service
                    let service = Extension(Protocol::Https).layer(service);
                    Ok((DualProtocolStream::Tls { inner: tls_stream }, service))
                }
                Protocol::Http => {
                    // Pass through plain HTTP with protocol extension
                    let service = Extension(protocol).layer(service);
                    Ok((DualProtocolStream::Plain { inner: peekable }, service))
                }
            }
        })
    }
}

/// Check if the first byte indicates a TLS handshake.
/// TLS handshakes start with a `ContentType` byte, where 0x16 (22) indicates a Handshake.
#[inline]
fn is_tls_handshake(first_byte: u8) -> bool {
    first_byte == 0x16
}

pin_project! {
    /// A stream that can be either plain HTTP or TLS.
    #[project = DualProtocolStreamProj]
    pub enum DualProtocolStream<I> {
        Plain {
            #[pin]
            inner: PeekableStream<I>,
        },
        Tls {
            #[pin]
            inner: tokio_rustls::server::TlsStream<PeekableStream<I>>,
        },
    }
}

impl<I> AsyncRead for DualProtocolStream<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            DualProtocolStreamProj::Plain { inner } => inner.poll_read(cx, buf),
            DualProtocolStreamProj::Tls { inner } => inner.poll_read(cx, buf),
        }
    }
}

impl<I> AsyncWrite for DualProtocolStream<I>
where
    I: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            DualProtocolStreamProj::Plain { inner } => inner.poll_write(cx, buf),
            DualProtocolStreamProj::Tls { inner } => inner.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            DualProtocolStreamProj::Plain { inner } => inner.poll_flush(cx),
            DualProtocolStreamProj::Tls { inner } => inner.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.project() {
            DualProtocolStreamProj::Plain { inner } => inner.poll_shutdown(cx),
            DualProtocolStreamProj::Tls { inner } => inner.poll_shutdown(cx),
        }
    }
}

pin_project! {
    /// A wrapper stream that allows peeking at the first byte without consuming it.
    pub struct PeekableStream<I> {
        #[pin]
        inner: I,
        peeked_byte: Option<u8>,
        buf: [u8; 1],
    }
}

impl<I> PeekableStream<I>
where
    I: AsyncRead + Unpin,
{
    fn new(inner: I) -> Self {
        Self {
            inner,
            peeked_byte: None,
            buf: [0],
        }
    }

    async fn peek_first_byte(&mut self) -> io::Result<u8> {
        if let Some(byte) = self.peeked_byte {
            return Ok(byte);
        }

        let n = self.inner.read(&mut self.buf).await?;
        if n == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "connection closed",
            ));
        }

        self.peeked_byte = Some(self.buf[0]);
        Ok(self.buf[0])
    }
}

impl<I> AsyncRead for PeekableStream<I>
where
    I: AsyncRead + Unpin,
{
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.project();

        // If we have a peeked byte, return it first
        if let Some(byte) = this.peeked_byte.take() {
            buf.put_slice(&[byte]);
            return Poll::Ready(Ok(()));
        }

        this.inner.poll_read(cx, buf)
    }
}

impl<I> AsyncWrite for PeekableStream<I>
where
    I: AsyncWrite + Unpin,
{
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.project().inner.poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.project().inner.poll_shutdown(cx)
    }
}

/// Layer that redirects HTTP requests to HTTPS, with exceptions for:
/// - Requests from localhost (`127.0.0.1`, `::1`)
/// - Requests to the /health or /see endpoints
/// - When `allow_unencrypted` is true
/// - When `protocol_header` indicates HTTPS from a proxy
#[derive(Clone)]
pub struct HttpsRedirectLayer {
    /// The port to redirect to (usually the same port)
    pub port: u16,
    /// Allow unencrypted HTTP connections from non-localhost addresses
    pub allow_unencrypted: bool,
    /// Header to check for proxy client protocol (e.g., "X-Forwarded-Proto")
    pub protocol_header: Option<String>,
}

impl<S> Layer<S> for HttpsRedirectLayer {
    type Service = HttpsRedirectService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        HttpsRedirectService {
            inner,
            port: self.port,
            allow_unencrypted: self.allow_unencrypted,
            protocol_header: self.protocol_header.clone(),
        }
    }
}

/// Service that redirects HTTP to HTTPS with exceptions
#[derive(Clone)]
pub struct HttpsRedirectService<S> {
    inner: S,
    port: u16,
    allow_unencrypted: bool,
    protocol_header: Option<String>,
}

impl<S> Service<Request<Body>> for HttpsRedirectService<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response<Body>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let mut inner = self.inner.clone();
        let port = self.port;
        let allow_unencrypted = self.allow_unencrypted;
        let protocol_header = self.protocol_header.clone();

        Box::pin(async move {
            // Check if connection is HTTPS (via Protocol extension)
            let is_https = req
                .extensions()
                .get::<Protocol>()
                .is_some_and(|p| *p == Protocol::Https);

            if is_https {
                return inner.call(req).await;
            }

            // Allow HTTP if /health or /sse endpoints
            let path = req.uri().path();
            if path.starts_with("/health") || path.starts_with("/sse") {
                return inner.call(req).await;
            }

            // Check if request is from localhost - allow HTTP
            if let Some(connect_info) = req.extensions().get::<ConnectInfo<SocketAddr>>() {
                if peer::is_loopback(connect_info.0.ip()) {
                    return inner.call(req).await;
                }
            }

            // Check if protocol header indicates HTTPS from a proxy
            if let Some(ref header_name) = protocol_header {
                if let Some(proto) = req.headers().get(header_name) {
                    if proto
                        .to_str()
                        .is_ok_and(|p| p.eq_ignore_ascii_case("https"))
                    {
                        return inner.call(req).await;
                    }
                }
            }

            // Allow unencrypted HTTP if configured
            if allow_unencrypted {
                return inner.call(req).await;
            }

            let redirect_uri = redirect_location(
                request_authority(&req).as_ref(),
                port,
                req.uri().path_and_query().map_or("/", PathAndQuery::as_str),
            );

            let response = Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header(header::LOCATION, redirect_uri)
                .body(Body::empty())
                .unwrap();

            Ok(response)
        })
    }
}

/// The authority the client addressed: `Host`, else the URI's own authority, which is where
/// HTTP/2 carries it. An unparseable or host-less value counts as absent.
fn request_authority(req: &Request<Body>) -> Option<Authority> {
    let has_host = |authority: &Authority| authority.host().is_empty().not();
    req.headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<Authority>().ok())
        .filter(has_host)
        .or_else(|| req.uri().authority().filter(|a| has_host(a)).cloned())
}

/// The HTTPS URL that serves the same resource.
///
/// The host is taken from a parsed authority rather than split on `:`, because an IPv6
/// literal such as `[::1]:11987` is itself full of colons. `Authority::host` keeps the
/// brackets, which the URL needs.
fn redirect_location(authority: Option<&Authority>, port: u16, path_and_query: &str) -> String {
    let host = authority.map_or("localhost", Authority::host);
    debug_assert!(host.is_empty().not());
    if port == 443 {
        format!("https://{host}{path_and_query}")
    } else {
        format!("https://{host}:{port}{path_and_query}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn authority(value: &str) -> Authority {
        value.parse().unwrap()
    }

    fn request(host_header: Option<&str>, uri: &str) -> Request<Body> {
        let mut builder = Request::builder().uri(uri);
        if let Some(value) = host_header {
            builder = builder.header(header::HOST, value);
        }
        builder.body(Body::empty()).unwrap()
    }

    /// Goal: IPv6 literals keep their brackets and lose only the port. Splitting on the first
    /// `:` used to turn `[::1]:11987` into `[`.
    #[test]
    fn redirect_keeps_ipv6_literal_hosts() {
        assert_eq!(
            redirect_location(Some(&authority("[::1]:11987")), 11987, "/devices"),
            "https://[::1]:11987/devices"
        );
        assert_eq!(
            redirect_location(Some(&authority("[fe80::1]")), 11987, "/"),
            "https://[fe80::1]:11987/"
        );
    }

    /// Goal: IPv4 and name hosts behave as before: the client's port is replaced by the
    /// listener's, and the query survives.
    #[test]
    fn redirect_replaces_the_port_for_ipv4_and_names() {
        assert_eq!(
            redirect_location(Some(&authority("192.168.1.5:11987")), 11987, "/a?b=c"),
            "https://192.168.1.5:11987/a?b=c"
        );
        assert_eq!(
            redirect_location(Some(&authority("cc.lan")), 8443, "/"),
            "https://cc.lan:8443/"
        );
    }

    /// Goal: the default HTTPS port is left implicit, and a missing host falls back to
    /// `localhost` rather than producing an empty authority.
    #[test]
    fn redirect_omits_443_and_defaults_the_host() {
        assert_eq!(
            redirect_location(Some(&authority("cc.lan:80")), 443, "/"),
            "https://cc.lan/"
        );
        assert_eq!(
            redirect_location(None, 11987, "/"),
            "https://localhost:11987/"
        );
    }

    /// Goal: `Host` wins, the URI authority covers HTTP/2 (which sends no `Host`), and an
    /// unparseable or empty value counts as absent.
    #[test]
    fn request_authority_prefers_host_then_uri() {
        let both = request(Some("[::1]:11987"), "http://other.lan/x");
        assert_eq!(request_authority(&both), Some(authority("[::1]:11987")));

        let uri_only = request(None, "http://[2001:db8::1]:11987/x");
        assert_eq!(
            request_authority(&uri_only),
            Some(authority("[2001:db8::1]:11987"))
        );

        let invalid = request(Some("not a host"), "/x");
        assert_eq!(request_authority(&invalid), None);

        let empty = request(Some(""), "/x");
        assert_eq!(request_authority(&empty), None);

        let neither = request(None, "/x");
        assert_eq!(request_authority(&neither), None);
    }

    /// A redirect layer in front of a service that always answers 200.
    async fn redirect_status(peer: &str) -> StatusCode {
        let service = HttpsRedirectLayer {
            port: 11987,
            allow_unencrypted: false,
            protocol_header: None,
        }
        .layer(tower::service_fn(|_request: Request<Body>| async {
            Ok::<_, std::convert::Infallible>(Response::new(Body::empty()))
        }));
        let mut request = request(Some("cc.lan:11987"), "/devices");
        let address: SocketAddr = peer.parse().unwrap();
        request.extensions_mut().insert(ConnectInfo(address));
        tower::ServiceExt::oneshot(service, request)
            .await
            .unwrap()
            .status()
    }

    /// Goal: a local IPv4 client seen through a dual-stack `::` listener keeps its plain HTTP
    /// exemption. Its address arrives as `::ffff:127.0.0.1`, which `is_loopback` rejects.
    #[tokio::test]
    async fn mapped_loopback_is_not_redirected() {
        assert_eq!(
            redirect_status("[::ffff:127.0.0.1]:40000").await,
            StatusCode::OK
        );
        assert_eq!(redirect_status("127.0.0.1:40000").await, StatusCode::OK);
        assert_eq!(redirect_status("[::1]:40000").await, StatusCode::OK);
    }

    /// Goal: a remote plain HTTP client is still redirected, in either address form.
    #[tokio::test]
    async fn remote_plain_http_is_redirected() {
        assert_eq!(
            redirect_status("[::ffff:192.0.2.1]:40000").await,
            StatusCode::MOVED_PERMANENTLY
        );
        assert_eq!(
            redirect_status("192.0.2.1:40000").await,
            StatusCode::MOVED_PERMANENTLY
        );
    }

    #[test]
    fn test_is_tls_handshake() {
        // TLS ClientHello starts with 0x16
        assert!(is_tls_handshake(0x16));

        // HTTP requests start with ASCII letters (GET, POST, etc.)
        assert!(!is_tls_handshake(b'G')); // GET
        assert!(!is_tls_handshake(b'P')); // POST, PUT, PATCH
        assert!(!is_tls_handshake(b'H')); // HEAD
        assert!(!is_tls_handshake(b'D')); // DELETE
        assert!(!is_tls_handshake(b'O')); // OPTIONS
        assert!(!is_tls_handshake(b'C')); // CONNECT
        assert!(!is_tls_handshake(b'T')); // TRACE
    }
}
