// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A blocking HTTP/1.1 client for one connection to liqctld.
//!
//! liqctld is Python's `ThreadingHTTPServer` with keep-alive: every reply carries
//! `Content-Length`, nothing is chunked, and idle connections stay open. That is all this client
//! speaks. It runs on a liqctld worker thread, never on the runtime, so it blocks in plain
//! syscalls bounded by socket timeouts.
use std::fmt;
use std::io::{self, ErrorKind, Read, Write};
use std::ops::{Not, Range};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use log::{debug, trace};
use serde::Serialize;
use thiserror::Error;

pub const LIQCTLD_SOCKET: &str = "/run/coolercontrold-liqctld.sock";
const LIQCTLD_HOST: &str = "127.0.0.1";
pub const LIQCTLD_CONNECTION_TRIES: usize = 10;
const LIQCTLD_CONNECTION_PAUSE: Duration = Duration::from_secs(1);
/// Socket read and write timeout. The backstop under every caller's own budget.
pub const LIQCTLD_RESPONSE_TIMEOUT: Duration = Duration::from_secs(15);
/// Resends allowed when the connection turns out to be closed before any reply arrives.
const LIQCTLD_EXPIRED_CONNECTION_RETRIES: usize = 7;

/// liqctld sends 5 headers. The rest is headroom.
const RESPONSE_HEADERS_MAX: usize = 16;
const RESPONSE_HEAD_BYTES_MAX: usize = 8 * 1024;
/// A device list is a few KiB and an error reply carries at most a Python traceback.
const RESPONSE_BODY_BYTES_MAX: usize = 1024 * 1024;
const READ_CHUNK_BYTES: usize = 4 * 1024;
const REQUEST_BYTES_INITIAL: usize = 512;
/// Signals in a row a single read may be interrupted by before giving up on the connection.
const READ_INTERRUPTED_TRIES_MAX: usize = 8;
const _: () = assert!(LIQCTLD_CONNECTION_TRIES > 0);
const _: () = assert!(LIQCTLD_EXPIRED_CONNECTION_RETRIES > 0);
const _: () = assert!(RESPONSE_HEAD_BYTES_MAX < RESPONSE_BODY_BYTES_MAX);

/// Where and how patiently to connect. Injectable so tests can use a temporary socket and short
/// timeouts.
#[derive(Clone, Debug)]
pub struct Endpoint {
    pub socket_path: PathBuf,
    pub connect_tries: usize,
    pub connect_pause: Duration,
    pub io_timeout: Duration,
}

impl Endpoint {
    /// The liqctld socket, with the daemon's standard timeouts.
    #[must_use]
    pub fn liqctld(connect_tries: usize) -> Self {
        assert!(connect_tries > 0);
        Self {
            socket_path: PathBuf::from(LIQCTLD_SOCKET),
            connect_tries,
            connect_pause: LIQCTLD_CONNECTION_PAUSE,
            io_timeout: LIQCTLD_RESPONSE_TIMEOUT,
        }
    }

    /// The longest one call can take against a service that is down: every connect try, then one
    /// full socket timeout.
    #[must_use]
    pub fn exchange_budget(&self) -> Duration {
        let tries = u32::try_from(self.connect_tries).unwrap_or(u32::MAX);
        self.connect_pause
            .saturating_mul(tries)
            .saturating_add(self.io_timeout)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Method {
    Get,
    Post,
    Put,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
            Self::Put => "PUT",
        }
    }
}

#[derive(Debug, Error)]
pub enum HttpError {
    /// Closed before the reply's first byte: an idle connection the server had already dropped.
    /// Safe to resend on a new connection, as the server never saw the request.
    #[error("liqctld closed the connection before answering")]
    Closed,
    /// The caller stopped waiting, so the request is not sent again.
    #[error("the caller no longer waits for this liqctld request")]
    Abandoned,
    #[error("liqctld did not answer within {0:?}")]
    TimedOut(Duration),
    #[error(
        "Failed to connect to coolercontrol-liqctld after {tries} tries: {source}. You can avoid \
         this warning by disabling Liquidctl integration in the Daemon settings."
    )]
    Connect { tries: usize, source: io::Error },
    #[error("Failed to get a live liqctld connection after {0} tries")]
    Exhausted(usize),
    #[error("liqctld connection error: {0}")]
    Io(#[from] io::Error),
    #[error("malformed liqctld response: {0}")]
    Malformed(&'static str),
    #[error("malformed liqctld response head: {0}")]
    Parse(#[from] httparse::Error),
    #[error("Liqctld Request failed with status:{status} - Body: {body}")]
    Status { status: u16, body: String },
    #[error("liqctld JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// One connection to liqctld, opened on first use and reopened when the server drops it.
pub struct LiqctldHttp {
    endpoint: Endpoint,
    stream: Option<UnixStream>,
    body: Vec<u8>,
    request: Vec<u8>,
    response: Vec<u8>,
}

impl LiqctldHttp {
    #[must_use]
    pub fn new(endpoint: Endpoint) -> Self {
        debug_assert!(endpoint.connect_tries > 0);
        debug_assert!(endpoint.io_timeout > Duration::ZERO);
        Self {
            endpoint,
            stream: None,
            body: Vec::with_capacity(REQUEST_BYTES_INITIAL),
            request: Vec::with_capacity(REQUEST_BYTES_INITIAL),
            response: Vec::with_capacity(READ_CHUNK_BYTES),
        }
    }

    /// Sends one request and returns the body of its 2xx reply.
    ///
    /// `abandoned` is asked before every connect and send. A request whose caller has gone must
    /// not reach a service that was restarted meanwhile: a late `/quit` would stop the new one.
    pub fn exchange<B: Serialize>(
        &mut self,
        method: Method,
        path: fmt::Arguments<'_>,
        body: Option<&B>,
        abandoned: &dyn Fn() -> bool,
    ) -> Result<&[u8], HttpError> {
        self.body.clear();
        if let Some(body) = body {
            serde_json::to_writer(&mut self.body, body)?;
        }
        render_request(
            &mut self.request,
            method,
            path,
            body.map(|_| self.body.as_slice()),
        )?;
        debug_assert!(self.request.is_empty().not());
        let mut head = None;
        for _ in 0..LIQCTLD_EXPIRED_CONNECTION_RETRIES {
            if abandoned() {
                return Err(HttpError::Abandoned);
            }
            // Owned for the attempt and only put back once a whole reply was read. A timeout or
            // a bad reply leaves the connection in an unknown state, so it is dropped.
            let mut stream = match self.stream.take() {
                Some(stream) => stream,
                None => connect(&self.endpoint, abandoned)?,
            };
            match send_and_receive(&mut stream, &self.request, &mut self.response) {
                Ok(received) => {
                    self.stream = Some(stream);
                    head = Some(received);
                    break;
                }
                Err(HttpError::Closed) => {
                    debug!("liqctld connection was closed; reconnecting and resending.");
                }
                Err(err) => return Err(err),
            }
        }
        let head = head.ok_or(HttpError::Exhausted(LIQCTLD_EXPIRED_CONNECTION_RETRIES))?;
        debug_assert!(self.stream.is_some());
        successful_body(&head, &self.response)
    }

    /// Closes the connection. The next exchange opens a new one.
    pub fn disconnect(&mut self) {
        if self.stream.take().is_some() {
            trace!("Closed the liqctld connection.");
        }
    }
}

/// Connects within the endpoint's tries, pausing between them.
///
/// liqctld's startup device scan can keep it from listening for a while, which is what the
/// retries are for.
fn connect(endpoint: &Endpoint, abandoned: &dyn Fn() -> bool) -> Result<UnixStream, HttpError> {
    let tries = endpoint.connect_tries;
    debug_assert!(tries > 0);
    let mut last_err = None;
    for attempt in 1..=tries {
        if attempt > 1 {
            thread::sleep(endpoint.connect_pause);
            if abandoned() {
                return Err(HttpError::Abandoned);
            }
        }
        match UnixStream::connect(&endpoint.socket_path) {
            Ok(stream) => {
                stream.set_read_timeout(Some(endpoint.io_timeout))?;
                stream.set_write_timeout(Some(endpoint.io_timeout))?;
                trace!("Created a new liqctld connection.");
                return Ok(stream);
            }
            Err(err) => {
                debug!(
                    "Could not establish socket connection to coolercontrol-liqctld, retry \
                     #{attempt} - {err}"
                );
                last_err = Some(err);
            }
        }
    }
    let source = last_err.unwrap_or_else(|| io::Error::other("no connect attempt was made"));
    Err(HttpError::Connect { tries, source })
}

/// Writes the request line and headers, then the body if there is one.
fn render_request(
    out: &mut Vec<u8>,
    method: Method,
    path: fmt::Arguments<'_>,
    body: Option<&[u8]>,
) -> io::Result<()> {
    debug_assert!(body.is_none() || method != Method::Get);
    out.clear();
    write!(
        out,
        "{} {path} HTTP/1.1\r\nHost: {LIQCTLD_HOST}\r\nConnection: keep-alive\r\n",
        method.as_str()
    )?;
    if let Some(body) = body {
        write!(out, "Content-Length: {}\r\n\r\n", body.len())?;
        out.extend_from_slice(body);
    } else {
        out.extend_from_slice(b"\r\n");
    }
    Ok(())
}

fn send_and_receive(
    stream: &mut UnixStream,
    request: &[u8],
    response: &mut Vec<u8>,
) -> Result<ResponseHead, HttpError> {
    debug_assert!(request.is_empty().not());
    if let Err(err) = stream.write_all(request) {
        return Err(match err.kind() {
            // The server never read the request, so it is safe to resend.
            ErrorKind::BrokenPipe | ErrorKind::ConnectionReset => HttpError::Closed,
            _ => io_error(err, stream),
        });
    }
    read_response(stream, response).map_err(|err| match err {
        HttpError::Io(io_err) => io_error(io_err, stream),
        other => other,
    })
}

/// Names a socket timeout as such, since the kernel reports `SO_RCVTIMEO` as `EAGAIN`.
fn io_error(err: io::Error, stream: &UnixStream) -> HttpError {
    match err.kind() {
        ErrorKind::WouldBlock | ErrorKind::TimedOut => {
            let timeout = stream.read_timeout().ok().flatten().unwrap_or_default();
            HttpError::TimedOut(timeout)
        }
        _ => HttpError::Io(err),
    }
}

/// Where a reply's parts sit in the response buffer.
#[derive(Debug, Eq, PartialEq)]
struct ResponseHead {
    status: u16,
    body: Range<usize>,
}

/// Reads one reply: its head, then a body of exactly `Content-Length` bytes.
///
/// `buf` is reused across replies and only ever grows to the largest one seen.
fn read_response(stream: &mut impl Read, buf: &mut Vec<u8>) -> Result<ResponseHead, HttpError> {
    let mut filled = 0;
    let (head_len, status, content_length) = loop {
        if filled > 0 {
            if let Some(head) = parse_head(&buf[..filled])? {
                break head;
            }
        }
        if filled >= RESPONSE_HEAD_BYTES_MAX {
            return Err(HttpError::Malformed("response head too large"));
        }
        let wanted = filled + READ_CHUNK_BYTES;
        if read_into(stream, buf, &mut filled, wanted)? == 0 {
            if filled == 0 {
                return Err(HttpError::Closed);
            }
            return Err(HttpError::Malformed("closed inside the response head"));
        }
    };
    let body_end = head_len + content_length;
    while filled < body_end {
        if read_into(stream, buf, &mut filled, body_end)? == 0 {
            return Err(HttpError::Malformed("closed inside the response body"));
        }
    }
    if filled > body_end {
        // Requests are never pipelined, so these bytes would be read as the next reply.
        return Err(HttpError::Malformed("bytes after the response body"));
    }
    debug_assert!(body_end <= buf.len());
    Ok(ResponseHead {
        status,
        body: head_len..body_end,
    })
}

/// One read into `buf[filled..until]`, growing `buf` to fit. Returns the bytes read.
fn read_into(
    stream: &mut impl Read,
    buf: &mut Vec<u8>,
    filled: &mut usize,
    until: usize,
) -> Result<usize, HttpError> {
    debug_assert!(*filled < until);
    if buf.len() < until {
        buf.resize(until, 0);
    }
    for _ in 0..READ_INTERRUPTED_TRIES_MAX {
        match stream.read(&mut buf[*filled..until]) {
            Ok(read) => {
                *filled += read;
                return Ok(read);
            }
            Err(err) if err.kind() == ErrorKind::Interrupted => {}
            Err(err) if *filled == 0 && err.kind() == ErrorKind::ConnectionReset => {
                return Err(HttpError::Closed);
            }
            Err(err) => return Err(HttpError::Io(err)),
        }
    }
    Err(HttpError::Io(io::Error::new(
        ErrorKind::Interrupted,
        format!("read interrupted {READ_INTERRUPTED_TRIES_MAX} times in a row"),
    )))
}

/// The head's length, status and `Content-Length`, or `None` while it is incomplete.
fn parse_head(bytes: &[u8]) -> Result<Option<(usize, u16, usize)>, HttpError> {
    let mut headers = [httparse::EMPTY_HEADER; RESPONSE_HEADERS_MAX];
    let mut response = httparse::Response::new(&mut headers);
    let head_len = match response.parse(bytes)? {
        httparse::Status::Complete(head_len) => head_len,
        httparse::Status::Partial => return Ok(None),
    };
    let status = response
        .code
        .ok_or(HttpError::Malformed("no status code"))?;
    let content_length = content_length(response.headers)?;
    debug_assert!(head_len <= bytes.len());
    Ok(Some((head_len, status, content_length)))
}

fn content_length(headers: &[httparse::Header<'_>]) -> Result<usize, HttpError> {
    let mut found = None;
    for header in headers {
        if header.name.eq_ignore_ascii_case("content-length").not() {
            continue;
        }
        if found.is_some() {
            return Err(HttpError::Malformed("more than one Content-Length"));
        }
        let length = std::str::from_utf8(header.value)
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .ok_or(HttpError::Malformed("Content-Length is not a number"))?;
        found = Some(length);
    }
    let length = found.ok_or(HttpError::Malformed("no Content-Length"))?;
    if length > RESPONSE_BODY_BYTES_MAX {
        return Err(HttpError::Malformed("Content-Length over the bound"));
    }
    Ok(length)
}

/// The body of a 2xx reply, or the status and body as an error.
fn successful_body<'a>(head: &ResponseHead, buf: &'a [u8]) -> Result<&'a [u8], HttpError> {
    let body = &buf[head.body.clone()];
    if (200..300).contains(&head.status) {
        trace!(
            "liqctld response {}: {}",
            head.status,
            String::from_utf8_lossy(body)
        );
        return Ok(body);
    }
    Err(HttpError::Status {
        status: head.status,
        body: String::from_utf8_lossy(body).into_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// A reader that hands out the given chunks one per read, then EOF.
    struct Chunks(VecDeque<Vec<u8>>);

    impl Chunks {
        fn of(chunks: &[&[u8]]) -> Self {
            Self(chunks.iter().map(|chunk| chunk.to_vec()).collect())
        }
    }

    impl Read for Chunks {
        fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
            let Some(mut chunk) = self.0.pop_front() else {
                return Ok(0);
            };
            let len = chunk.len().min(out.len());
            out[..len].copy_from_slice(&chunk[..len]);
            if len < chunk.len() {
                self.0.push_front(chunk.split_off(len));
            }
            Ok(len)
        }
    }

    /// A reader interrupted on its first `interruptions` reads, then at EOF.
    struct Interrupting {
        interruptions: usize,
        reads: usize,
    }

    impl Read for Interrupting {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            self.reads += 1;
            if self.reads <= self.interruptions {
                return Err(ErrorKind::Interrupted.into());
            }
            Ok(0)
        }
    }

    /// Goal: a read interrupted by a signal is retried, but not forever. Method: one reader
    /// interrupted just under the limit, which must reach its EOF, and one interrupted every
    /// time, which must fail after exactly the limit.
    #[test]
    fn interrupted_reads_are_retried_up_to_the_limit() {
        let mut buf = Vec::new();
        let mut recovers = Interrupting {
            interruptions: READ_INTERRUPTED_TRIES_MAX - 1,
            reads: 0,
        };
        let mut filled = 0;
        assert_eq!(
            read_into(&mut recovers, &mut buf, &mut filled, 1).unwrap(),
            0
        );
        assert_eq!(recovers.reads, READ_INTERRUPTED_TRIES_MAX);

        let mut stuck = Interrupting {
            interruptions: usize::MAX,
            reads: 0,
        };
        let err = read_into(&mut stuck, &mut buf, &mut filled, 1).unwrap_err();
        assert!(
            matches!(&err, HttpError::Io(io) if io.kind() == ErrorKind::Interrupted),
            "{err}"
        );
        assert_eq!(stuck.reads, READ_INTERRUPTED_TRIES_MAX);
    }

    fn reply(status_line: &str, body: &str) -> Vec<u8> {
        format!(
            "{status_line}\r\nServer: BasicHTTP/1.0\r\nContent-type: application/json\r\n\
             Content-Length: {}\r\nConnection: keep-alive\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn body_of(head: &ResponseHead, buf: &[u8]) -> String {
        String::from_utf8(buf[head.body.clone()].to_vec()).unwrap()
    }

    /// Goal: a socket hands a reply over in whatever pieces it likes, so a head cut mid-line and
    /// a body cut in two must reassemble to the same reply. Method: feed one liqctld reply in
    /// pieces split inside the head and inside the body.
    #[test]
    fn a_reply_split_across_reads_is_reassembled() {
        let bytes = reply(
            "HTTP/1.1 200 OK",
            r#"{"status": [["fan speed", "800", "rpm"]]}"#,
        );
        let head_end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
        let mut chunks = Chunks::of(&[
            &bytes[..10],
            &bytes[10..head_end - 1],
            &bytes[head_end - 1..head_end + 5],
            &bytes[head_end + 5..],
        ]);
        let mut buf = Vec::new();

        let head = read_response(&mut chunks, &mut buf).unwrap();

        assert_eq!(head.status, 200);
        assert_eq!(
            body_of(&head, &buf),
            r#"{"status": [["fan speed", "800", "rpm"]]}"#
        );
    }

    /// Goal: the buffer is reused, so a short reply after a long one must not carry the long
    /// one's tail. Method: read a long reply, then a short one, into the same buffer.
    #[test]
    fn a_reused_buffer_holds_only_the_latest_reply() {
        let mut buf = Vec::new();
        let long = reply("HTTP/1.1 200 OK", &"x".repeat(9_000));
        read_response(&mut Chunks::of(&[&long]), &mut buf).unwrap();

        let short = reply("HTTP/1.1 200 OK", "{}");
        let head = read_response(&mut Chunks::of(&[&short]), &mut buf).unwrap();

        assert_eq!(body_of(&head, &buf), "{}");
    }

    /// Goal: without `Content-Length` the end of a keep-alive reply cannot be found, so such a
    /// reply is a protocol error, never a read until EOF. Method: a reply with no length header.
    #[test]
    fn a_reply_without_content_length_is_rejected() {
        let bytes = b"HTTP/1.1 200 OK\r\nConnection: keep-alive\r\n\r\n{}";
        let err = read_response(&mut Chunks::of(&[bytes]), &mut Vec::new()).unwrap_err();

        assert!(
            matches!(err, HttpError::Malformed("no Content-Length")),
            "{err}"
        );
    }

    /// Goal: a length past the bound must be refused before anything is allocated for it.
    /// Method: a head claiming a body one byte over the bound, and no body.
    #[test]
    fn an_oversize_content_length_is_rejected() {
        let bytes = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
            RESPONSE_BODY_BYTES_MAX + 1
        );
        let mut buf = Vec::new();
        let err = read_response(&mut Chunks::of(&[bytes.as_bytes()]), &mut buf).unwrap_err();

        assert!(
            matches!(err, HttpError::Malformed("Content-Length over the bound")),
            "{err}"
        );
        assert!(
            buf.len() < RESPONSE_BODY_BYTES_MAX,
            "nothing was sized for the claim"
        );
    }

    /// Goal: liqctld reports device errors as non-2xx replies with the reason in the body, and
    /// the caller's log needs both. Method: a 502 reply, through the status check.
    #[test]
    fn a_non_success_status_errors_with_its_body() {
        let bytes = reply("HTTP/1.1 502 Bad Gateway", "Device communication error");
        let mut buf = Vec::new();
        let head = read_response(&mut Chunks::of(&[&bytes]), &mut buf).unwrap();

        let err = successful_body(&head, &buf).unwrap_err();

        match err {
            HttpError::Status { status, body } => {
                assert_eq!(status, 502);
                assert_eq!(body, "Device communication error");
            }
            other => panic!("expected a status error, got {other}"),
        }
    }

    /// Goal: only a connection closed before any byte may be resent, since only then did the
    /// server certainly not act on it. A close inside a reply must not be. Method: EOF at once,
    /// then EOF inside the head and inside the body.
    #[test]
    fn only_a_close_before_any_byte_counts_as_closed() {
        let bytes = reply("HTTP/1.1 200 OK", r#"{"shake": true}"#);
        let head_end = bytes.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;

        let at_once = read_response(&mut Chunks::of(&[]), &mut Vec::new()).unwrap_err();
        let in_head = read_response(&mut Chunks::of(&[&bytes[..12]]), &mut Vec::new());
        let in_body = read_response(&mut Chunks::of(&[&bytes[..head_end + 3]]), &mut Vec::new());

        assert!(matches!(at_once, HttpError::Closed), "{at_once}");
        assert!(
            matches!(in_head, Err(HttpError::Malformed(_))),
            "{:?}",
            in_head.map(|_| ())
        );
        assert!(
            matches!(in_body, Err(HttpError::Malformed(_))),
            "{:?}",
            in_body.map(|_| ())
        );
    }

    /// Goal: requests are never pipelined, so bytes past the body can only be a broken reply,
    /// and reading them as the next one would hand one device's status to the next call.
    /// Method: a reply followed by stray bytes in the same read.
    #[test]
    fn bytes_after_the_body_are_rejected() {
        let mut bytes = reply("HTTP/1.1 200 OK", "{}");
        bytes.extend_from_slice(b"HTTP/1.1");

        let err = read_response(&mut Chunks::of(&[&bytes]), &mut Vec::new()).unwrap_err();

        assert!(
            matches!(err, HttpError::Malformed("bytes after the response body")),
            "{err}"
        );
    }

    /// Goal: liqctld routes by path, keeps the connection by header and reads a body by
    /// `Content-Length`, so all three must be exact. Method: render a GET and a PUT and compare
    /// the bytes.
    #[test]
    fn a_request_names_its_path_host_and_body_length() {
        let mut out = Vec::new();
        render_request(
            &mut out,
            Method::Get,
            format_args!("/devices/{}/status", 3),
            None,
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out.clone()).unwrap(),
            "GET /devices/3/status HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\r\n"
        );

        let body = br#"{"channel":"fan1","duty":40}"#;
        render_request(
            &mut out,
            Method::Put,
            format_args!("/devices/{}/speed/fixed", 0),
            Some(body),
        )
        .unwrap();
        assert_eq!(
            String::from_utf8(out).unwrap(),
            "PUT /devices/0/speed/fixed HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: keep-alive\r\n\
             Content-Length: 28\r\n\r\n{\"channel\":\"fan1\",\"duty\":40}"
        );
    }

    /// Goal: the control budget must cover every connect try plus one full exchange, or a call
    /// made while liqctld starts gives up before its last try. Method: compute it for the
    /// standard endpoint.
    #[test]
    fn the_exchange_budget_covers_every_connect_try() {
        let endpoint = Endpoint::liqctld(LIQCTLD_CONNECTION_TRIES);

        assert_eq!(endpoint.exchange_budget(), Duration::from_secs(25));
    }
}
