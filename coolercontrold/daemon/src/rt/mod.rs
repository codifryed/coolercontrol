// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Async runtime facade.
//!
//! Centralizes the runtime entry, task spawning, timers, and shutdown-signal handling behind one
//! module so the underlying runtime can be swapped without touching call sites. The backend is a
//! tokio current-thread runtime with a `LocalSet` (`runtime`, `test_runtime`, `spawn`, `sleep`,
//! `sleep_until`, `interval`, `timeout`, `shutdown_signal`, `log_ignored_driver_override`).
//!
//! Only main-thread code goes through this facade; the sidecar thread uses its own Tokio runtime
//! directly. Channels (`tokio::sync`) and `CancellationToken` are reactor-agnostic and are used
//! directly, not wrapped here.

mod tokio_rt;
pub use tokio_rt::*;

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

/// Lets every other ready task run before the caller continues. This wakes the current task and
/// returns `Pending` once, which puts it at the back of the run queue without allocating. Unlike
/// tokio's own, it needs no runtime, so `poll_to_end` can drive it in allocation counts.
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

/// Polls `future` to completion with a waker that does nothing, and counts the polls. For futures
/// whose only pending points wake themselves, like `yield_now`, so a test can count their
/// allocations without a runtime, whose own setup allocates.
#[cfg(test)]
pub fn poll_to_end<F: std::future::Future>(future: F) -> (F::Output, u32) {
    let mut future = std::pin::pin!(future);
    let mut context = std::task::Context::from_waker(std::task::Waker::noop());
    for polls in 1..=1_000 {
        if let std::task::Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return (output, polls);
        }
    }
    panic!("the future never finished");
}
