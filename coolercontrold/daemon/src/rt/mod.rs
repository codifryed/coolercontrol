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

//! Async runtime facade.
//!
//! Centralizes the runtime entry, task spawning, timers, and shutdown-signal handling behind one
//! module so the underlying runtime can be swapped without touching call sites. The backend is
//! chosen at compile time: Tokio by default, compio under the `compio-rt` feature. Both backends
//! expose the same surface (`runtime`, `test_runtime`, `spawn`, `sleep`, `sleep_until`, `interval`,
//! `timeout`, `shutdown_signal`).
//!
//! Only main-thread code goes through this facade; the sidecar thread uses its Tokio runtime
//! directly. Channels (`tokio::sync`) and `CancellationToken` are reactor-agnostic and are used
//! directly, not wrapped here.

#[cfg(not(feature = "compio-rt"))]
mod tokio_rt;
#[cfg(not(feature = "compio-rt"))]
pub use tokio_rt::*;

#[cfg(feature = "compio-rt")]
mod compio_rt;
#[cfg(feature = "compio-rt")]
pub use compio_rt::*;

/// Runtime-agnostic blocking-join failure (the spawned closure panicked or the join was cancelled).
/// The backend's native join error is kept as a string so its concrete type never leaks past the
/// facade. Returned by `spawn_blocking`.
#[derive(Debug)]
pub struct JoinError(String);

impl JoinError {
    pub(crate) fn new(message: String) -> Self {
        Self(message)
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blocking task failed to join: {}", self.0)
    }
}

impl std::error::Error for JoinError {}
