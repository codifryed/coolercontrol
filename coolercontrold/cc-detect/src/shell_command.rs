// SPDX-FileCopyrightText: 2026 Guy Boldon, megadjc and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe shell command execution with timeout and output limits.
//!
//! Based on the daemon's `ShellCommand` pattern, adapted for synchronous use
//! in the detection library.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use log::{debug, warn};

/// The shell is spawned by absolute path rather than looked up on `$PATH`. A service
/// manager is free to hand the daemon a `PATH` that holds only its own directory, and on
/// distros that manage `/etc` declaratively it does exactly that, so a bare `sh` fails to
/// spawn. POSIX guarantees `/bin/sh`.
///
/// The daemon spawns its own shell from this same constant, for this same reason.
pub const SHELL: &str = "/bin/sh";
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
        let mut child = match Command::new(SHELL)
            .arg("-c")
            .arg(&self.command)
            .env("LC_ALL", "C")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(err) => {
                warn!(
                    "Failed to spawn process for command: {}, {err}",
                    self.command
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
                        debug!(
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
                    debug!("Error checking process status for: {}, {err}", self.command);
                    return ShellCommandResult::Error(err.to_string());
                }
            }
        }

        let output = match child.wait_with_output() {
            Ok(output) => output,
            Err(err) => {
                debug!("Error reading output for command: {}, {err}", self.command);
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
    fn shell_is_spawned_by_absolute_path() {
        // Goal: the shell never depends on $PATH. A service manager may hand the daemon a
        // PATH holding only its own directory, and a bare "sh" then fails to spawn. This
        // is the only copy: the daemon spawns the same constant.
        // Method: pin the constant as absolute, and require it to exist and be executable
        // on the machine running the suite.
        use std::os::unix::fs::PermissionsExt;
        let shell = std::path::Path::new(SHELL);
        assert!(shell.is_absolute(), "{SHELL} must be an absolute path");
        let metadata = std::fs::metadata(shell).unwrap_or_else(|_| panic!("{SHELL} must exist"));
        assert!(
            metadata.permissions().mode() & 0o111 != 0,
            "{SHELL} must be executable"
        );
    }

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
