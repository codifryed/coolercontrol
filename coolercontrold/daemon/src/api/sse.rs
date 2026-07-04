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

use crate::api::status::StatusResponse;
use crate::api::AppState;
use crate::device_health::{DeviceHealthDto, HealthEvent};
use crate::logger::LogBufHandle;
use aide::NoApi;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive};
use axum::response::Sse;
use futures_util::StreamExt;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tokio_stream::wrappers::BroadcastStream;
use zbus::export::futures_core::Stream;

const DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS: u64 = 30;

pub async fn logs(
    State(AppState { log_buf_handle, .. }): State<AppState>,
) -> NoApi<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let log_stream =
        log_lines(&log_buf_handle).map(|log| Ok(Event::default().event("log").data(log)));
    NoApi(Sse::new(log_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
}

/// Live log lines for one subscriber. Bursts arrive pre-coalesced (multi-line) from the
/// log-buffer actor. A lagged subscriber skips missed lines instead of receiving empty
/// events; recent history remains available via GET /logs.
fn log_lines(log_buf_handle: &LogBufHandle) -> impl Stream<Item = String> + use<> {
    let cancel_token = log_buf_handle.cancel_token();
    BroadcastStream::new(log_buf_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .filter_map(|result| async { result.ok() })
}

pub async fn status(
    State(AppState {
        status_handle,
        device_health_handle,
        ..
    }): State<AppState>,
) -> NoApi<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let cancel_token = status_handle.cancel_token();
    let status_stream =
        BroadcastStream::new(status_handle.broadcaster().subscribe()).map(|status| {
            Ok(Event::default()
                .event("status")
                .json_data(StatusResponse {
                    devices: status.unwrap_or_default(),
                })
                .expect("derived DTO serialization cannot fail"))
        });
    // Device-health transitions ride the status connection as named events so we
    // do not open another SSE connection (browsers cap at 6 per origin).
    let health_subscription = device_health_handle.broadcaster().subscribe();
    let health_stream = BroadcastStream::new(health_subscription).then(move |event| {
        let health_handle = device_health_handle.clone();
        async move {
            let event = match event {
                Ok(event) => health_event_to_sse(event),
                // This consumer missed transitions; resync it with the full
                // current snapshot instead of leaving it permanently stale.
                Err(BroadcastStreamRecvError::Lagged(_)) => {
                    health_snapshot_to_sse(health_handle.get_all().await)
                }
            };
            Ok(event)
        }
    });
    let combined = futures_util::stream::select(status_stream, health_stream)
        .take_until(async move { cancel_token.cancelled().await });
    NoApi(Sse::new(combined))
}

/// Maps one tick's device-health transition batch to its named SSE event
/// (`missing`, `stale-source`, or `failsafe`).
fn health_event_to_sse(event: HealthEvent) -> Event {
    match event {
        HealthEvent::Missing(deltas) => Event::default()
            .event("missing")
            .json_data(deltas)
            .expect("derived DTO serialization cannot fail"),
        HealthEvent::StaleSource(deltas) => Event::default()
            .event("stale-source")
            .json_data(deltas)
            .expect("derived DTO serialization cannot fail"),
        HealthEvent::Failsafe(deltas) => Event::default()
            .event("failsafe")
            .json_data(deltas)
            .expect("derived DTO serialization cannot fail"),
    }
}

/// Full-state `health` event sent to a consumer that lagged the broadcast
/// buffer, so it converges on the current state.
fn health_snapshot_to_sse(snapshot: DeviceHealthDto) -> Event {
    Event::default()
        .event("health")
        .json_data(snapshot)
        .expect("derived DTO serialization cannot fail")
}

pub async fn modes(
    State(AppState { mode_handle, .. }): State<AppState>,
) -> NoApi<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let cancel_token = mode_handle.cancel_token();
    let modes_stream = BroadcastStream::new(mode_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|mode_activated| {
            Ok(Event::default()
                .event("mode")
                .json_data(mode_activated.unwrap_or_default())
                .expect("derived DTO serialization cannot fail"))
        });
    NoApi(Sse::new(modes_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
}

pub async fn alerts(
    State(AppState { alert_handle, .. }): State<AppState>,
) -> NoApi<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let cancel_token = alert_handle.cancel_token();
    let alert_stream = BroadcastStream::new(alert_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|alert_state| {
            Ok(Event::default()
                .event("alert")
                .json_data(alert_state.unwrap_or_default())
                .expect("derived DTO serialization cannot fail"))
        });
    NoApi(Sse::new(alert_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
}

pub async fn notifications(
    State(AppState {
        notification_handle,
        ..
    }): State<AppState>,
) -> NoApi<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let cancel_token = notification_handle.cancel_token();
    let notification_stream = BroadcastStream::new(notification_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .filter_map(|result| async { result.ok() })
        .map(|notification| {
            Ok(Event::default()
                .event("notification")
                .json_data(notification)
                .expect("derived DTO serialization cannot fail"))
        });
    NoApi(Sse::new(notification_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_util::sync::CancellationToken;

    // Goal: a subscriber that lags the broadcast channel must silently skip missed lines,
    // never yielding empty items (the old behavior mapped Lagged to empty SSE events).
    // Methodology: subscribe first, overflow the channel far past capacity, then drain until
    // the newest line arrives and check everything yielded is a real line with a gap skipped.
    #[test]
    fn lagged_subscriber_skips_missed_lines() {
        crate::rt::test_runtime(async {
            let handle = LogBufHandle::new(CancellationToken::new());
            let mut lines = std::pin::pin!(log_lines(&handle));
            let sent_count = 40;
            for i in 0..sent_count {
                handle
                    .broadcaster()
                    .send(format!("line {i}\n"))
                    .expect("subscriber exists");
            }
            let last_line = format!("line {}\n", sent_count - 1);
            let mut received = Vec::new();
            for _ in 0..sent_count {
                let Some(line) = lines.next().await else {
                    break;
                };
                let is_last = line == last_line;
                received.push(line);
                if is_last {
                    break;
                }
            }
            assert!(received.len() < sent_count, "lag must have skipped lines");
            assert!(received.iter().all(|line| line.starts_with("line ")));
            assert_eq!(received.last(), Some(&last_line));
        });
    }
}
