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

use std::process::Stdio;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use moro_local::Scope;
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

use crate::api::actor::{run_api_actor, ApiActor};

const MAX_DURATION_SECS: u16 = 600;
const DEFAULT_DURATION_SECS: u16 = 60;
const EARLY_EXIT_CHECK: Duration = Duration::from_millis(500);

struct StressTestActor {
    receiver: mpsc::Receiver<StressTestMessage>,
    cpu_child: Option<Child>,
    cpu_duration_secs: Option<u16>,
    gpu_child: Option<Child>,
    gpu_duration_secs: Option<u16>,
    ram_child: Option<Child>,
    ram_duration_secs: Option<u16>,
    drive_child: Option<Child>,
    drive_duration_secs: Option<u16>,
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
pub struct StressTestStatus {
    pub cpu_active: bool,
    pub cpu_duration_secs: Option<u16>,
    pub gpu_active: bool,
    pub gpu_duration_secs: Option<u16>,
    pub ram_active: bool,
    pub ram_duration_secs: Option<u16>,
    pub drive_active: bool,
    pub drive_duration_secs: Option<u16>,
}

impl StressTestActor {
    fn new(receiver: mpsc::Receiver<StressTestMessage>) -> Self {
        Self {
            receiver,
            cpu_child: None,
            cpu_duration_secs: None,
            gpu_child: None,
            gpu_duration_secs: None,
            ram_child: None,
            ram_duration_secs: None,
            drive_child: None,
            drive_duration_secs: None,
        }
    }

    fn bin_path() -> Result<std::path::PathBuf> {
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

    async fn start_cpu(
        &mut self,
        thread_count: Option<u16>,
        duration_secs: Option<u16>,
    ) -> Result<()> {
        if self.cpu_child.is_some() {
            return Err(anyhow!("CPU stress test is already running"));
        }

        let bin_path = Self::bin_path()?;
        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let available_cpus = cc_stress::online_cpu_count();
        let threads = thread_count
            .unwrap_or(available_cpus)
            .min(available_cpus * 2)
            .max(1);

        info!(
            "Starting CPU stress test: {threads} threads, {duration}s, bin: {}",
            bin_path.display()
        );

        let mut cmd = Command::new(&bin_path);
        cmd.arg("stress-cpu")
            .arg("--timeout")
            .arg(duration.to_string())
            .arg("--threads")
            .arg(threads.to_string())
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| anyhow!("Failed to start CPU stress subprocess: {e}"))?;

        info!("CPU stress subprocess spawned with PID: {:?}", child.id());

        Self::check_early_exit(&mut child, "CPU").await?;

        self.cpu_child = Some(child);
        self.cpu_duration_secs = Some(duration);
        Ok(())
    }

    async fn stop_cpu(&mut self) {
        if let Some(mut child) = self.cpu_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.cpu_duration_secs = None;
            info!("CPU stress test stopped");
        }
    }

    async fn start_gpu(&mut self, duration_secs: Option<u16>) -> Result<()> {
        if self.gpu_child.is_some() {
            return Err(anyhow!("GPU stress test is already running"));
        }

        let bin_path = Self::bin_path()?;
        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);

        info!("Starting GPU stress test: {duration}s");

        let mut child = Command::new(&bin_path)
            .arg("stress-gpu")
            .arg("--timeout")
            .arg(duration.to_string())
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to start GPU stress subprocess: {e}"))?;

        Self::check_early_exit(&mut child, "GPU").await?;

        self.gpu_child = Some(child);
        self.gpu_duration_secs = Some(duration);
        Ok(())
    }

    async fn stop_gpu(&mut self) {
        if let Some(mut child) = self.gpu_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.gpu_duration_secs = None;
            info!("GPU stress test stopped");
        }
    }

    async fn start_ram(&mut self, duration_secs: Option<u16>) -> Result<()> {
        if self.ram_child.is_some() {
            return Err(anyhow!("RAM stress test is already running"));
        }

        let bin_path = Self::bin_path()?;
        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);

        let available = cc_stress::available_memory_bytes()
            .map_err(|e| anyhow!("Failed to read available memory: {e}"))?;
        let alloc_bytes = (available as f64 * cc_stress::RAM_STRESS_ALLOC_FRACTION) as u64;

        info!(
            "Starting RAM stress test: {duration}s, {} MiB",
            alloc_bytes / (1024 * 1024)
        );

        let mut child = Command::new(&bin_path)
            .arg("stress-ram")
            .arg("--bytes")
            .arg(alloc_bytes.to_string())
            .arg("--timeout")
            .arg(duration.to_string())
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to start RAM stress subprocess: {e}"))?;

        Self::check_early_exit(&mut child, "RAM").await?;

        self.ram_child = Some(child);
        self.ram_duration_secs = Some(duration);
        Ok(())
    }

    async fn stop_ram(&mut self) {
        if let Some(mut child) = self.ram_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.ram_duration_secs = None;
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

        let bin_path = Self::bin_path()?;
        let duration = duration_secs
            .unwrap_or(DEFAULT_DURATION_SECS)
            .min(MAX_DURATION_SECS);
        let thread_count = threads
            .unwrap_or(cc_stress::DRIVE_STRESS_DEFAULT_THREADS)
            .max(1);

        info!("Starting Drive stress test: {device_path}, {thread_count} threads, {duration}s");

        let mut child = Command::new(&bin_path)
            .arg("stress-drive")
            .arg("--device")
            .arg(&device_path)
            .arg("--threads")
            .arg(thread_count.to_string())
            .arg("--timeout")
            .arg(duration.to_string())
            .kill_on_drop(true)
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| anyhow!("Failed to start Drive stress subprocess: {e}"))?;

        Self::check_early_exit(&mut child, "Drive").await?;

        self.drive_child = Some(child);
        self.drive_duration_secs = Some(duration);
        Ok(())
    }

    async fn stop_drive(&mut self) {
        if let Some(mut child) = self.drive_child.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
            self.drive_duration_secs = None;
            info!("Drive stress test stopped");
        }
    }

    fn check_child_still_running(
        child: &mut Option<Child>,
        duration: &mut Option<u16>,
        label: &str,
    ) {
        if let Some(c) = child.as_mut() {
            match c.try_wait() {
                Ok(Some(_)) => {
                    debug!("{label} stress test process has exited");
                    *child = None;
                    *duration = None;
                }
                Ok(None) => {} // still running
                Err(err) => {
                    warn!("Error checking {label} stress test process: {err}");
                    *child = None;
                    *duration = None;
                }
            }
        }
    }

    fn status(&mut self) -> StressTestStatus {
        Self::check_child_still_running(&mut self.cpu_child, &mut self.cpu_duration_secs, "CPU");
        Self::check_child_still_running(&mut self.gpu_child, &mut self.gpu_duration_secs, "GPU");
        Self::check_child_still_running(&mut self.ram_child, &mut self.ram_duration_secs, "RAM");
        Self::check_child_still_running(
            &mut self.drive_child,
            &mut self.drive_duration_secs,
            "Drive",
        );
        StressTestStatus {
            cpu_active: self.cpu_child.is_some(),
            cpu_duration_secs: self.cpu_duration_secs,
            gpu_active: self.gpu_child.is_some(),
            gpu_duration_secs: self.gpu_duration_secs,
            ram_active: self.ram_child.is_some(),
            ram_duration_secs: self.ram_duration_secs,
            drive_active: self.drive_child.is_some(),
            drive_duration_secs: self.drive_duration_secs,
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

#[derive(Clone)]
pub struct StressTestHandle {
    sender: mpsc::Sender<StressTestMessage>,
}

impl StressTestHandle {
    pub fn new<'s>(
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let actor = StressTestActor::new(receiver);
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
            gpu_active: false,
            gpu_duration_secs: None,
            ram_active: false,
            ram_duration_secs: None,
            drive_active: false,
            drive_duration_secs: None,
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
            gpu_active: false,
            gpu_duration_secs: None,
            ram_active: false,
            ram_duration_secs: None,
            drive_active: false,
            drive_duration_secs: None,
        };
        assert!(!status.cpu_active);
        assert!(!status.gpu_active);
        assert!(!status.ram_active);
        assert!(!status.drive_active);
    }
}
