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
use log::info;
use nix::sys::signal::{SigSet, Signal};

pub use compio::time::{interval, sleep, timeout};

/// Initialize and run the main single-threaded runtime to completion.
pub fn runtime<F: Future>(future: F) -> F::Output {
    // compio is inherently single-threaded and `!Send`, so there is no `LocalSet` to set up nor a
    // multi-thread builder to constrain. The fusion driver (io-uring + polling) is selected at
    // compile time and auto-falls-back to polling when io_uring is unavailable.
    //
    // Mask termination signals before the runtime and the tokio sidecar spawn any threads, so they
    // all inherit the mask and compio's signalfd listener is the sole consumer. See
    // `block_termination_signals` for why this is required on the compio backend.
    block_termination_signals();
    Runtime::new()
        .expect("compio runtime builds")
        .block_on(future)
}

/// Log which fusion-driver backend compio selected. The fusion driver uses `io_uring` when the
/// kernel supports every opcode we need, otherwise it falls back to polling (epoll on Linux, e.g.
/// when `kernel.io_uring_disabled` blocks it). Call from within the runtime, after logging is set
/// up, so `with_current` sees the active runtime.
pub fn log_active_backend() {
    let driver_type = Runtime::with_current(Runtime::driver_type);
    if driver_type.is_iouring() {
        info!("Using the io_uring backend");
    } else {
        info!("Using the polling (epoll) fallback backend");
    }
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

/// Spawn a `!Send` future eagerly and return an awaitable handle. Test-only: tests need a task to
/// run concurrently and then await its result. The handle maps the backend join error to
/// `super::JoinError`; dropping it before awaiting cancels (compio) the task.
#[cfg(test)]
pub fn spawn_task<F>(future: F) -> SpawnTask<F::Output>
where
    F: Future + 'static,
{
    SpawnTask(compio::runtime::spawn(future))
}

/// Awaitable handle returned by `spawn_task`. See `spawn_task`.
#[cfg(test)]
pub struct SpawnTask<T>(compio::runtime::JoinHandle<T>);

#[cfg(test)]
impl<T> Future for SpawnTask<T> {
    type Output = Result<T, super::JoinError>;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        std::pin::Pin::new(&mut self.0)
            .poll(cx)
            .map_err(|payload| super::JoinError::new(panic_message(&payload)))
    }
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
        .map_err(|payload| super::JoinError::new(panic_message(&payload)))
}

/// Extract a human-readable message from a panic payload. compio's join/blocking error is the raw
/// `Box<dyn Any + Send>` from `catch_unwind` (no `Display`), so recover the common `&str`/`String`
/// cases and fall back to a generic note.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_owned()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "task panicked".to_owned()
    }
}

/// Sleep until the given deadline. Takes a `std::time::Instant` so call sites stay runtime-neutral.
pub async fn sleep_until(deadline: Instant) {
    compio::time::sleep_until(deadline).await;
}

/// Block `SIGINT`, `SIGTERM`, and `SIGQUIT` on the current thread.
///
/// compio's Linux signal listener is signalfd-based and masks the signal only on the thread that
/// creates the fd (`compio_signal::unix::signal` documents "sets the signal mask of the current
/// thread"). signalfd only receives a signal that is masked on the thread it is delivered to, but a
/// process-directed signal (terminal Ctrl-C, `kill`, systemd stop) is delivered to ANY thread that
/// still has it unmasked, where the default action terminates the process before graceful shutdown
/// can run. Calling this on the main thread before the compio runtime and the tokio sidecar spawn
/// their threads makes every thread inherit the mask, leaving the signalfd as the only consumer.
/// The tokio backend installs a real `sigaction` handler and needs none of this.
///
/// Spawned child processes inherit this mask too. That is fine here: liqctld is stopped via its
/// `/quit` request and then SIGKILL (unblockable), and shell helpers exit on their own or are
/// SIGKILLed on drop, so no child is ever stopped by a signal it could have masked.
fn block_termination_signals() {
    let mut set = SigSet::empty();
    set.add(Signal::SIGINT);
    set.add(Signal::SIGTERM);
    set.add(Signal::SIGQUIT);
    // `SIG_BLOCK` unions onto the current mask, so unrelated masked signals are preserved. The only
    // documented failure is an invalid signal/how, impossible for these constants; on the off
    // chance it fails we report it and continue rather than refuse to boot.
    if let Err(err) = set.thread_block() {
        eprintln!("could not mask termination signals for compio signalfd handling: {err}");
    }
}

/// Complete when any process-termination signal is received (`SIGINT`/Ctrl-C, `SIGTERM`,
/// `SIGQUIT`). The caller decides what to do on completion (typically cancel the run token).
///
/// Relies on the startup mask from `block_termination_signals`; without it compio's signalfd would
/// never see a process-directed signal.
pub async fn shutdown_signal() {
    // `compio::signal::ctrl_c` covers SIGINT; the others are awaited by number. We race them by
    // hand (`futures_util::select!` needs the `async-await` feature we do not enable). Each future
    // is one-shot, so once any is ready we return without re-polling a completed one. Scoped so the
    // signalfds drop (and with them the per-thread unmask their Drop performs) before we re-block.
    {
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
    // Dropping the signalfds above unmasked these signals on THIS thread (compio's per-thread
    // bookkeeping); the other threads stay masked from startup. Re-mask here so a second Ctrl-C
    // during graceful shutdown stays pending instead of hitting the default action, which would
    // force-kill the daemon. Matches the tokio backend, whose handler remains installed.
    block_termination_signals();
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::signal::{pthread_sigmask, SigmaskHow};
    use std::ops::Not;

    /// Goal: `block_termination_signals` must leave SIGINT, SIGTERM, and SIGQUIT masked on the
    /// calling thread. That mask is what lets compio's signalfd be the sole consumer of these
    /// signals across the daemon's threads; without it a process-directed signal hits the default
    /// action and force-kills the process before graceful shutdown (the bug this fixes). Method:
    /// snapshot the current mask, apply the block, read the mask back and assert all three are
    /// members, then restore the snapshot so the shared test-runner thread is left as found.
    #[test]
    fn block_termination_signals_masks_int_term_quit() {
        let mut original = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut original))
            .expect("read current signal mask");

        block_termination_signals();

        let mut now_masked = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut now_masked))
            .expect("read signal mask after block");
        assert!(now_masked.contains(Signal::SIGINT));
        assert!(now_masked.contains(Signal::SIGTERM));
        assert!(now_masked.contains(Signal::SIGQUIT));

        pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&original), None)
            .expect("restore original signal mask");
    }

    /// Goal: `log_active_backend` must be callable from within the compio runtime, where its
    /// `with_current` query reaches the live driver. The call site relies on this: `with_current`
    /// panics outside a runtime, so the function must only ever run inside `block_on`. Method: build
    /// a runtime, and inside `block_on` confirm the selected driver is one of the two Linux fusion
    /// options (`io_uring` or its polling/epoll fallback, never IOCP) and that the log call returns
    /// without panicking.
    #[test]
    fn log_active_backend_runs_within_runtime() {
        Runtime::new()
            .expect("compio runtime builds")
            .block_on(async {
                let driver_type = Runtime::with_current(Runtime::driver_type);
                assert!(driver_type.is_iouring() || driver_type.is_polling());
                assert!(driver_type.is_iocp().not());
                log_active_backend();
            });
    }
}
