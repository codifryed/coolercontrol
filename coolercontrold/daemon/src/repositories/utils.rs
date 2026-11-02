// SPDX-FileCopyrightText: 2023 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Prefer [`ShellCommand`] for commands needing shell features (pipes, globs), and
//! [`DirectCommand`] for direct binary execution. Avoid using `tokio::process::Command`
//! directly except where process lifecycle management is needed (e.g. `liqctld_service.rs`
//! for stdin piping, `stress_test.rs` for long-running child processes).

use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use crate::cc_fs;
use anyhow::{anyhow, Result};
// The daemon and the detection crate spawn the same shell for the same reason, so the
// path and its rationale live in one place.
use cc_detect::shell_command::SHELL;
use log::{debug, error, info, warn};
use nu_glob::{glob, Uninterruptible};
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

const THINKPAD_ACPI_CONF_PATH: &str = "/etc/modprobe.d";
const THINKPAD_ACPI_CONF_FILE: &str = "thinkpad_acpi.conf";
const RELOAD_THINKPAD_ACPI_MODULE_COMMAND: &str =
    "modprobe -r thinkpad_acpi && modprobe thinkpad_acpi";
const MAX_OUTPUT_LENGTH_BYTES: usize = 2_000; // This is the maximum length of the output we want to log
/// One byte past the limit, which is enough to see that the output was cut.
const OUTPUT_CAPTURE_BYTES: usize = MAX_OUTPUT_LENGTH_BYTES + 1;
/// A child's own output is in the pipe by the time it exits, so this only limits the wait on a
/// descendant that outlives it and holds the pipe open.
const PIPE_DRAIN_GRACE: Duration = Duration::from_millis(100);

/// This struct is essentially a wrapper around [`Command`] which adds some
/// additional safety measures and handling for our use cases.
pub struct ShellCommand {
    command: String,
    timeout: Duration,
    env: HashMap<String, String>,
}

pub enum ShellCommandResult {
    Success { stdout: String, stderr: String },
    Error(String),
}

impl ShellCommand {
    /// Creates a new `ShellCommand` instance with the specified command and timeout.
    /// Make sure to sanitize the command input before passing it to this function.
    pub fn new(command: &str, timeout: Duration) -> Self {
        let default_env = HashMap::from([("LC_ALL".to_string(), "C".to_string())]);
        Self {
            command: command.to_owned(),
            timeout,
            env: default_env,
        }
    }

    pub fn env(&mut self, key: &str, value: &str) -> &mut Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    pub async fn run(&self) -> ShellCommandResult {
        // tokio::process gives us async wait + kill-on-timeout + kill_on_drop (so a hung child
        // never blocks the runtime or fills the blocking pool, and is killed on overrun/cancel). It
        // runs on the sidecar, with the daemon's other child processes.
        let command = self.command.clone();
        let env = self.env.clone();
        let timeout = self.timeout;
        crate::sidecar::handle()
            .run(move || run_shell_command(command, env, timeout))
            .await
            .unwrap_or_else(|err| {
                ShellCommandResult::Error(format!("sidecar unavailable for command: {err}"))
            })
    }
}

/// Runs a shell command on the sidecar Tokio runtime, bounded by `timeout` (the child is killed if
/// it overruns), with `kill_on_drop` so a cancelled future cannot orphan the process.
async fn run_shell_command(
    command: String,
    env: HashMap<String, String>,
    timeout: Duration,
) -> ShellCommandResult {
    let mut shell_command = Command::new(SHELL);
    shell_command
        .arg("-c")
        .arg(&command)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &env {
        shell_command.env(key, value);
    }

    let mut child = match shell_command.spawn() {
        Ok(child) => child,
        Err(err) => {
            error!("Unexpected Error spawning process for command: {command}, {err}");
            return ShellCommandResult::Error(err.to_string());
        }
    };

    let output = wait_and_capture(&mut child, timeout, &command).await;
    let successful = match output.exit {
        Some(Ok(status)) => status.success(),
        Some(Err(err)) => {
            warn!("Error waiting for process for command: {command}, {err}");
            false
        }
        None => {
            warn!(
                "Shell command did not complete within the specified timeout: {timeout:?} \
                Killed process for: {command}"
            );
            false
        }
    };

    if successful {
        ShellCommandResult::Success {
            stdout: output.stdout,
            stderr: output.stderr,
        }
    } else {
        ShellCommandResult::Error(output.stderr)
    }
}

/// How a child ended, and what it wrote.
struct ChildOutput {
    /// `None` if the timeout passed first and the child was killed.
    exit: Option<io::Result<ExitStatus>>,
    stdout: String,
    stderr: String,
}

/// Reads both pipes while waiting for the child, all under `timeout`. A child blocks on a full
/// pipe and never exits, so the pipes cannot wait for the exit. The exit status is kept apart from
/// the reads, because a descendant can hold a pipe open long after the child has gone.
async fn wait_and_capture(child: &mut Child, timeout: Duration, label: &str) -> ChildOutput {
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let mut stdout = Vec::with_capacity(OUTPUT_CAPTURE_BYTES);
    let mut stderr = Vec::with_capacity(OUTPUT_CAPTURE_BYTES);
    let mut exit = None;
    let capture = async {
        let drains = async {
            let (stdout_read, stderr_read) = tokio::join!(
                drain_pipe(stdout_pipe, &mut stdout),
                drain_pipe(stderr_pipe, &mut stderr),
            );
            if let Err(err) = stdout_read.and(stderr_read) {
                warn!("Error reading output for: {label}, {err}");
            }
        };
        tokio::pin!(drains);
        tokio::select! {
            () = &mut drains => exit = Some(child.wait().await),
            status = child.wait() => {
                exit = Some(status);
                let _ = tokio::time::timeout(PIPE_DRAIN_GRACE, drains).await;
            }
        }
    };
    // A timeout drops `capture`, and the reads keep what they had: `read_to_end` appends as it goes.
    let _ = tokio::time::timeout(timeout, capture).await;
    if exit.is_none() {
        child.kill().await.ok();
        let _ = child.wait().await;
    }
    ChildOutput {
        exit,
        stdout: captured_to_string(&stdout),
        stderr: captured_to_string(&stderr),
    }
}

/// Reads a pipe to its end, keeping the start of it. The rest is read and dropped, as the child
/// would block on a full pipe. The caller's timeout bounds this.
async fn drain_pipe(
    pipe: Option<impl AsyncRead + Unpin>,
    captured: &mut Vec<u8>,
) -> io::Result<()> {
    let Some(pipe) = pipe else {
        return Ok(());
    };
    let mut head = pipe.take(OUTPUT_CAPTURE_BYTES as u64);
    head.read_to_end(captured).await?;
    debug_assert!(captured.len() <= OUTPUT_CAPTURE_BYTES);
    tokio::io::copy(&mut head.into_inner(), &mut tokio::io::sink()).await?;
    Ok(())
}

fn captured_to_string(captured: &[u8]) -> String {
    debug_assert!(captured.len() <= OUTPUT_CAPTURE_BYTES);
    let mut output = String::from_utf8_lossy(captured).into_owned();
    limit_output_length(&mut output);
    output.trim().to_string()
}

/// Runs a binary directly (no shell). Adds timeout protection and output capturing.
pub struct DirectCommand {
    binary: String,
    args: Vec<String>,
    timeout: Duration,
}

impl DirectCommand {
    pub fn new(binary: &str, timeout: Duration) -> Self {
        Self {
            binary: binary.to_owned(),
            args: Vec::new(),
            timeout,
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// Run. Returns `ShellCommandResult::Success` on exit 0, `Error` otherwise.
    pub async fn run(self) -> ShellCommandResult {
        match self.execute().await {
            Ok((0, stdout, stderr)) => ShellCommandResult::Success { stdout, stderr },
            Ok((_, _, stderr)) => ShellCommandResult::Error(stderr),
            Err(err) => ShellCommandResult::Error(err.to_string()),
        }
    }

    /// Run and return `(exit_code, stdout, stderr)`.
    /// `Err` only if the process could not be spawned or timed out.
    pub async fn run_with_code(self) -> Result<(i32, String, String)> {
        self.execute().await
    }

    async fn execute(self) -> Result<(i32, String, String)> {
        // See `ShellCommand::run`: tokio::process runs on the sidecar for resilient async
        // wait/kill, off the main thread and off the blocking pool.
        crate::sidecar::handle()
            .run(move || run_direct_command(self.binary, self.args, self.timeout))
            .await?
    }
}

/// A [`DirectCommand`] killed on overrun. Callers that must tell a timeout from a spawn failure
/// downcast to it.
#[derive(Debug, thiserror::Error)]
#[error("Command {binary} timed out after {timeout:?}, stderr: {stderr}")]
pub struct CommandTimedOut {
    pub binary: String,
    pub timeout: Duration,
    pub stderr: String,
}

/// Runs a binary directly on the sidecar Tokio runtime, bounded by `timeout` (killed on overrun),
/// with `kill_on_drop`. Returns `(exit_code, stdout, stderr)`; `Err` on spawn failure or timeout.
async fn run_direct_command(
    binary: String,
    args: Vec<String>,
    timeout: Duration,
) -> Result<(i32, String, String)> {
    let mut cmd = Command::new(&binary);
    cmd.args(&args)
        .kill_on_drop(true)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|err| anyhow!("Failed to spawn {binary}: {err}"))?;

    let output = wait_and_capture(&mut child, timeout, &binary).await;
    let exit_code = match output.exit {
        Some(Ok(status)) => status.code().unwrap_or(-1),
        Some(Err(err)) => {
            return Err(anyhow!("Error waiting for process {binary}: {err}"));
        }
        None => {
            debug!("DirectCommand did not complete within timeout: {timeout:?} Killed: {binary}");
            let stderr = output.stderr;
            return Err(CommandTimedOut {
                binary,
                timeout,
                stderr,
            }
            .into());
        }
    };

    Ok((exit_code, output.stdout, output.stderr))
}

/// This enables or disables the `thinkpad_acpi` kernel module `fan_control` option.
/// It also reloads the module so as to have immediate effect if possible.
pub async fn thinkpad_fan_control(enable: &bool) -> Result<()> {
    let fan_control_option = u8::from(*enable);
    let thinkpad_acpi_conf_file_path =
        PathBuf::from(THINKPAD_ACPI_CONF_PATH).join(THINKPAD_ACPI_CONF_FILE);
    let content = format!("options thinkpad_acpi fan_control={fan_control_option} ");
    cc_fs::create_dir_all(THINKPAD_ACPI_CONF_PATH).await?;
    cc_fs::write_string(thinkpad_acpi_conf_file_path, content).await?;
    let command_result =
        ShellCommand::new(RELOAD_THINKPAD_ACPI_MODULE_COMMAND, Duration::from_secs(1))
            .run()
            .await;
    match command_result {
        ShellCommandResult::Error(stderr) => Err(anyhow!(
            "Error trying to reload the thinkpad_acpi kernel module: {stderr}"
        )),
        ShellCommandResult::Success { stdout, stderr } => {
            debug!("ThinkPad ACPI Modprobe output: {stdout} - {stderr}");
            if stderr.is_empty() {
                Ok(())
            } else {
                Err(anyhow!(
                    "Error output received when trying to reload the thinkpad_acpi kernel module: {stderr}"
                ))
            }
        }
    }
}

fn limit_output_length(output: &mut String) {
    if output.len() > MAX_OUTPUT_LENGTH_BYTES {
        output.truncate(output.floor_char_boundary(MAX_OUTPUT_LENGTH_BYTES));
        output.push_str("... (truncated)");
    }
}

const GLOB_XAUTHORITY_PATH_GDM: &str = "/run/user/*/gdm/Xauthority";
const GLOB_XAUTHORITY_PATH_USER: &str = "/home/*/.Xauthority";
const GLOB_XAUTHORITY_PATH_SDDM: &str = "/run/sddm/xauth_*";
const GLOB_XAUTHORITY_PATH_SDDM_USER: &str = "/run/user/*/xauth_*";
const GLOB_XAUTHORITY_PATH_MUTTER_XWAYLAND_USER: &str = "/run/user/*/.*Xwaylandauth*";
const GLOB_XAUTHORITY_PATH_ROOT: &str = "/root/.Xauthority";

/// Searches for the Xauthority magic cookie on the system. Checks the XAUTHORITY
/// environment variable first, then searches common file paths used by various
/// display managers (GDM, SDDM, mutter-xwayland, etc.).
pub fn find_xauthority_path() -> Option<String> {
    if let Ok(environment_xauthority) = std::env::var("XAUTHORITY") {
        info!("Found existing Xauthority in the environment: {environment_xauthority}");
        return Some(environment_xauthority);
    }
    // glob() only fails for invalid pattern syntax; these are compile-time constants.
    let xauthority_path_opt = glob(GLOB_XAUTHORITY_PATH_GDM, Uninterruptible)
        .unwrap()
        .chain(glob(GLOB_XAUTHORITY_PATH_USER, Uninterruptible).unwrap())
        .chain(glob(GLOB_XAUTHORITY_PATH_SDDM, Uninterruptible).unwrap())
        .chain(glob(GLOB_XAUTHORITY_PATH_SDDM_USER, Uninterruptible).unwrap())
        .chain(glob(GLOB_XAUTHORITY_PATH_MUTTER_XWAYLAND_USER, Uninterruptible).unwrap())
        .chain(glob(GLOB_XAUTHORITY_PATH_ROOT, Uninterruptible).unwrap())
        .filter_map(Result::ok)
        .find(|path| path.is_absolute());
    if let Some(xauthority_path) = xauthority_path_opt {
        if let Some(xauthority_str) = xauthority_path.to_str() {
            info!("Xauthority found in file path: {xauthority_str}");
            return Some(xauthority_str.to_owned());
        }
    }
    None
}

/// Enforces a minimum delay after a device command completes.
/// Called at the end of each `apply_setting_*` method to guarantee the
/// configured minimum time gap before the next command can start.
pub async fn apply_device_command_delay(delay_millis: u16) {
    if delay_millis > 0 {
        crate::rt::sleep(Duration::from_millis(u64::from(delay_millis))).await;
    }
}

#[cfg(test)]
mod tests {
    use crate::repositories::utils::{
        captured_to_string, drain_pipe, limit_output_length, MAX_OUTPUT_LENGTH_BYTES,
        OUTPUT_CAPTURE_BYTES,
    };
    use std::ops::Not;
    use tokio::io::AsyncReadExt;

    #[tokio::test]
    async fn drain_keeps_the_start_of_a_pipe_and_reads_all_of_it() {
        // Goal: memory stays bounded, and the child is never left blocked on a full pipe.
        // Method: drain far more than the capture holds through a reader that is still usable
        // afterwards, and check that nothing is left to read from it.
        let written = vec![b'x'; OUTPUT_CAPTURE_BYTES * 8];
        let mut pipe = written.as_slice();
        let mut captured = Vec::new();
        drain_pipe(Some(&mut pipe), &mut captured).await.unwrap();
        assert_eq!(captured.len(), OUTPUT_CAPTURE_BYTES);
        let mut left = Vec::new();
        pipe.read_to_end(&mut left).await.unwrap();
        assert!(left.is_empty());
    }

    #[tokio::test]
    async fn drain_handles_a_missing_pipe() {
        // Goal: a child without a pipe captures nothing, and is not an error.
        // Method: drain `None`.
        let mut captured = Vec::new();
        drain_pipe(None::<&[u8]>, &mut captured).await.unwrap();
        assert!(captured.is_empty());
    }

    #[test]
    fn capture_cut_inside_a_char_decodes_cleanly() {
        // Goal: a capture that ends inside a char neither panics nor leaves a replacement char.
        // Method: offset a run of four byte chars through every alignment against the cut.
        for padding_bytes in 0..4 {
            let mut written = "a".repeat(padding_bytes);
            written.push_str(&"\u{1F600}".repeat(MAX_OUTPUT_LENGTH_BYTES));
            let captured = &written.as_bytes()[..OUTPUT_CAPTURE_BYTES];
            let output = captured_to_string(captured);
            assert!(output.ends_with("... (truncated)"));
            assert!(output.contains('\u{FFFD}').not());
            assert!(output.len() <= MAX_OUTPUT_LENGTH_BYTES + "... (truncated)".len());
        }
    }

    #[test]
    fn invalid_utf8_is_kept_rather_than_dropped() {
        // Goal: output that is not UTF-8 still reaches the caller. It used to be lost whole.
        // Method: decode a capture with an invalid byte in it.
        assert_eq!(
            captured_to_string(b"before\xFFafter"),
            "before\u{FFFD}after"
        );
    }

    #[test]
    fn output_at_the_limit_is_not_marked_as_truncated() {
        // Goal: the capture holds one byte past the limit so that only longer output is cut.
        // Method: decode exactly the limit, and one byte more.
        let at_limit = vec![b'x'; MAX_OUTPUT_LENGTH_BYTES];
        assert_eq!(captured_to_string(&at_limit).len(), MAX_OUTPUT_LENGTH_BYTES);
        let past_limit = vec![b'x'; OUTPUT_CAPTURE_BYTES];
        assert!(captured_to_string(&past_limit).ends_with("... (truncated)"));
    }

    // Should truncate the output string if it exceeds the maximum length and is ASCII-encoded
    #[test]
    fn should_truncate_output_if_exceeds_max_length_and_is_ascii() {
        let mut output = String::from(
            "nMy\n\r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",
        );
        limit_output_length(&mut output);
        assert_eq!(
            output,
            "nMy\n\rhNKPeX3FaVY5B4Z2yrZQhwRuPl1tLad2BWatY8946P0mWyYHDI6b2yPSb3h1wOeuCARUm2NqHk2srdXTBHUfLVgDXtSWttfWAoG8SI9ov2RKrDf9kcqkrCRrjfNuSQfQ4hsqbyfJb5CMMwoGVk7BtmbRkE9iH0qsfqd7NQGfUWv0Og2Mh9b1oZ4JsjF74hjlh7hmoqjgXxT35z4L6W7hTebrAXa8cVOWo7j0ZSJpOnXh9UBXRfsv0uBWykwo1uqiRzbeI0vp4Wwdnm40eWXA1H9J80pOQ5ooqGI9YUoeTCBLfFuu7Lwy6JkeqgSVQbKBHagZ6HXv4en5CAFN4mQGCSOlevkwHAcQIlCRcFNARRdtuIHGClovIczbSc17kckcvXnaPyRO2yScK0SAqdxtyvuW3YXZ1bTXAuHe5oT42hloGGfGycoT693L2HMPZAsnN4hLcc5fLKLW6R0UQWLDNnrLeiFyrV7MtDwdGoQVsH5Rhkwv3lIgCkShPqggrSMIV6joDg87SjQZhVBNcQAe1ZcCNzaGmqYrIg1mt2h3cyXZdMD8iMz6cyx3jUViMscgniegtgr1EmmmmMxgEGivwFTgoxFNdAC1a6ZoJbv9e6uqRNJ19OOpVxZeLrRfKVrwBTmUvmHO9040FLgPq1x1lZxXTC4NLLbdiNUxM5h0Z5fVn9xrta9hSv6B3NvgNMKbKZWVbOpD3C7NjlnS3e72IlLI2KIfv73DERk1jIf625TOSEAzUFG8uJfOr3nrNZGVKa8SlEoVPsghm1Yfsfn6oufQyyF9aKL9Apcw5jsmNJOcLelf7fd9xS8KyoZtyePCLvJbpS9kTzolSLdZmEVSlRs6xum4a7gnzSrBHmRQNtSB7oJfFMDJyWlb1ppOcWPKxn0IhRLQfLXwsUbsi5Q4MDOz2du0ScW45kAQejtNAzT8XljLo6lVQCFwti6vPZIPqOeThfPGaH678EgVNos27x7JSzI2SfsNjoaIgas7CWViXkDgh5aH7eJdFzJvn2OdNDQvDge6oCgWuRxj3oIhZ7ADH2vAdKM6v2EV6wgmD7Ihie2bQ2nI6EtwGAr6Hi2sv27xJYq45zsV9FvoeRNHQotJpYXJrgFZrpffvPiVCMbUw5XEsNgH4VtaaIHofL6ol0THeSxefmEBFeggfL2GR4H6JJ4YOfaIttLVbbgspsNWJyiBzCPGnxTw86Y26PW51vuwAuY68waUy9xsCqia7xxQizk1625NqC09mXD6BmvhTct4lUwzon8WTNnmB4SNmwHzOsRJj5UkQcJUl0emjAxEkObAKqU5woaPCcsZrucyu81C1yiVT6n3TUSN6ecx9M1exdw1bylfOXrs5tV9CsNM0GWqh2fEbEctEzcBWFivB9oPOXOGKYQ6CYLg3fWQNnyUGv73lvuipD84pxtloZM25KqSPaYg6EFgtTeCbV7Ozm4MFfifN7RWVkgGS2NXrANuMDc9cr6OtFTCTPnpMchMegOTrbhAabyrwpFsmrYsoW8YDxDAx2hvEfvyiXp64iNLKx5hVubriSDW4UTLdf1DvNbl5jIJLCq8eWsXGijWHLEljNl9xy8F9tmuMcsEgGvB8t30JmDsRt7FJESomJ8lVNeO7Y7Tv3PM5ajhLnSpiNRx4uJcZ6XLRsFkiIEHrC2JubSUkVFoptX6NNEbPzsiGwDZbwMk7KBimQM2yA0JFfQEb8LxyOQLpQpM4bD70dMfRJ4Y5rLN9HzSbwC1pFpY4w9pUS1P0dlZy77lq357wkz62I49dl8z1CKcZIkuXfZkyVn4qg26fAeRccz0QYAxnxIvPsruSt0i0EAMKg6cN7ay5JE60XMwGwNDc2KgYAys0y1xQt9xx4XaaF5aVhFVf1oG9nRUVH2bn9JIDwjFxgca1qBCZs5mzZH1TeXNFIbpJzPBAQ9iNr9P4l19jVI5v8l5jLpDyJfY4yCyjmMKsu3gpli1OC6M3ve3V8tDEs41ZTKHg3J... (truncated)"
        );
    }

    // Should not modify the output string if it is shorter than or equal to the maximum length
    #[test]
    fn should_not_modify_output_if_shorter_or_equal_to_max_length() {
        let mut output = String::from("Short output");
        limit_output_length(&mut output);
        assert_eq!(output, "Short output");
    }
}

// Gated: these spawn the host's /bin/sh and its tools, and bound wall-clock time.
#[cfg(all(test, feature = "gated-tests"))]
mod command_tests {
    use super::*;
    use std::ops::Not;
    use std::time::Instant;

    const TRUNCATED_OUTPUT_BYTES_MAX: usize = MAX_OUTPUT_LENGTH_BYTES + "... (truncated)".len();
    const PIPE_BUFFER_BYTES: usize = 64 * 1024;
    const TIMEOUT_GENEROUS: Duration = Duration::from_secs(10);
    const TIMEOUT_SHORT: Duration = Duration::from_secs(1);
    /// Under the 5 s the scripts sleep for, and loose enough for a loaded CI machine.
    const ELAPSED_MAX: Duration = Duration::from_secs(4);

    #[derive(Clone, Copy, Debug)]
    enum Runner {
        Shell,
        Direct,
    }

    const RUNNERS: [Runner; 2] = [Runner::Shell, Runner::Direct];

    async fn run(
        runner: Runner,
        script: &str,
        timeout: Duration,
    ) -> Result<(String, String), String> {
        match runner {
            Runner::Shell => {
                match run_shell_command(script.to_owned(), HashMap::new(), timeout).await {
                    ShellCommandResult::Success { stdout, stderr } => Ok((stdout, stderr)),
                    ShellCommandResult::Error(stderr) => Err(stderr),
                }
            }
            Runner::Direct => {
                let args = vec!["-c".to_owned(), script.to_owned()];
                match run_direct_command(SHELL.to_owned(), args, timeout).await {
                    Ok((0, stdout, stderr)) => Ok((stdout, stderr)),
                    Ok((_, _, stderr)) => Err(stderr),
                    Err(err) => Err(err.to_string()),
                }
            }
        }
    }

    fn fill_script(size_bytes: usize) -> String {
        format!("head -c {size_bytes} /dev/zero | tr '\\0' x")
    }

    #[tokio::test]
    async fn output_past_the_pipe_buffer_does_not_hang() {
        // Goal: a child that writes more than a pipe holds exits instead of blocking until the
        // timeout, and what comes back stays within the limit.
        // Method: write just past the pipe buffer and then 1 MiB, to each stream on each runner.
        for runner in RUNNERS {
            for size_bytes in [PIPE_BUFFER_BYTES + 1024, 1024 * 1024] {
                for redirect in ["", ">&2"] {
                    let script = format!("{{ {}; }} {redirect}", fill_script(size_bytes));
                    let start = Instant::now();
                    let result = run(runner, &script, TIMEOUT_GENEROUS).await;
                    assert!(start.elapsed() < ELAPSED_MAX, "{runner:?} {script}");
                    let (stdout, stderr) = result.unwrap();
                    assert!(stdout.len() <= TRUNCATED_OUTPUT_BYTES_MAX);
                    assert!(stderr.len() <= TRUNCATED_OUTPUT_BYTES_MAX);
                    let written = if redirect.is_empty() { stdout } else { stderr };
                    assert_eq!(written.len(), TRUNCATED_OUTPUT_BYTES_MAX);
                }
            }
        }
    }

    #[tokio::test]
    async fn neither_pipe_starves_the_other() {
        // Goal: both pipes are read at once. A reader that finished one pipe before starting the
        // other deadlocks with a child blocked on the other pipe.
        // Method: overfill stdout, then stderr, then stdout, so either read order would block.
        let fill = fill_script(PIPE_BUFFER_BYTES + 1024);
        let script = format!("{fill}; {{ {fill}; }} >&2; {fill}");
        for runner in RUNNERS {
            let start = Instant::now();
            let result = run(runner, &script, TIMEOUT_GENEROUS).await;
            assert!(start.elapsed() < ELAPSED_MAX, "{runner:?}");
            assert!(result.is_ok(), "{runner:?}");
        }
    }

    #[tokio::test]
    async fn long_non_ascii_output_stays_within_the_limit() {
        // Goal: the limit holds for non-ASCII output, which used to come back whole.
        // Method: overfill a pipe with a two byte char and check the length of what comes back.
        let script = format!("yes é | head -c {}", PIPE_BUFFER_BYTES + 1024);
        for runner in RUNNERS {
            let (stdout, _) = run(runner, &script, TIMEOUT_GENEROUS).await.unwrap();
            assert!(stdout.len() <= TRUNCATED_OUTPUT_BYTES_MAX, "{runner:?}");
            assert!(stdout.ends_with("... (truncated)"), "{runner:?}");
            assert!(stdout.contains('\u{FFFD}').not(), "{runner:?}");
        }
    }

    #[tokio::test]
    async fn timeout_kills_the_child_and_keeps_output_read_so_far() {
        // Goal: a child that overruns is killed at the timeout, and its output is not lost.
        // Method: the child writes its pid to stderr then becomes a long sleep. The error must
        // carry that pid, and that process must be gone.
        for runner in RUNNERS {
            let start = Instant::now();
            let result = run(runner, "echo $$ >&2; exec sleep 60", TIMEOUT_SHORT).await;
            assert!(start.elapsed() < ELAPSED_MAX, "{runner:?}");
            let message = result.unwrap_err();
            let pid = message.split_whitespace().last().unwrap();
            assert!(pid.parse::<u32>().is_ok(), "{runner:?}: {message}");
            assert!(
                PathBuf::from(format!("/proc/{pid}")).exists().not(),
                "{runner:?}"
            );
        }
    }

    #[tokio::test]
    async fn exit_status_is_trusted_when_a_descendant_holds_the_pipes() {
        // Goal: a child that exits 0 is a success, even though a background process it left
        // behind still holds its pipes, and that process is not killed.
        // Method: background a sleep past the timeout and report its pid on stdout.
        for runner in RUNNERS {
            let start = Instant::now();
            let result = run(runner, "sleep 5 & echo $!", TIMEOUT_SHORT).await;
            assert!(start.elapsed() < ELAPSED_MAX, "{runner:?}");
            let (pid, _) = result.unwrap();
            assert!(PathBuf::from(format!("/proc/{pid}")).exists(), "{runner:?}");
        }
    }

    #[tokio::test]
    async fn timeout_covers_reads_held_open_by_a_descendant() {
        // Goal: killing the child ends the run at the timeout, even with a process of its own
        // still holding the pipes.
        // Method: the shell waits on a sleep past the timeout. Killing the shell leaves the sleep.
        for runner in RUNNERS {
            let start = Instant::now();
            let result = run(runner, "sleep 5; true", TIMEOUT_SHORT).await;
            assert!(start.elapsed() < ELAPSED_MAX, "{runner:?}");
            assert!(result.is_err(), "{runner:?}");
        }
    }

    #[tokio::test]
    async fn a_descendant_that_keeps_writing_cannot_hold_the_caller() {
        // Goal: the read ends soon after the child exits, even if a pipe never goes quiet.
        // Method: leave a background writer running, with a timeout it cannot reach.
        for runner in RUNNERS {
            let start = Instant::now();
            let result = run(runner, "yes &", TIMEOUT_GENEROUS).await;
            assert!(start.elapsed() < ELAPSED_MAX, "{runner:?}");
            let (stdout, _) = result.unwrap();
            assert!(stdout.len() <= TRUNCATED_OUTPUT_BYTES_MAX, "{runner:?}");
        }
    }

    #[tokio::test]
    async fn closing_the_pipes_does_not_end_the_wait() {
        // Goal: the status is still the child's own when its pipes reach their end first.
        // Method: close both pipes, outlive that, then exit with a known code.
        for runner in RUNNERS {
            let script = "exec >&- 2>&-; sleep 0.2; exit 3";
            assert!(
                run(runner, script, TIMEOUT_GENEROUS).await.is_err(),
                "{runner:?}"
            );
            let script = "exec >&- 2>&-; sleep 0.2; exit 0";
            assert!(
                run(runner, script, TIMEOUT_GENEROUS).await.is_ok(),
                "{runner:?}"
            );
        }
    }
}
