// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! One GPU's NVML calls, isolated on a thread of its own.
//!
//! NVML is a blocking vendor FFI with no timeout, and its setters need `&mut Device`. On a shared
//! blocking pool behind a lock, `rt::timeout` abandons the caller but cannot cancel the thread, so
//! the lock is held forever and every later tick parks another pool thread. Owning the device on
//! its own thread means a hung driver parks that one thread and the GPU goes unreachable on the
//! same rules as a wedged sysfs device.
use std::ops::Not;
use std::thread;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::{debug, error};
use tokio::sync::{mpsc, oneshot};

use crate::repositories::hwmon::device_io::{DeviceHealth, HealthState, QUEUE_DEPTH};
use crate::rt;

/// One unit of work for a GPU's thread. The job owns its own reply channel, so every NVML call
/// shape fits one queue without an enum per setter.
type NvmlJob = Box<dyn FnOnce(&mut nvml_wrapper::Device<'static>) + Send>;

/// The main-thread half of a GPU's NVML worker.
pub struct NvmlIo {
    tx: mpsc::Sender<NvmlJob>,
    reply_timeout: Duration,
    state: HealthState,
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
        thread::Builder::new()
            .name(thread_name)
            .spawn(move || run_worker(device, rx))?;
        Ok(Self {
            tx,
            reply_timeout,
            state: HealthState::new(device_name),
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
        // Refuse to queue for a GPU that is not answering until its next probe is due, or every
        // tick adds a job to a queue nobody drains and pays a full timeout to learn that.
        if self.state.dispatchable().not() {
            return Err(anyhow!(
                "GPU {} is not responding; skipping NVML {what}",
                self.state.device_name()
            ));
        }
        let (reply_tx, reply_rx) = oneshot::channel();
        let job: NvmlJob = Box::new(move |device| {
            // A dropped receiver means the caller already timed out. Expected, not an error.
            let _ = reply_tx.send(f(device));
        });
        let exchange = async {
            self.tx
                .send(job)
                .await
                .map_err(|_| self.worker_gone(what))?;
            reply_rx.await.map_err(|_| self.worker_gone(what))
        };
        match rt::timeout(self.reply_timeout, exchange).await {
            Ok(result) => {
                self.state.record_answered();
                result
            }
            Err(_elapsed) => {
                self.state.record_timeout(&what);
                Err(anyhow!(
                    "NVML {what} for GPU {} did not complete within {:?}",
                    self.state.device_name(),
                    self.reply_timeout
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
        job(&mut device);
    }
    debug!("NVML worker stopped");
}

/// Logs a worker that could not start. The GPU is dropped rather than run un-isolated.
pub fn log_spawn_failure(device_name: &str, err: &anyhow::Error) {
    error!("Could not start an NVML thread for GPU {device_name}: {err}. This GPU is unmanaged.");
}
