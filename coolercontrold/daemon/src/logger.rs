// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::diagnostics::support::SystemInfo;
use crate::log_writer::{LogWriter, QueuedRecord, DRAIN_TIMEOUT};
use crate::repositories::liquidctl::liqctld_service;
use crate::rt;
use crate::{cc_fs, exit_successfully, Args, ENV_CC_LOG, ENV_LOG, VERSION};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use env_logger::fmt::TimestampPrecision;
use env_logger::{Logger, WriteStyle};
use log::{debug, info, trace, Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use nix::NixPath;
use nu_glob::{glob, Uninterruptible};
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::io::Write as _;
use std::ops::Not;
use std::path::PathBuf;
use std::str::{from_utf8_unchecked, FromStr};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::ThreadId;
use std::time::Duration;
use systemd_journal_logger::{connected_to_journal, JournalLog};
use tokio::sync::mpsc::error::TryRecvError;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const LOG_BUFFER_LINE_SIZE: usize = 500;
// Bounds for the UI-facing ring buffer; the journal/stderr sink always keeps full lines.
// liqctld can relay huge single lines (Python tracebacks), so entries are truncated and the
// buffer is bounded in total bytes as well as entries.
const LOG_ENTRY_MAX_BYTES: usize = 8 * 1024;
const LOG_BUFFER_MAX_BYTES: usize = 1024 * 1024;
const LOG_TRUNCATION_MARKER: &str = " ...[truncated]\n";
// Broadcast slots for SSE subscribers. Bursts coalesce into single events, so this rarely
// fills; a lagged subscriber skips missed lines (recent history stays at GET /logs).
const NEW_LOG_CHANNEL_CAP: usize = 16;
// Bounded buffer between the synchronous `Write` impl (called from arbitrary threads) and the
// log-buffer actor. Sized so bursts rarely overflow; on overflow a UI-buffer line is dropped.
const LOG_MSG_CHANNEL_CAP: usize = 64;
const _: () = assert!(LOG_TRUNCATION_MARKER.len() < LOG_ENTRY_MAX_BYTES);
const _: () = assert!(LOG_ENTRY_MAX_BYTES <= LOG_BUFFER_MAX_BYTES);

pub async fn setup_logging(
    cmd_args: &Args,
    debug_logging_setting: bool,
    run_token: CancellationToken,
) -> Result<LogBufHandle> {
    let (level, source) = resolve_log_level(cmd_args.debug, env_log_level(), debug_logging_setting);
    let level_info = LogLevelInfo {
        level,
        source,
        journal: connected_to_journal(),
    };
    // Connecting sends to journald, which can be slow to read.
    let journal = if level_info.journal {
        Some(rt::spawn_blocking(JournalLog::new).await??)
    } else {
        None
    };
    let (logger, log_buf_handle) = CCLogger::new(level_info, journal, VERSION, run_token)?;
    logger.init()?;
    if cmd_args.wants_system_info_banner() {
        info!("Log level: {level} ({})", source.description());
        log_system_info().await;
    }
    if cmd_args.system_info {
        // verify_env spawns a Python process via `tokio::process`; run it on the sidecar.
        let _ = crate::sidecar::handle()
            .run(liqctld_service::verify_env)
            .await;
        exit_successfully();
    }
    Ok(log_buf_handle)
}

/// Where the effective log level came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LogLevelSource {
    /// Nothing requested a level, so INFO applies.
    Default,
    /// `CC_LOG`, or the deprecated `COOLERCONTROL_LOG`.
    Env,
    /// The `--debug` command line flag.
    Flag,
    /// The `debug_logging` setting.
    Settings,
}

impl LogLevelSource {
    fn description(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Env => "environment",
            Self::Flag => "--debug flag",
            Self::Settings => "debug logging setting",
        }
    }
}

/// The log level, fixed at startup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogLevelInfo {
    pub level: LevelFilter,
    pub source: LogLevelSource,
    /// Whether log lines go to the systemd journal rather than stderr.
    pub journal: bool,
}

impl Default for LogLevelInfo {
    fn default() -> Self {
        Self {
            level: LevelFilter::Info,
            source: LogLevelSource::Default,
            journal: false,
        }
    }
}

/// The most verbose request wins, so the setting can raise the level but never lower it. On a
/// tie the env var or flag is reported, since the UI setting cannot turn those off.
fn resolve_log_level(
    debug_flag: bool,
    env_level: Option<LevelFilter>,
    debug_setting: bool,
) -> (LevelFilter, LogLevelSource) {
    let mut level = LevelFilter::Info;
    let mut source = LogLevelSource::Default;
    if let Some(env_level) = env_level {
        level = env_level;
        source = LogLevelSource::Env;
    }
    if debug_flag && LevelFilter::Debug > level {
        level = LevelFilter::Debug;
        source = LogLevelSource::Flag;
    }
    if debug_setting && LevelFilter::Debug > level {
        level = LevelFilter::Debug;
        source = LogLevelSource::Settings;
    }
    if debug_flag {
        assert!(level >= LevelFilter::Debug);
    }
    if debug_setting {
        assert!(level >= LevelFilter::Debug);
    }
    if source == LogLevelSource::Default {
        assert_eq!(level, LevelFilter::Info);
    }
    (level, source)
}

/// Reads `CC_LOG`, falling back to the deprecated `COOLERCONTROL_LOG`. An unparsable value
/// counts as unset.
fn env_log_level() -> Option<LevelFilter> {
    let log_level = std::env::var(ENV_CC_LOG)
        .or_else(|_| std::env::var(ENV_LOG))
        .ok()?;
    LevelFilter::from_str(&log_level).ok()
}

/// Logs the daemon/host banner (version, OS, board, BIOS, desktop) to the journal.
///
/// Read on the blocking pool: the desktop scan reads every process's environment, and each of
/// those reads waits on that process's memory map lock, which a stuck process can hold.
async fn log_system_info() {
    match rt::spawn_blocking(SystemBanner::read).await {
        Ok(banner) => banner.log(),
        Err(err) => debug!("Failed to read the system info: {err}"),
    }
}

/// What the startup banner shows, read in one go off the runtime thread.
struct SystemBanner {
    name: String,
    os: String,
    host: String,
    kernel: String,
    arch: String,
    board: SystemInfo,
    desktops: Result<(String, String)>,
}

impl SystemBanner {
    /// Blocking calls, for the blocking pool.
    fn read() -> Self {
        Self {
            name: sysinfo::System::name().unwrap_or_default(),
            os: sysinfo::System::long_os_version().unwrap_or_default(),
            host: sysinfo::System::host_name().unwrap_or_default(),
            kernel: sysinfo::System::kernel_version().unwrap_or_default(),
            arch: sysinfo::System::cpu_arch(),
            board: SystemInfo::read_blocking(),
            desktops: read_xdg_desktop_info(),
        }
    }

    fn log(&self) {
        info!("System Info:");
        info!("  {}", "-".repeat(60));
        info!("  {:<20} {}", "CoolerControlD", VERSION);
        info!("  {:<20} {}", "Name", self.name);
        info!("  {:<20} {}", "OS", self.os);
        info!("  {:<20} {}", "Host", self.host);
        info!("  {:<20} {}", "Kernel", self.kernel);
        info!("  {:<20} {}", "Arch", self.arch);
        info!("  {:<20} {}", "Board Manufacturer", self.board.board_vendor);
        info!("  {:<20} {}", "Board Name", self.board.board_name);
        info!("  {:<20} {}", "Board Version", self.board.board_version);
        info!("  {:<20} {}", "BIOS Manufacturer", self.board.bios_vendor);
        info!("  {:<20} {}", "BIOS Version", self.board.bios_release);
        match &self.desktops {
            Ok((desktops, sessions)) => {
                info!("  {:<20} {}", "XDG Desktops", desktops);
                info!("  {:<20} {}", "XDG Session Types", sessions);
            }
            Err(err) => debug!("Failed to get XDG desktop info: {err}"),
        }
    }
}

/// One DMI identity attribute, trimmed, or empty when it does not read as text.
///
/// Blocking calls: callers run it on the blocking pool.
pub fn read_dmi_system_info(name: &str) -> String {
    let path = PathBuf::from(format!("/sys/devices/virtual/dmi/id/{name}"));
    cc_fs::read_bytes_blocking(&path)
        .ok()
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .map(|text| text.trim().to_owned())
        .unwrap_or_default()
}

/// The desktops and session types named in any process's environment.
///
/// Blocking calls, for the blocking pool. An environment past `cc_fs::ATTRIBUTE_MAX_BYTES` is
/// skipped, as is one that is not text.
fn read_xdg_desktop_info() -> Result<(String, String)> {
    let mut desktops = HashSet::new();
    let mut sessions_types = HashSet::new();
    let environ_paths = glob("/proc/*/environ", Uninterruptible)?
        .filter_map(Result::ok)
        .collect::<Vec<PathBuf>>();
    let regex_desktop = Regex::new(r"XDG_SESSION_DESKTOP=(?P<desktop>\w+)")?;
    let regex_session_type = Regex::new(r"XDG_SESSION_TYPE=(?P<session_type>\w+)")?;
    for path in environ_paths {
        if path.is_empty() {
            continue;
        }
        let Some(content) = cc_fs::read_bytes_blocking(&path)
            .ok()
            .and_then(|bytes| String::from_utf8(bytes).ok())
        else {
            continue;
        };
        if let Some(desktop_captures) = regex_desktop.captures(&content) {
            let desktop = desktop_captures
                .name("desktop")
                .context("Desktop Group should exist")?
                .as_str()
                .to_owned();
            desktops.insert(desktop);
        }
        if let Some(type_captures) = regex_session_type.captures(&content) {
            let session_type = type_captures
                .name("session_type")
                .context("Session Type should exist")?
                .as_str()
                .to_owned();
            sessions_types.insert(session_type);
        }
    }
    if desktops.is_empty() {
        Err(anyhow::anyhow!("No XDG Desktops found"))
    } else {
        let desktop_list = Vec::from_iter(desktops).join(", ");
        let session_list = Vec::from_iter(sessions_types).join(", ");
        Ok((desktop_list, session_list))
    }
}

/// This is our own Logger, which handles appropriate logging dependent on the environment.
struct CCLogger {
    max_level: LevelFilter,
    log_filter: Logger,
    sink: SinkFeed,
    buf_logger: Box<dyn Log>,
}

impl CCLogger {
    /// Logs to `journal` when given, to stderr otherwise.
    fn new(
        level_info: LogLevelInfo,
        journal: Option<JournalLog>,
        version: &str,
        run_token: CancellationToken,
    ) -> Result<(Self, LogBufHandle)> {
        let max_level = level_info.level;
        let timestamp_precision = if max_level >= LevelFilter::Debug {
            TimestampPrecision::Millis
        } else {
            TimestampPrecision::Seconds
        };
        let log_filter = Self::build_log_filter(max_level);
        let sink = if let Some(journal) = journal {
            SinkFeed::journal(journal.with_extra_fields(vec![("VERSION", version)]))?
        } else {
            let write_style = stderr_write_style();
            SinkFeed::stderr(
                max_level,
                timestamp_precision,
                write_style,
                stderr_writer(timestamp_precision, write_style)?,
            )
        };
        let log_buf_handle = LogBufHandle::new(run_token).with_level_info(level_info);
        // We use a 2nd logger here for now. It's not super efficient, but in normal circumstances
        // we rarely log anything anyway.
        let buf_logger = Box::new(
            env_logger::Builder::new()
                .filter_level(max_level)
                .format_timestamp(Some(timestamp_precision))
                .target(env_logger::Target::Pipe(Box::new(log_buf_handle.clone())))
                .build(),
        );
        Ok((
            Self {
                max_level,
                log_filter,
                sink,
                buf_logger,
            },
            log_buf_handle,
        ))
    }

    /// Library log levels sit one level above the application's to keep chatter down.
    fn build_log_filter(max_level: LevelFilter) -> Logger {
        let lib_log_level = if max_level == LevelFilter::Trace {
            LevelFilter::Debug
        } else if max_level == LevelFilter::Debug {
            LevelFilter::Info
        } else {
            LevelFilter::Warn
        };
        let lib_very_reduced_level = if max_level == LevelFilter::Trace {
            LevelFilter::Info
        } else if max_level == LevelFilter::Debug {
            LevelFilter::Warn
        } else {
            LevelFilter::Error
        };
        let lib_disabled_level = if max_level >= LevelFilter::Debug {
            LevelFilter::Warn
        } else {
            LevelFilter::Off
        };
        let env_log_name = if std::env::var(ENV_CC_LOG).is_ok() {
            ENV_CC_LOG
        } else {
            ENV_LOG
        };
        env_logger::Builder::from_env(env_log_name)
            .filter_level(max_level)
            .filter_module("zbus", lib_log_level)
            .filter_module("tracing", lib_disabled_level)
            .filter_module("aide", lib_disabled_level)
            .filter_module("tower_http", lib_disabled_level)
            // hyper now uses tracing, but doesn't seem to log as other "tracing crates" do.
            .filter_module("hyper", lib_log_level)
            .filter_module("h2", lib_disabled_level) // h2::codec writes every frame
            .filter_module("tower_sessions_core", lib_very_reduced_level)
            .build()
    }

    /// Called on the main runtime thread, which the panic hook tells apart by it.
    fn init(self) -> Result<(), SetLoggerError> {
        let drainer = self.sink.drainer();
        log::set_max_level(self.max_level);
        log::set_boxed_logger(Box::new(self))?;
        drain_on_panic(drainer, std::thread::current().id());
        Ok(())
    }
}

impl Log for CCLogger {
    /// Whether this logger is enabled.
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.log_filter.enabled(metadata)
    }

    /// Logs the messages and filters them by matching against the `env_logger` filter
    fn log(&self, record: &Record) {
        if self.log_filter.matches(record) {
            self.sink.log(record);
            if is_ui_buffered(record.level()) {
                self.buf_logger.log(record);
            }
        }
    }

    /// Waits, for at most `log_writer::DRAIN_TIMEOUT`, until the lines logged so far reach the
    /// journal or stderr. The process exiting would lose them.
    fn flush(&self) {
        self.sink.drain();
    }
}

/// How long a panic on the main runtime thread waits for the log queue. The runtime catches task
/// panics and carries on, so a longer wait would stall fan ticks and heartbeats.
const MAIN_THREAD_PANIC_DRAIN_TIMEOUT: Duration = Duration::from_millis(100);
const _: () = assert!(MAIN_THREAD_PANIC_DRAIN_TIMEOUT.as_millis() < DRAIN_TIMEOUT.as_millis());

/// Waits, for at most the timeout given, until the lines logged so far reach the sink.
type Drainer = Box<dyn Fn(Duration) + Send + Sync>;

/// Drains the log queue before a panic is reported, so the lines that led up to it reach the
/// journal ahead of the panic message.
fn drain_on_panic(drainer: Drainer, main_thread: ThreadId) {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        drain_for_panic(&drainer, main_thread);
        report(info);
    }));
}

fn drain_for_panic(drainer: &Drainer, main_thread: ThreadId) {
    drainer(panic_drain_timeout(
        std::thread::current().id(),
        main_thread,
    ));
}

fn panic_drain_timeout(current: ThreadId, main_thread: ThreadId) -> Duration {
    if current == main_thread {
        MAIN_THREAD_PANIC_DRAIN_TIMEOUT
    } else {
        DRAIN_TIMEOUT
    }
}

/// How a record reaches the `cc-log` thread, which owns the journal or stderr.
enum SinkFeed {
    /// The journal takes records, and stamps each one as it arrives.
    Journal(LogWriter<QueuedRecord>),
    /// stderr takes lines formatted, and so stamped, on the thread that logged them, which keeps
    /// their time true while stderr stalls. `format` writes into `writer`.
    Stderr {
        format: Logger,
        writer: LogWriter<Vec<u8>>,
    },
}

impl SinkFeed {
    fn journal(journal: JournalLog) -> Result<Self> {
        let writer = LogWriter::start(
            "the journal",
            move |record: QueuedRecord| record.log_to(&journal),
            QueuedRecord::of,
        )?;
        Ok(Self::Journal(writer))
    }

    fn stderr(
        max_level: LevelFilter,
        timestamp_precision: TimestampPrecision,
        write_style: WriteStyle,
        writer: LogWriter<Vec<u8>>,
    ) -> Self {
        let format = env_logger::Builder::new()
            .filter_level(max_level)
            .format_timestamp(Some(timestamp_precision))
            .write_style(write_style)
            .target(env_logger::Target::Pipe(Box::new(writer.clone())))
            .build();
        Self::Stderr { format, writer }
    }

    fn log(&self, record: &Record) {
        match self {
            Self::Journal(writer) => writer.push(QueuedRecord::of(record)),
            Self::Stderr { format, .. } => format.log(record),
        }
    }

    fn drain(&self) {
        match self {
            Self::Journal(writer) => writer.drain(),
            Self::Stderr { writer, .. } => writer.drain(),
        }
    }

    /// Drains this feed's queue from wherever it is held, the panic hook included.
    fn drainer(&self) -> Drainer {
        match self {
            Self::Journal(writer) => {
                let writer = writer.clone();
                Box::new(move |timeout| writer.drain_within(timeout))
            }
            Self::Stderr { writer, .. } => {
                let writer = writer.clone();
                Box::new(move |timeout| writer.drain_within(timeout))
            }
        }
    }
}

fn stderr_writer(
    timestamp_precision: TimestampPrecision,
    write_style: WriteStyle,
) -> Result<LogWriter<Vec<u8>>> {
    let mut stderr = std::io::stderr();
    let writer = LogWriter::start(
        "stderr",
        move |line: Vec<u8>| {
            // A failed write has nowhere left to be reported.
            let _ = stderr.write_all(&line);
        },
        report_formatter(timestamp_precision, write_style),
    )?;
    Ok(writer)
}

/// Formats the dropped-lines report as stderr's lines are formatted, but into a buffer of its
/// own: the report skips the queue. It passes at INFO whatever the log level.
fn report_formatter(
    timestamp_precision: TimestampPrecision,
    write_style: WriteStyle,
) -> impl FnMut(&Record) -> Vec<u8> + Send + 'static {
    let line = LineBuf::default();
    let format = env_logger::Builder::new()
        .filter_level(LevelFilter::Info)
        .format_timestamp(Some(timestamp_precision))
        .write_style(write_style)
        .target(env_logger::Target::Pipe(Box::new(line.clone())))
        .build();
    move |record| {
        format.log(record);
        line.take()
    }
}

/// A pipe target whose writes are taken back out with `take`.
#[derive(Clone, Default)]
struct LineBuf(Arc<Mutex<Vec<u8>>>);

impl LineBuf {
    fn take(&self) -> Vec<u8> {
        std::mem::take(&mut *self.0.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

impl std::io::Write for LineBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// The colour choice `env_logger` makes for its own stderr target, which a pipe target skips.
fn stderr_write_style() -> WriteStyle {
    anstream::AutoStream::choice(&std::io::stderr()).into()
}

/// DEBUG and TRACE stay out of the UI ring buffer and its SSE stream: their volume would evict
/// warnings within seconds and flood every client. The journal or stderr sink keeps them.
fn is_ui_buffered(level: Level) -> bool {
    level <= Level::Info
}

pub struct CCLog {
    pub timestamp: DateTime<Local>,
    pub message: String,
}

struct LogBufferActor {
    buf: VecDeque<CCLog>,
    buf_bytes: usize,
    acknowledge_issues_timestamp: DateTime<Local>,
    new_log_broadcaster: broadcast::Sender<String>,
    msg_receiver: mpsc::Receiver<CCLogBufferMessage>,
}

enum CCLogBufferMessage {
    GetLogs {
        respond_to: oneshot::Sender<String>,
    },
    WarningsErrors {
        respond_to: oneshot::Sender<(usize, usize)>,
    },
    Log {
        log: String,
    },
    AcknowledgeIssues {
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl LogBufferActor {
    pub fn new(
        new_log_broadcaster: broadcast::Sender<String>,
        msg_receiver: mpsc::Receiver<CCLogBufferMessage>,
    ) -> Self {
        Self {
            buf: VecDeque::with_capacity(LOG_BUFFER_LINE_SIZE),
            buf_bytes: 0,
            acknowledge_issues_timestamp: Local::now(),
            new_log_broadcaster,
            msg_receiver,
        }
    }

    fn msg_receiver(&mut self) -> &mut mpsc::Receiver<CCLogBufferMessage> {
        &mut self.msg_receiver
    }

    /// Handles one received message, then drains everything already queued so a burst of
    /// lines broadcasts as ONE coalesced event instead of one event per line. Bounded by
    /// the channel capacity so cancellation stays responsive under sustained spam.
    fn handle_burst(&mut self, first_msg: CCLogBufferMessage) {
        // broadcast::send takes ownership, so no buffer to reuse; String::new() defers allocation.
        let mut pending_broadcast = String::new();
        self.handle_msg(first_msg, &mut pending_broadcast);
        for _ in 0..LOG_MSG_CHANNEL_CAP {
            match self.msg_receiver.try_recv() {
                Ok(msg) => self.handle_msg(msg, &mut pending_broadcast),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        if pending_broadcast.is_empty().not() {
            let _ = self.new_log_broadcaster.send(pending_broadcast);
        }
    }

    fn handle_msg(&mut self, msg: CCLogBufferMessage, pending_broadcast: &mut String) {
        match msg {
            CCLogBufferMessage::GetLogs { respond_to } => {
                let mut all_logs = String::with_capacity(self.buf_bytes);
                for cc_log in &self.buf {
                    all_logs.push_str(cc_log.message.as_str());
                }
                let _ = respond_to.send(all_logs);
            }
            CCLogBufferMessage::Log { log } => {
                let log = truncate_entry(log);
                pending_broadcast.push_str(&log);
                self.push_entry(log);
            }
            CCLogBufferMessage::WarningsErrors { respond_to } => {
                let warnings = self
                    .buf
                    .iter()
                    .filter(|cc_log| {
                        self.acknowledge_issues_timestamp < cc_log.timestamp
                            && cc_log.message.contains("WARN")
                    })
                    .count();
                let errors = self
                    .buf
                    .iter()
                    .filter(|cc_log| {
                        self.acknowledge_issues_timestamp < cc_log.timestamp
                            && cc_log.message.contains("ERROR")
                    })
                    .count();
                let _ = respond_to.send((warnings, errors));
            }
            CCLogBufferMessage::AcknowledgeIssues { respond_to } => {
                self.acknowledge_issues_timestamp = Local::now();
                let _ = respond_to.send(Ok(()));
            }
        }
    }

    fn push_entry(&mut self, message: String) {
        assert!(message.len() <= LOG_ENTRY_MAX_BYTES);
        self.buf_bytes += message.len();
        self.buf.push_back(CCLog {
            timestamp: Local::now(),
            message,
        });
        // Evict oldest entries until back under both bounds. Terminates: each iteration
        // pops one entry, and an empty buffer is trivially under budget.
        while self.buf.len() > LOG_BUFFER_LINE_SIZE || self.buf_bytes > LOG_BUFFER_MAX_BYTES {
            let Some(evicted) = self.buf.pop_front() else {
                break;
            };
            assert!(self.buf_bytes >= evicted.message.len());
            self.buf_bytes -= evicted.message.len();
        }
        debug_assert!(self.buf.len() <= LOG_BUFFER_LINE_SIZE);
        debug_assert!(self.buf_bytes <= LOG_BUFFER_MAX_BYTES);
    }
}

/// Truncates one formatted log line to the entry cap on a char boundary, marking the cut.
fn truncate_entry(mut log: String) -> String {
    if log.len() <= LOG_ENTRY_MAX_BYTES {
        return log;
    }
    let mut cut_index = LOG_ENTRY_MAX_BYTES - LOG_TRUNCATION_MARKER.len();
    while log.is_char_boundary(cut_index).not() {
        cut_index -= 1;
    }
    log.truncate(cut_index);
    log.push_str(LOG_TRUNCATION_MARKER);
    debug_assert!(log.len() <= LOG_ENTRY_MAX_BYTES);
    log
}

#[derive(Clone)]
pub struct LogBufHandle {
    msg_sender: mpsc::Sender<CCLogBufferMessage>,
    new_log_sender: broadcast::Sender<String>,
    cancel_token: CancellationToken,
    level_info: LogLevelInfo,
}

impl LogBufHandle {
    pub fn new(cancel_token: CancellationToken) -> Self {
        let (msg_sender, receiver) = mpsc::channel(LOG_MSG_CHANNEL_CAP);
        let (new_log_sender, _new_log_rx) = broadcast::channel(NEW_LOG_CHANNEL_CAP);
        let log_buf_actor = LogBufferActor::new(new_log_sender.clone(), receiver);
        rt::spawn(run_log_buf_actor(log_buf_actor, cancel_token.clone()));
        Self {
            msg_sender,
            new_log_sender,
            cancel_token,
            level_info: LogLevelInfo::default(),
        }
    }

    pub fn with_level_info(mut self, level_info: LogLevelInfo) -> Self {
        self.level_info = level_info;
        self
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<String> {
        &self.new_log_sender
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    pub fn level_info(&self) -> LogLevelInfo {
        self.level_info
    }

    #[allow(dead_code)]
    pub async fn log(&self, log: String) {
        let _ = self.msg_sender.send(CCLogBufferMessage::Log { log }).await;
    }

    pub async fn get_logs(&self) -> String {
        let (tx, rx) = oneshot::channel();
        let msg = CCLogBufferMessage::GetLogs { respond_to: tx };
        let _ = self.msg_sender.send(msg).await;
        rx.await.unwrap_or_default()
    }

    pub async fn warning_errors(&self) -> (usize, usize) {
        let (tx, rx) = oneshot::channel();
        let msg = CCLogBufferMessage::WarningsErrors { respond_to: tx };
        let _ = self.msg_sender.send(msg).await;
        rx.await.unwrap_or((0, 0))
    }

    pub async fn acknowledge_issues(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = CCLogBufferMessage::AcknowledgeIssues { respond_to: tx };
        let _ = self.msg_sender.send(msg).await;
        rx.await?
    }
}

async fn run_log_buf_actor(mut log_buf_actor: LogBufferActor, cancel_token: CancellationToken) {
    loop {
        tokio::select! {
        // guarantees that this task is shut down.
        () = cancel_token.cancelled() => {
            log_buf_actor.buf.clear();
            log_buf_actor.buf_bytes = 0;
            break;
        }
        Some(msg) = log_buf_actor.msg_receiver().recv() => {
            log_buf_actor.handle_burst(msg);
        }
        else => break,
        }
    }
    trace!("LogBuffer is shutting down");
}

impl std::io::Write for LogBufHandle {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let log = unsafe { from_utf8_unchecked(buf).to_owned() };
        // Called synchronously from arbitrary threads (e.g. library logging on their own threads),
        // so we cannot await and cannot assume a runtime is entered. `try_send` is non-blocking and
        // thread-safe. On a full channel the line is dropped: this only feeds the UI log ring, while
        // the journal/stderr sink still records every line.
        let _ = self.msg_sender.try_send(CCLogBufferMessage::Log { log });
        Ok(buf.len())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Test-only: sets the process-wide max log level and restores the previous one on drop, so a
/// panicking test cannot leave it changed for the rest of the run.
#[cfg(test)]
pub struct MaxLevelGuard {
    previous: LevelFilter,
}

#[cfg(test)]
impl MaxLevelGuard {
    pub fn set(level: LevelFilter) -> Self {
        let previous = log::max_level();
        log::set_max_level(level);
        Self { previous }
    }
}

#[cfg(test)]
impl Drop for MaxLevelGuard {
    fn drop(&mut self) {
        log::set_max_level(self.previous);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::io::Write;

    // Goal: the guard restores the previous max level even when the test panics while it is held.
    #[test]
    #[serial]
    fn max_level_guard_restores_the_level_on_panic() {
        let before = log::max_level();
        let raised = if before == LevelFilter::Trace {
            LevelFilter::Error
        } else {
            LevelFilter::Trace
        };

        let result = std::panic::catch_unwind(|| {
            let _guard = MaxLevelGuard::set(raised);
            assert_eq!(log::max_level(), raised);
            panic!("while the level is raised");
        });

        assert!(result.is_err());
        assert_eq!(log::max_level(), before);
    }

    // Goal: the synchronous `Write` impl delivers a line to the buffer actor via `try_send` (no
    // runtime handle), and `get_logs` returns it. The actor drains the bounded channel FIFO, so the
    // logged line is processed before the later `GetLogs` request resolves.
    #[test]
    fn write_delivers_line_to_buffer_actor() {
        crate::rt::test_runtime(async {
            let mut handle = LogBufHandle::new(CancellationToken::new());
            handle.write_all(b"hello buffer\n").unwrap();
            let logs = handle.get_logs().await;
            assert!(logs.contains("hello buffer"), "logs were: {logs:?}");
        });
    }

    // Goal: writes past the channel capacity must not panic or block (they drop), since `write` is
    // called synchronously from threads with no runtime. We never drain here, so the channel fills.
    #[test]
    fn write_does_not_block_or_panic_when_channel_full() {
        crate::rt::test_runtime(async {
            let mut handle = LogBufHandle::new(CancellationToken::new());
            for _ in 0..(LOG_MSG_CHANNEL_CAP * 4) {
                handle.write_all(b"overflow\n").unwrap();
            }
        });
    }

    // Goal: an oversized line is truncated on a char boundary to the entry cap with a marker,
    // while short lines pass through untouched. A multi-byte char spanning the cut position
    // must not split (String::truncate would panic).
    #[test]
    fn oversized_entry_is_truncated_with_marker() {
        let short = truncate_entry("short line\n".to_owned());
        assert_eq!(short, "short line\n");

        let long = truncate_entry("x".repeat(LOG_ENTRY_MAX_BYTES * 2));
        assert!(long.len() <= LOG_ENTRY_MAX_BYTES);
        assert!(long.ends_with(LOG_TRUNCATION_MARKER));

        let cut_index = LOG_ENTRY_MAX_BYTES - LOG_TRUNCATION_MARKER.len();
        let mut multibyte = "y".repeat(cut_index - 1);
        multibyte.push_str(&"ä".repeat(LOG_ENTRY_MAX_BYTES));
        let truncated = truncate_entry(multibyte);
        assert!(truncated.len() <= LOG_ENTRY_MAX_BYTES);
        assert!(truncated.ends_with(LOG_TRUNCATION_MARKER));
    }

    // Goal: the buffer evicts oldest entries once total bytes exceed the budget, so GET /logs
    // can never return more than LOG_BUFFER_MAX_BYTES. Methodology: push ~7 KB entries until
    // well past the budget, then verify total size and that only the oldest lines are gone.
    #[test]
    fn buffer_evicts_oldest_past_byte_budget() {
        crate::rt::test_runtime(async {
            let handle = LogBufHandle::new(CancellationToken::new());
            let entry_count = 200; // ~7 KB * 200 = ~1.4 MB, past the 1 MB budget
            for i in 0..entry_count {
                let line = format!("{i:04} {}\n", "z".repeat(7 * 1024));
                handle.log(line).await;
            }
            let logs = handle.get_logs().await;
            assert!(logs.len() <= LOG_BUFFER_MAX_BYTES);
            assert!(logs.contains("0000 ").not(), "oldest entry must be evicted");
            assert!(logs.contains(&format!("{:04} ", entry_count - 1)));
        });
    }

    // Goal: the entry-count cap holds independently of bytes: pushing more than the cap
    // drops the oldest lines and keeps exactly the newest LOG_BUFFER_LINE_SIZE.
    #[test]
    fn buffer_evicts_oldest_past_entry_cap() {
        crate::rt::test_runtime(async {
            let handle = LogBufHandle::new(CancellationToken::new());
            let entry_count = LOG_BUFFER_LINE_SIZE + 100;
            for i in 0..entry_count {
                handle.log(format!("entry {i}\n")).await;
            }
            let logs = handle.get_logs().await;
            assert_eq!(logs.lines().count(), LOG_BUFFER_LINE_SIZE);
            assert!(logs.starts_with("entry 100\n"));
            assert!(logs.ends_with(&format!("entry {}\n", entry_count - 1)));
        });
    }

    // Goal: a burst of writes queued before the actor runs must broadcast as ONE coalesced
    // event containing every line, not one event per line. Methodology: on the single-threaded
    // test runtime, synchronous try_send writes queue up while the actor task has not yet been
    // polled; the first recv() then observes the actor's single drained broadcast.
    #[test]
    fn burst_broadcasts_as_single_coalesced_event() {
        crate::rt::test_runtime(async {
            let mut handle = LogBufHandle::new(CancellationToken::new());
            let mut rx = handle.broadcaster().subscribe();
            let line_count = 10;
            for i in 0..line_count {
                handle.write_all(format!("burst {i}\n").as_bytes()).unwrap();
            }
            let event = rx.recv().await.unwrap();
            assert_eq!(event.lines().count(), line_count);
            for i in 0..line_count {
                assert!(event.contains(&format!("burst {i}\n")));
            }
            assert!(matches!(
                rx.try_recv(),
                Err(broadcast::error::TryRecvError::Empty)
            ));
        });
    }

    // Goal: the most verbose request wins, the setting never lowers a level, and a tie
    // reports the env var or flag. Methodology: table of every source combination that
    // matters, including the packaged default of CC_LOG=INFO.
    #[test]
    fn resolve_log_level_most_verbose_wins() {
        use LevelFilter::{Debug, Error, Info, Off, Trace, Warn};
        use LogLevelSource::{Default, Env, Flag, Settings};
        let cases = [
            // (flag, env, setting) => (level, source)
            ((false, None, false), (Info, Default)),
            ((false, Some(Info), false), (Info, Env)),
            ((false, Some(Error), false), (Error, Env)),
            ((false, None, true), (Debug, Settings)),
            ((false, Some(Info), true), (Debug, Settings)),
            ((false, Some(Warn), true), (Debug, Settings)),
            ((false, Some(Off), true), (Debug, Settings)),
            ((false, Some(Debug), true), (Debug, Env)),
            ((false, Some(Trace), true), (Trace, Env)),
            ((true, None, false), (Debug, Flag)),
            ((true, Some(Info), false), (Debug, Flag)),
            ((true, Some(Trace), false), (Trace, Env)),
            ((true, None, true), (Debug, Flag)),
        ];
        for ((flag, env, setting), expected) in cases {
            assert_eq!(
                resolve_log_level(flag, env, setting),
                expected,
                "flag={flag} env={env:?} setting={setting}"
            );
        }
    }

    // Goal: only ERROR, WARN and INFO reach the UI buffer; DEBUG and TRACE never do.
    #[test]
    fn ui_buffer_takes_info_and_above_only() {
        assert!(is_ui_buffered(Level::Error));
        assert!(is_ui_buffered(Level::Warn));
        assert!(is_ui_buffered(Level::Info));
        assert!(is_ui_buffered(Level::Debug).not());
        assert!(is_ui_buffered(Level::Trace).not());
    }

    // Goal: at DEBUG level, a debug record still skips the UI buffer while an info record
    // lands in it, and the handle reports the level it was built with. Methodology: build
    // the real logger, swap its journal/stderr sink for a no-op, log one of each, then read
    // the buffer back.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn debug_records_skip_the_ui_buffer() {
        crate::rt::test_runtime(async {
            let level_info = LogLevelInfo {
                level: LevelFilter::Debug,
                source: LogLevelSource::Settings,
                journal: false,
            };
            let (mut cc_logger, handle) =
                CCLogger::new(level_info, None, VERSION, CancellationToken::new()).unwrap();
            cc_logger.sink = SinkFeed::Journal(
                LogWriter::start("nowhere", drop::<QueuedRecord>, QueuedRecord::of).unwrap(),
            );
            cc_logger.log(
                &Record::builder()
                    .level(Level::Info)
                    .target("coolercontrold")
                    .args(format_args!("info line"))
                    .build(),
            );
            cc_logger.log(
                &Record::builder()
                    .level(Level::Debug)
                    .target("coolercontrold")
                    .args(format_args!("debug line"))
                    .build(),
            );
            let logs = handle.get_logs().await;
            assert!(logs.contains("info line"), "logs were: {logs:?}");
            assert!(logs.contains("debug line").not(), "logs were: {logs:?}");
            assert_eq!(handle.level_info(), level_info);
        });
    }

    // Goal: a stderr line carries the time it was logged, not the later time a stalled stderr
    // took it. Method: wedge the sink on a first line, log a second at millisecond precision,
    // hold the sink well past that, then read the second line's timestamp back.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn stderr_lines_keep_the_time_they_were_logged() {
        use std::sync::{mpsc, Arc, Mutex};
        let lines = Arc::new(Mutex::new(Vec::new()));
        let sink_lines = Arc::clone(&lines);
        let (started_tx, started) = mpsc::sync_channel(4);
        let (gate, gate_rx) = mpsc::sync_channel::<()>(0);
        let writer = LogWriter::start(
            "the test sink",
            move |line: Vec<u8>| {
                let _ = started_tx.try_send(());
                let _ = gate_rx.recv();
                sink_lines.lock().unwrap().push(line);
            },
            |_: &Record| Vec::new(),
        )
        .unwrap();
        let feed = SinkFeed::stderr(
            LevelFilter::Info,
            TimestampPrecision::Millis,
            WriteStyle::Never,
            writer.clone(),
        );
        writer.push(b"first\n".to_vec());
        started.recv().unwrap();

        feed.log(
            &Record::builder()
                .level(Level::Info)
                .target("coolercontrold")
                .args(format_args!("held line"))
                .build(),
        );
        let logged_by = chrono::Utc::now();
        std::thread::sleep(std::time::Duration::from_millis(300));
        drop(gate);
        feed.drain();

        let lines = lines.lock().unwrap();
        let held = String::from_utf8(lines[1].clone()).unwrap();
        assert!(held.contains("held line"), "{held:?}");
        let stamp = held.trim_start_matches('[').split(' ').next().unwrap();
        let stamped = DateTime::parse_from_rfc3339(stamp).unwrap();
        assert!(
            stamped < logged_by + chrono::Duration::milliseconds(100),
            "{held:?} was stamped after {logged_by}"
        );
    }

    /// A journal feed whose sink records each message. The sink signals `started` as it takes a
    /// record, then waits `delay`, and holds until the returned gate drops when `hold` is set.
    fn recording_journal(
        delay: std::time::Duration,
        hold: bool,
    ) -> (
        SinkFeed,
        Arc<Mutex<Vec<String>>>,
        std::sync::mpsc::Receiver<()>,
        Option<std::sync::mpsc::SyncSender<()>>,
    ) {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink_messages = Arc::clone(&messages);
        let (started_tx, started) = std::sync::mpsc::sync_channel(16);
        let (gate, gate_rx) = std::sync::mpsc::sync_channel::<()>(0);
        let writer = LogWriter::start(
            "the test sink",
            move |record: QueuedRecord| {
                let _ = started_tx.try_send(());
                std::thread::sleep(delay);
                let _ = gate_rx.recv();
                let fields = FirstMessage(Mutex::new(None));
                record.log_to(&fields);
                sink_messages
                    .lock()
                    .unwrap()
                    .extend(fields.0.into_inner().unwrap());
            },
            QueuedRecord::of,
        )
        .unwrap();
        let gate = if hold { Some(gate) } else { None };
        (SinkFeed::Journal(writer), messages, started, gate)
    }

    /// Keeps the message of the record it is given.
    struct FirstMessage(Mutex<Option<String>>);

    impl Log for FirstMessage {
        fn enabled(&self, _metadata: &Metadata) -> bool {
            true
        }

        fn log(&self, record: &Record) {
            *self.0.lock().unwrap() = Some(record.args().to_string());
        }

        fn flush(&self) {}
    }

    fn log_info(feed: &SinkFeed, message: &str) {
        feed.log(
            &Record::builder()
                .level(Level::Info)
                .target("coolercontrold")
                .args(format_args!("{message}"))
                .build(),
        );
    }

    // Goal: a panic off the main thread waits for the lines queued before it, so they reach the
    // sink ahead of the panic report. Method: queue a line to a slow sink, run the hook's drain
    // as a thread other than the main one, and read the sink.
    #[test]
    fn a_panic_drains_queued_lines_first() {
        let (feed, messages, _started, _gate) =
            recording_journal(std::time::Duration::from_millis(50), false);
        let other_thread = std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap();
        log_info(&feed, "before the panic");

        drain_for_panic(&feed.drainer(), other_thread);

        assert_eq!(*messages.lock().unwrap(), vec!["before the panic"]);
    }

    // Goal: only the main thread gets the short panic drain bound; every other thread waits the
    // full one. Method: ask for the bound as the main thread and as a spawned thread.
    #[test]
    fn only_the_main_thread_gets_the_short_panic_drain() {
        let main_thread = std::thread::current().id();
        let other_thread = std::thread::spawn(|| std::thread::current().id())
            .join()
            .unwrap();

        assert_eq!(
            panic_drain_timeout(main_thread, main_thread),
            MAIN_THREAD_PANIC_DRAIN_TIMEOUT
        );
        assert_eq!(
            panic_drain_timeout(other_thread, main_thread),
            DRAIN_TIMEOUT
        );
    }

    // Goal: a panic on the main runtime thread, which carries on afterwards, waits only briefly
    // for a stalled sink. Method: wedge the sink, queue a line, run the hook's drain as the main
    // thread, and time it.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_main_thread_panic_waits_briefly_for_a_stalled_sink() {
        let (feed, messages, started, gate) = recording_journal(std::time::Duration::ZERO, true);
        log_info(&feed, "wedges the sink");
        started.recv().unwrap();
        log_info(&feed, "queued");
        let start = std::time::Instant::now();

        drain_for_panic(&feed.drainer(), std::thread::current().id());

        let elapsed = start.elapsed();
        assert!(elapsed >= MAIN_THREAD_PANIC_DRAIN_TIMEOUT, "{elapsed:?}");
        assert!(elapsed < DRAIN_TIMEOUT, "{elapsed:?}");
        assert!(messages.lock().unwrap().is_empty());
        drop(gate);
    }

    // Goal: the stderr report comes out as one whole formatted line per call, nothing carried
    // over. Method: format two records and compare each line's message and ending.
    #[test]
    fn the_stderr_report_is_one_formatted_line_per_call() {
        let mut report = report_formatter(TimestampPrecision::Seconds, WriteStyle::Never);
        let mut record = |message: &str| {
            report(
                &Record::builder()
                    .level(Level::Info)
                    .target("coolercontrold::log_writer")
                    .args(format_args!("{message}"))
                    .build(),
            )
        };

        let first = String::from_utf8(record("first report")).unwrap();
        let second = String::from_utf8(record("second report")).unwrap();

        assert!(first.ends_with("first report\n"), "{first:?}");
        assert!(second.ends_with("second report\n"), "{second:?}");
        assert!(second.contains("first").not(), "{second:?}");
    }

    // Goal: a handle built without level info reports the INFO default, so tests and any
    // caller that skips `with_level_info` never claim debug is on.
    #[test]
    fn handle_defaults_to_info_level() {
        crate::rt::test_runtime(async {
            let handle = LogBufHandle::new(CancellationToken::new());
            assert_eq!(handle.level_info(), LogLevelInfo::default());
            assert_eq!(handle.level_info().level, LevelFilter::Info);
            assert_eq!(handle.level_info().source, LogLevelSource::Default);
        });
    }
}
