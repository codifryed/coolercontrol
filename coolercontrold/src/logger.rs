/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use crate::{cc_fs, exit_successfully, Args, LOG_ENV, VERSION};
use anyhow::{Context, Result};
use chrono::{DateTime, Local};
use env_logger::Logger;
use log::{info, trace, LevelFilter, Log, Metadata, Record, SetLoggerError};
use nix::NixPath;
use nu_glob::glob;
use regex::Regex;
use std::collections::{HashSet, VecDeque};
use std::path::PathBuf;
use std::str::{from_utf8_unchecked, FromStr};
use systemd_journal_logger::{connected_to_journal, JournalLog};
use tokio::runtime::Handle;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const LOG_BUFFER_LINE_SIZE: usize = 500;
const NEW_LOG_CHANNEL_CAP: usize = 2;

pub async fn setup_logging(cmd_args: &Args, run_token: CancellationToken) -> Result<LogBufHandle> {
    let version = VERSION.unwrap_or("unknown");
    let log_level = if cmd_args.debug {
        LevelFilter::Debug
    } else if let Ok(log_lvl) = std::env::var(LOG_ENV) {
        LevelFilter::from_str(&log_lvl).unwrap_or(LevelFilter::Info)
    } else {
        LevelFilter::Info
    };
    let (logger, log_buf_handle) = CCLogger::new(log_level, version, run_token)?;
    logger.init()?;
    info!("Logging Level: {}", log::max_level());
    info!(
        "System Info:\n\
        CoolerControlD {version}\n\
        Name:\t{}\n\
        OS:\t\t{}\n\
        Host:\t{}\n\
        Kernel:\t{}\n\
        Board Manufacturer:\t{}\n\
        Board Name:\t\t{}\n\
        Board Version:\t{}\n\
        BIOS Manufacturer:\t{}\n\
        BIOS Version:\t{}\n\
        {}",
        sysinfo::System::name().unwrap_or_default(),
        sysinfo::System::long_os_version().unwrap_or_default(),
        sysinfo::System::host_name().unwrap_or_default(),
        sysinfo::System::kernel_version().unwrap_or_default(),
        get_dmi_system_info("board_vendor").await,
        get_dmi_system_info("board_name").await,
        get_dmi_system_info("board_version").await,
        get_dmi_system_info("bios_vendor").await,
        get_dmi_system_info("bios_release").await,
        get_xdg_desktop_info().await.unwrap_or_default(),
    );
    if cmd_args.version {
        exit_successfully();
    }
    Ok(log_buf_handle)
}

async fn get_dmi_system_info(name: &str) -> String {
    cc_fs::read_txt(format!("/sys/devices/virtual/dmi/id/{name}"))
        .await
        .unwrap_or_default()
        .trim()
        .to_owned()
}

async fn get_xdg_desktop_info() -> Result<String> {
    let mut desktops = HashSet::new();
    let mut sessions_types = HashSet::new();
    let environ_paths = glob("/proc/*/environ", None)?
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
        Ok(String::default())
    } else {
        let desktop_list = Vec::from_iter(desktops).join(", ");
        let session_list = Vec::from_iter(sessions_types).join(", ");
        Ok(format!(
            "XDG Desktops:\t{desktop_list}\nXDG Session Types:\t{session_list}"
        ))
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
        let log_filter = env_logger::Builder::from_env(LOG_ENV)
            .filter_level(max_level)
            .filter_module("zbus", lib_log_level)
            .filter_module("tracing", lib_disabled_level)
            .filter_module("aide", lib_disabled_level)
            .filter_module("tower_http", lib_disabled_level)
            // hyper now uses tracing, but doesn't seem to log as other "tracing crates" do.
            .filter_module("hyper", lib_log_level)
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
            acknowledge_issues_timestamp: Local::now(),
            new_log_broadcaster,
            msg_receiver,
        }
    }

    fn msg_receiver(&mut self) -> &mut mpsc::Receiver<CCLogBufferMessage> {
        &mut self.msg_receiver
    }

    fn handle_msg(&mut self, msg: CCLogBufferMessage) {
        match msg {
            CCLogBufferMessage::GetLogs { respond_to } => {
                let all_logs = self.buf.iter().fold(String::new(), |mut acc, cc_log| {
                    acc.push_str(cc_log.message.as_str());
                    acc
                });
                let _ = respond_to.send(all_logs);
            }
            CCLogBufferMessage::Log { log } => {
                if self.buf.len() >= LOG_BUFFER_LINE_SIZE {
                    self.buf.pop_front();
                }

                self.buf.push_back(CCLog {
                    timestamp: Local::now(),
                    message: log.clone(),
                });
                let _ = self.new_log_broadcaster.send(log);
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
}

#[derive(Clone)]
pub struct LogBufHandle {
    msg_sender: mpsc::Sender<CCLogBufferMessage>,
    new_log_sender: broadcast::Sender<String>,
    cancel_token: CancellationToken,
}

impl LogBufHandle {
    pub fn new(cancel_token: CancellationToken) -> Self {
        let (msg_sender, receiver) = mpsc::channel(10);
        let (new_log_sender, _new_log_rx) = broadcast::channel(NEW_LOG_CHANNEL_CAP);
        let log_buf_actor = LogBufferActor::new(new_log_sender.clone(), receiver);
        tokio::task::spawn_local(run_log_buf_actor(log_buf_actor, cancel_token.clone()));
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
            break;
        }
        Some(msg) = log_buf_actor.msg_receiver().recv() => {
            log_buf_actor.handle_msg(msg);
        }
        else => break,
        }
    }
    trace!("LogBuffer is shutting down");
}

impl std::io::Write for LogBufHandle {
    #[inline]
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let log_string = unsafe { from_utf8_unchecked(buf).to_owned() };
        // trick to enter the runtime context inside a non-async trait impl
        let runtime_handle = Handle::current();
        let _ = runtime_handle.enter();
        let sender = self.msg_sender.clone();
        runtime_handle.spawn(async move {
            let _ = sender
                .send(CCLogBufferMessage::Log { log: log_string })
                .await;
        });
        Ok(buf.len())
    }

    #[inline]
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
