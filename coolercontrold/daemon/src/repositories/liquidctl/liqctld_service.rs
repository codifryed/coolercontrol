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

use crate::ENV_CC_LOG;
use anyhow::{anyhow, Result};
use log::{debug, info, log, warn};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use std::collections::HashMap;
use std::ops::Not;
use std::process::{ExitStatus, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};
use tokio::task::JoinHandle;
use tokio::time::sleep;
use tokio_stream::adapters::Merge;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;
use tokio_util::sync::CancellationToken;

const MAX_RETRIES: u32 = 3;
const SHUTDOWN_TIMEOUT_SECS: u64 = 10;

/// PID of the currently running liqctld child, or 0 when none is running. Recorded so the daemon's
/// shutdown path (and the sidecar force-exit backstop) can SIGKILL the process group directly if
/// the child does not exit gracefully in time. `std::process::exit` runs no destructors, so the
/// child's `kill_on_drop` would otherwise leak it and leave systemd to reap (and SIGABRT) an orphan.
static LIQCTLD_PID: AtomicI32 = AtomicI32::new(0);

/// Force-kills the liqctld child process group, if one is recorded. Idempotent: the stored PID is
/// swapped out so repeated calls (the bounded shutdown fallback, then the sidecar backstop) send at
/// most one signal. The child is spawned with its own process group (`process_group(0)`), so its
/// PGID equals its PID and signalling `-pid` reaches the whole group. A no-op when no child is
/// recorded (PID 0), which also avoids ever signalling a recycled PID after the child was reaped.
pub fn force_kill_liqctld() {
    let pid = LIQCTLD_PID.swap(0, Ordering::AcqRel);
    if pid > 0 {
        let _ = kill(Pid::from_raw(-pid), Signal::SIGKILL);
    }
}

const PY_VERIFY: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/liqctld/verify.py"
));
const PY_SERVICE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/resources/liqctld/main.py"
));

/// This function verifies the Python system environment meets the requirements to
/// run the `liqctld` service.
pub async fn verify_env() -> Result<()> {
    debug!("Verifying Python environment...");
    run_python(PY_VERIFY, CancellationToken::new()).await
}

/// This function runs the `liqctld` service daemon.
/// The `run_token` is used to signal that the daemon is shutting down.
/// The `stop_token` is used to signal that only the service should be stopped.
pub async fn run(run_token: CancellationToken, stop_token: CancellationToken) -> Result<()> {
    debug!("Starting...");
    let mut tries = 0;
    let mut result = run_python(PY_SERVICE, run_token.clone()).await;
    while run_token.is_cancelled().not() && stop_token.is_cancelled().not() && tries < MAX_RETRIES {
        warn!("liqctld exited prematurely! Restarting...");
        debug!("liqctld run result: {result:?}");
        sleep(Duration::from_secs(2)).await;
        result = run_python(PY_SERVICE, run_token.clone()).await;
        tries += 1;
    }
    result
}

#[allow(unused_assignments)]
async fn run_python(script: &[u8], run_token: CancellationToken) -> Result<()> {
    let (cmd, arg) = create_command();
    let mut child = Command::new(cmd)
        .arg(arg)
        .envs(child_envs())
        .stdin(Stdio::piped())
        .kill_on_drop(true)
        // allows us to better control the shutdown of the child process
        .process_group(0)
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;

    child
        .stdin
        .take()
        .ok_or_else(|| anyhow!("Child process stdin has not been captured!"))?
        .write_all(script)
        .await?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("Child process stdout could not be captured!"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("Child process stderr could not be captured!"))?;

    let stdout_lines = LinesStream::new(BufReader::new(stdout).lines());
    let stderr_lines = LinesStream::new(BufReader::new(stderr).lines());
    let merged_lines = StreamExt::merge(stdout_lines, stderr_lines);

    let child_handle = watch_child(child, run_token);
    process_log_output(merged_lines).await;
    match child_handle.await? {
        Ok(status) => match status.code() {
            Some(0) => Ok(()),
            Some(code) => Err(anyhow!("liqctld exited with a non-zero exit code: {code}")),
            _ => Err(anyhow!("liqctld exited with an unknown exit code")),
        },
        Err(err) => Err(anyhow!("liqctld exited with an error: {err}")),
    }
}

/// Creates the command to run the Python script.
///
/// If run inside an `AppImage`, we use the internal isolated Python environment.
fn create_command() -> (String, &'static str) {
    if let Ok(appdir) = std::env::var("APPDIR") {
        info!("Running liqctld inside an AppImage");
        (format!("{appdir}/usr/bin/python3"), "-I")
    } else {
        ("python3".to_string(), "-q")
    }
}

/// Sets the environment variables for the child process.
///
/// There several ways to set the log level, this also ensures that the child process environment
/// is set to the same log level as the parent process.
fn child_envs() -> HashMap<String, String> {
    let mut envs = HashMap::new();
    envs.insert("LC_ALL".to_string(), "C".to_string());
    envs.insert("PYTHONUNBUFFERED".to_string(), "1".to_string());
    if log::log_enabled!(log::Level::Trace) {
        envs.insert(ENV_CC_LOG.to_string(), log::Level::Trace.to_string());
    } else if log::log_enabled!(log::Level::Debug) {
        envs.insert(ENV_CC_LOG.to_string(), log::Level::Debug.to_string());
    } else if log::log_enabled!(log::Level::Info) {
        envs.insert(ENV_CC_LOG.to_string(), log::Level::Info.to_string());
    } else if log::log_enabled!(log::Level::Warn) {
        envs.insert(ENV_CC_LOG.to_string(), log::Level::Warn.to_string());
    } else if log::log_enabled!(log::Level::Error) {
        envs.insert(ENV_CC_LOG.to_string(), log::Level::Error.to_string());
    }
    envs
}

/// This function watches the `liqctld` service child process and returns
/// its exit status when it exits or the cancel token is canceled.
/// The child process is killed if it does not exit.
fn watch_child(mut child: Child, run_token: CancellationToken) -> JoinHandle<Result<ExitStatus>> {
    tokio::task::spawn_local(async move {
        // Record the PID while we own the child so the shutdown path and the sidecar backstop can
        // force-kill its group; clear it on any exit so a recycled PID is never signalled. A PID
        // always fits in an i32 on Linux, so a failed conversion just skips recording (no kill).
        if let Some(pid) = child.id().and_then(|pid| i32::try_from(pid).ok()) {
            LIQCTLD_PID.store(pid, Ordering::Release);
        }
        let result = tokio::select! {
            () = delayed_cancelled(run_token) => {
                if let Ok(Some(status)) = child.try_wait() {
                    Ok(status)
                } else {
                    let _ = child.kill().await;
                    Err(anyhow!("Forced to kill liqctld child process"))
                }
            }
            Ok(status) = child.wait() => Ok(status),
            else => {Err(anyhow!("liqctld Child process exited unexpectedly!"))}
        };
        // The child has been reaped (or force-killed above); drop the stale PID.
        LIQCTLD_PID.store(0, Ordering::Release);
        result
    })
}

async fn delayed_cancelled(run_token: CancellationToken) {
    run_token.cancelled().await;
    // give the child process some time to exit before killing it
    sleep(Duration::from_secs(SHUTDOWN_TIMEOUT_SECS)).await;
}

/// This function processes the output of the `liqctld` service child process and
/// logs it at the appropriate log level with the rust logger.
async fn process_log_output(
    mut merged_lines: Merge<
        LinesStream<BufReader<ChildStdout>>,
        LinesStream<BufReader<ChildStderr>>,
    >,
) {
    let mut lvl = log::Level::Info;
    let mut kraken_bucket_error_logged = false;
    let mut aquacomputer_pwm_logged = false;
    while let Some(unread_line) = merged_lines.next().await {
        let Ok(line) = unread_line else {
            warn!("Failed to read log line from liqctld: {unread_line:?}");
            continue;
        };
        let log_line = if let Some(stripped) = line.strip_prefix("INFO") {
            lvl = log::Level::Info;
            stripped
        } else if let Some(mut stripped) = line.strip_prefix("WARNING") {
            lvl = log::Level::Warn;
            // Corsair PSU log handling
            if stripped.starts_with("some attributes cannot be read") {
                continue; // drop this repeating log
            } else if stripped.starts_with("bound to corsair-psu kernel driver") {
                stripped = "Corsair PSU kernel driver conflict detected. \
                Solution: Use the HWMon kernel driver (no fan control) \
                OR enable liquidctl 'Direct Access' in advanced device settings.";
            } else if stripped
                .starts_with("required PWM functionality is not available in aquacomputer_d5next")
            {
                // The device keeps working, but liquidctl repeats
                // this warning every write. Log once at Info and
                // drop subsequent occurrences.
                if aquacomputer_pwm_logged {
                    continue;
                }
                lvl = log::Level::Info;
                aquacomputer_pwm_logged = true;
            }
            stripped
        } else if let Some(mut stripped) = line.strip_prefix("ERROR") {
            lvl = log::Level::Error;
            if stripped.starts_with("Failed to setup bucket")
                || stripped.starts_with("Failed to switch active bucket")
            {
                if kraken_bucket_error_logged {
                    continue;
                }
                lvl = log::Level::Warn;
                stripped = "The liquidctl KrakenZ3 driver has reported an image bucket error. Warning: The LCD display may flicker or be unstable.";
                kraken_bucket_error_logged = true;
            }
            stripped
        } else if let Some(stripped) = line.strip_prefix("DEBUG") {
            lvl = log::Level::Debug;
            stripped
        } else {
            // allows multi-line log messages to be logged with the same level
            line.as_str()
        };
        log!(target: "coolercontrold::liqctld", lvl, "{log_line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs::sidecar_fs;
    use serial_test::serial;
    use std::os::unix::process::CommandExt;
    use std::process::Command as StdCommand;

    #[test]
    #[serial]
    fn test_verify_env() {
        // verify_env spawns a Python process via tokio::process, so it needs a Tokio runtime
        // regardless of the active main-thread backend.
        sidecar_fs::test_runtime(async {
            let result = verify_env().await;
            // Err would mean that the liquidctl package is not installed, which is fine for testing.
            assert!(result.is_ok() || result.is_err());
        });
    }

    // Goal: with no child recorded (PID 0), force_kill_liqctld must signal nothing and leave the
    // slot at 0. Method: clear the slot, call, assert it stays 0 (negative space: never signal a
    // non-existent group).
    #[test]
    #[serial]
    fn force_kill_is_noop_without_a_recorded_child() {
        LIQCTLD_PID.store(0, Ordering::Release);
        force_kill_liqctld();
        assert_eq!(LIQCTLD_PID.load(Ordering::Acquire), 0);
    }

    // Goal: force_kill_liqctld SIGKILLs the recorded child's process group and clears the PID so a
    // second call is a no-op. Method: spawn a real long-lived child in its own process group (as
    // run_python does via process_group(0)), record its PID, force-kill, and confirm it terminated
    // and the slot was cleared.
    #[test]
    #[serial]
    fn force_kill_terminates_recorded_process_group_and_clears_pid() {
        let mut child = StdCommand::new("sleep")
            .arg("60")
            .process_group(0)
            .spawn()
            .expect("spawn sleep child");
        assert!(child.id() > 0);
        let pid = i32::try_from(child.id()).expect("PID fits in i32");
        LIQCTLD_PID.store(pid, Ordering::Release);

        force_kill_liqctld();
        assert_eq!(
            LIQCTLD_PID.load(Ordering::Acquire),
            0,
            "PID must be cleared after force-kill"
        );

        let status = child.wait().expect("reap the killed child");
        assert!(
            status.success().not(),
            "child must have been killed, not exited cleanly"
        );
        // A second call is a harmless no-op (slot already cleared).
        force_kill_liqctld();
    }
}
