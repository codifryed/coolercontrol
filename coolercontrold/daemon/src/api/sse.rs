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
    let cancel_token = log_buf_handle.cancel_token();
    let log_stream = BroadcastStream::new(log_buf_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|log| Ok(Event::default().event("log").data(log.unwrap_or_default())));
    NoApi(Sse::new(log_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
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
            match event {
                Ok(event) => health_event_to_sse(event),
                // This consumer missed transitions; resync it with the full
                // current snapshot instead of leaving it permanently stale.
                Err(BroadcastStreamRecvError::Lagged(_)) => {
                    health_snapshot_to_sse(health_handle.get_all().await)
                }
            }
        }
    });
    let combined = futures_util::stream::select(status_stream, health_stream)
        .take_until(async move { cancel_token.cancelled().await });
    NoApi(Sse::new(combined))
}

/// Maps one tick's device-health transition batch to its named SSE event
/// (`missing`, `stale-source`, or `failsafe`).
fn health_event_to_sse(event: HealthEvent) -> Result<Event, Infallible> {
    match event {
        HealthEvent::Missing(deltas) => Ok(Event::default()
            .event("missing")
            .json_data(deltas)
            .expect("derived DTO serialization cannot fail")),
        HealthEvent::StaleSource(deltas) => Ok(Event::default()
            .event("stale-source")
            .json_data(deltas)
            .expect("derived DTO serialization cannot fail")),
        HealthEvent::Failsafe(deltas) => Ok(Event::default()
            .event("failsafe")
            .json_data(deltas)
            .expect("derived DTO serialization cannot fail")),
    }
}

/// Full-state `health` event sent to a consumer that lagged the broadcast
/// buffer, so it converges on the current state.
fn health_snapshot_to_sse(snapshot: DeviceHealthDto) -> Result<Event, Infallible> {
    Ok(Event::default()
        .event("health")
        .json_data(snapshot)
        .expect("derived DTO serialization cannot fail"))
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
