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

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use moro_local::Scope;
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

/// Which backend is running (or will run) a stress test.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StressBackend {
    BuiltIn,
    StressNg,
}

impl std::fmt::Display for StressBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn => f.write_str("built-in"),
            Self::StressNg => f.write_str("stress-ng"),
        }
    }
}

/// Detected stress-ng capabilities, probed once at startup.
struct StressNgCaps {
    /// Path to the stress-ng binary, if found.
    path: Option<PathBuf>,
    /// Whether the GPU stressor is compiled into stress-ng.
    gpu: bool,
}

struct StressTestActor {
    receiver: mpsc::Receiver<StressTestMessage>,
    stress_ng: StressNgCaps,
    cpu_child: Option<Child>,
    cpu_duration_secs: Option<u16>,
    cpu_backend: Option<StressBackend>,
    gpu_child: Option<Child>,
    gpu_duration_secs: Option<u16>,
    gpu_backend: Option<StressBackend>,
    ram_child: Option<Child>,
    ram_duration_secs: Option<u16>,
    ram_backend: Option<StressBackend>,
    drive_child: Option<Child>,
    drive_duration_secs: Option<u16>,
    drive_backend: Option<StressBackend>,
}

enum StressTestMessage {
    StartCpu {
        thread_count: Option<u16>,
        duration_secs: Option<u16>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopCpu {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StartGpu {
        duration_secs: Option<u16>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopGpu {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StartRam {
        duration_secs: Option<u16>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopRam {
        respond_to: oneshot::Sender<Result<()>>,
    },
    StartDrive {
        device_path: String,
        threads: Option<u16>,
        duration_secs: Option<u16>,
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
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct StressTestStatus {
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
    fn new(receiver: mpsc::Receiver<StressTestMessage>, stress_ng: StressNgCaps) -> Self {
        Self {
            receiver,
            stress_ng,
            cpu_child: None,
            cpu_duration_secs: None,
            cpu_backend: None,
            gpu_child: None,
            gpu_duration_secs: None,
            gpu_backend: None,
            ram_child: None,
            ram_duration_secs: None,
            ram_backend: None,
            drive_child: None,
            drive_duration_secs: None,
            drive_backend: None,
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
    ) -> Result<()> {
        if self.cpu_child.is_some() {
            return Err(anyhow!("CPU stress test is already running"));
        }

        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let available_cpus = cc_stress::online_cpu_count();
        let threads = thread_count
            .unwrap_or(available_cpus)
            .min(available_cpus.saturating_mul(2))
            .max(1);
        let threads_str = threads.to_string();
        let timeout_str = format!("{duration}s");
        let duration_str = duration.to_string();

        let (mut child, backend) = if let Some(path) = &self.stress_ng.path {
            info!("Starting CPU stress test via stress-ng: {threads} threads, {duration}s");
            let child = Self::spawn_stress_ng(
                path,
                &["--cpu", &threads_str, "--timeout", &timeout_str],
                "CPU",
            )?;
            (child, StressBackend::StressNg)
        } else {
            let bin_path = Self::bin_path()?;
            info!(
                "Starting CPU stress test (built-in): {threads} threads, {duration}s, bin: {}",
                bin_path.display()
            );
            let child = Self::spawn_builtin(
                &[
                    "stress-cpu",
                    "--timeout",
                    &duration_str,
                    "--threads",
                    &threads_str,
                ],
                "CPU",
            )?;
            (child, StressBackend::BuiltIn)
        };

        info!(
            "CPU stress subprocess spawned with PID: {:?} ({})",
            child.id(),
            backend
        );

        Self::check_early_exit(&mut child, "CPU").await?;

        self.cpu_child = Some(child);
        self.cpu_duration_secs = Some(duration);
        self.cpu_backend = Some(backend);
        Ok(())
    }

    async fn stop_cpu(&mut self) {
        if let Some(mut child) = self.cpu_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.cpu_duration_secs = None;
            self.cpu_backend = None;
            info!("CPU stress test stopped");
        }
    }

    async fn start_gpu(&mut self, duration_secs: Option<u16>) -> Result<()> {
        if self.gpu_child.is_some() {
            return Err(anyhow!("GPU stress test is already running"));
        }

        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let timeout_str = format!("{duration}s");
        let duration_str = duration.to_string();

        let (mut child, backend) = if let Some(path) = &self.stress_ng.path {
            if self.stress_ng.gpu {
                info!("Starting GPU stress test via stress-ng: {duration}s");
                let child =
                    Self::spawn_stress_ng(path, &["--gpu", "1", "--timeout", &timeout_str], "GPU")?;
                (child, StressBackend::StressNg)
            } else {
                info!(
                    "Starting GPU stress test (built-in, stress-ng GPU not available): {duration}s"
                );
                let child =
                    Self::spawn_builtin(&["stress-gpu", "--timeout", &duration_str], "GPU")?;
                (child, StressBackend::BuiltIn)
            }
        } else {
            info!("Starting GPU stress test (built-in): {duration}s");
            let child = Self::spawn_builtin(&["stress-gpu", "--timeout", &duration_str], "GPU")?;
            (child, StressBackend::BuiltIn)
        };

        Self::check_early_exit(&mut child, "GPU").await?;

        self.gpu_child = Some(child);
        self.gpu_duration_secs = Some(duration);
        self.gpu_backend = Some(backend);
        Ok(())
    }

    async fn stop_gpu(&mut self) {
        if let Some(mut child) = self.gpu_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.gpu_duration_secs = None;
            self.gpu_backend = None;
            info!("GPU stress test stopped");
        }
    }

    async fn start_ram(&mut self, duration_secs: Option<u16>) -> Result<()> {
        if self.ram_child.is_some() {
            return Err(anyhow!("RAM stress test is already running"));
        }

        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);

        let available = cc_stress::available_memory_bytes()
            .map_err(|e| anyhow!("Failed to read available memory: {e}"))?;
        // Precision loss is acceptable for a memory size estimate.
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let alloc_bytes = (available as f64 * cc_stress::RAM_STRESS_ALLOC_FRACTION) as u64;

        let timeout_str = format!("{duration}s");
        let duration_str = duration.to_string();

        let (mut child, backend) = if let Some(path) = &self.stress_ng.path {
            let num_workers = u64::from(cc_stress::online_cpu_count()).max(1);
            let per_worker_bytes = alloc_bytes / num_workers;
            let workers_str = num_workers.to_string();
            let bytes_str = format!("{per_worker_bytes}");
            info!(
                "Starting RAM stress test via stress-ng: {duration}s, \
                 {num_workers} workers x {} MiB",
                per_worker_bytes / (1024 * 1024)
            );
            let child = Self::spawn_stress_ng(
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
            )?;
            (child, StressBackend::StressNg)
        } else {
            let alloc_str = alloc_bytes.to_string();
            info!(
                "Starting RAM stress test (built-in): {duration}s, {} MiB",
                alloc_bytes / (1024 * 1024)
            );
            let child = Self::spawn_builtin(
                &[
                    "stress-ram",
                    "--bytes",
                    &alloc_str,
                    "--timeout",
                    &duration_str,
                ],
                "RAM",
            )?;
            (child, StressBackend::BuiltIn)
        };

        Self::check_early_exit(&mut child, "RAM").await?;

        self.ram_child = Some(child);
        self.ram_duration_secs = Some(duration);
        self.ram_backend = Some(backend);
        Ok(())
    }

    async fn stop_ram(&mut self) {
        if let Some(mut child) = self.ram_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
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
    ) -> Result<()> {
        if self.drive_child.is_some() {
            return Err(anyhow!("Drive stress test is already running"));
        }

        // Validate device path for safety.
        if !device_path.starts_with("/dev/") {
            return Err(anyhow!("Device path must start with /dev/"));
        }
        if device_path.contains("..") {
            return Err(anyhow!("Device path must not contain '..'"));
        }
        let path = std::path::Path::new(&device_path);
        if !path.exists() {
            return Err(anyhow!("Device {device_path} does not exist"));
        }

        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let thread_count = threads
            .unwrap_or(cc_stress::DRIVE_STRESS_DEFAULT_THREADS)
            .max(1);
        let threads_str = thread_count.to_string();
        let timeout_str = format!("{duration}s");
        let duration_str = duration.to_string();

        // Try stress-ng with mount point mapping; fall back to built-in.
        let (mut child, backend) = if let Some(ng_path) = &self.stress_ng.path {
            if let Some(mount_point) = find_mount_point(&device_path) {
                info!(
                    "Starting Drive stress test via stress-ng: {device_path} \
                         (mount: {mount_point}), {thread_count} threads, {duration}s"
                );
                let child = Self::spawn_stress_ng(
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
                )?;
                (child, StressBackend::StressNg)
            } else {
                info!(
                    "Starting Drive stress test (built-in, device not mounted): \
                         {device_path}, {thread_count} threads, {duration}s"
                );
                let child = Self::spawn_builtin(
                    &[
                        "stress-drive",
                        "--device",
                        &device_path,
                        "--threads",
                        &threads_str,
                        "--timeout",
                        &duration_str,
                    ],
                    "Drive",
                )?;
                (child, StressBackend::BuiltIn)
            }
        } else {
            info!(
                "Starting Drive stress test (built-in): \
                     {device_path}, {thread_count} threads, {duration}s"
            );
            let child = Self::spawn_builtin(
                &[
                    "stress-drive",
                    "--device",
                    &device_path,
                    "--threads",
                    &threads_str,
                    "--timeout",
                    &duration_str,
                ],
                "Drive",
            )?;
            (child, StressBackend::BuiltIn)
        };

        Self::check_early_exit(&mut child, "Drive").await?;

        self.drive_child = Some(child);
        self.drive_duration_secs = Some(duration);
        self.drive_backend = Some(backend);
        Ok(())
    }

    async fn stop_drive(&mut self) {
        if let Some(mut child) = self.drive_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.drive_duration_secs = None;
            self.drive_backend = None;
            info!("Drive stress test stopped");
        }
    }

    fn check_child_still_running(
        child: &mut Option<Child>,
        duration: &mut Option<u16>,
        backend: &mut Option<StressBackend>,
        label: &str,
    ) {
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(_)) => {
                    debug!("{label} stress test process has exited");
                    *child = None;
                    *duration = None;
                    *backend = None;
                }
                Ok(None) => {} // still running
                Err(err) => {
                    warn!("Error checking {label} stress test process: {err}");
                    *child = None;
                    *duration = None;
                    *backend = None;
                }
            }
        }
    }

    /// Returns which backend will be used for each test type, taking into
    /// account what is currently running (active backend) and what would
    /// be used if started now (default backend based on capabilities).
    #[allow(clippy::unused_self)] // Keeps consistent method-call style with other actor methods.
    fn backend_for(&self, active: Option<StressBackend>, has_stress_ng: bool) -> StressBackend {
        active.unwrap_or(if has_stress_ng {
            StressBackend::StressNg
        } else {
            StressBackend::BuiltIn
        })
    }

    fn status(&mut self) -> StressTestStatus {
        Self::check_child_still_running(
            &mut self.cpu_child,
            &mut self.cpu_duration_secs,
            &mut self.cpu_backend,
            "CPU",
        );
        Self::check_child_still_running(
            &mut self.gpu_child,
            &mut self.gpu_duration_secs,
            &mut self.gpu_backend,
            "GPU",
        );
        Self::check_child_still_running(
            &mut self.ram_child,
            &mut self.ram_duration_secs,
            &mut self.ram_backend,
            "RAM",
        );
        Self::check_child_still_running(
            &mut self.drive_child,
            &mut self.drive_duration_secs,
            &mut self.drive_backend,
            "Drive",
        );
        let has_ng = self.stress_ng.path.is_some();
        StressTestStatus {
            cpu_active: self.cpu_child.is_some(),
            cpu_duration_secs: self.cpu_duration_secs,
            cpu_backend: self.backend_for(self.cpu_backend, has_ng),
            gpu_active: self.gpu_child.is_some(),
            gpu_duration_secs: self.gpu_duration_secs,
            gpu_backend: self.backend_for(self.gpu_backend, has_ng && self.stress_ng.gpu),
            ram_active: self.ram_child.is_some(),
            ram_duration_secs: self.ram_duration_secs,
            ram_backend: self.backend_for(self.ram_backend, has_ng),
            drive_active: self.drive_child.is_some(),
            drive_duration_secs: self.drive_duration_secs,
            // Drive backend depends on whether device is mounted, so
            // when idle we report built-in as the conservative default.
            drive_backend: self.backend_for(self.drive_backend, false),
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
                respond_to,
            } => {
                let result = self.start_cpu(thread_count, duration_secs).await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopCpu { respond_to } => {
                self.stop_cpu().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::StartGpu {
                duration_secs,
                respond_to,
            } => {
                let result = self.start_gpu(duration_secs).await;
                let _ = respond_to.send(result);
            }
            StressTestMessage::StopGpu { respond_to } => {
                self.stop_gpu().await;
                let _ = respond_to.send(Ok(()));
            }
            StressTestMessage::StartRam {
                duration_secs,
                respond_to,
            } => {
                let result = self.start_ram(duration_secs).await;
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
                respond_to,
            } => {
                let result = self.start_drive(device_path, threads, duration_secs).await;
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
        }
    }
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

/// Detect stress-ng binary and probe its GPU capability.
async fn detect_stress_ng() -> StressNgCaps {
    // Check if stress-ng is installed.
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

    let Some(ref ng_path) = path else {
        info!(
            "stress-ng is not installed. \
             Install it for improved stress test results."
        );
        return StressNgCaps {
            path: None,
            gpu: false,
        };
    };

    info!("stress-ng found at: {}", ng_path.display());

    // Probe GPU support by running a zero-duration GPU test.
    // stress-ng writes info messages (including "not implemented" and
    // "skipped") to stdout, not stderr, so we must capture both.
    let gpu = match Command::new(ng_path)
        .args(["--gpu", "1", "--timeout", "0"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
    {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let combined = format!("{stdout}{stderr}");
            let unsupported = combined.contains("not implemented") || combined.contains("skipped");
            if unsupported {
                info!("stress-ng GPU stressor is not available on this build");
            } else {
                info!("stress-ng GPU stressor is available");
            }
            !unsupported
        }
        Err(e) => {
            warn!("Failed to probe stress-ng GPU support: {e}");
            false
        }
    };

    StressNgCaps { path, gpu }
}

#[derive(Clone)]
pub struct StressTestHandle {
    sender: mpsc::Sender<StressTestMessage>,
}

impl StressTestHandle {
    pub async fn new<'s>(
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let stress_ng = detect_stress_ng().await;
        // Depth 1: callers await the oneshot response, so at most one message
        // is in flight per caller. Backpressure is handled by the sender
        // awaiting channel capacity.
        let (sender, receiver) = mpsc::channel(1);
        let actor = StressTestActor::new(receiver, stress_ng);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn start_cpu(
        &self,
        thread_count: Option<u16>,
        duration_secs: Option<u16>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartCpu {
            thread_count,
            duration_secs,
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

    pub async fn start_gpu(&self, duration_secs: Option<u16>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartGpu {
            duration_secs,
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

    pub async fn start_ram(&self, duration_secs: Option<u16>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartRam {
            duration_secs,
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
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = StressTestMessage::StartDrive {
            device_path,
            threads,
            duration_secs,
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
        let status = StressTestStatus {
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
        assert!(!status.cpu_active);
        assert!(!status.gpu_active);
        assert!(!status.ram_active);
        assert!(!status.drive_active);
        assert_eq!(status.cpu_backend, StressBackend::BuiltIn);
    }

    #[test]
    fn stress_backend_display() {
        assert_eq!(StressBackend::BuiltIn.to_string(), "built-in");
        assert_eq!(StressBackend::StressNg.to_string(), "stress-ng");
    }

    #[test]
    fn stress_backend_serde_roundtrip() {
        let json = serde_json::to_string(&StressBackend::StressNg).unwrap();
        assert_eq!(json, "\"stress_ng\"");
        let back: StressBackend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, StressBackend::StressNg);

        let json = serde_json::to_string(&StressBackend::BuiltIn).unwrap();
        assert_eq!(json, "\"built_in\"");
    }

    #[test]
    fn find_mount_point_parses_proc_mounts() {
        // find_mount_point reads /proc/mounts which exists on Linux test hosts.
        // We just verify it returns Some or None without panicking.
        let result = find_mount_point("/dev/nonexistent_device_xyz");
        assert!(result.is_none());
    }
}
