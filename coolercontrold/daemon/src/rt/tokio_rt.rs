// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Tokio backend for the runtime facade: a current-thread runtime with a `LocalSet`, so tasks stay
//! on the main thread and need not be `Send`. See `super` for the facade contract.
use std::future::{poll_fn, Future};
use std::ops::Not;
use std::task::Poll;
use std::time::{Duration, Instant};

use log::info;
use nix::sys::signal::{SigSet, Signal};
use tokio::runtime::{Builder, Runtime};
use tokio::signal::unix::{signal, SignalKind};
use tokio::task::LocalSet;

use crate::ENV_RUNTIME_DRIVER;

pub use tokio::time::{sleep, timeout};

/// Threads the blocking pool may hold at once. A read that never answers parks its thread for
/// good, so the cap is what bounds a run of stalled one-shot reads, with room left for the rest.
const BLOCKING_THREADS_MAX: usize = 256;

/// How long an idle blocking thread lingers for the next job before it exits.
const BLOCKING_THREAD_KEEP_ALIVE: Duration = Duration::from_secs(60);

/// Initialize and run the main single-threaded runtime to completion.
pub fn runtime<F: Future>(future: F) -> F::Output {
    // Mask termination signals before the runtime and the tokio sidecar spawn any threads, so
    // every thread, and every process spawned later, inherits the mask. `shutdown_signal` lifts it
    // once its listeners exist. See `block_termination_signals`.
    block_termination_signals();
    // A failed build means the OS denied the epoll reactor at startup; the daemon cannot run
    // without it, so failing fast is correct.
    let runtime = build_runtime().expect("tokio runtime builds");
    let output = LocalSet::new().block_on(&runtime, future);
    // A blocking job still running now is a read a timeout already gave up on. Waiting for it
    // could hold the exit forever, so leave it to end with the process.
    runtime.shutdown_background();
    output
}

fn build_runtime() -> std::io::Result<Runtime> {
    Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(BLOCKING_THREADS_MAX)
        .thread_keep_alive(BLOCKING_THREAD_KEEP_ALIVE)
        .thread_name("cc-main-wrk")
        .build()
}

/// Notes an `ENV_RUNTIME_DRIVER` left over from the compio runtime, which read it. Call after
/// logging is set up.
pub fn log_ignored_driver_override() {
    if std::env::var_os(ENV_RUNTIME_DRIVER).is_some() {
        info!("{ENV_RUNTIME_DRIVER} no longer has an effect: the runtime always uses epoll");
    }
}

/// Initialize and run a runtime for tests.
///
/// Each call builds its own runtime, so callers may run in parallel. `#[serial]` only excludes
/// other `#[serial]` tests, never the rest of the suite, so it is not a licence to touch
/// process-global state such as the environment or the current directory.
#[allow(dead_code)]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    let runtime = build_runtime().expect("tokio test runtime builds");
    let output = LocalSet::new().block_on(&runtime, future);
    // Tests park blocking jobs on purpose (a FIFO nobody writes), which a plain drop would wait
    // on forever.
    runtime.shutdown_background();
    output
}

/// Spawn a `!Send` future on the current-thread runtime, detached (fire-and-forget).
pub fn spawn<F>(future: F)
where
    F: Future + 'static,
{
    // Dropping a tokio `JoinHandle` detaches the task, which keeps running.
    tokio::task::spawn_local(future);
}

/// Spawn a `!Send` future eagerly and return an awaitable handle. Test-only: tests need a task to
/// run concurrently and then await its result. The handle maps the backend join error to
/// `super::JoinError`; dropping it before awaiting detaches the task.
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
/// (e.g. a `timeout` fires) the blocking thread runs to completion in the background (tokio does
/// not cancel blocking tasks).
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

/// An interval whose first tick completes at once and whose later ticks land on the start's phase.
pub fn interval(period: Duration) -> Interval {
    assert!(period.is_zero().not());
    Interval {
        start: Instant::now(),
        period,
        first_ticked: false,
    }
}

/// See `interval`. A tick called late waits for the next boundary, skipping the ones it missed.
/// tokio's own interval would instead fire at once, off the phase, which moves every later
/// main-loop tick that follows a long one.
pub struct Interval {
    start: Instant,
    period: Duration,
    first_ticked: bool,
}

impl Interval {
    pub async fn tick(&mut self) -> Instant {
        if self.first_ticked.not() {
            self.first_ticked = true;
            return self.start;
        }
        let now = Instant::now();
        let into_period_nanos = (now - self.start).as_nanos() % self.period.as_nanos();
        // Below the period, so far under `u64::MAX` nanoseconds for any poll rate. The zero
        // fallback would only wait one whole period.
        let into_period = Duration::from_nanos(u64::try_from(into_period_nanos).unwrap_or(0));
        let until_next = self
            .period
            .checked_sub(into_period)
            .expect("the remainder of a period is below the period");
        let next = now + until_next;
        debug_assert!(next > now);
        sleep_until(next).await;
        next
    }
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
/// Called on the main thread before the runtime and the tokio sidecar spawn their threads, so
/// every thread, and every process spawned later, inherits the mask. That buys two things:
///
/// - A signal arriving before the listeners exist stays pending instead of taking the default
///   action and killing the daemon part-way through startup.
/// - liqctld and the shell helpers inherit the mask, so the cgroup-wide `SIGTERM` from
///   `systemctl stop` cannot kill them out from under a graceful shutdown that still has to talk
///   to them. liqctld is stopped by its own `/quit` request and then SIGKILL (unblockable).
///
/// The mask has to come back off before the daemon waits on a signal, which `shutdown_signal` does
/// once its listeners exist. tokio delivers signals through a process-wide `sigaction` handler,
/// and a blocked signal never reaches a handler: leaving the mask on across the wait makes the
/// daemon deaf to every termination signal, so `/shutdown` does nothing and `systemctl stop` has
/// to escalate to SIGKILL.
fn block_termination_signals() {
    // `SIG_BLOCK` unions onto the current mask, so unrelated masked signals are preserved. The only
    // documented failure is an invalid signal/how, impossible for these constants; on the off
    // chance it fails we report it and continue rather than refuse to boot.
    if let Err(err) = termination_signals().thread_block() {
        eprintln!("could not mask termination signals: {err}");
    }
}

/// Unblock `SIGINT`, `SIGTERM`, and `SIGQUIT` on the current thread, so the handlers can actually
/// receive them. Only the thread awaiting `shutdown_signal` calls this; every other thread, and
/// every child process, keeps the startup mask `block_termination_signals` set.
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
/// Lifts the startup mask from `block_termination_signals` once the listeners exist, and puts it
/// back before returning.
pub async fn shutdown_signal() {
    // tokio installs each signal's handler when its listener is created, so all three exist
    // before the mask comes off. A registration error (a bad signal number, no runtime) leaves the
    // daemon unable to observe a shutdown request; panicking is better than silently never
    // shutting down.
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut sigterm = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut sigquit = signal(SignalKind::quit()).expect("failed to install SIGQUIT handler");
    // This is the only thread that ever unmasks, so a process-directed signal is steered to the
    // one thread waiting for it, and anything left pending from startup is delivered now.
    unblock_termination_signals();
    // Raced by hand, like the select it replaces, so a ready listener returns before the others are
    // polled again.
    poll_fn(|cx| {
        let sigint_state = sigint.poll_recv(cx);
        let sigterm_state = sigterm.poll_recv(cx);
        let sigquit_state = sigquit.poll_recv(cx);
        if sigint_state.is_ready() || sigterm_state.is_ready() || sigquit_state.is_ready() {
            return Poll::Ready(());
        }
        Poll::Pending
    })
    .await;
    // tokio's handlers stay installed, so a second Ctrl-C or a stray SIGTERM would be caught and
    // dropped. Re-masking keeps it pending through graceful shutdown instead.
    block_termination_signals();
}

#[cfg(test)]
mod tests {
    use super::*;
    use nix::sys::signal::{pthread_sigmask, SigmaskHow};
    use std::ops::Not;
    use std::pin::pin;

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

    /// Goal: the daemon masks termination signals at startup, and tokio delivers them through a
    /// process-wide `sigaction` handler that a blocked signal never reaches. So the thread awaiting
    /// `shutdown_signal` must unmask as soon as its listeners exist, or the daemon is deaf to every
    /// termination signal: `/shutdown` (which sends the process SIGQUIT) does nothing and
    /// `systemctl stop` has to escalate to SIGKILL. Method: block the signals the way `runtime`
    /// does, poll `shutdown_signal` exactly once, and read the thread's mask back. No signal is
    /// ever raised, so the shared test process is untouched; the original mask is put back before
    /// the assertions so a failure cannot strand the thread.
    #[test]
    fn shutdown_signal_lifts_the_startup_mask_once_handlers_are_installed() {
        test_runtime(async {
            let mut original = SigSet::empty();
            pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut original))
                .expect("read current signal mask");

            block_termination_signals();
            let (state, masked) = {
                let mut waiting = pin!(shutdown_signal());
                let state = poll_fn(|cx| Poll::Ready(waiting.as_mut().poll(cx))).await;

                let mut masked = SigSet::empty();
                pthread_sigmask(SigmaskHow::SIG_SETMASK, None, Some(&mut masked))
                    .expect("read signal mask after the first poll");
                (state, masked)
            };
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

    /// Goal: a tick called late waits for the next boundary of the start's phase, as compio's
    /// interval did, rather than firing at once off the phase or bursting the missed ticks. The
    /// main loop's timing model depends on it. Method: take the first tick, stall three and a half
    /// periods, then time the next tick: it must wait out the rest of the period it landed in.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_late_tick_waits_for_the_next_boundary() {
        test_runtime(async {
            let period = Duration::from_millis(40);
            let mut ticks = interval(period);
            ticks.tick().await;
            std::thread::sleep(period * 3 + period / 2);
            let late = Instant::now();

            ticks.tick().await;

            assert!(late.elapsed() >= period / 4, "{:?}", late.elapsed());
        });
    }

    /// Goal: the runtime must end even with a blocking job still parked, as a read that never
    /// answers leaves one. Method: park a job on a channel nobody sends on, let the runtime
    /// return, and confirm the call comes back.
    #[test]
    fn a_parked_blocking_job_does_not_hold_the_runtime() {
        let (_never, parked) = std::sync::mpsc::channel::<()>();
        test_runtime(async move {
            let job = spawn_blocking(move || parked.recv());
            let waited = timeout(Duration::from_millis(10), job).await;
            assert!(waited.is_err(), "the job must still be parked");
        });
    }
}
