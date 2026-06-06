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

//! Tokio backend for the runtime facade (the default). Selected when the `compio-rt` feature is
//! off. See `super` for the facade contract.

use std::future::Future;
use std::time::{Duration, Instant};
use tokio::runtime::Builder;
use tokio::signal;
use tokio::signal::unix::SignalKind;
use tokio::task::LocalSet;

pub use tokio::time::{interval, sleep, timeout};

/// Initialize and run the main single-threaded runtime to completion.
pub fn runtime<F: Future>(future: F) -> F::Output {
    let rt_builder = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        // By default, this pool can grow large and fluctuate over time.
        // A large thread pool is less efficient for us, but we want more than a single
        // thread in case a device has severe latency:
        .max_blocking_threads(4)
        .thread_keep_alive(Duration::from_secs(60))
        .thread_name("cc-wrk")
        .event_interval(200)
        .global_queue_interval(200)
        .build();
    // requires tokio unstable: (but would make all our spawns !Send by default)
    // .build_local(&Default::default());
    // ^ until then, this allows us to use spawn_local:
    let rt = rt_builder.unwrap();
    let output = rt.block_on(LocalSet::new().run_until(future));
    // should a background thread still be running, this will force the runtime process to stop:
    rt.shutdown_timeout(Duration::from_secs(3));
    output
}

/// Initialize and run a runtime for tests.
///
/// Important: cargo tests need to be run single threaded, i.e. `-- --test-threads=1`, as cargo
/// runs tests in parallel by default. We use the `serial_test` crate to explicitly ensure this.
#[allow(dead_code)]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    let rt = Builder::new_current_thread().enable_all().build();
    rt.unwrap().block_on(LocalSet::new().run_until(future))
}

/// Spawn a `!Send` future on the current-thread runtime, detached (fire-and-forget).
pub fn spawn<F>(future: F)
where
    F: Future + 'static,
{
    // Dropping the JoinHandle detaches the task on Tokio (it keeps running). The compio backend
    // instead calls `.detach()`, since dropping a compio handle cancels the task.
    tokio::task::spawn_local(future);
}

/// Spawn a `!Send` future eagerly and return an awaitable handle. Test-only: tests need a task to
/// run concurrently and then await its result. The handle maps the backend join error to
/// `super::JoinError`; dropping it before awaiting detaches (Tokio) the task.
#[cfg(test)]
pub fn spawn_task<F>(future: F) -> SpawnTask<F::Output>
where
    F: Future + 'static,
{
    SpawnTask(tokio::task::spawn_local(future))
}

/// Awaitable handle returned by `spawn_task`. See `spawn_task`.
#[cfg(test)]
pub struct SpawnTask<T>(tokio::task::JoinHandle<T>);

#[cfg(test)]
impl<T> Future for SpawnTask<T> {
    type Output = Result<T, super::JoinError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0)
            .poll(cx)
            .map_err(|err| super::JoinError::new(err.to_string()))
    }
}

/// Run a blocking closure on the runtime's blocking-thread pool and await its result.
///
/// The returned future is lazy: the closure is spawned when first polled. If the future is dropped
/// (e.g. a `timeout` fires) the blocking thread is detached and runs to completion in the background.
pub async fn spawn_blocking<F, T>(f: F) -> Result<T, super::JoinError>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|err| super::JoinError::new(err.to_string()))
}

/// Sleep until the given deadline. Takes a `std::time::Instant` so call sites stay runtime-neutral.
pub async fn sleep_until(deadline: Instant) {
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
}

/// Complete when any process-termination signal is received (`SIGINT`/Ctrl-C, `SIGTERM`,
/// `SIGQUIT`). The caller decides what to do on completion (typically cancel the run token).
pub async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };
    let sigterm = async {
        signal::unix::signal(SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    let sigint = async {
        signal::unix::signal(SignalKind::interrupt())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    let sigquit = async {
        signal::unix::signal(SignalKind::quit())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };
    tokio::select! {
        () = ctrl_c => {},
        () = sigterm => {},
        () = sigint => {},
        () = sigquit => {},
    }
}
