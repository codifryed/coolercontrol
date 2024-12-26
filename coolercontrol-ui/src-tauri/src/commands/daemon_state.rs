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
 ******************************************************************************/
use crate::commands::notifications::send_notification;
use reqwest_eventsource::retry::Constant;
use reqwest_eventsource::{Event, EventSource};
use serde::{Deserialize, Serialize};
use std::ops::Not;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::command;
use zbus::export::futures_util::StreamExt;

#[command]
pub async fn acknowledge_daemon_issues(
    daemon_state_init: tauri::State<'_, Arc<DaemonState>>,
) -> Result<(), ()> {
    *daemon_state_init.has_errors.lock().unwrap() = false;
    Ok(())
}

/// This is designed to be used when the frontend Window is Closed to Tray, which
/// suspends the engine, but we still have backend running.
#[command]
pub async fn connected_to_daemon(
    daemon_address: String,
    has_errors: bool,
    daemon_state_init: tauri::State<'_, Arc<DaemonState>>,
) -> Result<(), ()> {
    if has_errors {
        let _ = send_notification(
            "Daemon Errors",
            "The daemon logs contain errors. You should investigate.",
            Some("dialog-error"),
        )
        .await;
    }
    if daemon_state_init.first_run.lock().unwrap().not() {
        return Ok(());
    }
    *daemon_state_init.has_errors.lock().unwrap() = has_errors;
    watch_connection_and_logs(daemon_address.clone(), Arc::clone(&*daemon_state_init));
    watch_mode_activation(daemon_address);
    Ok(())
}

/// Watches the SSE stream of the daemon logs, checks for errors and updates the state accordingly.
/// When the connection is lost, it sends a notification.
/// When the connection is restored, it sends a notification.
/// When a log contains an error, it sends a notification if the state is not already in an error state.
/// The SSE connection is retried every second if it fails.
///
/// The `daemon_state` is used to sync frontend state.
/// The address is the url of the daemon api, witch is determined on a successful connection.
fn watch_connection_and_logs(address: String, daemon_state: Arc<DaemonState>) {
    tauri::async_runtime::spawn(async move {
        let mut es = EventSource::get(format!("{address}sse/logs"));
        es.set_retry_policy(Box::new(Constant::new(Duration::from_secs(1), None)));
        let mut is_connected = true;
        while let Some(event) = es.next().await {
            match event {
                Ok(Event::Open) => {
                    // On reconnect, we don't have a message, just re-opened connection
                    if !is_connected {
                        let _ = send_notification(
                            "Daemon Connection Restored",
                            "Connection with the daemon has been restored.",
                            Some("dialog-information"),
                        )
                        .await;
                        is_connected = true;
                    } else if *daemon_state.first_run.lock().unwrap() {
                        let _ = send_notification(
                            "Daemon Connection Established",
                            "The connection with the daemon has been established.",
                            Some("dialog-information"),
                        )
                        .await;
                        *daemon_state.first_run.lock().unwrap() = false;
                    }
                }
                Ok(Event::Message(msg)) => {
                    if !is_connected {
                        let _ = send_notification(
                            "Daemon Connection Restored",
                            "Connection with the daemon has been restored.",
                            Some("dialog-information"),
                        )
                        .await;
                        is_connected = true;
                    }
                    let log_contains_error = msg.data.contains("ERROR");
                    if log_contains_error && !*daemon_state.has_errors.lock().unwrap() {
                        *daemon_state.has_errors.lock().unwrap() = true;
                        let _ = send_notification(
                            "Daemon Errors",
                            "The daemon logs contain errors. You should investigate.",
                            Some("dialog-error"),
                        )
                        .await;
                    }
                }
                Err(_) => {
                    if is_connected {
                        let _ = send_notification(
                            "Daemon Disconnected",
                            "Connection with the daemon has been lost",
                            Some("dialog-error"),
                        )
                        .await;
                        is_connected = false;
                    }
                }
            }
        }
    });
}

/// Watches SSE endpoint for mode activations and sends a notification.
/// Retries/Reconnection attempts are automatic with retry policy.
fn watch_mode_activation(address: String) {
    tauri::async_runtime::spawn(async move {
        let mut es = EventSource::get(format!("{address}sse/modes"));
        es.set_retry_policy(Box::new(Constant::new(Duration::from_secs(1), None)));
        while let Some(event) = es.next().await {
            if let Ok(Event::Message(msg)) = event {
                let mode_activated = match serde_json::from_str::<ModeActivated>(&msg.data) {
                    Ok(mode_activated) => mode_activated,
                    Err(err) => {
                        println!("Modes Activated Serialization Error: {err}");
                        return;
                    }
                };
                let title = if mode_activated.already_active {
                    format!("Mode {} Already Active", mode_activated.name)
                } else {
                    format!("Mode {} Activated", mode_activated.name)
                };
                let _ = send_notification(&title, "", Some("dialog-information")).await;
            }
        }
    });
}

pub struct DaemonState {
    has_errors: Mutex<bool>,
    first_run: Mutex<bool>,
}

impl Default for DaemonState {
    fn default() -> Self {
        Self {
            has_errors: Mutex::new(false),
            first_run: Mutex::new(true),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeActivated {
    pub name: String,
    pub already_active: bool,
}
