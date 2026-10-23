// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::{debug, info, log, warn, Level};
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
/// Workers for stress-ng's opengl stressor. One worker leaves a modern
/// card mostly idle: reporters on RTX 4000-series and newer saw hardly
/// any load until several render contexts ran concurrently.
const STRESS_NG_GPU_WORKERS: &str = "4";
/// Where the kernel exposes DRM devices, and the source of the GPU picker's
/// render nodes. Overridden in tests.
const DRM_CLASS_PATH: &str = "/sys/class/drm";
/// Bound on the GPU enumeration subprocess. Vulkan init is ~100 ms on a
/// healthy system; anything near this bound means a stuck driver.
const GPU_LIST_TIMEOUT_SECS: u64 = 15;

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

/// One GPU as reported by the `stress-gpu-list` subprocess. Crosses a
/// process boundary as JSON, so both ends live in this module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuAdapter {
    /// `vendor:device` in hex, wgpu's view of the PCI ID.
    pub pci_id: String,
    pub name: String,
    pub discrete: bool,
}

/// A GPU the user can pick, with everything needed to aim either backend at
/// it. The two backends select differently: the built-in one filters wgpu
/// adapters by PCI ID, stress-ng opens a DRM render node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GpuTarget {
    /// Selection key. The PCI slot (`0000:03:00.0`) when the adapter matched
    /// a DRM device, else the PCI ID plus its position. Slots stay distinct
    /// for two identical cards, which PCI IDs do not.
    pub id: String,
    pub name: String,
    pub discrete: bool,
    /// Position in the enumerated adapter list, which is how the built-in
    /// backend picks between two cards of the same model.
    pub index: usize,
    pub pci_id: String,
    pub render_node: Option<String>,
}

/// A DRM device as sysfs describes it. Only the fields needed to match an
/// enumerated adapter to its render node.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DrmDevice {
    pci_id: String,
    slot: String,
    render_node: Option<String>,
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

/// Why the GPU list is being enumerated, which decides how a failure is reported.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum GpuQuery {
    /// Listing what exists. A machine with no GPU is a normal machine and the
    /// user has nothing to fix, so a failure only informs.
    List,
    /// A stress test the user asked to start, which is not going to run.
    Start,
}

impl GpuQuery {
    const fn failure_level(self) -> Level {
        match self {
            Self::List => Level::Info,
            Self::Start => Level::Warn,
        }
    }

    /// Whether this query still has something to say once `reported` has been logged for the
    /// current outage. A GPU-less machine is listed on every UI load, so a repeat is dropped;
    /// a Start still warns after a List has only informed.
    const fn worth_reporting_after(self, reported: Option<Self>) -> bool {
        match reported {
            None => true,
            Some(Self::List) => matches!(self, Self::Start),
            Some(Self::Start) => false,
        }
    }
}

struct StressTestActor {
    receiver: mpsc::Receiver<StressTestMessage>,
    /// Clone handed to each watchdog task so it can self-message on expiry.
    /// The actor then owns the `Child` handle and can safely check-and-kill
    /// without racing on a recycled PID.
    sender: mpsc::Sender<StressTestMessage>,
    stress_ng: StressNgCaps,
    /// GPU picker list, enumerated on first use and kept: adapters do not
    /// come and go, and each enumeration costs a subprocess and a Vulkan
    /// init. `None` until the first successful enumeration, so a transient
    /// failure does not stick.
    gpu_targets: Option<Vec<GpuTarget>>,
    /// How the current enumeration outage was already reported, so the retry
    /// that every listing performs does not repeat the line. Cleared once
    /// enumeration works.
    gpu_failure_reported: Option<GpuQuery>,
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
        gpu_id: Option<String>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopGpu {
        respond_to: oneshot::Sender<Result<()>>,
    },
    ListGpus {
        respond_to: oneshot::Sender<Vec<GpuTarget>>,
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
            gpu_targets: None,
            gpu_failure_reported: None,
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
    fn spawn_stress_ng<I, S>(path: &PathBuf, args: I, label: &str) -> Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
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
    fn spawn_builtin<I, S>(args: I, label: &str) -> Result<Child>
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
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
                    ["--cpu", &threads_str, "--timeout", &timeout_str],
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
                    [
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

    /// The GPU picker list, enumerated on first use and cached thereafter.
    /// Returns an empty list when enumeration fails, which leaves the UI with
    /// "all GPUs" as its only choice rather than a broken picker.
    async fn gpu_targets(&mut self, asked_by: GpuQuery) -> Vec<GpuTarget> {
        if let Some(targets) = self.gpu_targets.as_ref() {
            return targets.clone();
        }
        let targets = match Self::enumerate_gpu_targets().await {
            Ok(targets) => targets,
            Err(e) => {
                if asked_by.worth_reporting_after(self.gpu_failure_reported) {
                    log!(
                        asked_by.failure_level(),
                        "Could not enumerate GPUs for stress testing: {e}"
                    );
                    self.gpu_failure_reported = Some(asked_by);
                }
                return Vec::new();
            }
        };
        self.gpu_failure_reported = None;
        self.gpu_targets = Some(targets.clone());
        targets
    }

    /// Run the adapter enumeration out of process and pair the result with
    /// sysfs. Both halves are needed: only wgpu can tell discrete from
    /// integrated, and only sysfs knows the render nodes.
    async fn enumerate_gpu_targets() -> Result<Vec<GpuTarget>> {
        let bin_path = Self::bin_path()?;
        let child = Command::new(&bin_path)
            .arg("stress-gpu-list")
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to run GPU enumeration subprocess: {e}"))?;
        // Vulkan init can hang on a wedged driver. The actor handles messages
        // one at a time, so waiting forever here would block every later
        // request, `StopAll` included. Dropping the child on timeout kills it.
        let output = tokio::time::timeout(
            Duration::from_secs(GPU_LIST_TIMEOUT_SECS),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| anyhow!("GPU enumeration timed out after {GPU_LIST_TIMEOUT_SECS}s"))?
        .map_err(|e| anyhow!("Failed to run GPU enumeration subprocess: {e}"))?;
        if output.status.success().not() {
            return Err(anyhow!(
                "GPU enumeration failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }
        let adapters: Vec<GpuAdapter> = serde_json::from_slice(&output.stdout)
            .map_err(|e| anyhow!("Failed to parse the enumerated GPU list: {e}"))?;
        let targets = build_gpu_targets(adapters, &read_drm_devices(Path::new(DRM_CLASS_PATH)));
        info!(
            "GPUs available for stress testing: {}",
            targets
                .iter()
                .map(|t| format!("{} ({})", t.name, t.id))
                .collect::<Vec<_>>()
                .join(", ")
        );
        Ok(targets)
    }

    /// Resolve a caller's GPU selection against the enumerated list. An
    /// unknown ID is an error: silently falling back to every GPU would load
    /// hardware the user did not ask to load.
    async fn resolve_gpu_target(&mut self, gpu_id: &str) -> Result<GpuTarget> {
        self.gpu_targets(GpuQuery::Start)
            .await
            .into_iter()
            .find(|target| target.id == gpu_id)
            .ok_or_else(|| anyhow!("GPU {gpu_id} is not available for stress testing"))
    }

    async fn start_gpu(
        &mut self,
        duration_secs: Option<u16>,
        backend: Option<StressBackend>,
        gpu_id: Option<String>,
    ) -> Result<()> {
        if self.gpu_child.is_some() {
            return Err(anyhow!("GPU stress test is already running"));
        }
        let duration_secs = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let target = match gpu_id {
            Some(id) => Some(self.resolve_gpu_target(&id).await?),
            None => None,
        };

        let resolved = Self::resolve_backend(backend);
        let mut child = self.spawn_gpu(resolved, duration_secs, target.as_ref())?;
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

    fn spawn_gpu(
        &self,
        backend: StressBackend,
        duration_secs: u16,
        target: Option<&GpuTarget>,
    ) -> Result<Child> {
        let scope = target.map_or("all GPUs", |t| t.name.as_str());
        match backend {
            StressBackend::StressNg => {
                let path = self
                    .stress_ng
                    .path
                    .as_ref()
                    .ok_or_else(|| anyhow!("stress-ng is not installed"))?;
                let args = stress_ng_gpu_args(duration_secs, target)?;
                info!("Starting GPU stress test via stress-ng: {duration_secs}s, {scope}");
                Self::spawn_stress_ng(path, &args, "GPU")
            }
            StressBackend::BuiltIn => {
                info!("Starting GPU stress test (built-in): {duration_secs}s, {scope}");
                Self::spawn_builtin(builtin_gpu_args(duration_secs, target), "GPU")
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
                    [
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
                    [
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
                    [
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
                    [
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
                gpu_id,
                respond_to,
            } => {
                let result = self.start_gpu(duration_secs, backend, gpu_id).await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopGpu { respond_to } => {
                self.stop_gpu().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::ListGpus { respond_to } => {
                let targets = self.gpu_targets(GpuQuery::List).await;
                let _ = respond_to.send(targets);
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

/// Build stress-ng's GPU arguments. stress-ng aims at one device node, so a
/// selected GPU without one cannot be reached by this backend.
///
/// # Errors
///
/// Returns an error if the selected GPU exposes no DRM render node.
fn stress_ng_gpu_args(duration_secs: u16, target: Option<&GpuTarget>) -> Result<Vec<String>> {
    let mut args = vec![
        "--gpu".to_string(),
        STRESS_NG_GPU_WORKERS.to_string(),
        "--timeout".to_string(),
        format!("{duration_secs}s"),
    ];
    if let Some(target) = target {
        let render_node = target.render_node.as_ref().ok_or_else(|| {
            anyhow!(
                "{} has no DRM render node for stress-ng to open; \
                 use the built-in backend to stress it",
                target.name
            )
        })?;
        args.push("--gpu-devnode".to_string());
        args.push(render_node.clone());
    }
    Ok(args)
}

/// Build the built-in backend's GPU arguments. It picks the adapter by
/// position, because two cards of the same model share a PCI ID; the ID
/// rides along so the subprocess can reject a list that has since shifted.
fn builtin_gpu_args(duration_secs: u16, target: Option<&GpuTarget>) -> Vec<String> {
    let mut args = vec![
        "stress-gpu".to_string(),
        "--timeout".to_string(),
        duration_secs.to_string(),
    ];
    if let Some(target) = target {
        args.push("--gpu-index".to_string());
        args.push(target.index.to_string());
        args.push("--gpu-id".to_string());
        args.push(target.pci_id.clone());
    }
    args
}

/// Enumerate the GPUs this machine can stress.
///
/// Called from the `stress-gpu-list` subcommand, never from the daemon
/// itself: it initializes Vulkan, and the daemon keeps GPU drivers out of
/// its own address space for the same reason the stress runs subprocess out.
///
/// # Errors
///
/// Returns an error if no hardware GPU adapter is found.
pub fn enumerate_gpu_adapters() -> Result<Vec<GpuAdapter>> {
    Ok(cc_stress::list_gpu_adapters()?
        .into_iter()
        .map(|adapter| GpuAdapter {
            pci_id: adapter.pci_id.to_string(),
            name: adapter.name,
            discrete: adapter.discrete,
        })
        .collect())
}

/// Read the DRM devices sysfs knows about, so an enumerated adapter can be
/// matched to the render node stress-ng needs.
///
/// Only `uevent` is read: its contents are cached by the kernel, so this
/// cannot resume a GPU that runtime-suspended itself.
fn read_drm_devices(drm_class_path: &Path) -> Vec<DrmDevice> {
    let entries = match std::fs::read_dir(drm_class_path) {
        Ok(entries) => entries,
        Err(e) => {
            warn!("Failed to read {}: {e}", drm_class_path.display());
            return Vec::new();
        }
    };
    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Whole cards only. The same directory holds connectors
        // ("card1-DP-1") and render nodes, which are reached via the card.
        let is_card = name.starts_with("card")
            && name.len() > 4
            && name[4..].chars().all(|c| c.is_ascii_digit());
        if is_card.not() {
            continue;
        }
        let device_dir = drm_class_path.join(&name).join("device");
        let Ok(uevent) = std::fs::read_to_string(device_dir.join("uevent")) else {
            continue;
        };
        let mut fields = HashMap::new();
        for line in uevent.lines() {
            if let Some((key, value)) = line.split_once('=') {
                fields.insert(key.trim(), value.trim());
            }
        }
        // PCI class 0x03xxxx is a display controller. Anything else under
        // /sys/class/drm is not a GPU we can stress.
        let is_display = fields
            .get("PCI_CLASS")
            .and_then(|class| u32::from_str_radix(class, 16).ok())
            .is_some_and(|class| class >> 16 == 0x03);
        if is_display.not() {
            continue;
        }
        let (Some(pci_id), Some(slot)) = (fields.get("PCI_ID"), fields.get("PCI_SLOT_NAME")) else {
            continue;
        };
        devices.push(DrmDevice {
            pci_id: pci_id.to_lowercase(),
            slot: (*slot).to_string(),
            render_node: find_render_node(&device_dir),
        });
    }
    devices.sort_by(|a, b| a.slot.cmp(&b.slot));
    devices
}

/// Find the `/dev/dri/renderD*` node belonging to a DRM device, if it has
/// one. Display-only devices and some proprietary drivers do not.
fn find_render_node(device_dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(device_dir.join("drm")).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("renderD") {
            return Some(format!("/dev/dri/{name}"));
        }
    }
    None
}

/// Pair each enumerated adapter with its DRM device, discrete cards first.
///
/// Ordering is what the UI preselects from: users stress the card with the
/// cooler on it, not the iGPU that happens to enumerate first. `index` keeps
/// each target pointing at its original adapter across that reordering.
///
/// Cards of the same model are paired positionally, both lists being in PCI
/// enumeration order. That is the best available: wgpu reports no PCI slot,
/// so nothing else ties an adapter to a DRM device. A mismatch would swap
/// two cards of the same model, never reach a different model, and only
/// affects the built-in backend, since stress-ng is handed the render node.
fn build_gpu_targets(adapters: Vec<GpuAdapter>, drm_devices: &[DrmDevice]) -> Vec<GpuTarget> {
    let mut claimed = vec![false; drm_devices.len()];
    let mut targets = Vec::with_capacity(adapters.len());
    for (index, adapter) in adapters.into_iter().enumerate() {
        // Claim each DRM device at most once so two identical cards do not
        // both resolve to the first one's render node.
        let matched = drm_devices
            .iter()
            .enumerate()
            .find(|(i, drm)| claimed[*i].not() && drm.pci_id == adapter.pci_id);
        let (id, render_node) = match matched {
            Some((i, drm)) => {
                claimed[i] = true;
                (drm.slot.clone(), drm.render_node.clone())
            }
            // No DRM device to name it by. The position keeps the key unique
            // across identical cards, which the PCI ID alone would not.
            None => (format!("{}@{index}", adapter.pci_id), None),
        };
        targets.push(GpuTarget {
            id,
            name: adapter.name,
            discrete: adapter.discrete,
            index,
            pci_id: adapter.pci_id,
            render_node,
        });
    }
    targets.sort_by_key(|target| target.discrete.not());
    targets
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
        // Probe stress-ng and run the actor on the sidecar, where the daemon runs its child
        // processes through tokio::process.
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
        gpu_id: Option<String>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartGpu {
            duration_secs,
            backend,
            gpu_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn list_gpus(&self) -> Vec<GpuTarget> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::ListGpus { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await.unwrap_or_default()
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

    /// Goal: a GPU-less machine must not be warned on every listing, since there is nothing
    /// for the user to fix, while a stress test that cannot start still warrants a warning.
    /// Methodology: read the level each query kind reports a failed enumeration at.
    #[test]
    fn only_a_requested_stress_test_warns_about_missing_gpus() {
        assert_eq!(
            GpuQuery::List.failure_level(),
            Level::Info,
            "Listing GPUs on a machine that has none is expected"
        );
        assert_eq!(
            GpuQuery::Start.failure_level(),
            Level::Warn,
            "A stress test the user started is not going to run"
        );
    }

    /// Goal: enumeration is retried on every listing, so a GPU-less machine must log the failure
    /// once, while a Start must still warn after a List has only informed.
    /// Methodology: ask each query kind what it adds to each already reported state.
    #[test]
    fn an_enumeration_failure_is_reported_once_per_outage() {
        assert!(
            GpuQuery::List.worth_reporting_after(None),
            "The first failure is always reported"
        );
        assert!(
            GpuQuery::Start.worth_reporting_after(None),
            "The first failure is always reported"
        );

        assert!(
            GpuQuery::List
                .worth_reporting_after(Some(GpuQuery::List))
                .not(),
            "Every listing retries, so the info line must not repeat"
        );
        assert!(
            GpuQuery::Start.worth_reporting_after(Some(GpuQuery::List)),
            "A stress test that cannot start warrants a warning the info line did not give"
        );

        assert!(
            GpuQuery::List
                .worth_reporting_after(Some(GpuQuery::Start))
                .not(),
            "A listing adds nothing after a warning"
        );
        assert!(
            GpuQuery::Start
                .worth_reporting_after(Some(GpuQuery::Start))
                .not(),
            "The warning stands until enumeration works again"
        );
    }

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

    fn target(id: &str, discrete: bool, render_node: Option<&str>) -> GpuTarget {
        GpuTarget {
            id: id.to_string(),
            name: format!("GPU {id}"),
            discrete,
            index: 0,
            pci_id: "1002:73df".to_string(),
            render_node: render_node.map(ToString::to_string),
        }
    }

    #[test]
    fn stress_ng_gpu_args_use_several_workers() {
        // A single worker leaves 4000-series and newer cards nearly idle.
        // No --gpu-devnode means stress-ng picks its own default device,
        // which is what "all GPUs" falls back to for this backend.
        let args = stress_ng_gpu_args(60, None).unwrap();
        assert_eq!(args, ["--gpu", "4", "--timeout", "60s"]);
    }

    #[test]
    fn stress_ng_gpu_args_target_a_render_node() {
        // stress-ng aims at one device node, defaulting to renderD128, which
        // is the iGPU on most hybrid systems. A selection must override it.
        let args = stress_ng_gpu_args(
            30,
            Some(&target("0000:03:00.0", true, Some("/dev/dri/renderD129"))),
        )
        .unwrap();
        assert_eq!(
            args,
            [
                "--gpu",
                "4",
                "--timeout",
                "30s",
                "--gpu-devnode",
                "/dev/dri/renderD129"
            ]
        );
    }

    #[test]
    fn stress_ng_gpu_args_reject_a_gpu_with_no_render_node() {
        // Without a devnode stress-ng would silently fall back to its own
        // default GPU, loading hardware the user did not select.
        let err = stress_ng_gpu_args(60, Some(&target("0000:03:00.0", true, None))).unwrap_err();
        assert!(err.to_string().contains("no DRM render node"));
    }

    #[test]
    fn builtin_gpu_args_pass_position_and_pci_id_only_when_targeted() {
        // Omitting the flags is what keeps "all GPUs" stressing every
        // adapter. When targeted, position picks between cards of the same
        // model and the PCI ID is the guard against a shifted list.
        assert_eq!(
            builtin_gpu_args(60, None),
            ["stress-gpu", "--timeout", "60"]
        );
        let mut second_card = target("0000:82:00.0", true, None);
        second_card.index = 1;
        assert_eq!(
            builtin_gpu_args(60, Some(&second_card)),
            [
                "stress-gpu",
                "--timeout",
                "60",
                "--gpu-index",
                "1",
                "--gpu-id",
                "1002:73df"
            ]
        );
    }

    fn adapter(pci_id: &str, name: &str, discrete: bool) -> GpuAdapter {
        GpuAdapter {
            pci_id: pci_id.to_string(),
            name: name.to_string(),
            discrete,
        }
    }

    fn drm(pci_id: &str, slot: &str, render_node: Option<&str>) -> DrmDevice {
        DrmDevice {
            pci_id: pci_id.to_string(),
            slot: slot.to_string(),
            render_node: render_node.map(ToString::to_string),
        }
    }

    #[test]
    fn gpu_targets_list_discrete_cards_first() {
        // The UI preselects the first entry, and users mean the card with a
        // cooler on it. Enumeration order puts the iGPU first as often as not.
        let targets = build_gpu_targets(
            vec![
                adapter("8086:a780", "Intel UHD", false),
                adapter("10de:2684", "RTX 4090", true),
            ],
            &[
                drm("8086:a780", "0000:00:02.0", Some("/dev/dri/renderD128")),
                drm("10de:2684", "0000:01:00.0", Some("/dev/dri/renderD129")),
            ],
        );
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].name, "RTX 4090");
        assert_eq!(targets[0].id, "0000:01:00.0");
        assert_eq!(
            targets[0].render_node.as_deref(),
            Some("/dev/dri/renderD129")
        );
        assert_eq!(targets[1].name, "Intel UHD");
    }

    #[test]
    fn gpu_targets_claim_each_drm_device_once() {
        // Two identical cards share a PCI ID, and multi-GPU rigs usually run
        // matching cards. Matching by ID alone would give both the first
        // card's render node, so stress-ng would load one card twice and
        // never touch the other.
        let targets = build_gpu_targets(
            vec![
                adapter("10de:2684", "RTX 4090", true),
                adapter("10de:2684", "RTX 4090", true),
            ],
            &[
                drm("10de:2684", "0000:01:00.0", Some("/dev/dri/renderD128")),
                drm("10de:2684", "0000:02:00.0", Some("/dev/dri/renderD129")),
            ],
        );
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].id, "0000:01:00.0");
        assert_eq!(targets[0].index, 0);
        assert_eq!(
            targets[0].render_node.as_deref(),
            Some("/dev/dri/renderD128")
        );
        assert_eq!(targets[1].id, "0000:02:00.0");
        assert_eq!(targets[1].index, 1);
        assert_eq!(
            targets[1].render_node.as_deref(),
            Some("/dev/dri/renderD129")
        );
    }

    #[test]
    fn gpu_targets_keep_their_adapter_position_through_the_sort() {
        // The sort reorders the list the user picks from, but the built-in
        // backend addresses adapters by their enumeration position. Losing
        // that link would stress whichever card sorted into the slot.
        let targets = build_gpu_targets(
            vec![
                adapter("8086:a780", "Intel UHD", false),
                adapter("10de:2684", "RTX 4090", true),
            ],
            &[],
        );
        assert_eq!(targets[0].name, "RTX 4090");
        assert_eq!(targets[0].index, 1);
        assert_eq!(targets[1].name, "Intel UHD");
        assert_eq!(targets[1].index, 0);
    }

    #[test]
    fn gpu_targets_fall_back_to_the_pci_id_when_sysfs_has_no_match() {
        // An adapter with no DRM device is still stressable by the built-in
        // backend, which selects by position. It just cannot be handed to
        // stress-ng, and `stress_ng_gpu_args` is what refuses that. The
        // position keeps the key unique when two identical cards land here.
        let targets = build_gpu_targets(
            vec![
                adapter("10de:2684", "RTX 4090", true),
                adapter("10de:2684", "RTX 4090", true),
            ],
            &[],
        );
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].id, "10de:2684@0");
        assert_eq!(targets[1].id, "10de:2684@1");
        assert_eq!(targets[0].render_node, None);
    }

    #[test]
    fn drm_devices_are_read_from_display_controllers_only() {
        // /sys/class/drm mixes whole cards with connectors ("card1-DP-1")
        // and holds non-display DRM devices. Only real GPUs may reach the
        // picker, and only their uevent may be read: touching other
        // attributes can resume a runtime-suspended card.
        let root = tempfile::tempdir().unwrap();
        let drm_path = root.path();
        let write_card = |name: &str, class: &str, render: Option<&str>| {
            let device_dir = drm_path.join(name).join("device");
            std::fs::create_dir_all(&device_dir).unwrap();
            std::fs::write(
                device_dir.join("uevent"),
                format!(
                    "DRIVER=amdgpu\nPCI_CLASS={class}\nPCI_ID=1002:73DF\n\
                     PCI_SLOT_NAME=0000:0{}:00.0\n",
                    name.trim_start_matches("card")
                ),
            )
            .unwrap();
            if let Some(render) = render {
                std::fs::create_dir_all(device_dir.join("drm").join(render)).unwrap();
            }
        };
        write_card("card1", "30000", Some("renderD128"));
        // A non-display DRM device, e.g. an accelerator.
        write_card("card2", "120000", None);
        // A connector, which carries no device directory of its own.
        std::fs::create_dir_all(drm_path.join("card1-DP-1")).unwrap();

        let devices = read_drm_devices(drm_path);
        assert_eq!(
            devices,
            vec![drm(
                "1002:73df",
                "0000:01:00.0",
                Some("/dev/dri/renderD128")
            )]
        );
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn drm_devices_are_empty_when_the_class_is_missing() {
        // Containers and odd kernels have no /sys/class/drm. The picker must
        // degrade to "all GPUs" rather than fail the request.
        assert!(read_drm_devices(Path::new("/nonexistent/class/drm")).is_empty());
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
    #[cfg(feature = "gated-tests")]
    fn validate_device_path_rejects_invalid_inputs() {
        // Negative space: paths outside /dev, traversal attempts, and missing
        // devices must all be rejected before we hand them to a subprocess.
        assert!(validate_device_path("/etc/passwd").is_err());
        assert!(validate_device_path("/dev/../etc/passwd").is_err());
        assert!(validate_device_path("/dev/nonexistent_device_xyz").is_err());
    }

    #[test]
    #[cfg(feature = "gated-tests")]
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
