// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Bounds on API connections before any request is authenticated.
//!
//! Everything here runs for peers that have proven nothing, so each connection must end on
//! its own if the peer stalls.

use hyper_util::rt::{TokioExecutor, TokioTimer};
use hyper_util::server::conn::auto::Builder;
use std::io;
use std::net::SocketAddr;
use std::ops::Not;
use std::time::Duration;

#[derive(Debug, Clone, Copy)]
struct HttpTimeouts {
    /// How long hyper waits for a complete request head, and for the next one on an idle
    /// keep-alive connection.
    header_read: Duration,
    /// How often an idle HTTP/2 connection is pinged. A peer that stops answering, such as
    /// one behind an expired NAT mapping, is dropped after hyper's 20 s ping timeout.
    h2_keep_alive_interval: Duration,
}

/// The header timeout is hyper's own default, which it silently drops without a timer.
const HTTP_TIMEOUTS: HttpTimeouts = HttpTimeouts {
    header_read: Duration::from_secs(30),
    h2_keep_alive_interval: Duration::from_secs(20),
};

/// The API server for `listener`, with the connection timeouts applied.
pub fn server(listener: std::net::TcpListener) -> io::Result<axum_server::Server<SocketAddr>> {
    server_with(listener, HTTP_TIMEOUTS)
}

fn server_with(
    listener: std::net::TcpListener,
    timeouts: HttpTimeouts,
) -> io::Result<axum_server::Server<SocketAddr>> {
    let mut server = axum_server::from_tcp(listener)?;
    configure_http(server.http_builder(), timeouts);
    Ok(server)
}

/// axum-server builds hyper's builder without a timer, and hyper ignores a default timeout
/// that has no timer to run it. So until both timers are set, a peer sending headers a
/// byte at a time, or an idle keep-alive connection, holds its slot forever. The timeouts
/// only run while hyper waits for a request head, never while a response streams, so SSE
/// and long LCD uploads are unaffected. Hyper panics on a configured timeout without a
/// timer, hence the timer on each protocol it is configured for.
fn configure_http(builder: &mut Builder<TokioExecutor>, timeouts: HttpTimeouts) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::routing::get;
    use axum::Router;
    use std::time::Instant;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;
    use tokio::time::timeout;

    /// Real time with short timeouts. A paused clock cannot measure these: it advances to
    /// the next timer before the runtime reads socket events, so a close is seen late.
    const TEST_TIMEOUTS: HttpTimeouts = HttpTimeouts {
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
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = server_with(listener.into_std().unwrap(), TEST_TIMEOUTS).unwrap();
        tokio::spawn(async move {
            server
                .serve(router.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap();
        });
        address
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
        let started = Instant::now();
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
        let started = Instant::now();
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
