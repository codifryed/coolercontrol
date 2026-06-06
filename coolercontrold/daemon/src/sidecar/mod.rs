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

//! The Tokio sidecar thread.
//!
//! A single long-lived OS thread running a current-thread Tokio runtime. It hosts the parts of the
//! daemon that require a Tokio reactor (the REST/gRPC servers, the dbus sleep listener, and the
//! liqctld/service-plugin transports) so the main thread can run a non-Tokio runtime.
//!
//! The hosted actors are `!Send` (they hold `Rc`/`RefCell` state), and a `!Send` future cannot be
//! moved across threads. So the main thread does not send a future: it sends a `Send` *builder*
//! closure (capturing only `Send` data) which the sidecar invokes on its own thread to construct
//! the `!Send` future, then drives it locally. Main-thread state is reached only over `tokio::sync`
//! channels.
//!
//! The REST/gRPC servers run here; the dbus sleep listener and the liqctld/service-plugin
//! transports move here in the subsequent Phase 1 sub-deliverables.

use anyhow::{anyhow, Result};
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::sync::{mpsc, oneshot};
use tokio::task::LocalSet;
use tokio_util::sync::CancellationToken;

/// Builds a hosted actor's `!Send` future on the sidecar thread. The closure is `Send` so it can
/// cross the thread boundary; the future it returns does not need to be.
type TaskBuilder = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()>>> + Send + 'static>;

/// Grace period for the sidecar's blocking pool on shutdown. The bounded thread-join that guards
/// against a wedged hosted task is added with the shutdown sub-deliverable.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(3);

/// A cheap, cloneable handle for submitting work to the sidecar. Held by anything that needs to run
/// reactor-bound work there (e.g. the service-plugin transport).
#[derive(Clone)]
pub struct SidecarHandle {
    task_tx: mpsc::UnboundedSender<TaskBuilder>,
}

impl SidecarHandle {
    /// Submit a `!Send` actor to run on the sidecar. `make_actor` must capture only `Send` data; it
    /// is invoked on the sidecar thread to construct the actor's future.
    pub fn spawn<F, Fut>(&self, make_actor: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        let builder: TaskBuilder = Box::new(move || Box::pin(make_actor()));
        // Sending only fails once the sidecar thread has exited, in which case there is nothing to
        // run the actor on and dropping it is correct.
        let _ = self.task_tx.send(builder);
    }

    /// Run a one-shot task on the sidecar and await its result. `make_fut` is `Send` (captures only
    /// `Send` data) and builds the future on the sidecar; `T` is `Send` so it returns over the
    /// channel. Errors only if the sidecar thread has already exited.
    #[allow(dead_code)]
    pub async fn run<F, Fut, T>(&self, make_fut: F) -> Result<T>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = T> + 'static,
        T: Send + 'static,
    {
        let (tx, rx) = oneshot::channel();
        self.spawn(move || async move {
            let _ = tx.send(make_fut().await);
        });
        rx.await
            .map_err(|_| anyhow!("sidecar dropped the task before responding"))
    }
}

pub struct Sidecar {
    handle: SidecarHandle,
    thread: std::thread::JoinHandle<()>,
}

impl Sidecar {
    /// Start the sidecar thread. It accepts hosted actors until `cancel_token` fires (then drains
    /// them) or until the `Sidecar` is dropped and all hosted actors complete.
    pub fn start(cancel_token: CancellationToken) -> Self {
        let (task_tx, task_rx) = mpsc::unbounded_channel();
        let thread = std::thread::Builder::new()
            .name("cc-sidecar".to_owned())
            .spawn(move || run(task_rx, cancel_token))
            .expect("sidecar thread spawns");
        Self {
            handle: SidecarHandle { task_tx },
            thread,
        }
    }

    /// A cloneable handle for submitting work to the sidecar from elsewhere.
    #[allow(dead_code)]
    pub fn handle(&self) -> SidecarHandle {
        self.handle.clone()
    }

    /// Submit a `!Send` actor to run on the sidecar. See `SidecarHandle::spawn`.
    pub fn spawn<F, Fut>(&self, make_actor: F)
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + 'static,
    {
        self.handle.spawn(make_actor);
    }

    /// Join the sidecar thread. The caller must already have cancelled the token so the hosted
    /// actors can finish. (A bounded join with a force-exit fallback is added with the shutdown
    /// sub-deliverable, which is also where this is first called.)
    #[allow(dead_code)]
    pub fn join(self) {
        drop(self.handle);
        let _ = self.thread.join();
    }
}

fn run(mut task_rx: mpsc::UnboundedReceiver<TaskBuilder>, cancel_token: CancellationToken) {
    let runtime = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .max_blocking_threads(2)
        .thread_keep_alive(Duration::from_secs(60))
        .thread_name("cc-sidecar-wrk")
        .event_interval(200)
        .global_queue_interval(200)
        .build()
        .expect("sidecar runtime builds");
    runtime.block_on(LocalSet::new().run_until(async move {
        let mut handles = Vec::new();
        loop {
            tokio::select! {
                () = cancel_token.cancelled() => break,
                maybe_builder = task_rx.recv() => match maybe_builder {
                    Some(builder) => handles.push(tokio::task::spawn_local(builder())),
                    None => break,
                },
            }
        }
        // Cancelled: hosted actors watch the same token and exit. Await them so their own graceful
        // shutdown (e.g. the liqctld `/quit`) completes before the thread ends.
        for handle in handles {
            let _ = handle.await;
        }
    }));
    runtime.shutdown_timeout(SHUTDOWN_GRACE);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;
    use tokio::sync::oneshot;

    // Goal: a `!Send` actor (holding Rc/RefCell) submitted from this thread is built and driven on
    // the sidecar thread, and bridges its result back over a Send channel. Proves the
    // Send-closure-builds-a-!Send-future mechanism and the cross-thread channel bridge.
    #[test]
    fn runs_non_send_actor_and_bridges_over_channel() {
        let cancel = CancellationToken::new();
        let sidecar = Sidecar::start(cancel.clone());
        let (tx, rx) = oneshot::channel::<u32>();
        sidecar.spawn(move || async move {
            // `!Send` state lives entirely on the sidecar thread.
            let counter = Rc::new(RefCell::new(0u32));
            *counter.borrow_mut() += 42;
            let value = *counter.borrow();
            let _ = tx.send(value);
        });
        // Block this thread (no runtime here) until the sidecar actor reports back.
        let value = rx.blocking_recv().expect("actor sends a value");
        assert_eq!(value, 42);
        cancel.cancel();
        sidecar.join();
    }

    // Goal: `run` dispatches a one-shot task to the sidecar and returns its result back to the
    // caller's runtime. The closure runs on the sidecar thread; the `u32` returns over the channel.
    #[test]
    fn run_dispatches_task_and_returns_result() {
        let cancel = CancellationToken::new();
        let sidecar = Sidecar::start(cancel.clone());
        let handle = sidecar.handle();
        let result: u32 = crate::rt::test_runtime(async move {
            handle
                .run(|| async { 7u32 * 6 })
                .await
                .expect("sidecar ran the task")
        });
        assert_eq!(result, 42);
        cancel.cancel();
        sidecar.join();
    }

    // Goal: dropping the `Sidecar` (no cancel) still lets a submitted actor finish, then the thread
    // ends because the task channel closes. Negative-space check on the shutdown-via-drop path.
    #[test]
    fn drains_actor_then_exits_when_dropped_without_cancel() {
        let cancel = CancellationToken::new();
        let sidecar = Sidecar::start(cancel);
        let (tx, rx) = oneshot::channel::<()>();
        sidecar.spawn(move || async move {
            let _ = tx.send(());
        });
        rx.blocking_recv().expect("actor ran");
        // No cancel: join drops the task sender, the recv loop ends, and the thread joins.
        sidecar.join();
    }
}
