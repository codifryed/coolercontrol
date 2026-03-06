/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2026  Guy Boldon, megadjc and contributors
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

//! Safe shell command execution with timeout and output limits.
//!
//! Based on the daemon's `ShellCommand` pattern, adapted for synchronous use
//! in the detection library.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use log::{debug, error, warn};

const MAX_OUTPUT_LENGTH_BYTES: usize = 2_000;

/// Result of a shell command execution.
pub enum ShellCommandResult {
    Success { stdout: String, stderr: String },
    Error(String),
}

/// Safe shell command wrapper with timeout and output limiting.
pub struct ShellCommand {
    command: String,
    timeout: Duration,
}

impl ShellCommand {
    /// Create a new shell command with the given timeout.
    #[must_use]
    pub fn new(command: &str, timeout: Duration) -> Self {
        Self {
            command: command.to_owned(),
            timeout,
        }
    }

    /// Execute the command synchronously with timeout enforcement.
    /// Kills the process if the timeout is exceeded.
    /// Captures stdout/stderr (truncated to `MAX_OUTPUT_LENGTH_BYTES`).
    #[must_use]
    pub fn run(&self) -> ShellCommandResult {
        debug!("Running shell command: {}", self.command);
        let mut child = match Command::new("sh")
            .arg("-c")
            .arg(&self.command)
            .env("LC_ALL", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                error!(
                    "Failed to spawn process for command: {}, {err}",
                    &self.command
                );
                return ShellCommandResult::Error(err.to_string());
            }
        };

        let start = Instant::now();
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if start.elapsed() >= self.timeout {
                        warn!(
                            "Shell command timed out after {:?}, killing: {}",
                            self.timeout, self.command
                        );
                        let _ = child.kill();
                        let _ = child.wait();
                        return ShellCommandResult::Error(format!(
                            "command timed out after {:?}: {}",
                            self.timeout, self.command
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(err) => {
                    error!(
                        "Error checking process status for: {}, {err}",
                        &self.command
                    );
                    return ShellCommandResult::Error(err.to_string());
                }
            }
        }

        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(err) => {
                error!("Error reading output for command: {}, {err}", &self.command);
                return ShellCommandResult::Error(err.to_string());
            }
        };

        let mut stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let mut stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        limit_output_length(&mut stdout);
        limit_output_length(&mut stderr);

        if output.status.success() {
            ShellCommandResult::Success { stdout, stderr }
        } else {
            ShellCommandResult::Error(stderr)
        }
    }
}

fn limit_output_length(output: &mut String) {
    if output.len() > MAX_OUTPUT_LENGTH_BYTES {
        output.truncate(MAX_OUTPUT_LENGTH_BYTES);
        output.push_str("...[truncated]");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_successful_command() {
        let cmd = ShellCommand::new("echo hello", Duration::from_secs(5));
        match cmd.run() {
            ShellCommandResult::Success { stdout, .. } => {
                assert_eq!(stdout, "hello");
            }
            ShellCommandResult::Error(e) => panic!("expected success, got error: {e}"),
        }
    }

    #[test]
    fn test_error_command() {
        let cmd = ShellCommand::new("false", Duration::from_secs(5));
        match cmd.run() {
            ShellCommandResult::Error(_) => {}
            ShellCommandResult::Success { .. } => panic!("expected error"),
        }
    }

    #[test]
    fn test_timeout_handling() {
        let cmd = ShellCommand::new("sleep 60", Duration::from_millis(200));
        match cmd.run() {
            ShellCommandResult::Error(msg) => {
                assert!(msg.contains("timed out"), "expected timeout message: {msg}");
            }
            ShellCommandResult::Success { .. } => panic!("expected timeout error"),
        }
    }

    #[test]
    fn test_output_truncation() {
        // Generate output larger than MAX_OUTPUT_LENGTH_BYTES
        let cmd = ShellCommand::new(
            "python3 -c 'print(\"A\" * 5000)' 2>/dev/null || python -c 'print(\"A\" * 5000)' 2>/dev/null || printf '%0.sA' $(seq 1 5000)",
            Duration::from_secs(5),
        );
        match cmd.run() {
            ShellCommandResult::Success { stdout, .. } => {
                assert!(
                    stdout.len() <= MAX_OUTPUT_LENGTH_BYTES + 20,
                    "output should be truncated"
                );
            }
            ShellCommandResult::Error(_) => {
                // OK if the command isn't available
            }
        }
    }
}
