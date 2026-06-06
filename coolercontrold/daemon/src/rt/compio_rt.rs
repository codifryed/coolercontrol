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

//! compio backend for the runtime facade. Selected by the `compio-rt` feature. See `super` for the
//! facade contract; this mirrors the tokio backend's surface on compio.

use std::future::{poll_fn, Future};
use std::pin::pin;
use std::task::Poll;
use std::time::Instant;

use compio::runtime::Runtime;

pub use compio::time::{interval, sleep, timeout};

/// Initialize and run the main single-threaded runtime to completion.
pub fn runtime<F: Future>(future: F) -> F::Output {
    // compio is inherently single-threaded and `!Send`, so there is no `LocalSet` to set up nor a
    // multi-thread builder to constrain. The fusion driver (io-uring + polling) is selected at
    // compile time and auto-falls-back to polling when io_uring is unavailable.
    Runtime::new()
        .expect("compio runtime builds")
        .block_on(future)
}

/// Initialize and run a runtime for tests.
///
/// Important: cargo tests need to be run single threaded, i.e. `-- --test-threads=1`, as cargo
/// runs tests in parallel by default. We use the `serial_test` crate to explicitly ensure this.
#[allow(dead_code)]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    Runtime::new()
        .expect("compio test runtime builds")
        .block_on(future)
}

/// Spawn a `!Send` future on the current-thread runtime, detached (fire-and-forget).
pub fn spawn<F>(future: F)
where
    F: Future + 'static,
{
    // Dropping a compio `JoinHandle` CANCELS the task (opposite of Tokio), so detach explicitly to
    // keep the fire-and-forget semantics the facade promises.
    compio::runtime::spawn(future).detach();
}

/// Run a blocking closure on the runtime's blocking-thread pool and await its result.
///
/// The returned future is lazy: the closure is spawned when first polled. If the future is dropped
/// (e.g. a `timeout` fires) the blocking thread runs to completion in the background (compio does
/// not cancel blocking tasks).
pub async fn spawn_blocking<F, T>(f: F) -> Result<T, super::JoinError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    compio::runtime::spawn_blocking(f)
        .await
        .map_err(|err| super::JoinError::new(err.to_string()))
}

/// Sleep until the given deadline. Takes a `std::time::Instant` so call sites stay runtime-neutral.
pub async fn sleep_until(deadline: Instant) {
    compio::time::sleep_until(deadline).await;
}

/// Complete when any process-termination signal is received (`SIGINT`/Ctrl-C, `SIGTERM`,
/// `SIGQUIT`). The caller decides what to do on completion (typically cancel the run token).
pub async fn shutdown_signal() {
    // `compio::signal::ctrl_c` covers SIGINT; the others are awaited by number. We race them by
    // hand (`futures_util::select!` needs the `async-await` feature we do not enable). Each future
    // is one-shot, so once any is ready we return without re-polling a completed one.
    let mut ctrl_c = pin!(compio::signal::ctrl_c());
    let mut sigterm = pin!(compio::signal::unix::signal(nix::libc::SIGTERM));
    let mut sigquit = pin!(compio::signal::unix::signal(nix::libc::SIGQUIT));
    poll_fn(|cx| {
        if let Poll::Ready(res) = ctrl_c.as_mut().poll(cx) {
            res.expect("failed to install Ctrl+C handler");
            return Poll::Ready(());
        }
        if let Poll::Ready(res) = sigterm.as_mut().poll(cx) {
            res.expect("failed to install SIGTERM handler");
            return Poll::Ready(());
        }
        if let Poll::Ready(res) = sigquit.as_mut().poll(cx) {
            res.expect("failed to install SIGQUIT handler");
            return Poll::Ready(());
        }
        Poll::Pending
    })
    .await;
}
