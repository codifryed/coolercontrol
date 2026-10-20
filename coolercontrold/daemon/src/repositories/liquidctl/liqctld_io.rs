// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! liqctld requests, isolated on threads of their own.
//!
//! One worker thread and one connection per liquidctl device, so the main thread does no socket
//! IO and a request stalled inside liqctld parks only its own device's thread. Health follows the
//! rules of a sysfs device: enough timeouts in a row take a device out of rotation until its next
//! probe. A control thread serves the calls that belong to no device.
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use log::{debug, trace};
use serde::de::{DeserializeOwned, IgnoredAny};
use serde::Serialize;
use tokio::sync::{mpsc, oneshot};

#[cfg(all(test, feature = "gated-tests"))]
use crate::repositories::hwmon::device_io::DeviceHealth;
use crate::repositories::hwmon::device_io::{HealthState, QUEUE_DEPTH};
use crate::repositories::liquidctl::liqctld_client::{
    ColorRequest, ControlModeRequest, DeviceResponse, DevicesResponse, FixedSpeedRequest,
    InitializeRequest, ScreenRequest, SpeedProfileRequest, StatusResponse,
};
use crate::repositories::liquidctl::liqctld_http::{
    Endpoint, LiqctldHttp, Method, LIQCTLD_RESPONSE_TIMEOUT,
};
use crate::rt;

/// Budget for every device call but status. Matches the socket timeout, which covers
/// liquidctl's own 9.5 s job timeout.
const COMMAND_TIMEOUT: Duration = LIQCTLD_RESPONSE_TIMEOUT;
/// Stack for a worker thread. It runs one blocking loop, and liqctld's replies nest a few levels
/// deep at most, so the default 8 MiB is wasted address space once there is one per device.
const WORKER_STACK_BYTES: usize = 256 * 1024;
const THREAD_NAME_PREFIX: &str = "ccliq-";
const CONTROL_THREAD_NAME: &str = "ccliq-ctl";
/// The kernel caps thread names at 15 bytes, and a `u8` id adds at most 3.
const _: () = assert!(THREAD_NAME_PREFIX.len() + 3 <= 15);
const _: () = assert!(CONTROL_THREAD_NAME.len() <= 15);
const NO_BODY: Option<&()> = None;

type Reply<T> = oneshot::Sender<Result<T>>;

/// Work for a device's worker. Every payload is plain data, so the thread boundary needs no
/// shared ownership and no lock.
#[derive(Debug)]
pub enum Request {
    Status {
        reply: Reply<StatusResponse>,
    },
    Initialize {
        body: InitializeRequest,
        reply: Reply<StatusResponse>,
    },
    Legacy690 {
        reply: Reply<DeviceResponse>,
    },
    Reconnect {
        reply: Reply<()>,
    },
    DirectAccess {
        reply: Reply<()>,
    },
    FixedSpeed {
        body: FixedSpeedRequest,
        reply: Reply<()>,
    },
    SpeedProfile {
        body: SpeedProfileRequest,
        reply: Reply<()>,
    },
    ControlMode {
        body: ControlModeRequest,
        reply: Reply<()>,
    },
    Color {
        body: ColorRequest,
        reply: Reply<()>,
    },
    Screen {
        body: ScreenRequest,
        reply: Reply<()>,
    },
    /// Closes the connection, for when the service behind it is gone.
    Disconnect,
}

/// Work for the control thread: the calls that belong to no device.
#[derive(Debug)]
pub enum ControlRequest {
    Handshake { reply: Reply<()> },
    Devices { reply: Reply<DevicesResponse> },
    Scan { reply: Reply<DevicesResponse> },
    Quit { reply: Reply<()> },
    Disconnect,
}

/// What a worker loop needs to know about a request before serving it.
trait Job {
    /// Whether the caller stopped waiting. Such a request is not sent at all.
    fn is_abandoned(&self) -> bool;
}

impl Job for Request {
    fn is_abandoned(&self) -> bool {
        match self {
            Self::Status { reply } | Self::Initialize { reply, .. } => reply.is_closed(),
            Self::Legacy690 { reply } => reply.is_closed(),
            Self::Reconnect { reply }
            | Self::DirectAccess { reply }
            | Self::FixedSpeed { reply, .. }
            | Self::SpeedProfile { reply, .. }
            | Self::ControlMode { reply, .. }
            | Self::Color { reply, .. }
            | Self::Screen { reply, .. } => reply.is_closed(),
            Self::Disconnect => false,
        }
    }
}

impl Job for ControlRequest {
    fn is_abandoned(&self) -> bool {
        match self {
            Self::Handshake { reply } | Self::Quit { reply } => reply.is_closed(),
            Self::Devices { reply } | Self::Scan { reply } => reply.is_closed(),
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
        let mut http = LiqctldHttp::new(endpoint);
        spawn_worker(format!("{THREAD_NAME_PREFIX}{device_index}"), move || {
            run_loop(rx, |request| serve_device(&mut http, device_index, request));
        })?;
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

    pub async fn initialize(&self, pump_mode: Option<String>) -> Result<StatusResponse> {
        let body = InitializeRequest { pump_mode };
        self.dispatch("initialize", COMMAND_TIMEOUT, |reply| Request::Initialize {
            body,
            reply,
        })
        .await
    }

    /// Sets the device to legacy 690 mode (old Krakens and the EVGA CLC).
    pub async fn put_legacy690(&self) -> Result<DeviceResponse> {
        self.dispatch("legacy690", COMMAND_TIMEOUT, |reply| Request::Legacy690 {
            reply,
        })
        .await
    }

    /// Releases and re-opens the device's USB handle, leaving the others alone.
    ///
    /// The lighter half of recovery: it does to a single device what restarting liqctld does to
    /// every device, and keeps the driver object, so a legacy690 flip and a forced direct access
    /// survive it. The device still has to be re-initialized afterwards.
    pub async fn put_reconnect(&self) -> Result<()> {
        self.dispatch("reconnect", COMMAND_TIMEOUT, |reply| Request::Reconnect {
            reply,
        })
        .await
    }

    /// Forces direct access, before the device is initialized.
    pub async fn put_direct_access(&self) -> Result<()> {
        self.dispatch("direct access", COMMAND_TIMEOUT, |reply| {
            Request::DirectAccess { reply }
        })
        .await
    }

    pub async fn put_fixed_speed(&self, channel_name: &str, duty: u8) -> Result<()> {
        let body = FixedSpeedRequest {
            channel: channel_name.to_owned(),
            duty,
        };
        self.dispatch("fixed speed", COMMAND_TIMEOUT, |reply| {
            Request::FixedSpeed { body, reply }
        })
        .await
    }

    pub async fn put_speed_profile(
        &self,
        channel_name: &str,
        profile: &[(f64, u8)],
        temperature_sensor: Option<u8>,
    ) -> Result<()> {
        let body = SpeedProfileRequest {
            channel: channel_name.to_owned(),
            profile: profile.to_vec(),
            temperature_sensor,
        };
        self.dispatch("speed profile", COMMAND_TIMEOUT, |reply| {
            Request::SpeedProfile { body, reply }
        })
        .await
    }

    pub async fn put_control_mode(&self, channel_name: &str, control_mode: &str) -> Result<()> {
        let body = ControlModeRequest {
            channel: channel_name.to_owned(),
            desired_state: control_mode.to_owned(),
        };
        self.dispatch("control mode", COMMAND_TIMEOUT, |reply| {
            Request::ControlMode { body, reply }
        })
        .await
    }

    pub async fn put_color(&self, body: ColorRequest) -> Result<()> {
        self.dispatch("color", COMMAND_TIMEOUT, |reply| Request::Color {
            body,
            reply,
        })
        .await
    }

    /// `mode` is one of "gif", "static", "orientation", "brightness" or a liquid mode.
    pub async fn put_screen(
        &self,
        channel_name: &str,
        mode: &str,
        value: Option<String>,
    ) -> Result<()> {
        let body = ScreenRequest {
            channel: channel_name.to_owned(),
            mode: mode.to_owned(),
            value,
        };
        self.dispatch("screen", COMMAND_TIMEOUT, |reply| Request::Screen {
            body,
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
        let gone = || worker_gone(self.state.device_name());
        let request = make_request(reply_tx);
        let Some(result) = round_trip(&self.tx, request, reply_rx, budget, gone).await else {
            self.state.record_timeout(&what);
            return Err(timed_out(self.state.device_name(), what, budget));
        };
        self.state.record_answered();
        result
    }
}

/// The main-thread half of the control thread.
///
/// Not health-gated: startup, recovery and shutdown depend on these calls going through, and
/// they are rare.
pub struct LiqctldControl {
    tx: mpsc::Sender<ControlRequest>,
    budget: Duration,
}

impl LiqctldControl {
    /// Start the control thread. Each call may take every connect try of `endpoint`, for
    /// liqctld's startup scan.
    ///
    /// # Errors
    ///
    /// When the OS refuses the thread.
    pub fn spawn(endpoint: Endpoint) -> Result<Self> {
        let budget = endpoint.exchange_budget();
        let (tx, rx) = mpsc::channel::<ControlRequest>(QUEUE_DEPTH);
        let mut http = LiqctldHttp::new(endpoint);
        spawn_worker(CONTROL_THREAD_NAME.to_owned(), move || {
            run_loop(rx, |request| serve_control(&mut http, request));
        })?;
        Ok(Self { tx, budget })
    }

    /// Verifies liqctld is answering requests.
    pub async fn handshake(&self) -> Result<()> {
        self.dispatch("handshake", |reply| ControlRequest::Handshake { reply })
            .await
    }

    /// The devices liqctld connected at startup.
    pub async fn get_all_devices(&self) -> Result<DevicesResponse> {
        self.dispatch("devices", |reply| ControlRequest::Devices { reply })
            .await
    }

    /// A fresh scan for connected devices, leaving liqctld's own device state alone.
    pub async fn scan_devices(&self) -> Result<DevicesResponse> {
        self.dispatch("scan", |reply| ControlRequest::Scan { reply })
            .await
    }

    /// Shuts the liqctld service down.
    pub async fn post_quit(&self) -> Result<()> {
        self.dispatch("quit", |reply| ControlRequest::Quit { reply })
            .await
    }

    /// Closes the control connection, so it reconnects on its next request.
    pub fn disconnect(&self) {
        if self.tx.try_send(ControlRequest::Disconnect).is_err() {
            debug!("could not disconnect the liqctld control connection: busy or gone");
        }
    }

    async fn dispatch<T, F>(&self, what: &'static str, make_request: F) -> Result<T>
    where
        F: FnOnce(Reply<T>) -> ControlRequest,
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        let gone = || worker_gone("control");
        round_trip(
            &self.tx,
            make_request(reply_tx),
            reply_rx,
            self.budget,
            gone,
        )
        .await
        .unwrap_or_else(|| Err(timed_out("control", what, self.budget)))
    }
}

/// Queues `request` and waits for its reply. `None` when `budget` ran out first.
async fn round_trip<R, T>(
    tx: &mpsc::Sender<R>,
    request: R,
    reply_rx: oneshot::Receiver<Result<T>>,
    budget: Duration,
    gone: impl Fn() -> anyhow::Error,
) -> Option<Result<T>> {
    debug_assert!(budget > Duration::ZERO);
    let exchange = async {
        tx.send(request).await.map_err(|_| gone())?;
        reply_rx.await.map_err(|_| gone())?
    };
    rt::timeout(budget, exchange).await.ok()
}

fn spawn_worker(thread_name: String, body: impl FnOnce() + Send + 'static) -> Result<()> {
    debug!("liqctld worker starting: {thread_name}");
    // Detached deliberately: a worker stuck in a socket read only returns at its timeout, so a
    // handle would only invite a `join` that cannot be bounded.
    thread::Builder::new()
        .name(thread_name)
        .stack_size(WORKER_STACK_BYTES)
        .spawn(body)?;
    Ok(())
}

/// The worker thread body: one blocking request at a time, until the last handle is dropped
/// and the queue closes.
fn run_loop<J: Job>(mut rx: mpsc::Receiver<J>, mut serve: impl FnMut(J)) {
    while let Some(job) = rx.blocking_recv() {
        if job.is_abandoned() {
            trace!("Skipping an abandoned liqctld request");
            continue;
        }
        serve(job);
    }
    debug!("liqctld worker stopped");
}

fn serve_device(http: &mut LiqctldHttp, id: u8, request: Request) {
    match request {
        Request::Status { reply } => answer(reply, |abandoned| {
            call(http, id, Method::Get, "status", NO_BODY, abandoned)
        }),
        Request::Initialize { body, reply } => answer(reply, |abandoned| {
            call(http, id, Method::Post, "initialize", Some(&body), abandoned)
        }),
        Request::Legacy690 { reply } => answer(reply, |abandoned| {
            call(http, id, Method::Put, "legacy690", NO_BODY, abandoned)
        }),
        Request::Reconnect { reply } => answer(reply, |abandoned| {
            put(http, id, "reconnect", NO_BODY, abandoned)
        }),
        Request::DirectAccess { reply } => answer(reply, |abandoned| {
            put(http, id, "direct-access", NO_BODY, abandoned)
        }),
        Request::FixedSpeed { body, reply } => answer(reply, |abandoned| {
            put(http, id, "speed/fixed", Some(&body), abandoned)
        }),
        Request::SpeedProfile { body, reply } => answer(reply, |abandoned| {
            put(http, id, "speed/profile", Some(&body), abandoned)
        }),
        Request::ControlMode { body, reply } => answer(reply, |abandoned| {
            put(http, id, "control-mode", Some(&body), abandoned)
        }),
        Request::Color { body, reply } => answer(reply, |abandoned| {
            put(http, id, "color", Some(&body), abandoned)
        }),
        Request::Screen { body, reply } => answer(reply, |abandoned| {
            put(http, id, "screen", Some(&body), abandoned)
        }),
        Request::Disconnect => http.disconnect(),
    }
}

fn serve_control(http: &mut LiqctldHttp, request: ControlRequest) {
    match request {
        ControlRequest::Handshake { reply } => answer(reply, |abandoned| {
            control_call::<IgnoredAny>(http, Method::Get, "/handshake", abandoned).map(drop)
        }),
        ControlRequest::Devices { reply } => answer(reply, |abandoned| {
            control_call(http, Method::Get, "/devices", abandoned)
        }),
        ControlRequest::Scan { reply } => answer(reply, |abandoned| {
            control_call(http, Method::Get, "/devices/scan", abandoned)
        }),
        ControlRequest::Quit { reply } => answer(reply, |abandoned| {
            control_call::<IgnoredAny>(http, Method::Post, "/quit", abandoned).map(drop)
        }),
        ControlRequest::Disconnect => http.disconnect(),
    }
}

/// Runs `work` while its caller waits, then hands the caller the result.
fn answer<T>(reply: Reply<T>, work: impl FnOnce(&dyn Fn() -> bool) -> Result<T>) {
    let result = work(&|| reply.is_closed());
    // A dropped receiver means the caller already timed out. Expected, not an error.
    let _ = reply.send(result);
}

/// One exchange on a device's path, with its reply parsed here rather than on the main thread.
fn call<T, B>(
    http: &mut LiqctldHttp,
    id: u8,
    method: Method,
    route: &str,
    body: Option<&B>,
    abandoned: &dyn Fn() -> bool,
) -> Result<T>
where
    T: DeserializeOwned,
    B: Serialize,
{
    let reply_body = http.exchange(
        method,
        format_args!("/devices/{id}/{route}"),
        body,
        abandoned,
    )?;
    Ok(serde_json::from_slice(reply_body)?)
}

/// A device command whose reply carries nothing but success. Still parsed, so a reply that is
/// not JSON fails loudly.
fn put<B: Serialize>(
    http: &mut LiqctldHttp,
    id: u8,
    route: &str,
    body: Option<&B>,
    abandoned: &dyn Fn() -> bool,
) -> Result<()> {
    call::<IgnoredAny, B>(http, id, Method::Put, route, body, abandoned).map(drop)
}

fn control_call<T: DeserializeOwned>(
    http: &mut LiqctldHttp,
    method: Method,
    path: &str,
    abandoned: &dyn Fn() -> bool,
) -> Result<T> {
    let reply_body = http.exchange(method, format_args!("{path}"), NO_BODY, abandoned)?;
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
    use std::path::{Path, PathBuf};
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
        Request { line: String, body: String },
        Closed,
    }

    impl Event {
        fn request(line: &str, body: &str) -> Self {
            Self::Request {
                line: line.to_owned(),
                body: body.to_owned(),
            }
        }
    }

    type Handler = dyn Fn(usize) -> Action + Send + Sync;

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
            let socket_path = socket_in(dir.path());
            let events = listen(&socket_path, Arc::new(handler));
            Self {
                _dir: dir,
                socket_path,
                events,
            }
        }

        fn endpoint(&self, io_timeout: Duration) -> Endpoint {
            test_endpoint(self.socket_path.clone(), 2, io_timeout)
        }

        fn next_event(&self) -> Event {
            self.events.recv_timeout(EVENT_WAIT).unwrap()
        }

        /// Every request so far, without waiting for more.
        fn requests(&self) -> Vec<Event> {
            self.events
                .try_iter()
                .filter(|event| matches!(event, Event::Request { .. }))
                .collect()
        }
    }

    fn socket_in(dir: &Path) -> PathBuf {
        dir.join("liqctld.sock")
    }

    fn test_endpoint(socket_path: PathBuf, connect_tries: usize, io_timeout: Duration) -> Endpoint {
        Endpoint {
            socket_path,
            connect_tries,
            connect_pause: Duration::from_millis(10),
            io_timeout,
        }
    }

    /// Binds `socket_path` and serves it until the test process exits.
    fn listen(socket_path: &Path, handler: Arc<Handler>) -> std_mpsc::Receiver<Event> {
        let listener = UnixListener::bind(socket_path).unwrap();
        let (events_tx, events) = std_mpsc::channel();
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
        events
    }

    fn serve_connection(
        stream: UnixStream,
        handler: &Handler,
        served: &AtomicUsize,
        events: &std_mpsc::Sender<Event>,
    ) {
        let mut writer = stream.try_clone().unwrap();
        let mut reader = BufReader::new(stream);
        while let Some((line, body)) = read_request(&mut reader) {
            let _ = events.send(Event::Request { line, body });
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

    /// The request line and body. `None` at EOF.
    fn read_request(reader: &mut BufReader<UnixStream>) -> Option<(String, String)> {
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
        Some((
            request_line.trim_end().to_owned(),
            String::from_utf8(body).unwrap(),
        ))
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
                Event::request("GET /devices/3/status HTTP/1.1", "")
            );
            assert_eq!(io.health(), DeviceHealth::Healthy);
        });
    }

    /// Goal: liqctld reads a command's arguments from its JSON body by `Content-Length`, so the
    /// body must arrive whole and as liqctld names the fields. Method: a fixed speed and an
    /// initialize, then compare what the stand-in read.
    #[test]
    fn a_command_carries_its_json_body() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::Reply);
            let io = worker(&server, LONG, LONG);

            io.put_fixed_speed("fan1", 40).await.unwrap();
            io.initialize(Some("quiet".to_owned())).await.unwrap();

            assert_eq!(
                server.requests(),
                vec![
                    Event::request(
                        "PUT /devices/3/speed/fixed HTTP/1.1",
                        r#"{"channel":"fan1","duty":40}"#
                    ),
                    Event::request(
                        "POST /devices/3/initialize HTTP/1.1",
                        r#"{"pump_mode":"quiet"}"#
                    ),
                ]
            );
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
                .events
                .try_iter()
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

            assert_eq!(
                server.requests().len(),
                2,
                "the abandoned second request was sent"
            );
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
            assert!(matches!(server.next_event(), Event::Request { .. }));

            io.disconnect();

            assert_eq!(server.next_event(), Event::Closed);
        });
    }

    /// Goal: the control thread serves the calls that belong to no device, on liqctld's own
    /// routes. Method: a handshake and a quit against a stand-in, then compare what it saw.
    #[test]
    fn control_calls_use_the_service_routes() {
        rt::test_runtime(async {
            let server = FakeLiqctld::start(|_| Action::Reply);
            let control = LiqctldControl::spawn(server.endpoint(LONG)).unwrap();

            control.handshake().await.unwrap();
            control.post_quit().await.unwrap();

            assert_eq!(
                server.requests(),
                vec![
                    Event::request("GET /handshake HTTP/1.1", ""),
                    Event::request("POST /quit HTTP/1.1", ""),
                ]
            );
        });
    }

    /// Goal: liqctld's startup scan can keep it from listening for a while, and the first call
    /// must ride that out on its connect tries rather than fail. Method: start the control
    /// thread before the socket exists, bind it a moment later, then handshake.
    #[test]
    fn a_control_call_waits_for_liqctld_to_listen() {
        rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let socket_path = socket_in(dir.path());
            let endpoint = test_endpoint(socket_path.clone(), 100, LONG);
            let control = LiqctldControl::spawn(endpoint).unwrap();
            let binder = thread::spawn(move || {
                thread::sleep(Duration::from_millis(50));
                listen(&socket_path, Arc::new(|_: usize| Action::Reply))
            });

            control.handshake().await.unwrap();

            let events = binder.join().unwrap();
            assert_eq!(events.recv_timeout(EVENT_WAIT).unwrap(), Event::Accepted);
        });
    }

    /// Goal: a call abandoned while the worker waits to connect must not be sent once it does.
    /// A late `/quit` would stop the liqctld that a restart has just started. Method: give up on
    /// a quit before the socket exists, bind it, then handshake. Only the handshake may arrive.
    #[test]
    fn a_call_abandoned_while_connecting_is_never_sent() {
        rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let socket_path = socket_in(dir.path());
            let endpoint = test_endpoint(socket_path.clone(), 100, LONG);
            let control = LiqctldControl::spawn(endpoint).unwrap();
            let quit = rt::timeout(Duration::from_millis(30), control.post_quit()).await;
            assert!(quit.is_err(), "nothing is listening yet, so the quit waits");

            let events = listen(&socket_path, Arc::new(|_: usize| Action::Reply));
            control.handshake().await.unwrap();

            let requests: Vec<Event> = events
                .try_iter()
                .filter(|event| matches!(event, Event::Request { .. }))
                .collect();
            assert_eq!(
                requests,
                vec![Event::request("GET /handshake HTTP/1.1", "")]
            );
        });
    }
}
