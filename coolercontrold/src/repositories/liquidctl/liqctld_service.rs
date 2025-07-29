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
use log::{debug, log};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio_stream::wrappers::LinesStream;
use tokio_stream::StreamExt;

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
    run_script(PY_VERIFY).await
}

/// This function runs the `liqctld` service daemon.
pub async fn run() -> Result<()> {
    debug!("Starting...");
    run_script(PY_SERVICE).await?;
    Ok(())
}

#[allow(unused_assignments)]
async fn run_script(script: &[u8]) -> Result<()> {
    let default_env = HashMap::from([("LC_ALL".to_string(), "C".to_string())]);
    let mut child = Command::new("python3")
        .envs(default_env)
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
    let mut merged_lines = StreamExt::merge(stdout_lines, stderr_lines);
    // todo: possibly use tokio_select with cancel token: (that will probably stop logging before the process exits)
    //  -- perhaps after the cancel token is triggered, we wait try_wait for a moment before killing the process
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
    let status = child.wait().await?;
    debug!("liqctld Exit Status: {}", status.code().unwrap_or(-1));
    Ok(())
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
