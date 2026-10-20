// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! liqctld requests, isolated on threads of their own.
//!
//! One worker thread and one connection per liquidctl device, so the main thread does no socket
//! IO and a request stalled inside liqctld parks only its own device's thread. Health follows the
//! rules of a sysfs device: enough timeouts in a row take a device out of rotation until its next
//! probe.
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use log::{debug, trace};
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

#[cfg(all(test, feature = "gated-tests"))]
use crate::repositories::hwmon::device_io::DeviceHealth;
use crate::repositories::hwmon::device_io::{HealthState, QUEUE_DEPTH};
use crate::repositories::liquidctl::liqctld_client::StatusResponse;
use crate::repositories::liquidctl::liqctld_http::{Endpoint, LiqctldHttp, Method};
use crate::rt;

/// Stack for a worker thread. It runs one blocking loop, and liqctld's replies nest a few levels
/// deep at most, so the default 8 MiB is wasted address space once there is one per device.
const WORKER_STACK_BYTES: usize = 256 * 1024;
const THREAD_NAME_PREFIX: &str = "ccliq-";
/// The kernel caps thread names at 15 bytes, and a `u8` id adds at most 3.
const _: () = assert!(THREAD_NAME_PREFIX.len() + 3 <= 15);
const NO_BODY: Option<&()> = None;

type Reply<T> = oneshot::Sender<Result<T>>;

/// Work for a device's worker. Every payload is plain data, so the thread boundary needs no
/// shared ownership and no lock.
#[derive(Debug)]
pub enum Request {
    Status {
        reply: Reply<StatusResponse>,
    },
    /// Closes the connection, for when the service behind it is gone.
    Disconnect,
}

impl Request {
    /// Whether the caller stopped waiting. Such a request is not sent at all.
    fn is_abandoned(&self) -> bool {
        match self {
            Self::Status { reply } => reply.is_closed(),
            Self::Disconnect => false,
        }
    }
}

/// The main-thread half of one liquidctl device's worker.
pub struct LiqctldIo {
    tx: mpsc::Sender<Request>,
    status_timeout: Duration,
    state: HealthState,
}

impl LiqctldIo {
    /// Start the worker for the liqctld device `device_index`. `device_name` names it in logs.
    ///
    /// # Errors
    ///
    /// When the OS refuses the thread.
    pub fn spawn(
        device_index: u8,
        device_name: String,
        endpoint: Endpoint,
        status_timeout: Duration,
    ) -> Result<Self> {
        debug_assert!(status_timeout > Duration::ZERO);
        let (tx, rx) = mpsc::channel::<Request>(QUEUE_DEPTH);
        let thread_name = format!("{THREAD_NAME_PREFIX}{device_index}");
        debug!("liqctld worker starting: {thread_name}");
        let http = LiqctldHttp::new(endpoint);
        // Detached deliberately: a worker stuck in a socket read only returns at its timeout, so
        // a handle would only invite a `join` that cannot be bounded.
        thread::Builder::new()
            .name(thread_name)
            .stack_size(WORKER_STACK_BYTES)
            .spawn(move || run_worker(http, device_index, rx))?;
        Ok(Self {
            tx,
            status_timeout,
            state: HealthState::new(device_name),
        })
    }

    /// The device's current status, within the poll-rate budget.
    pub async fn status(&self) -> Result<StatusResponse> {
        self.dispatch("status", self.status_timeout, |reply| Request::Status {
            reply,
        })
        .await
    }

    /// Closes the worker's connection, so it reconnects on its next request.
    ///
    /// Best effort: a worker stuck in a read acts on it once the read returns.
    pub fn disconnect(&self) {
        if self.tx.try_send(Request::Disconnect).is_err() {
            debug!(
                "could not disconnect liquidctl device {}: worker busy or gone",
                self.state.device_name()
            );
        }
    }

    /// Whether this device is answering.
    #[cfg(all(test, feature = "gated-tests"))]
    #[must_use]
    pub fn health(&self) -> DeviceHealth {
        self.state.health()
    }

    /// Lets the next request through even if the device is currently unreachable, for when the
    /// service behind it was restarted.
    pub fn allow_probe_now(&self) {
        self.state.allow_next_dispatch();
    }

    /// Names the device in later logs.
    pub fn rename(&mut self, device_name: String) {
        self.state.rename(device_name);
    }

    /// Send one request and wait for its reply, within `budget`.
    ///
    /// Only a timeout counts against the device's health. An error coming back means liqctld
    /// answered, which says nothing about whether the device is wedged.
    async fn dispatch<T, F>(
        &self,
        what: &'static str,
        budget: Duration,
        make_request: F,
    ) -> Result<T>
    where
        F: FnOnce(Reply<T>) -> Request,
    {
        // Refuse to queue for a device that is not answering until its next probe is due, or
        // every tick queues a request nobody serves and pays a full timeout to learn that.
        if self.state.dispatchable().not() {
            return Err(unreachable(self.state.device_name(), what));
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let request = make_request(reply_tx);
        let exchange = async {
            self.tx
                .send(request)
                .await
                .map_err(|_| worker_gone(self.state.device_name()))?;
            reply_rx
                .await
                .map_err(|_| worker_gone(self.state.device_name()))?
        };
        match rt::timeout(budget, exchange).await {
            Ok(result) => {
                self.state.record_answered();
                result
            }
            Err(_elapsed) => {
                self.state.record_timeout(&what);
                Err(timed_out(self.state.device_name(), what, budget))
            }
        }
    }
}

/// The worker thread body: one connection, one blocking request at a time, until the last
/// handle is dropped and the queue closes.
fn run_worker(mut http: LiqctldHttp, device_index: u8, mut rx: mpsc::Receiver<Request>) {
    while let Some(request) = rx.blocking_recv() {
        if request.is_abandoned() {
            trace!("Skipping an abandoned request for liquidctl device #{device_index}");
            continue;
        }
        serve_one(&mut http, device_index, request);
    }
    debug!("liqctld worker for device #{device_index} stopped");
}

fn serve_one(http: &mut LiqctldHttp, device_index: u8, request: Request) {
    match request {
        Request::Status { reply } => {
            let result = call(
                http,
                Method::Get,
                format_args!("/devices/{device_index}/status"),
                NO_BODY,
                &|| reply.is_closed(),
            );
            // A dropped receiver means the caller already timed out. Expected, not an error.
            let _ = reply.send(result);
        }
        Request::Disconnect => http.disconnect(),
    }
}

/// One exchange, with its reply parsed here rather than on the main thread.
fn call<T, B>(
    http: &mut LiqctldHttp,
    method: Method,
    path: std::fmt::Arguments<'_>,
    body: Option<&B>,
    abandoned: &dyn Fn() -> bool,
) -> Result<T>
where
    T: DeserializeOwned,
    B: Serialize,
{
    let reply_body = http.exchange(method, path, body, abandoned)?;
    Ok(serde_json::from_slice(reply_body)?)
}

fn timed_out(device_name: &str, what: &str, budget: Duration) -> anyhow::Error {
    Error::new(
        ErrorKind::TimedOut,
        format!(
            "liquidctl device {device_name} did not answer {what} within {} ms",
            budget.as_millis()
        ),
    )
    .into()
}

fn unreachable(device_name: &str, what: &str) -> anyhow::Error {
    Error::new(
        ErrorKind::HostUnreachable,
        format!("liquidctl device {device_name} is not responding; skipping {what}"),
    )
    .into()
}

fn worker_gone(device_name: &str) -> anyhow::Error {
    Error::new(
        ErrorKind::BrokenPipe,
        format!("liqctld worker for liquidctl device {device_name} is gone"),
    )
    .into()
}

#[cfg(all(test, feature = "gated-tests"))]
mod tests {
    use super::*;
    use crate::repositories::hwmon::device_io::UNREACHABLE_AFTER_TIMEOUTS;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::os::unix::net::{UnixListener, UnixStream};
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{mpsc as std_mpsc, Arc, Mutex};

    const STATUS_BODY: &str = r#"{"status": [["Liquid temperature", "31.5", "°C"]]}"#;
    const LONG: Duration = Duration::from_secs(5);
    const EVENT_WAIT: Duration = Duration::from_secs(5);

    /// What the stand-in does with one request.
    enum Action {
        Reply,
        ReplyAndClose,
        /// Never answers, and keeps the connection open.
        Hold,
    }

    #[derive(Debug, PartialEq)]
    enum Event {
        Accepted,
        Request(String),
        Closed,
    }

    /// A liqctld stand-in on a socket in a temporary directory. Every connection is served on a
    /// thread of its own, and `handler` picks the action by request number.
    struct FakeLiqctld {
        _dir: tempfile::TempDir,
        socket_path: PathBuf,
        events: std_mpsc::Receiver<Event>,
    }

    impl FakeLiqctld {
        fn start(handler: impl Fn(usize) -> Action + Send + Sync + 'static) -> Self {
            let dir = tempfile::tempdir().unwrap();
            let socket_path = dir.path().join("liqctld.sock");
            let listener = UnixListener::bind(&socket_path).unwrap();
            let (events_tx, events) = std_mpsc::channel();
            let handler: Arc<dyn Fn(usize) -> Action + Send + Sync> = Arc::new(handler);
            let served = Arc::new(AtomicUsize::new(0));
            thread::spawn(move || {
                for stream in listener.incoming() {
                    let Ok(stream) = stream else { return };
                    let _ = events_tx.send(Event::Accepted);
                    let (handler, served, events_tx) =
                        (Arc::clone(&handler), Arc::clone(&served), events_tx.clone());
                    thread::spawn(move || serve_connection(stream, &*handler, &served, &events_tx));
                }
            });
            Self {
                _dir: dir,
                socket_path,
                events,
            }
        }

        fn endpoint(&self, io_timeout: Duration) -> Endpoint {
            Endpoint {
                socket_path: self.socket_path.clone(),
                connect_tries: 2,
                connect_pause: Duration::from_millis(10),
                io_timeout,
            }
        }

        fn next_event(&self) -> Event {
            self.events.recv_timeout(EVENT_WAIT).unwrap()
        }

        /// Every event so far, without waiting for more.
        fn drain(&self) -> Vec<Event> {
            self.events.try_iter().collect()
        }
    }

    fn serve_connection(
        stream: UnixStream,
        handler: &(dyn Fn(usize) -> Action + Send + Sync),
        served: &AtomicUsize,
        events: &std_mpsc::Sender<Event>,
    ) {
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        while let Some(request_line) = read_request(&mut reader) {
            let _ = events.send(Event::Request(request_line));
            match handler(served.fetch_add(1, Ordering::SeqCst)) {
                Action::Reply => write_reply(&mut writer),
                Action::ReplyAndClose => {
                    write_reply(&mut writer);
                    return;
                }
                Action::Hold => {}
            }
        }
        let _ = events.send(Event::Closed);
    }

    /// The request line, after reading the head and any body. `None` at EOF.
    fn read_request(reader: &mut BufReader<UnixStream>) -> Option<String> {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).ok()? == 0 {
            return None;
        }
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).ok()? == 0 {
                return None;
            }
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().unwrap();
            }
        }
        let mut body = vec![0; content_length];
        reader.read_exact(&mut body).ok()?;
        Some(request_line.trim_end().to_owned())
    }

    fn write_reply(writer: &mut UnixStream) {
        let reply = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{STATUS_BODY}",
            STATUS_BODY.len()
        );
        let _ = writer.write_all(reply.as_bytes());
    }

    fn worker(server: &FakeLiqctld, status_timeout: Duration, io_timeout: Duration) -> LiqctldIo {
        LiqctldIo::spawn(
            3,
            "Test Cooler".to_owned(),
            server.endpoint(io_timeout),
            status_timeout,
        )
        .unwrap()
    }

    fn kind_of(err: &anyhow::Error) -> ErrorKind {
        err.downcast_ref::<Error>().unwrap().kind()
    }

    /// Goal: a status request reaches liqctld on the device's own path and comes back parsed.
    /// Method: one call against a stand-in that answers, then check what it saw.
    #[test]
    fn a_status_round_trip_reaches_the_device_path() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::Reply);
            let io = worker(&server, LONG, LONG);

            let status = io.status().await.unwrap();

            assert_eq!(status.status.len(), 1);
            assert_eq!(status.status[0].0, "Liquid temperature");
            assert_eq!(server.next_event(), Event::Accepted);
            assert_eq!(
                server.next_event(),
                Event::Request("GET /devices/3/status HTTP/1.1".to_owned())
            );
            assert_eq!(io.health(), DeviceHealth::Healthy);
        });
    }

    /// Goal: liqctld may drop an idle connection, and the next request must reconnect and be
    /// answered rather than fail. Method: a stand-in that closes after every reply, then two
    /// calls, each of which must land on a connection of its own.
    #[test]
    fn a_connection_closed_between_requests_is_reopened() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::ReplyAndClose);
            let io = worker(&server, LONG, LONG);

            io.status().await.unwrap();
            io.status().await.unwrap();

            let accepted = server
                .drain()
                .into_iter()
                .filter(|event| *event == Event::Accepted)
                .count();
            assert_eq!(accepted, 2, "the second request must have reconnected");
        });
    }

    /// Goal: a device that stops answering must cost the caller its budget, not the socket
    /// timeout, and must count against its health. Method: a stand-in that never answers.
    #[test]
    fn a_stalled_reply_times_out_within_the_budget() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::Hold);
            let io = worker(
                &server,
                Duration::from_millis(50),
                Duration::from_millis(500),
            );

            let err = io.status().await.unwrap_err();

            assert_eq!(kind_of(&err), ErrorKind::TimedOut, "{err}");
            assert_eq!(
                io.health(),
                DeviceHealth::Degraded {
                    consecutive_timeouts: 1
                }
            );
        });
    }

    /// Goal: once a device has timed out enough times in a row, requests stop being queued for
    /// it until its probe is due. Method: time out the threshold's worth of calls against a
    /// stand-in that never answers, then check the next call is refused without waiting.
    #[test]
    fn enough_timeouts_stop_dispatch_until_the_probe() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::Hold);
            let budget = Duration::from_millis(20);
            let io = worker(&server, budget, Duration::from_secs(1));
            for _ in 0..UNREACHABLE_AFTER_TIMEOUTS {
                let err = io.status().await.unwrap_err();
                assert_eq!(kind_of(&err), ErrorKind::TimedOut, "{err}");
            }
            assert!(matches!(io.health(), DeviceHealth::Unreachable { .. }));

            let err = io.status().await.unwrap_err();

            assert_eq!(
                kind_of(&err),
                ErrorKind::HostUnreachable,
                "an unreachable device must be skipped, not waited on: {err}"
            );
        });
    }

    /// Goal: a request whose caller gave up must not be sent, or a stalled device works through
    /// a backlog of stale reads before the request that is still wanted. Method: hold the first
    /// request, abandon a second behind it, release the first, then make a third. The stand-in
    /// must see only the first and the third.
    #[test]
    fn an_abandoned_request_is_skipped() {
        rt::test_runtime(async {
            let (release_tx, release_rx) = std_mpsc::channel::<()>();
            let release_rx = Mutex::new(release_rx);
            let server = FakeLiqctld::start(move |index| {
                if index == 0 {
                    let _ = release_rx.lock().unwrap().recv();
                }
                Action::Reply
            });
            let mut io = worker(&server, Duration::from_millis(50), LONG);
            assert!(
                io.status().await.is_err(),
                "the held first request times out"
            );
            assert!(
                io.status().await.is_err(),
                "the second times out while queued"
            );

            release_tx.send(()).unwrap();
            io.status_timeout = LONG;
            io.status().await.unwrap();

            let requests = server
                .drain()
                .into_iter()
                .filter(|event| matches!(event, Event::Request(_)))
                .count();
            assert_eq!(requests, 2, "the abandoned second request was sent");
        });
    }

    /// Goal: after liqctld is told to quit, the worker must let go of its connection rather
    /// than hold a socket to a dead service. Method: a round trip, then `disconnect`, and wait
    /// for the stand-in to see the connection close.
    #[test]
    fn disconnect_closes_the_connection() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::Reply);
            let io = worker(&server, LONG, LONG);
            io.status().await.unwrap();
            assert_eq!(server.next_event(), Event::Accepted);
            assert!(matches!(server.next_event(), Event::Request(_)));

            io.disconnect();

            assert_eq!(server.next_event(), Event::Closed);
        });
    }
}
