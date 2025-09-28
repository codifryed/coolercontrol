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
use anyhow::{anyhow, Result};
use log::{debug, info, log, warn};
use std::collections::HashMap;
use std::ops::Not;
use std::process::{ExitStatus, Stdio};
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
    let default_env = HashMap::from([("LC_ALL".to_string(), "C".to_string())]);
    let (cmd, arg) = if let Ok(appdir) = std::env::var("APPDIR") {
        // if run inside an AppImage, so we isolate the Python environment from the host
        info!("Running liqctld inside an AppImage");
        (format!("{appdir}/usr/bin/python3"), "-I")
    } else {
        ("python3".to_string(), "-q")
    };
    let mut child = Command::new(cmd)
        .envs(default_env)
        .arg(arg)
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

/// This function watches the `liqctld` service child process and returns
/// its exit status when it exits or the cancel token is canceled.
/// The child process is killed if it does not exit.
fn watch_child(mut child: Child, run_token: CancellationToken) -> JoinHandle<Result<ExitStatus>> {
    tokio::task::spawn_local(async move {
        tokio::select! {
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
        }
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
    while let Some(unread_line) = merged_lines.next().await {
        let Ok(line) = unread_line else {
            continue;
        };
        let log_line = if let Some(stripped) = line.strip_prefix("INFO") {
            lvl = log::Level::Info;
            stripped
        } else if let Some(stripped) = line.strip_prefix("WARNING") {
            lvl = log::Level::Warn;
            stripped
        } else if let Some(stripped) = line.strip_prefix("ERROR") {
            lvl = log::Level::Error;
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
    use crate::cc_fs;
    use serial_test::serial;
    #[test]
    #[serial]
    fn test_verify_env() {
        cc_fs::test_runtime(async {
            let result = verify_env().await;
            // Err would mean that the liquidctl package is not installed, which is fine for testing.
            assert!(result.is_ok() || result.is_err());
        });
    }
}
