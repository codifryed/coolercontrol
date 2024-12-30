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

use crate::api::status::StatusResponse;
use crate::api::AppState;
use aide::NoApi;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive};
use axum::response::Sse;
use std::convert::Infallible;
use std::time::Duration;
use tokio_stream::wrappers::BroadcastStream;
use zbus::export::futures_core::Stream;
use zbus::export::futures_util::StreamExt;

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
    State(AppState { status_handle, .. }): State<AppState>,
) -> NoApi<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    let cancel_token = status_handle.cancel_token();
    let status_stream = BroadcastStream::new(status_handle.broadcaster().subscribe())
        .take_until(async move { cancel_token.cancelled().await })
        .map(|status| {
            Ok(Event::default()
                .event("status")
                .json_data(StatusResponse {
                    devices: status.unwrap_or_default(),
                })
                .unwrap())
        });
    NoApi(Sse::new(status_stream))
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
                .unwrap())
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
                .unwrap())
        });
    NoApi(Sse::new(alert_stream).keep_alive(
        KeepAlive::new().interval(Duration::from_secs(DEFAULT_KEEP_ALIVE_INTERVAL_SECONDS)),
    ))
}
