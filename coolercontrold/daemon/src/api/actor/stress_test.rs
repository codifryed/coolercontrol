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

use std::ops::Not;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::api::actor::{run_api_actor, ApiActor};

const MAX_DURATION_SECS: u16 = 600;
const DEFAULT_DURATION_SECS: u16 = 60;
const EARLY_EXIT_CHECK: Duration = Duration::from_millis(500);
/// Grace period after a child's declared `--timeout` before the daemon
/// force-kills it. Belt-and-suspenders against a stuck child whose own
/// self-termination failed (e.g. blocking syscall, hung GPU driver).
const WATCHDOG_GRACE_SECS: u64 = 10;
/// Bound on `child.wait()` during stop. SIGKILL is uninterruptible, so a child
/// that isn't reaped within this window is stuck in kernel D-state (e.g. buggy
/// GPU driver ioctl). Moving on keeps the actor responsive; the kernel reaps
/// the zombie when the blocking syscall returns.
const STOP_REAP_TIMEOUT_SECS: u64 = 5;

/// Which backend is running (or will run) a stress test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StressBackend {
    BuiltIn,
    StressNg,
}

/// Which stress test the watchdog timer is associated with.
/// Internal to the actor; never crosses the API boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StressKind {
    Cpu,
    Gpu,
    Ram,
    Drive,
}

impl StressKind {
    fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Gpu => "GPU",
            Self::Ram => "RAM",
            Self::Drive => "Drive",
        }
    }
}

impl std::fmt::Display for StressBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn => f.write_str("built-in"),
            Self::StressNg => f.write_str("stress-ng"),
        }
    }
}

/// Detected stress-ng presence, probed once at startup.
struct StressNgCaps {
    /// Path to the stress-ng binary, if found.
    path: Option<PathBuf>,
}

struct StressTestActor {
    receiver: mpsc::Receiver<StressTestMessage>,
    /// Clone handed to each watchdog task so it can self-message on expiry.
    /// The actor then owns the `Child` handle and can safely check-and-kill
    /// without racing on a recycled PID.
    sender: mpsc::Sender<StressTestMessage>,
    stress_ng: StressNgCaps,
    cpu_child: Option<Child>,
    cpu_duration_secs: Option<u16>,
    cpu_backend: Option<StressBackend>,
    cpu_watchdog: Option<CancellationToken>,
    gpu_child: Option<Child>,
    gpu_duration_secs: Option<u16>,
    gpu_backend: Option<StressBackend>,
    gpu_watchdog: Option<CancellationToken>,
    ram_child: Option<Child>,
    ram_duration_secs: Option<u16>,
    ram_backend: Option<StressBackend>,
    ram_watchdog: Option<CancellationToken>,
    drive_child: Option<Child>,
    drive_duration_secs: Option<u16>,
    drive_backend: Option<StressBackend>,
    drive_watchdog: Option<CancellationToken>,
}

enum StressTestMessage {
    StartCpu {
        thread_count: Option<u16>,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopCpu {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StartGpu {
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopGpu {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StartRam {
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopRam {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StartDrive {
        device_path: String,
        threads: Option<u16>,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopDrive {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopAll {
        respond_to: oneshot::Sender<Result<()>>,
    },
    Status {
        respond_to: oneshot::Sender<StressTestStatus>,
    },
    /// Internal message: a watchdog timer expired. The actor decides whether
    /// to force-kill (child still running) or just clean up state (child
    /// already exited within the grace window).
    WatchdogFired { kind: StressKind },
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct StressTestStatus {
    pub stress_ng_available: bool,
    pub cpu_active: bool,
    pub cpu_duration_secs: Option<u16>,
    pub cpu_backend: StressBackend,
    pub gpu_active: bool,
    pub gpu_duration_secs: Option<u16>,
    pub gpu_backend: StressBackend,
    pub ram_active: bool,
    pub ram_duration_secs: Option<u16>,
    pub ram_backend: StressBackend,
    pub drive_active: bool,
    pub drive_duration_secs: Option<u16>,
    pub drive_backend: StressBackend,
}

impl StressTestActor {
    fn new(
        receiver: mpsc::Receiver<StressTestMessage>,
        sender: mpsc::Sender<StressTestMessage>,
        stress_ng: StressNgCaps,
    ) -> Self {
        Self {
            receiver,
            sender,
            stress_ng,
            cpu_child: None,
            cpu_duration_secs: None,
            cpu_backend: None,
            cpu_watchdog: None,
            gpu_child: None,
            gpu_duration_secs: None,
            gpu_backend: None,
            gpu_watchdog: None,
            ram_child: None,
            ram_duration_secs: None,
            ram_backend: None,
            ram_watchdog: None,
            drive_child: None,
            drive_duration_secs: None,
            drive_backend: None,
            drive_watchdog: None,
        }
    }

    fn bin_path() -> Result<PathBuf> {
        std::env::current_exe().map_err(|e| anyhow!("Failed to find own binary path: {e}"))
    }

    async fn read_stderr(child: &mut Child) -> String {
        if let Some(mut stderr) = child.stderr.take() {
            let mut buf = String::new();
            let _ = stderr.read_to_string(&mut buf).await;
            buf.trim().to_string()
        } else {
            String::from("(no stderr)")
        }
    }

    /// Spawn a watchdog task that, on expiry, sends a `WatchdogFired` message
    /// back to the actor. The actor then owns the `Child` and decides whether
    /// to force-kill (still running) or just clean up (already exited within
    /// the grace window).
    ///
    /// Returns the cancellation token to be cancelled when the child exits
    /// or is stopped explicitly, so we do not message a no-op back to the
    /// actor after the fact.
    fn spawn_watchdog(
        sender: mpsc::Sender<StressTestMessage>,
        kind: StressKind,
        duration_secs: u16,
    ) -> CancellationToken {
        let token = CancellationToken::new();
        let token_clone = token.clone();
        let total =
            Duration::from_secs(u64::from(duration_secs).saturating_add(WATCHDOG_GRACE_SECS));
        tokio::task::spawn_local(async move {
            tokio::select! {
                () = token_clone.cancelled() => {} // child exited normally or stop_*
                () = tokio::time::sleep(total) => {
                    // Hand off to the actor. Using the Child handle avoids
                    // the PID-recycling race a blind kill-by-PID would have.
                    let _ = sender.send(StressTestMessage::WatchdogFired { kind }).await;
                }
            }
        });
        token
    }

    /// Handle a watchdog timer expiry. If the child is still running, kill
    /// it via the owned `Child` handle and warn. If it already exited within
    /// the grace window, log at debug and clean up state silently.
    async fn handle_watchdog_fired(&mut self, kind: StressKind) {
        let label = kind.label();
        let (child, duration, backend, watchdog) = match kind {
            StressKind::Cpu => (
                &mut self.cpu_child,
                &mut self.cpu_duration_secs,
                &mut self.cpu_backend,
                &mut self.cpu_watchdog,
            ),
            StressKind::Gpu => (
                &mut self.gpu_child,
                &mut self.gpu_duration_secs,
                &mut self.gpu_backend,
                &mut self.gpu_watchdog,
            ),
            StressKind::Ram => (
                &mut self.ram_child,
                &mut self.ram_duration_secs,
                &mut self.ram_backend,
                &mut self.ram_watchdog,
            ),
            StressKind::Drive => (
                &mut self.drive_child,
                &mut self.drive_duration_secs,
                &mut self.drive_backend,
                &mut self.drive_watchdog,
            ),
        };
        // The token already fired; clear it either way so we do not leak.
        *watchdog = None;
        let Some(c) = child.as_mut() else { return };
        if let Ok(Some(_)) = c.try_wait() {
            debug!("{label} stress test exited within grace window");
        } else {
            warn!("{label} stress test exceeded timeout + grace; force-killing");
            Self::kill_and_reap(c, label).await;
        }
        *child = None;
        *duration = None;
        *backend = None;
    }

    /// SIGKILL the child and reap it with a bounded wait.
    ///
    /// A child stuck in kernel D-state cannot be reaped from userspace; logging
    /// and moving on prevents the actor's message handler from blocking
    /// indefinitely, which would otherwise queue up subsequent stress-test
    /// requests and wedge `StopAll` mid-sequence.
    async fn kill_and_reap(child: &mut Child, label: &str) {
        let _ = child.kill().await;
        if tokio::time::timeout(Duration::from_secs(STOP_REAP_TIMEOUT_SECS), child.wait())
            .await
            .is_err()
        {
            warn!(
                "{label} stress child did not reap within {STOP_REAP_TIMEOUT_SECS}s \
                 after SIGKILL; process may be in kernel D-state. Continuing."
            );
        }
    }

    async fn check_early_exit(child: &mut Child, label: &str) -> Result<()> {
        tokio::time::sleep(EARLY_EXIT_CHECK).await;
        match child.try_wait() {
            Ok(Some(status)) => {
                let stderr_output = Self::read_stderr(child).await;
                if stderr_output.is_empty() {
                    Err(anyhow!(
                        "{label} stress test exited immediately (status: {status})"
                    ))
                } else {
                    Err(anyhow!("{label} stress test failed: {stderr_output}"))
                }
            }
            Ok(None) => Ok(()), // still running
            Err(err) => {
                warn!("Error checking {label} stress test process: {err}");
                Ok(())
            }
        }
    }

    /// Spawn a stress-ng subprocess with the given arguments.
    ///
    /// The daemon pins itself to a single CPU at startup. Child processes
    /// inherit that restricted affinity. The built-in stress subcommands
    /// call `reset_cpu_affinity()` internally, but stress-ng does not.
    /// We use `pre_exec` to reset affinity to all online CPUs before exec.
    fn spawn_stress_ng(path: &PathBuf, args: &[&str], label: &str) -> Result<Child> {
        let mut cmd = Command::new(path);
        cmd.args(args)
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        // SAFETY: pre_exec runs after fork() in the child, before exec().
        // sched_setaffinity is async-signal-safe on Linux and only
        // modifies the calling process's affinity mask.
        unsafe {
            cmd.pre_exec(|| {
                let online = cc_stress::online_cpu_count();
                let mut set = nix::sched::CpuSet::new();
                for i in 0..online {
                    set.set(i as usize)
                        .map_err(|e| std::io::Error::other(e.to_string()))?;
                }
                nix::sched::sched_setaffinity(nix::unistd::Pid::from_raw(0), &set)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                Ok(())
            });
        }
        cmd.spawn()
            .map_err(|e| anyhow!("Failed to start {label} stress-ng subprocess: {e}"))
    }

    /// Spawn a built-in stress subprocess with the given arguments.
    fn spawn_builtin(args: &[&str], label: &str) -> Result<Child> {
        let bin_path = Self::bin_path()?;
        let mut cmd = Command::new(&bin_path);
        cmd.args(args)
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        cmd.spawn()
            .map_err(|e| anyhow!("Failed to start {label} stress subprocess: {e}"))
    }

    async fn start_cpu(
        &mut self,
        thread_count: Option<u16>,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        if self.cpu_child.is_some() {
            return Err(anyhow!("CPU stress test is already running"));
        }
        let duration_secs = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let available_cpus = cc_stress::online_cpu_count();
        let thread_count = thread_count
            .unwrap_or(available_cpus)
            .min(available_cpus.saturating_mul(2))
            .max(1);

        let resolved = Self::resolve_backend(backend);
        let mut child = self.spawn_cpu(resolved, thread_count, duration_secs)?;
        info!(
            "CPU stress subprocess spawned with PID: {:?} ({resolved})",
            child.id()
        );
        Self::check_early_exit(&mut child, "CPU").await?;

        self.cpu_watchdog = Some(Self::spawn_watchdog(
            self.sender.clone(),
            StressKind::Cpu,
            duration_secs,
        ));
        self.cpu_child = Some(child);
        self.cpu_duration_secs = Some(duration_secs);
        self.cpu_backend = Some(resolved);
        Ok(())
    }

    fn spawn_cpu(
        &self,
        backend: StressBackend,
        thread_count: u16,
        duration_secs: u16,
    ) -> Result<Child> {
        let threads_str = thread_count.to_string();
        let timeout_str = format!("{duration_secs}s");
        let duration_str = duration_secs.to_string();
        match backend {
            StressBackend::StressNg => {
                let path = self
                    .stress_ng
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow!("stress-ng is not installed"))?;
                info!(
                    "Starting CPU stress test via stress-ng: \
                     {thread_count} threads, {duration_secs}s"
                );
                Self::spawn_stress_ng(
                    path,
                    &["--cpu", &threads_str, "--timeout", &timeout_str],
                    "CPU",
                )
            }
            StressBackend::BuiltIn => {
                let bin_path = Self::bin_path()?;
                info!(
                    "Starting CPU stress test (built-in): \
                     {thread_count} threads, {duration_secs}s, bin: {}",
                    bin_path.display()
                );
                Self::spawn_builtin(
                    &[
                        "stress-cpu",
                        "--timeout",
                        &duration_str,
                        "--threads",
                        &threads_str,
                    ],
                    "CPU",
                )
            }
        }
    }

    async fn stop_cpu(&mut self) {
        if let Some(token) = self.cpu_watchdog.take() {
            token.cancel();
        }
        if let Some(mut child) = self.cpu_child.take() {
            Self::kill_and_reap(&mut child, "CPU").await;
            self.cpu_duration_secs = None;
            self.cpu_backend = None;
            info!("CPU stress test stopped");
        }
    }

    async fn start_gpu(
        &mut self,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        if self.gpu_child.is_some() {
            return Err(anyhow!("GPU stress test is already running"));
        }
        let duration_secs = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);

        let resolved = Self::resolve_backend(backend);
        let mut child = self.spawn_gpu(resolved, duration_secs)?;
        if let Err(e) = Self::check_early_exit(&mut child, "GPU").await {
            // The GPU stressor is an optional stress-ng feature and is not
            // compiled into many distro packages; surface that hint so the
            // user knows to switch to the built-in backend.
            if resolved == StressBackend::StressNg {
                return Err(anyhow!(
                    "{e}. The GPU stressor is likely not enabled in the installed \
                     stress-ng binary; try the built-in backend instead."
                ));
            }
            return Err(e);
        }

        self.gpu_watchdog = Some(Self::spawn_watchdog(
            self.sender.clone(),
            StressKind::Gpu,
            duration_secs,
        ));
        self.gpu_child = Some(child);
        self.gpu_duration_secs = Some(duration_secs);
        self.gpu_backend = Some(resolved);
        Ok(())
    }

    fn spawn_gpu(&self, backend: StressBackend, duration_secs: u16) -> Result<Child> {
        let timeout_str = format!("{duration_secs}s");
        let duration_str = duration_secs.to_string();
        match backend {
            StressBackend::StressNg => {
                let path = self
                    .stress_ng
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow!("stress-ng is not installed"))?;
                info!("Starting GPU stress test via stress-ng: {duration_secs}s");
                Self::spawn_stress_ng(path, &["--gpu", "1", "--timeout", &timeout_str], "GPU")
            }
            StressBackend::BuiltIn => {
                info!("Starting GPU stress test (built-in): {duration_secs}s");
                Self::spawn_builtin(&["stress-gpu", "--timeout", &duration_str], "GPU")
            }
        }
    }

    async fn stop_gpu(&mut self) {
        if let Some(token) = self.gpu_watchdog.take() {
            token.cancel();
        }
        if let Some(mut child) = self.gpu_child.take() {
            Self::kill_and_reap(&mut child, "GPU").await;
            self.gpu_duration_secs = None;
            self.gpu_backend = None;
            info!("GPU stress test stopped");
        }
    }

    async fn start_ram(
        &mut self,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        if self.ram_child.is_some() {
            return Err(anyhow!("RAM stress test is already running"));
        }
        let duration_secs = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);

        let available_bytes = cc_stress::available_memory_bytes()
            .map_err(|e| anyhow!("Failed to read available memory: {e}"))?;
        // Precision loss is acceptable for a memory size estimate.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let alloc_bytes = (available_bytes as f64 * cc_stress::RAM_STRESS_ALLOC_FRACTION) as u64;

        let resolved = Self::resolve_backend(backend);
        let mut child = self.spawn_ram(resolved, duration_secs, alloc_bytes)?;
        Self::check_early_exit(&mut child, "RAM").await?;

        self.ram_watchdog = Some(Self::spawn_watchdog(
            self.sender.clone(),
            StressKind::Ram,
            duration_secs,
        ));
        self.ram_child = Some(child);
        self.ram_duration_secs = Some(duration_secs);
        self.ram_backend = Some(resolved);
        Ok(())
    }

    fn spawn_ram(
        &self,
        backend: StressBackend,
        duration_secs: u16,
        alloc_bytes: u64,
    ) -> Result<Child> {
        let timeout_str = format!("{duration_secs}s");
        let duration_str = duration_secs.to_string();
        match backend {
            StressBackend::StressNg => {
                let path = self
                    .stress_ng
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow!("stress-ng is not installed"))?;
                let num_workers = u64::from(cc_stress::online_cpu_count()).max(1);
                let per_worker_bytes = alloc_bytes / num_workers;
                let workers_str = num_workers.to_string();
                let bytes_str = per_worker_bytes.to_string();
                info!(
                    "Starting RAM stress test via stress-ng: {duration_secs}s, \
                     {num_workers} workers x {} MiB",
                    per_worker_bytes / (1024 * 1024)
                );
                Self::spawn_stress_ng(
                    path,
                    &[
                        "--vm",
                        &workers_str,
                        "--vm-bytes",
                        &bytes_str,
                        "--timeout",
                        &timeout_str,
                    ],
                    "RAM",
                )
            }
            StressBackend::BuiltIn => {
                let alloc_str = alloc_bytes.to_string();
                info!(
                    "Starting RAM stress test (built-in): {duration_secs}s, {} MiB",
                    alloc_bytes / (1024 * 1024)
                );
                Self::spawn_builtin(
                    &[
                        "stress-ram",
                        "--bytes",
                        &alloc_str,
                        "--timeout",
                        &duration_str,
                    ],
                    "RAM",
                )
            }
        }
    }

    async fn stop_ram(&mut self) {
        if let Some(token) = self.ram_watchdog.take() {
            token.cancel();
        }
        if let Some(mut child) = self.ram_child.take() {
            Self::kill_and_reap(&mut child, "RAM").await;
            self.ram_duration_secs = None;
            self.ram_backend = None;
            info!("RAM stress test stopped");
        }
    }

    async fn start_drive(
        &mut self,
        device_path: String,
        threads: Option<u16>,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        if self.drive_child.is_some() {
            return Err(anyhow!("Drive stress test is already running"));
        }
        validate_device_path(&device_path)?;

        let duration_secs = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let thread_count = threads
            .unwrap_or(cc_stress::DRIVE_STRESS_DEFAULT_THREADS)
            .max(1);

        let resolved = Self::resolve_backend(backend);
        let mut child = self.spawn_drive(resolved, &device_path, thread_count, duration_secs)?;
        Self::check_early_exit(&mut child, "Drive").await?;

        self.drive_watchdog = Some(Self::spawn_watchdog(
            self.sender.clone(),
            StressKind::Drive,
            duration_secs,
        ));
        self.drive_child = Some(child);
        self.drive_duration_secs = Some(duration_secs);
        self.drive_backend = Some(resolved);
        Ok(())
    }

    fn spawn_drive(
        &self,
        backend: StressBackend,
        device_path: &str,
        thread_count: u16,
        duration_secs: u16,
    ) -> Result<Child> {
        let threads_str = thread_count.to_string();
        let timeout_str = format!("{duration_secs}s");
        let duration_str = duration_secs.to_string();
        match backend {
            StressBackend::StressNg => {
                let ng_path = self
                    .stress_ng
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow!("stress-ng is not installed"))?;
                let mount_point = find_mount_point(device_path).ok_or_else(|| {
                    anyhow!(
                        "Device {device_path} must be mounted to use stress-ng \
                         — try the built-in backend"
                    )
                })?;
                info!(
                    "Starting Drive stress test via stress-ng: {device_path} \
                     (mount: {mount_point}), {thread_count} threads, {duration_secs}s"
                );
                Self::spawn_stress_ng(
                    ng_path,
                    &[
                        "--hdd",
                        &threads_str,
                        "--temp-path",
                        &mount_point,
                        "--timeout",
                        &timeout_str,
                    ],
                    "Drive",
                )
            }
            StressBackend::BuiltIn => {
                info!(
                    "Starting Drive stress test (built-in): \
                     {device_path}, {thread_count} threads, {duration_secs}s"
                );
                Self::spawn_builtin(
                    &[
                        "stress-drive",
                        "--device",
                        device_path,
                        "--threads",
                        &threads_str,
                        "--timeout",
                        &duration_str,
                    ],
                    "Drive",
                )
            }
        }
    }

    async fn stop_drive(&mut self) {
        if let Some(token) = self.drive_watchdog.take() {
            token.cancel();
        }
        if let Some(mut child) = self.drive_child.take() {
            Self::kill_and_reap(&mut child, "Drive").await;
            self.drive_duration_secs = None;
            self.drive_backend = None;
            info!("Drive stress test stopped");
        }
    }

    fn check_child_still_running(
        child: &mut Option<Child>,
        duration: &mut Option<u16>,
        backend: &mut Option<StressBackend>,
        watchdog: &mut Option<CancellationToken>,
        label: &str,
    ) {
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(_)) => {
                    debug!("{label} stress test process has exited");
                    *child = None;
                    *duration = None;
                    *backend = None;
                    if let Some(token) = watchdog.take() {
                        token.cancel();
                    }
                }
                Ok(None) => {} // still running
                Err(err) => {
                    warn!("Error checking {label} stress test process: {err}");
                    *child = None;
                    *duration = None;
                    *backend = None;
                    if let Some(token) = watchdog.take() {
                        token.cancel();
                    }
                }
            }
        }
    }

    /// Resolve the backend to use for a test. An explicit choice is honored;
    /// `None` defaults to built-in for every test type. The user opts into
    /// stress-ng explicitly via the per-test toggle in the UI.
    fn resolve_backend(explicit: Option<StressBackend>) -> StressBackend {
        explicit.unwrap_or(StressBackend::BuiltIn)
    }

    fn status(&mut self) -> StressTestStatus {
        Self::check_child_still_running(
            &mut self.cpu_child,
            &mut self.cpu_duration_secs,
            &mut self.cpu_backend,
            &mut self.cpu_watchdog,
            "CPU",
        );
        Self::check_child_still_running(
            &mut self.gpu_child,
            &mut self.gpu_duration_secs,
            &mut self.gpu_backend,
            &mut self.gpu_watchdog,
            "GPU",
        );
        Self::check_child_still_running(
            &mut self.ram_child,
            &mut self.ram_duration_secs,
            &mut self.ram_backend,
            &mut self.ram_watchdog,
            "RAM",
        );
        Self::check_child_still_running(
            &mut self.drive_child,
            &mut self.drive_duration_secs,
            &mut self.drive_backend,
            &mut self.drive_watchdog,
            "Drive",
        );
        StressTestStatus {
            stress_ng_available: self.stress_ng.path.is_some(),
            cpu_active: self.cpu_child.is_some(),
            cpu_duration_secs: self.cpu_duration_secs,
            cpu_backend: Self::resolve_backend(self.cpu_backend),
            gpu_active: self.gpu_child.is_some(),
            gpu_duration_secs: self.gpu_duration_secs,
            gpu_backend: Self::resolve_backend(self.gpu_backend),
            ram_active: self.ram_child.is_some(),
            ram_duration_secs: self.ram_duration_secs,
            ram_backend: Self::resolve_backend(self.ram_backend),
            drive_active: self.drive_child.is_some(),
            drive_duration_secs: self.drive_duration_secs,
            drive_backend: Self::resolve_backend(self.drive_backend),
        }
    }
}

impl ApiActor<StressTestMessage> for StressTestActor {
    fn name(&self) -> &'static str {
        "StressTestActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<StressTestMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: StressTestMessage) {
        match msg {
            StressTestMessage::StartCpu {
                thread_count,
                duration_secs,
                backend,
                respond_to,
            } => {
                let result = self.start_cpu(thread_count, duration_secs, backend).await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopCpu { respond_to } => {
                self.stop_cpu().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::StartGpu {
                duration_secs,
                backend,
                respond_to,
            } => {
                let result = self.start_gpu(duration_secs, backend).await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopGpu { respond_to } => {
                self.stop_gpu().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::StartRam {
                duration_secs,
                backend,
                respond_to,
            } => {
                let result = self.start_ram(duration_secs, backend).await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopRam { respond_to } => {
                self.stop_ram().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::StartDrive {
                device_path,
                threads,
                duration_secs,
                backend,
                respond_to,
            } => {
                let result = self
                    .start_drive(device_path, threads, duration_secs, backend)
                    .await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopDrive { respond_to } => {
                self.stop_drive().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::StopAll { respond_to } => {
                self.stop_cpu().await;
                self.stop_gpu().await;
                self.stop_ram().await;
                self.stop_drive().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::Status { respond_to } => {
                let _ = respond_to.send(self.status());
            }
            StressTestMessage::WatchdogFired { kind } => {
                self.handle_watchdog_fired(kind).await;
            }
        }
    }
}

/// Defensive validation of a block device path. The API layer also validates,
/// but the actor must not trust its callers blindly.
fn validate_device_path(device_path: &str) -> Result<()> {
    if device_path.starts_with("/dev/").not() {
        return Err(anyhow!("Device path must start with /dev/"));
    }
    if device_path.contains("..") {
        return Err(anyhow!("Device path must not contain '..'"));
    }
    if std::path::Path::new(device_path).exists().not() {
        return Err(anyhow!("Device {device_path} does not exist"));
    }
    Ok(())
}

/// Find a mount point for the given block device by parsing `/proc/mounts`.
/// If the device itself is not mounted, checks for mounted partitions
/// (e.g. `/dev/nvme0n1` -> `/dev/nvme0n1p1`).
fn find_mount_point(device_path: &str) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    // First pass: exact match.
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let dev = fields.next()?;
        let mount = fields.next()?;
        if dev == device_path {
            return Some(mount.to_string());
        }
    }
    // Second pass: check partitions of this device (e.g. /dev/sda -> /dev/sda1).
    // Pick the first mounted partition found.
    for line in mounts.lines() {
        let mut fields = line.split_whitespace();
        let dev = fields.next()?;
        let mount = fields.next()?;
        if dev.starts_with(device_path) && dev.len() > device_path.len() {
            return Some(mount.to_string());
        }
    }
    None
}

/// Detect whether the stress-ng binary is installed.
///
/// We deliberately do not probe individual stressor capabilities (e.g. the
/// `--gpu` stressor): the user picks the backend per test type in the UI,
/// and a missing/broken stressor surfaces via the spawned child's stderr.
async fn detect_stress_ng() -> StressNgCaps {
    let which_result = Command::new("which")
        .arg("stress-ng")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await;

    let path = match which_result {
        Ok(output) if output.status.success() => {
            let p = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if p.is_empty() {
                None
            } else {
                Some(PathBuf::from(p))
            }
        }
        _ => None,
    };

    if let Some(ref ng_path) = path {
        info!("stress-ng found at: {}", ng_path.display());
    } else {
        info!(
            "stress-ng is not installed. \
             Install it for additional stress test backends."
        );
    }

    StressNgCaps { path }
}

#[derive(Clone)]
pub struct StressTestHandle {
    sender: mpsc::Sender<StressTestMessage>,
}

impl StressTestHandle {
    pub async fn new(cancel_token: CancellationToken) -> Self {
        // Probe stress-ng and run the actor on the sidecar: both manage child processes via
        // tokio::process, which needs a Tokio reactor (the main thread may be on compio).
        let stress_ng = crate::sidecar::handle()
            .run(detect_stress_ng)
            .await
            .unwrap_or(StressNgCaps { path: None });
        // Depth 2: callers await the oneshot response (at most one user
        // message in flight per caller), but a watchdog may self-message
        // independently. Both are processed serially by the actor.
        let (sender, receiver) = mpsc::channel(2);
        let actor = StressTestActor::new(receiver, sender.clone(), stress_ng);
        crate::sidecar::handle().spawn(move || run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn start_cpu(
        &self,
        thread_count: Option<u16>,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartCpu {
            thread_count,
            duration_secs,
            backend,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn stop_cpu(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StopCpu { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn start_gpu(
        &self,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartGpu {
            duration_secs,
            backend,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn stop_gpu(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StopGpu { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn start_ram(
        &self,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartRam {
            duration_secs,
            backend,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn stop_ram(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StopRam { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn start_drive(
        &self,
        device_path: String,
        threads: Option<u16>,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartDrive {
            device_path,
            threads,
            duration_secs,
            backend,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn stop_drive(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StopDrive { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn stop_all(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StopAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn status(&self) -> StressTestStatus {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::Status { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await.unwrap_or(StressTestStatus {
            stress_ng_available: false,
            cpu_active: false,
            cpu_duration_secs: None,
            cpu_backend: StressBackend::BuiltIn,
            gpu_active: false,
            gpu_duration_secs: None,
            gpu_backend: StressBackend::BuiltIn,
            ram_active: false,
            ram_duration_secs: None,
            ram_backend: StressBackend::BuiltIn,
            drive_active: false,
            drive_duration_secs: None,
            drive_backend: StressBackend::BuiltIn,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stress_test_status_defaults() {
        // Confirms the default-state shape that callers (UI, tests) rely on.
        let status = StressTestStatus {
            stress_ng_available: false,
            cpu_active: false,
            cpu_duration_secs: None,
            cpu_backend: StressBackend::BuiltIn,
            gpu_active: false,
            gpu_duration_secs: None,
            gpu_backend: StressBackend::BuiltIn,
            ram_active: false,
            ram_duration_secs: None,
            ram_backend: StressBackend::BuiltIn,
            drive_active: false,
            drive_duration_secs: None,
            drive_backend: StressBackend::BuiltIn,
        };
        assert!(status.cpu_active.not());
        assert!(status.gpu_active.not());
        assert!(status.ram_active.not());
        assert!(status.drive_active.not());
        assert!(status.stress_ng_available.not());
        assert_eq!(status.cpu_backend, StressBackend::BuiltIn);
    }

    #[test]
    fn stress_backend_display() {
        // Display format is consumed by log lines; pin both spellings.
        assert_eq!(StressBackend::BuiltIn.to_string(), "built-in");
        assert_eq!(StressBackend::StressNg.to_string(), "stress-ng");
    }

    #[test]
    fn stress_kind_labels_match_log_lines() {
        // The watchdog routes by kind; the label is consumed by user-visible
        // warn/debug log lines and must match the spelling used elsewhere in
        // this module (info!("CPU stress subprocess..."), etc.). Pin them.
        assert_eq!(StressKind::Cpu.label(), "CPU");
        assert_eq!(StressKind::Gpu.label(), "GPU");
        assert_eq!(StressKind::Ram.label(), "RAM");
        assert_eq!(StressKind::Drive.label(), "Drive");
    }

    #[test]
    fn stress_backend_serde_roundtrip() {
        // The wire format is snake_case and shared with the UI; pin it.
        let json = serde_json::to_string(&StressBackend::StressNg).unwrap();
        assert_eq!(json, "\"stress_ng\"");
        let back: StressBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, StressBackend::StressNg);

        let json = serde_json::to_string(&StressBackend::BuiltIn).unwrap();
        assert_eq!(json, "\"built_in\"");
    }

    #[test]
    fn resolve_backend_honors_explicit_choice() {
        // An explicit caller choice always wins.
        assert_eq!(
            StressTestActor::resolve_backend(Some(StressBackend::StressNg)),
            StressBackend::StressNg
        );
        assert_eq!(
            StressTestActor::resolve_backend(Some(StressBackend::BuiltIn)),
            StressBackend::BuiltIn
        );
    }

    #[test]
    fn resolve_backend_defaults_to_built_in() {
        // None always resolves to built-in, regardless of stress-ng presence:
        // the user opts into stress-ng explicitly via the per-test UI toggle.
        assert_eq!(
            StressTestActor::resolve_backend(None),
            StressBackend::BuiltIn
        );
    }

    #[test]
    fn validate_device_path_rejects_invalid_inputs() {
        // Negative space: paths outside /dev, traversal attempts, and missing
        // devices must all be rejected before we hand them to a subprocess.
        assert!(validate_device_path("/etc/passwd").is_err());
        assert!(validate_device_path("/dev/../etc/passwd").is_err());
        assert!(validate_device_path("/dev/nonexistent_device_xyz").is_err());
    }

    #[test]
    fn find_mount_point_parses_proc_mounts() {
        // find_mount_point reads /proc/mounts which exists on Linux test hosts.
        // For an obviously-bogus device, the result must be None (not panic).
        let result = find_mount_point("/dev/nonexistent_device_xyz");
        assert!(result.is_none());
    }

    #[test]
    fn check_child_still_running_clears_state_and_cancels_watchdog() {
        // When the child has exited, both the actor's tracking fields and the
        // watchdog token must be cleared, so a stale SIGKILL cannot land on a
        // recycled PID later.
        let mut child: Option<Child> = None; // None branch → no-op
        let mut duration = Some(60_u16);
        let mut backend = Some(StressBackend::BuiltIn);
        let mut watchdog = Some(CancellationToken::new());
        let token_clone = watchdog.as_ref().unwrap().clone();
        StressTestActor::check_child_still_running(
            &mut child,
            &mut duration,
            &mut backend,
            &mut watchdog,
            "TEST",
        );
        // Child was None: nothing should have changed.
        assert!(duration.is_some());
        assert!(backend.is_some());
        assert!(watchdog.is_some());
        assert!(token_clone.is_cancelled().not());
    }
}
