// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! compio backend for the runtime facade. See `super` for the facade contract.

use std::future::{poll_fn, Future};
use std::ops::Not;
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
/// `io_uring` when every opcode we need is supported, falling back to epoll otherwise.
///
/// Stays on `io_uring`. Issue 606 (an idle core reading ~100% iowait on per-core monitors) is its
/// `io_uring_enter(GETEVENTS)` wait being accounted as iowait since kernel 6.5. compio-driver
/// 0.12.5 sets `IORING_ENTER_NO_IOWAIT`, which clears that outright on kernel 6.15+. Kernels 6.5
/// to 6.14 have the accounting but not the opt-out, so `CC_RUNTIME_DRIVER=epoll` stays their
/// answer. It is not the default because the iowait is a reporting artifact while epoll costs
/// real wakeups: measured over 90s windows it took daemon context switches from 30/s to 159/s and
/// threads from 3 to 13, since on Linux it sends every file op to the `AsyncifyPool` (the `aio`
/// path is BSD-only).
const DEFAULT_DRIVER: Option<DriverType> = None;

/// Initialize and run the main single-threaded runtime to completion.
pub fn runtime<F: Future>(future: F) -> F::Output {
    // compio is inherently single-threaded and `!Send`, so there is no `LocalSet` to set up nor a
    // multi-thread builder to constrain. The fusion driver (io-uring + polling) is selected at
    // compile time; which half runs is `DEFAULT_DRIVER`, overridable with `ENV_RUNTIME_DRIVER`.
    //
    // Mask termination signals before the runtime and the tokio sidecar spawn any threads, so
    // every thread, and every process spawned later, inherits the mask. `shutdown_signal` lifts it
    // on its own thread once compio's handlers exist. See `block_termination_signals`.
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
    driver_override_from(std::env::var(ENV_RUNTIME_DRIVER).ok().as_deref())
}

/// Resolve a raw `ENV_RUNTIME_DRIVER` value, `None` when unset. Split from the read so the
/// branches are testable without writing to the process environment, which is shared with
/// every other test running at the same time.
fn driver_override_from(raw: Option<&str>) -> Option<DriverType> {
    let Some(raw) = raw else {
        return DEFAULT_DRIVER;
    };
    let Some(choice) = parse_driver_override(raw) else {
        // Logging is not set up this early, so stderr is the only channel available. An unusable
        // value must not stop the daemon booting, so fall back to the default.
        eprintln!(
            "{ENV_RUNTIME_DRIVER}: unrecognized value {raw:?}, expected epoll or io_uring, \
             using the default reactor backend"
        );
        return DEFAULT_DRIVER;
    };
    choice.forced_driver()
}

/// What a recognized `ENV_RUNTIME_DRIVER` value asks the runtime to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriverChoice {
    /// Force the epoll driver, which compio names `Poll`.
    Epoll,
    /// Let compio probe the kernel and keep its own fallback.
    Probe,
}

impl DriverChoice {
    /// The driver to force, or `None` to leave compio's probe in charge.
    fn forced_driver(self) -> Option<DriverType> {
        match self {
            Self::Epoll => Some(DriverType::Poll),
            Self::Probe => None,
        }
    }
}

/// Resolve an `ENV_RUNTIME_DRIVER` value, or `None` when the value is not recognized.
///
/// Exactly two values are accepted, one per backend, so the documented set and the parsed set
/// cannot drift. `epoll` names the mechanism against its real alternative, `io_uring`. compio
/// calls this driver `Poll` since the same type is kqueue on the BSDs, but the daemon is Linux
/// only, so it is always epoll here. Case and surrounding whitespace are forgiven because the
/// value arrives through `systemd` unit files and compose files, which invite both.
///
/// Note that `io_uring` deliberately resolves to `Probe` rather than forcing `DriverType::IoUring`.
/// Setting that explicitly clears compio's `fallback` flag, turning an unavailable `io_uring` into
/// a hard startup failure instead of a graceful degrade to epoll. Probing selects the same driver
/// without that cliff, so only epoll is ever forced.
fn parse_driver_override(raw: &str) -> Option<DriverChoice> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "epoll" => Some(DriverChoice::Epoll),
        "io_uring" => Some(DriverChoice::Probe),
        _ => None,
    }
}

/// Log which fusion-driver backend compio selected. `io_uring` is the default and is used when the
/// kernel supports every opcode we need; epoll is reached either as compio's fallback or by opting
/// in through `ENV_RUNTIME_DRIVER`. Call from within the runtime, after logging is set up, so
/// `with_current` sees the active runtime.
pub fn log_active_backend() {
    let driver_type = Runtime::with_current(Runtime::driver_type);
    if driver_type.is_iouring() {
        info!("Using the io_uring backend");
    } else {
        info!("Using the epoll backend");
    }
}

/// Initialize and run a runtime for tests.
///
/// Each call builds its own runtime, so callers may run in parallel. `#[serial]` only excludes
/// other `#[serial]` tests, never the rest of the suite, so it is not a licence to touch
/// process-global state such as the environment or the current directory.
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

/// The signals the daemon shuts down on. `SIGQUIT` is the one the REST `/shutdown` endpoint sends
/// itself, so a restart from the UI rides on this set just as `systemctl stop` does.
fn termination_signals() -> SigSet {
    let mut set = SigSet::empty();
    set.add(Signal::SIGINT);
    set.add(Signal::SIGTERM);
    set.add(Signal::SIGQUIT);
    set
}

/// Block `SIGINT`, `SIGTERM`, and `SIGQUIT` on the current thread.
///
/// Called on the main thread before the compio runtime and the tokio sidecar spawn their threads,
/// so every thread, and every process spawned later, inherits the mask. That buys two things:
///
/// - A signal arriving before the listeners exist stays pending instead of taking the default
///   action and killing the daemon part-way through startup.
/// - liqctld and the shell helpers inherit the mask, so the cgroup-wide `SIGTERM` from
///   `systemctl stop` cannot kill them out from under a graceful shutdown that still has to talk
///   to them. liqctld is stopped by its own `/quit` request and then SIGKILL (unblockable).
///
/// The mask has to come back off before the daemon waits on a signal, which `shutdown_signal` does
/// on its own thread once compio's handlers are installed. compio-signal 0.10 dropped the Linux
/// signalfd listener for a process-wide `sigaction` handler, and a blocked signal never reaches a
/// handler: leaving the mask on across the wait makes the daemon deaf to every termination signal,
/// so `/shutdown` does nothing and `systemctl stop` has to escalate to SIGKILL.
fn block_termination_signals() {
    // `SIG_BLOCK` unions onto the current mask, so unrelated masked signals are preserved. The only
    // documented failure is an invalid signal/how, impossible for these constants; on the off
    // chance it fails we report it and continue rather than refuse to boot.
    if let Err(err) = termination_signals().thread_block() {
        eprintln!("could not mask termination signals: {err}");
    }
}

/// Unblock `SIGINT`, `SIGTERM`, and `SIGQUIT` on the current thread, so compio's handlers can
/// actually receive them. Only the thread awaiting `shutdown_signal` calls this; every other
/// thread, and every child process, keeps the startup mask `block_termination_signals` set.
fn unblock_termination_signals() {
    // `SIG_UNBLOCK` subtracts from the current mask, leaving unrelated masked signals alone. The
    // failure mode matches `block_termination_signals`: report and continue.
    if let Err(err) = termination_signals().thread_unblock() {
        eprintln!("could not unmask termination signals: {err}");
    }
}

/// Complete when any process-termination signal is received (`SIGINT`/Ctrl-C, `SIGTERM`,
/// `SIGQUIT`). The caller decides what to do on completion (typically cancel the run token).
///
/// Lifts the startup mask from `block_termination_signals` once compio's handlers are installed,
/// and puts it back before returning.
pub async fn shutdown_signal() {
    // `compio::signal::ctrl_c` covers SIGINT; the others are awaited by number. We race them by
    // hand (`futures_util::select!` needs the `async-await` feature we do not enable). Each future
    // is one-shot, so once any is ready we return without re-polling a completed one. Scoped so the
    // listeners drop, handing these signals back to their default action, before we re-block.
    {
        let mut ctrl_c = pin!(compio::signal::ctrl_c());
        let mut sigterm = pin!(compio::signal::unix::signal(nix::libc::SIGTERM));
        let mut sigquit = pin!(compio::signal::unix::signal(nix::libc::SIGQUIT));
        let mut mask_lifted = false;
        // A handler registration error (memory exhaustion, a bad signal number) leaves the daemon
        // unable to observe a shutdown request; panicking is better than silently never shutting
        // down.
        poll_fn(|cx| {
            // Poll all three before deciding anything: compio installs a signal's handler on its
            // listener's first poll, and the mask must not come off until every one of them
            // exists. Whichever listener completes here is not polled again, since any ready
            // state returns from the `poll_fn`.
            let ctrl_c_state = ctrl_c.as_mut().poll(cx);
            let sigterm_state = sigterm.as_mut().poll(cx);
            let sigquit_state = sigquit.as_mut().poll(cx);
            if mask_lifted.not() {
                mask_lifted = true;
                // This is the only thread that ever unmasks, so a process-directed signal is
                // steered to the one thread waiting for it, and anything left pending from
                // startup is delivered now.
                unblock_termination_signals();
            }
            if let Poll::Ready(res) = ctrl_c_state {
                res.expect("failed to install Ctrl+C handler");
                return Poll::Ready(());
            }
            if let Poll::Ready(res) = sigterm_state {
                res.expect("failed to install SIGTERM handler");
                return Poll::Ready(());
            }
            if let Poll::Ready(res) = sigquit_state {
                res.expect("failed to install SIGQUIT handler");
                return Poll::Ready(());
            }
            Poll::Pending
        })
        .await;
    }
    // The listeners are dropped now, which hands these signals back to their default action.
    // Re-masking keeps a second Ctrl-C, or a stray SIGTERM, pending through graceful shutdown
    // rather than force-killing the daemon part-way through applying shutdown settings.
    block_termination_signals();
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::signal::{pthread_sigmask, SigmaskHow};
    use std::ops::Not;

    /// Goal: `block_termination_signals` must leave SIGINT, SIGTERM, and SIGQUIT masked on the
    /// calling thread. Every thread and every child process inherits that mask from startup, which
    /// is what keeps a signal arriving before the listeners exist, or a cgroup-wide `SIGTERM` aimed
    /// at liqctld, from taking the default action mid-shutdown. Method: snapshot the current mask,
    /// apply the block, read the mask back and assert all three are members, then restore the
    /// snapshot so the shared test-runner thread is left as found.
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

    /// Goal: `unblock_termination_signals` must clear exactly the three termination signals and
    /// leave anything else the thread has masked alone, since it runs on a live runtime thread
    /// whose mask is not ours alone to reset. Method: mask an unrelated signal alongside the three,
    /// unblock, then read the mask back; restore before asserting so a failure cannot leave the
    /// shared test-runner thread masked.
    #[test]
    fn unblock_termination_signals_clears_only_the_termination_set() {
        let mut original = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut original))
            .expect("read current signal mask");

        let mut unrelated = SigSet::empty();
        unrelated.add(Signal::SIGUSR1);
        unrelated.thread_block().expect("mask an unrelated signal");
        block_termination_signals();
        unblock_termination_signals();

        let mut now_masked = SigSet::empty();
        pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut now_masked))
            .expect("read signal mask after unblock");
        pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&original), None)
            .expect("restore original signal mask");

        assert!(now_masked.contains(Signal::SIGINT).not());
        assert!(now_masked.contains(Signal::SIGTERM).not());
        assert!(now_masked.contains(Signal::SIGQUIT).not());
        assert!(now_masked.contains(Signal::SIGUSR1));
    }

    /// Goal: the daemon masks termination signals at startup, and compio-signal 0.10 delivers them
    /// through a process-wide `sigaction` handler that a blocked signal never reaches. So the
    /// thread awaiting `shutdown_signal` must unmask as soon as those handlers exist, or the daemon
    /// is deaf to every termination signal: `/shutdown` (which sends the process SIGQUIT) does
    /// nothing and `systemctl stop` has to escalate to SIGKILL. This is the regression the
    /// compio 0.18 -> 0.19 upgrade introduced, and the assertion that pins the contract we depend
    /// on compio for. Method: block the signals the way `runtime` does, poll `shutdown_signal`
    /// exactly once (compio registers a handler on a listener's first poll), and read the thread's
    /// mask back. No signal is ever raised, so the shared test process is untouched; the future is
    /// dropped, restoring the default disposition, and the original mask is put back before the
    /// assertions so a failure cannot strand the thread.
    #[test]
    fn shutdown_signal_lifts_the_startup_mask_once_handlers_are_installed() {
        test_runtime(async {
            let mut original = SigSet::empty();
            pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut original))
                .expect("read current signal mask");

            block_termination_signals();
            let mut waiting = pin!(shutdown_signal());
            let state = poll_fn(|cx| Poll::Ready(waiting.as_mut().poll(cx))).await;

            let mut masked = SigSet::empty();
            pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut masked))
                .expect("read signal mask after the first poll");
            drop(waiting);
            pthread_sigmask(SigmaskHow::SIG_SETMASK, Some(&original), None)
                .expect("restore original signal mask");

            assert!(
                state.is_pending(),
                "no signal was raised, so the wait must still be pending"
            );
            assert!(masked.contains(Signal::SIGINT).not());
            assert!(masked.contains(Signal::SIGTERM).not());
            assert!(masked.contains(Signal::SIGQUIT).not());
        });
    }

    /// Goal: the two documented values must resolve, in any case and with surrounding whitespace,
    /// and `io_uring` must map to "do not force" so compio keeps its graceful fallback. Method:
    /// table-drive each value with the casing and padding a unit or compose file can introduce.
    #[test]
    fn parse_driver_override_accepts_the_documented_values() {
        for raw in ["epoll", "EPOLL", "  epoll  ", "EPoll"] {
            assert_eq!(
                parse_driver_override(raw),
                Some(DriverChoice::Epoll),
                "{raw:?} should force the epoll driver"
            );
        }
        for raw in ["io_uring", "IO_URing", " io_uring "] {
            assert_eq!(
                parse_driver_override(raw),
                Some(DriverChoice::Probe),
                "{raw:?} should probe rather than force io_uring"
            );
        }
    }

    /// Goal: an unrecognized value must be reported as unrecognized rather than silently treated as
    /// a driver choice, so `driver_override` can warn and fall back instead of forcing the wrong
    /// reactor. Method: assert the negative space. The plausible alternate spellings are listed
    /// deliberately: only the two documented values parse, so `poll` and `io-uring` must be
    /// rejected loudly rather than quietly accepted as undocumented aliases.
    #[test]
    fn parse_driver_override_rejects_unknown_values() {
        for raw in [
            "", "  ", "iocp", "tokio", "io uring", "polled", "1", "true", "poll", "polling",
            "io-uring", "iouring", "uring",
        ] {
            assert_eq!(
                parse_driver_override(raw),
                None,
                "{raw:?} should not resolve to a driver"
            );
        }
    }

    /// Goal: forcing `DriverType::Poll` must actually produce an epoll runtime, since that is the
    /// whole mechanism behind the `CC_RUNTIME_DRIVER=epoll` escape hatch for the io_uring iowait
    /// accounting. Method: build a runtime through the same helper `runtime` uses and assert from
    /// inside `block_on` that the live driver is epoll and specifically not io_uring, which is
    /// what this machine would otherwise have selected.
    #[test]
    fn build_runtime_forces_the_epoll_driver() {
        build_runtime(Some(DriverType::Poll))
            .expect("compio runtime builds with the epoll driver forced")
            .block_on(async {
                let driver_type = Runtime::with_current(Runtime::driver_type);
                assert!(driver_type.is_polling());
                assert!(driver_type.is_iouring().not());
            });
    }

    /// Goal: `driver_override` must read the variable the documentation promises, and must resolve
    /// every branch the way issue 606 settled: unset and unrecognized both fall back to the
    /// default, `epoll` forces epoll, and `io_uring` opts back in without pinning the
    /// driver. Method: drive the resolver directly and assert the name of the variable the one
    /// reader passes it, which is the typo `parse_driver_override`'s own tests cannot catch.
    /// Setting the variable for real would write to the process environment while the rest of the
    /// suite reads it from other threads.
    #[test]
    fn driver_override_reads_the_documented_variable() {
        assert_eq!(ENV_RUNTIME_DRIVER, "CC_RUNTIME_DRIVER");

        assert_eq!(driver_override_from(None), DEFAULT_DRIVER);
        assert_eq!(driver_override_from(Some("epoll")), Some(DriverType::Poll));
        assert_eq!(driver_override_from(Some("io_uring")), None);

        // Negative space: an unusable value must boot on the default, never force a driver.
        // `poll` is checked alongside nonsense because it reads like a valid value and is
        // deliberately not one.
        assert_eq!(driver_override_from(Some("nonsense")), DEFAULT_DRIVER);
        assert_eq!(driver_override_from(Some("poll")), DEFAULT_DRIVER);
    }

    /// Goal: the shipped default must leave compio's probe in charge rather than pinning a
    /// driver, so an unavailable `io_uring` still degrades to epoll instead of failing startup.
    /// The explicit `is_none` assertion is the tripwire: issue 606 is a standing temptation to
    /// force epoll here, and that trade costs real wakeups, so a flip must be deliberate.
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
    /// options (`io_uring` or its epoll fallback, never IOCP) and that the log call returns
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
