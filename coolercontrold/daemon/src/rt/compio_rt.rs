// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! compio backend for the runtime facade. Selected by the `compio-rt` feature. See `super` for the
//! facade contract; this mirrors the tokio backend's surface on compio.

use std::future::{poll_fn, Future};
use std::pin::pin;
use std::task::Poll;
use std::time::Instant;

use compio::driver::{DriverType, ProactorBuilder};
use compio::runtime::Runtime;
use log::info;
use nix::sys::signal::{SigSet, Signal};

use crate::ENV_RUNTIME_DRIVER;

pub use compio::time::{interval, sleep, timeout};

/// Driver used when `ENV_RUNTIME_DRIVER` is unset. `None` lets compio probe the kernel and pick
/// `io_uring` when every opcode we need is supported, falling back to polling otherwise.
///
/// Left on `io_uring` despite issue 606, where its `io_uring_enter(GETEVENTS)` wait is accounted
/// as iowait (kernel 6.5+) and so pins an idle core at ~100% on per-core monitors. That is a
/// reporting artifact, while polling costs real wakeups: measured over 90s windows, polling took
/// daemon context switches from 30/s to 159/s and threads from 3 to 13, because on Linux it sends
/// every file op to the `AsyncifyPool` (the `aio` path is BSD-only). Set `CC_RUNTIME_DRIVER=poll`
/// to take that trade. The real fix is `IORING_ENTER_NO_IOWAIT`, which lands in compio-driver
/// 0.12.5; that is reachable only through compio 0.19, whose `cfg_select!` usage requires rustc
/// 1.95. EL10 `AppStream` is on 1.92, so the MSRV bump waits on it. Even after that, polling stays
/// the only answer for kernels 6.5 to 6.14, which have the accounting but not the opt-out.
const DEFAULT_DRIVER: Option<DriverType> = None;

/// Initialize and run the main single-threaded runtime to completion.
pub fn runtime<F: Future>(future: F) -> F::Output {
    // compio is inherently single-threaded and `!Send`, so there is no `LocalSet` to set up nor a
    // multi-thread builder to constrain. The fusion driver (io-uring + polling) is selected at
    // compile time; which half runs is `DEFAULT_DRIVER`, overridable with `ENV_RUNTIME_DRIVER`.
    //
    // Mask termination signals before the runtime and the tokio sidecar spawn any threads, so they
    // all inherit the mask and compio's signalfd listener is the sole consumer. See
    // `block_termination_signals` for why this is required on the compio backend.
    block_termination_signals();
    // A failed build means the OS denied the io_uring/epoll reactor at startup; the daemon cannot
    // run without it, so failing fast is correct.
    build_runtime(driver_override())
        .expect("compio runtime builds")
        .block_on(future)
}

/// Build the main runtime, forcing `driver` when one is given and letting compio probe otherwise.
fn build_runtime(driver: Option<DriverType>) -> std::io::Result<Runtime> {
    let Some(driver_type) = driver else {
        return Runtime::new();
    };
    let mut proactor_builder = ProactorBuilder::new();
    proactor_builder.driver_type(driver_type);
    let runtime = Runtime::builder().with_proactor(proactor_builder).build()?;
    // Read the driver back. Forcing a type clears compio's fallback, so the built runtime must be
    // running exactly what was asked for; anything else means the request was silently dropped.
    debug_assert_eq!(runtime.driver_type(), driver_type);
    Ok(runtime)
}

/// Read `ENV_RUNTIME_DRIVER` and resolve it to a driver to force, falling back to
/// `DEFAULT_DRIVER`.
fn driver_override() -> Option<DriverType> {
    let Ok(raw) = std::env::var(ENV_RUNTIME_DRIVER) else {
        return DEFAULT_DRIVER;
    };
    let Some(choice) = parse_driver_override(&raw) else {
        // Logging is not set up this early, so stderr is the only channel available. An unusable
        // value must not stop the daemon booting, so fall back to the default.
        eprintln!(
            "{ENV_RUNTIME_DRIVER}: unrecognized value {raw:?}, using the default reactor backend"
        );
        return DEFAULT_DRIVER;
    };
    choice.forced_driver()
}

/// What a recognized `ENV_RUNTIME_DRIVER` value asks the runtime to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriverChoice {
    /// Force the polling (epoll) driver.
    Poll,
    /// Let compio probe the kernel and keep its own fallback.
    Probe,
}

impl DriverChoice {
    /// The driver to force, or `None` to leave compio's probe in charge.
    fn forced_driver(self) -> Option<DriverType> {
        match self {
            Self::Poll => Some(DriverType::Poll),
            Self::Probe => None,
        }
    }
}

/// Resolve an `ENV_RUNTIME_DRIVER` value, or `None` when the value is not recognized.
///
/// Note that `io_uring` deliberately resolves to `Probe` rather than forcing `DriverType::IoUring`.
/// Setting that explicitly clears compio's `fallback` flag, turning an unavailable `io_uring` into
/// a hard startup failure instead of a graceful degrade to polling. Probing selects the same driver
/// without that cliff, so only polling is ever forced.
fn parse_driver_override(raw: &str) -> Option<DriverChoice> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "poll" | "polling" | "epoll" => Some(DriverChoice::Poll),
        "io_uring" | "io-uring" | "iouring" | "uring" => Some(DriverChoice::Probe),
        _ => None,
    }
}

/// Log which fusion-driver backend compio selected. `io_uring` is the default and is used when the
/// kernel supports every opcode we need; polling (epoll on Linux) is reached either as compio's
/// fallback or by opting in through `ENV_RUNTIME_DRIVER`. Call from within the runtime, after
/// logging is set up, so `with_current` sees the active runtime.
pub fn log_active_backend() {
    let driver_type = Runtime::with_current(Runtime::driver_type);
    if driver_type.is_iouring() {
        info!("Using the io_uring backend");
    } else {
        info!("Using the polling (epoll) backend");
    }
}

/// Initialize and run a runtime for tests.
///
/// Important: cargo tests need to be run single threaded, i.e. `-- --test-threads=1`, as cargo
/// runs tests in parallel by default. We use the `serial_test` crate to explicitly ensure this.
#[allow(dead_code)]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    // Follows the compiled-in default, so a flip of `DEFAULT_DRIVER` moves the tests with it.
    // `ENV_RUNTIME_DRIVER` is deliberately not consulted: test results must not depend on the
    // ambient environment.
    build_runtime(DEFAULT_DRIVER)
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
/// `SIGKILLed` on drop, so no child is ever stopped by a signal it could have masked.
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
        // A signalfd registration error (fd/memory exhaustion) leaves the daemon unable to observe a
        // shutdown request; panicking is better than silently never shutting down.
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
    use serial_test::serial;
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

    /// Goal: `parse_driver_override` must accept the spellings a user is likely to reach for, in
    /// any case and with surrounding whitespace, and must map every `io_uring` spelling to "do not
    /// force" so compio keeps its graceful fallback. Method: table-drive the accepted values and
    /// assert the resolved driver for each.
    #[test]
    fn parse_driver_override_accepts_known_spellings() {
        for raw in ["poll", "polling", "epoll", "POLL", "  Poll  ", "EPoll"] {
            assert_eq!(
                parse_driver_override(raw),
                Some(DriverChoice::Poll),
                "{raw:?} should force the polling driver"
            );
        }
        for raw in [
            "io_uring",
            "io-uring",
            "iouring",
            "uring",
            "IO_URing",
            " io-uring ",
        ] {
            assert_eq!(
                parse_driver_override(raw),
                Some(DriverChoice::Probe),
                "{raw:?} should probe rather than force io_uring"
            );
        }
    }

    /// Goal: an unrecognized value must be reported as unrecognized rather than silently treated as
    /// a driver choice, so `driver_override` can warn and fall back instead of forcing the wrong
    /// reactor. Method: assert the negative space, including the empty string and near-misses.
    #[test]
    fn parse_driver_override_rejects_unknown_values() {
        for raw in ["", "  ", "iocp", "tokio", "io uring", "polled", "1", "true"] {
            assert_eq!(
                parse_driver_override(raw),
                None,
                "{raw:?} should not resolve to a driver"
            );
        }
    }

    /// Goal: forcing `DriverType::Poll` must actually produce a polling runtime, since that is the
    /// whole mechanism behind the `CC_RUNTIME_DRIVER=poll` escape hatch for the io_uring iowait
    /// accounting. Method: build a runtime through the same helper `runtime` uses and assert from
    /// inside `block_on` that the live driver is polling and specifically not io_uring, which is
    /// what this machine would otherwise have selected.
    #[test]
    fn build_runtime_forces_the_polling_driver() {
        build_runtime(Some(DriverType::Poll))
            .expect("compio runtime builds with the polling driver forced")
            .block_on(async {
                let driver_type = Runtime::with_current(Runtime::driver_type);
                assert!(driver_type.is_polling());
                assert!(driver_type.is_iouring().not());
            });
    }

    /// Goal: `driver_override` must read the variable the documentation promises, and must resolve
    /// every branch the way issue 606 settled: unset and unrecognized both fall back to the
    /// default, `poll` forces polling, and an `io_uring` spelling opts back in without pinning the
    /// driver. `parse_driver_override`'s own tests cannot catch a typo in the variable name, which
    /// is the failure that would silently make the escape hatch unreachable. Method: drive the
    /// real process environment, which is global, so the test is serial and restores the prior
    /// value on the way out.
    #[test]
    #[serial]
    fn driver_override_reads_the_documented_variable() {
        let original = std::env::var(ENV_RUNTIME_DRIVER).ok();
        assert_eq!(ENV_RUNTIME_DRIVER, "CC_RUNTIME_DRIVER");

        // SAFETY: `set_var`/`remove_var` are unsound only alongside concurrent environment
        // access. `#[serial]` gives this test the process to itself, and the value is restored
        // before it yields.
        unsafe {
            std::env::remove_var(ENV_RUNTIME_DRIVER);
            assert_eq!(driver_override(), DEFAULT_DRIVER);

            std::env::set_var(ENV_RUNTIME_DRIVER, "poll");
            assert_eq!(driver_override(), Some(DriverType::Poll));

            std::env::set_var(ENV_RUNTIME_DRIVER, "io_uring");
            assert_eq!(driver_override(), None);

            // Negative space: an unusable value must boot on the default, never force a driver.
            std::env::set_var(ENV_RUNTIME_DRIVER, "nonsense");
            assert_eq!(driver_override(), DEFAULT_DRIVER);

            match original {
                Some(value) => std::env::set_var(ENV_RUNTIME_DRIVER, value),
                None => std::env::remove_var(ENV_RUNTIME_DRIVER),
            }
        }
    }

    /// Goal: the shipped default must leave compio's probe in charge rather than pinning a
    /// driver, so an unavailable `io_uring` still degrades to polling instead of failing startup.
    /// The explicit `is_none` assertion is the tripwire: issue 606 is a standing temptation to
    /// force polling here, and that trade costs real wakeups, so a flip must be deliberate.
    /// Method: assert the constant, then build through the same helper `runtime` uses and confirm
    /// the live driver is one of the two Linux fusion options.
    #[test]
    fn default_driver_leaves_compio_probing() {
        assert!(DEFAULT_DRIVER.is_none());
        build_runtime(DEFAULT_DRIVER)
            .expect("compio runtime builds on the default driver")
            .block_on(async {
                let driver_type = Runtime::with_current(Runtime::driver_type);
                assert!(driver_type.is_iouring() || driver_type.is_polling());
                assert!(driver_type.is_iocp().not());
            });
    }

    /// Goal: opting back in to `io_uring` must leave compio's own probe in charge, so that path
    /// keeps the graceful fallback rather than being pinned by this module. Method: build through
    /// the helper with `None` and assert the driver is one of the two Linux fusion options.
    #[test]
    fn build_runtime_without_override_lets_compio_probe() {
        build_runtime(None)
            .expect("compio runtime builds without a forced driver")
            .block_on(async {
                let driver_type = Runtime::with_current(Runtime::driver_type);
                assert!(driver_type.is_iouring() || driver_type.is_polling());
                assert!(driver_type.is_iocp().not());
            });
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
