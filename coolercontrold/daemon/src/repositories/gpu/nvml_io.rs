// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One GPU's NVML calls, isolated on a thread of its own.
//!
//! NVML is a blocking vendor FFI with no timeout, and its setters need `&mut Device`. On a shared
//! blocking pool behind a lock, `rt::timeout` abandons the caller but cannot cancel the thread, so
//! the lock is held forever and every later tick parks another pool thread. Owning the device on
//! its own thread means a hung driver parks that one thread and the GPU goes unreachable on the
//! same rules as a wedged sysfs device.
use std::future::Future;
use std::ops::Not;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::{debug, error};
use nvml_wrapper::error::NvmlError;
use tokio::sync::{mpsc, oneshot};

use crate::affinity;
use crate::repositories::hwmon::device_io::{DeviceHealth, HealthState, QUEUE_DEPTH};
use crate::rt;
use crate::rt::reply::{ReplyPool, ReplySender};

/// One unit of work for a GPU's thread.
pub enum NvmlJob {
    /// Any call. It owns its own reply channel, so every NVML call shape fits one queue without
    /// a variant per setter.
    Call(Box<dyn FnOnce(&mut nvml_wrapper::Device<'static>) + Send>),
    /// A fan duty write, the one call a profile makes on every duty change. Unboxed, with a
    /// recycled reply channel, so a warm write allocates nothing.
    SetFanSpeed {
        fan_index: u32,
        duty_percent: u32,
        reply: ReplySender<Result<(), NvmlError>>,
    },
}

/// The main-thread half of a GPU's NVML worker.
pub struct NvmlIo {
    tx: mpsc::Sender<NvmlJob>,
    reply_timeout: Duration,
    state: HealthState,
    /// Reply channels for fan duty writes, kept so a warm one allocates nothing.
    fan_speed_replies: ReplyPool<Result<(), NvmlError>>,
}

impl NvmlIo {
    /// Move `device` onto a thread of its own and return the handle that talks to it.
    ///
    /// # Errors
    ///
    /// When the OS refuses the thread. The GPU is then unusable rather than un-isolated: NVML
    /// cannot be shared back to the caller, since the device was moved.
    pub fn spawn(
        device_name: String,
        device: nvml_wrapper::Device<'static>,
        reply_timeout: Duration,
    ) -> Result<Self> {
        debug_assert!(reply_timeout > Duration::ZERO);
        let (tx, rx) = mpsc::channel::<NvmlJob>(QUEUE_DEPTH);
        let thread_name = format!("nvml-{}", device_name.replace(' ', "-"));
        debug!("NVML worker starting: {thread_name}");
        // Detached deliberately: a wedged worker never returns, so a handle would only invite a
        // `join` that cannot be bounded.
        let builder = thread::Builder::new().name(thread_name);
        affinity::spawn_worker(builder, move || run_worker(device, rx))?;
        Ok(Self {
            tx,
            reply_timeout,
            state: HealthState::new(device_name),
            fan_speed_replies: ReplyPool::default(),
        })
    }

    /// A handle with no worker behind it, for tests.
    ///
    /// The handle never owned the device: the worker thread does. So a test can hold one without
    /// NVML, an NVIDIA driver, or a card. Nothing drains the returned receiver, so every call
    /// times out, which is also what a wedged GPU looks like from here.
    #[cfg(test)]
    #[must_use]
    pub fn for_test(device_name: &str) -> (Self, mpsc::Receiver<NvmlJob>) {
        let (tx, rx) = mpsc::channel::<NvmlJob>(QUEUE_DEPTH);
        (
            Self {
                tx,
                reply_timeout: Duration::from_millis(10),
                state: HealthState::new(device_name.to_owned()),
                fan_speed_replies: ReplyPool::default(),
            },
            rx,
        )
    }

    /// Run one NVML call on the device's thread, within its budget. `what` names it for logs.
    ///
    /// Only a timeout counts against the GPU's health; an error from NVML means it answered.
    ///
    /// # Errors
    ///
    /// When the GPU is unreachable, its worker is gone, or the call outran `reply_timeout`.
    pub async fn call<T, F>(&self, what: &str, f: F) -> Result<T>
    where
        F: FnOnce(&mut nvml_wrapper::Device<'static>) -> T + Send + 'static,
        T: Send + 'static,
    {
        self.call_within(what, self.reply_timeout, f).await
    }

    /// `call` within `budget` rather than the per-tick reply timeout, for init's one call of many
    /// reads, which may first have to wake the GPU.
    ///
    /// # Errors
    ///
    /// As `call`.
    pub async fn call_within<T, F>(&self, what: &str, budget: Duration, f: F) -> Result<T>
    where
        F: FnOnce(&mut nvml_wrapper::Device<'static>) -> T + Send + 'static,
        T: Send + 'static,
    {
        self.dispatch(what, budget, oneshot::channel, |reply_tx| {
            NvmlJob::Call(Box::new(move |device| {
                // A dropped receiver means the caller already timed out. Expected, not an error.
                let _ = reply_tx.send(f(device));
            }))
        })
        .await
    }

    /// Sets one fan's duty, within the per-tick reply timeout.
    ///
    /// # Errors
    ///
    /// As `call`. NVML's own answer is the inner result.
    pub async fn set_fan_speed(
        &self,
        fan_index: u32,
        duty_percent: u32,
    ) -> Result<Result<(), NvmlError>> {
        debug_assert!(duty_percent <= 100);
        self.dispatch(
            "fan duty",
            self.reply_timeout,
            || self.fan_speed_replies.channel(),
            |reply| NvmlJob::SetFanSpeed {
                fan_index,
                duty_percent,
                reply,
            },
        )
        .await
    }

    /// Queues one job and waits for its reply, within `budget`. `channel` makes the reply
    /// channel, and is called only once the job will be sent.
    async fn dispatch<T, Tx, Rx, E>(
        &self,
        what: &str,
        budget: Duration,
        channel: impl FnOnce() -> (Tx, Rx),
        make_job: impl FnOnce(Tx) -> NvmlJob,
    ) -> Result<T>
    where
        Rx: Future<Output = std::result::Result<T, E>>,
    {
        debug_assert!(budget > Duration::ZERO);
        // Refuse to queue for a GPU that is not answering until its next probe is due, or every
        // tick adds a job to a queue nobody drains and pays a full timeout to learn that.
        if self.state.dispatchable().not() {
            return Err(anyhow!(
                "GPU {} is not responding; skipping NVML {what}",
                self.state.device_name()
            ));
        }
        let (reply_tx, reply_rx) = channel();
        let job = make_job(reply_tx);
        let exchange = async {
            self.tx
                .send(job)
                .await
                .map_err(|_| self.worker_gone(what))?;
            reply_rx.await.map_err(|_| self.worker_gone(what))
        };
        match rt::timeout(budget, exchange).await {
            Ok(result) => {
                self.state.record_answered();
                result
            }
            Err(_elapsed) => {
                self.state.record_timeout(&what);
                Err(anyhow!(
                    "NVML {what} for GPU {} did not complete within {budget:?}",
                    self.state.device_name(),
                ))
            }
        }
    }

    /// The GPU's name, as detection resolved it.
    #[must_use]
    pub fn device_name(&self) -> &str {
        self.state.device_name()
    }

    /// Whether this GPU is answering.
    #[must_use]
    pub fn health(&self) -> DeviceHealth {
        self.state.health()
    }

    /// Lets the next call through even if the GPU is currently unreachable, for shutdown, where
    /// handing fan control back is worth one more attempt.
    pub fn allow_probe_now(&self) {
        self.state.allow_next_dispatch();
    }

    fn worker_gone(&self, what: &str) -> anyhow::Error {
        anyhow!(
            "NVML worker for GPU {} is gone, cannot {what}",
            self.state.device_name()
        )
    }
}

/// The worker thread body: own the device, serve one call at a time until the queue closes.
///
/// A plain blocking loop. NVML is a synchronous FFI, so there is nothing here for a reactor to do.
fn run_worker(mut device: nvml_wrapper::Device<'static>, mut rx: mpsc::Receiver<NvmlJob>) {
    while let Some(job) = rx.blocking_recv() {
        match job {
            NvmlJob::Call(call) => call(&mut device),
            NvmlJob::SetFanSpeed {
                fan_index,
                duty_percent,
                reply,
            } => {
                // A caller that already timed out never reads it. Expected, not an error.
                reply.send(device.set_fan_speed(fan_index, duty_percent));
            }
        }
    }
    debug!("NVML worker stopped");
}

/// Logs a worker that could not start. The GPU is dropped rather than run un-isolated.
pub fn log_spawn_failure(device_name: &str, err: &anyhow::Error) {
    error!("Could not start an NVML thread for GPU {device_name}: {err}. This GPU is unmanaged.");
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers each fan duty job from `rx` as NVML would, recording the write, until the queue
    /// closes. Stands in for the worker, which needs a GPU.
    async fn answer_fan_speed_jobs(
        mut rx: mpsc::Receiver<NvmlJob>,
        written: std::rc::Rc<std::cell::Cell<Option<(u32, u32)>>>,
    ) {
        while let Some(job) = rx.recv().await {
            let NvmlJob::SetFanSpeed {
                fan_index,
                duty_percent,
                reply,
            } = job
            else {
                panic!("only fan duty jobs are expected");
            };
            written.set(Some((fan_index, duty_percent)));
            reply.send(Ok(()));
        }
    }

    /// Goal: a fan duty write reaches the worker as its own job, carrying the fan and the duty.
    /// Method: answer the queue as the worker would and check what it was asked.
    #[test]
    fn a_fan_duty_write_carries_its_fan_and_duty() {
        rt::test_runtime(async {
            let (io, rx) = NvmlIo::for_test("test-gpu");
            let written = std::rc::Rc::default();
            rt::spawn(answer_fan_speed_jobs(rx, std::rc::Rc::clone(&written)));

            io.set_fan_speed(2, 55).await.unwrap().unwrap();

            assert_eq!(written.get(), Some((2, 55)));
            assert_eq!(io.fan_speed_replies.spare_count(), 1);
        });
    }

    /// Goal: a warm fan duty write costs the main thread no allocation. Method: write once to
    /// warm the reply channel, then count a second write's round trip.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_warm_fan_duty_write_allocates_nothing() {
        rt::test_runtime(async {
            let (io, rx) = NvmlIo::for_test("test-gpu");
            let written = std::rc::Rc::default();
            rt::spawn(answer_fan_speed_jobs(rx, std::rc::Rc::clone(&written)));
            io.set_fan_speed(0, 40).await.unwrap().unwrap();

            let (result, allocations) =
                crate::alloc_count::count_allocations_async(io.set_fan_speed(0, 60)).await;

            result.unwrap().unwrap();
            assert_eq!(written.get(), Some((0, 60)));
            assert_eq!(allocations, 0);
        });
    }
}
