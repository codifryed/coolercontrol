/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

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

use axum::body::Body;
use axum::extract::ConnectInfo;
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
                let client_ip = connect_info.0.ip();
                if client_ip.is_loopback() {
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

            // Redirect to HTTPS
            let host = req
                .headers()
                .get(header::HOST)
                .and_then(|h| h.to_str().ok())
                .map_or("localhost", |h| {
                    // Remove port from host if present
                    h.split(':').next().unwrap_or(h)
                });

            let redirect_uri = if port == 443 {
                format!(
                    "https://{host}{}",
                    req.uri().path_and_query().map_or("/", |pq| pq.as_str())
                )
            } else {
                format!(
                    "https://{host}:{port}{}",
                    req.uri().path_and_query().map_or("/", |pq| pq.as_str())
                )
            };

            let response = Response::builder()
                .status(StatusCode::MOVED_PERMANENTLY)
                .header(header::LOCATION, redirect_uri)
                .body(Body::empty())
                .unwrap();

            Ok(response)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
