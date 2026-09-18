// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Async runtime facade.
//!
//! Centralizes the runtime entry, task spawning, timers, and shutdown-signal handling behind one
//! module so the underlying runtime can be swapped without touching call sites. The backend is
//! compio (`runtime`, `test_runtime`, `spawn`, `sleep`, `sleep_until`, `interval`, `timeout`,
//! `shutdown_signal`, `log_active_backend`).
//!
//! Only main-thread code goes through this facade; the sidecar thread uses its Tokio runtime
//! directly. Channels (`tokio::sync`) and `CancellationToken` are reactor-agnostic and are used
//! directly, not wrapped here.

mod compio_rt;
pub use compio_rt::*;

/// Runtime-agnostic blocking-join failure (the spawned closure panicked or the join was cancelled).
/// The backend's native join error is kept as a string so its concrete type never leaks past the
/// facade. Returned by `spawn_blocking`.
#[derive(Debug)]
pub struct JoinError(String);

impl JoinError {
    pub fn new(message: String) -> Self {
        Self(message)
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "blocking task failed to join: {}", self.0)
    }
}

impl std::error::Error for JoinError {}

/// Cooperatively yield to the runtime once. Runtime-agnostic (compio has no `yield_now`). Test-only:
/// used by tests that need a concurrently-spawned task to make progress before they inspect state.
#[cfg(test)]
pub async fn yield_now() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await;
}
