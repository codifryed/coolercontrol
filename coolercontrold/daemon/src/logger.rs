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

use crate::repositories::liquidctl::liqctld_service;
use crate::rt;
use crate::{cc_fs, exit_successfully, Args, ENV_CC_LOG, ENV_LOG, VERSION};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use env_logger::Logger;
use log::{debug, info, trace, LevelFilter, Log, Metadata, Record, SetLoggerError};
use nix::NixPath;
use nu_glob::{glob, Uninterruptible};
use regex::Regex;
use std::collections::{HashSet, VecDeque};
use std::ops::Not;
use std::path::PathBuf;
use std::str::{from_utf8_unchecked, FromStr};
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

pub async fn setup_logging(cmd_args: &Args, run_token: CancellationToken) -> Result<LogBufHandle> {
    let log_level = if cmd_args.debug {
        LevelFilter::Debug
    } else if let Ok(log_lvl) = std::env::var(ENV_CC_LOG).or_else(|_| std::env::var(ENV_LOG)) {
        LevelFilter::from_str(&log_lvl).unwrap_or(LevelFilter::Info)
    } else {
        LevelFilter::Info
    };
    let (logger, log_buf_handle) = CCLogger::new(log_level, VERSION, run_token)?;
    logger.init()?;
    if cmd_args.wants_system_info_banner() {
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

/// Logs the daemon/host banner (version, OS, board, BIOS, desktop) to the journal.
async fn log_system_info() {
    info!("System Info:");
    info!("  {}", "-".repeat(60));
    info!("  {:<20} {}", "CoolerControlD", VERSION);
    info!(
        "  {:<20} {}",
        "Name",
        sysinfo::System::name().unwrap_or_default()
    );
    info!(
        "  {:<20} {}",
        "OS",
        sysinfo::System::long_os_version().unwrap_or_default()
    );
    info!(
        "  {:<20} {}",
        "Host",
        sysinfo::System::host_name().unwrap_or_default()
    );
    info!(
        "  {:<20} {}",
        "Kernel",
        sysinfo::System::kernel_version().unwrap_or_default()
    );
    info!("  {:<20} {}", "Arch", sysinfo::System::cpu_arch());
    info!(
        "  {:<20} {}",
        "Board Manufacturer",
        get_dmi_system_info("board_vendor").await
    );
    info!(
        "  {:<20} {}",
        "Board Name",
        get_dmi_system_info("board_name").await
    );
    info!(
        "  {:<20} {}",
        "Board Version",
        get_dmi_system_info("board_version").await
    );
    info!(
        "  {:<20} {}",
        "BIOS Manufacturer",
        get_dmi_system_info("bios_vendor").await
    );
    info!(
        "  {:<20} {}",
        "BIOS Version",
        get_dmi_system_info("bios_release").await
    );
    match get_xdg_desktop_info().await {
        Ok((desktops, sessions)) => {
            info!("  {:<20} {}", "XDG Desktops", desktops);
            info!("  {:<20} {}", "XDG Session Types", sessions);
        }
        Err(err) => debug!("Failed to get XDG desktop info: {err}"),
    }
}

async fn get_dmi_system_info(name: &str) -> String {
    cc_fs::read_txt(format!("/sys/devices/virtual/dmi/id/{name}"))
        .await
        .unwrap_or_default()
        .trim()
        .to_owned()
}

async fn get_xdg_desktop_info() -> Result<(String, String)> {
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
        let Ok(content) = cc_fs::read_txt(&path).await else {
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
    logger: Box<dyn Log>,
    buf_logger: Box<dyn Log>,
}

impl CCLogger {
    fn new(
        max_level: LevelFilter,
        version: &str,
        run_token: CancellationToken,
    ) -> Result<(Self, LogBufHandle)> {
        // set library logging levels to one level above the application's to keep chatter down
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
        let timestamp_precision = if max_level >= LevelFilter::Debug {
            env_logger::fmt::TimestampPrecision::Millis
        } else {
            env_logger::fmt::TimestampPrecision::Seconds
        };
        let env_log_name = if std::env::var(ENV_CC_LOG).is_ok() {
            ENV_CC_LOG
        } else {
            ENV_LOG
        };
        let log_filter = env_logger::Builder::from_env(env_log_name)
            .filter_level(max_level)
            .filter_module("zbus", lib_log_level)
            .filter_module("tracing", lib_disabled_level)
            .filter_module("aide", lib_disabled_level)
            .filter_module("tower_http", lib_disabled_level)
            // hyper now uses tracing, but doesn't seem to log as other "tracing crates" do.
            .filter_module("hyper", lib_log_level)
            .filter_module("h2", lib_disabled_level) // h2::codec writes every frame
            .filter_module("tower_sessions_core", lib_very_reduced_level)
            .build();
        let logger: Box<dyn Log> = if connected_to_journal() {
            Box::new(JournalLog::new()?.with_extra_fields(vec![("VERSION", version)]))
        } else {
            Box::new(
                env_logger::Builder::new()
                    .filter_level(max_level)
                    .format_timestamp(Some(timestamp_precision))
                    .build(),
            )
        };
        let log_buf_handle = LogBufHandle::new(run_token);
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
                logger,
                buf_logger,
            },
            log_buf_handle,
        ))
    }

    fn init(self) -> Result<(), SetLoggerError> {
        log::set_max_level(self.max_level);
        log::set_boxed_logger(Box::new(self))
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
            self.logger.log(record);
            self.buf_logger.log(record);
        }
    }

    /// Flush log records.
    ///
    /// A no-op for this implementation.
    fn flush(&self) {}
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
        }
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<String> {
        &self.new_log_sender
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

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
}
