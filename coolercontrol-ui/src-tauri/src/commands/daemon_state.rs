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
        )
        .await;
    }
    *daemon_state_init.has_errors.lock().unwrap() = has_errors;

    // Log & Connection watch
    let dsc = Arc::clone(&*daemon_state_init);
    tauri::async_runtime::spawn(async move {
        let daemon_state = dsc.clone();
        let mut es = EventSource::get(format!("{daemon_address}sse/logs"));
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
                        )
                        .await;
                        is_connected = true;
                    }
                }
                Ok(Event::Message(msg)) => {
                    if !is_connected {
                        let _ = send_notification(
                            "Daemon Connection Restored",
                            "Connection with the daemon has been restored.",
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
                        )
                        .await;
                    }
                }
                Err(_) => {
                    if is_connected {
                        let _ = send_notification(
                            "Daemon Disconnected",
                            "Connection with the daemon has been lost",
                        )
                        .await;
                        is_connected = false;
                    }
                }
            }
        }
    });
    Ok(())
}

#[derive(Default)]
pub struct DaemonState {
    has_errors: Mutex<bool>,
}
