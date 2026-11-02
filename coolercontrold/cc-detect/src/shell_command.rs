// SPDX-FileCopyrightText: 2026 Guy Boldon, megadjc and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Safe shell command execution with timeout and output limits.
//!
//! Based on the daemon's `ShellCommand` pattern, adapted for synchronous use
//! in the detection library.

use std::io::{self, ErrorKind, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
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
/// One byte past the limit, which is enough to see that the output was cut.
const OUTPUT_CAPTURE_BYTES: usize = MAX_OUTPUT_LENGTH_BYTES + 1;
const PIPE_READ_CHUNK_BYTES: usize = 8 * 1024;
const EXIT_POLL_INTERVAL: Duration = Duration::from_millis(50);
const PIPE_POLL_INTERVAL: Duration = Duration::from_millis(5);
/// A child's own output is in the pipe by the time it exits, so this only limits the caller's wait
/// on a descendant that outlives it and holds the pipe open. The drain itself carries on.
const PIPE_DRAIN_GRACE: Duration = Duration::from_millis(100);

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

        let deadline = Instant::now() + self.timeout;
        let (stdout, stderr) = match spawn_pipe_readers(&mut child) {
            Ok(readers) => readers,
            Err(err) => {
                warn!("Failed to read output for command: {}, {err}", self.command);
                let _ = child.kill();
                let _ = child.wait();
                return ShellCommandResult::Error(err.to_string());
            }
        };

        let status = match wait_for_exit(&mut child, deadline) {
            Ok(Some(status)) => status,
            Ok(None) => {
                debug!(
                    "Shell command timed out after {:?}, killing: {}",
                    self.timeout, self.command
                );
                let _ = child.kill();
                let _ = child.wait();
                return ShellCommandResult::Error(format!(
                    "command timed out after {:?}: {}, stderr: {}",
                    self.timeout,
                    self.command,
                    stderr.output()
                ));
            }
            Err(err) => {
                debug!("Error checking process status for: {}, {err}", self.command);
                return ShellCommandResult::Error(err.to_string());
            }
        };

        // The status is the child's own, so it holds however the reads end.
        wait_for_readers(
            &stdout,
            &stderr,
            deadline.min(Instant::now() + PIPE_DRAIN_GRACE),
        );
        if status.success() {
            ShellCommandResult::Success {
                stdout: stdout.output(),
                stderr: stderr.output(),
            }
        } else {
            ShellCommandResult::Error(stderr.output())
        }
    }
}

/// One pipe, read on its own thread while the child runs. A child blocks on a full pipe and never
/// exits, so the pipes cannot wait for the exit.
///
/// A descendant that outlives the child can hold a pipe open, and nothing interrupts a blocking
/// read, so the thread is left behind rather than joined. It ends when the last writer closes the
/// pipe: closing the read end sooner would kill that descendant with SIGPIPE at its next write.
struct PipeReader {
    captured: Arc<Mutex<Vec<u8>>>,
    thread: JoinHandle<()>,
}

impl PipeReader {
    fn spawn(pipe: Option<impl Read + Send + 'static>) -> io::Result<Self> {
        let captured = Arc::new(Mutex::new(Vec::with_capacity(OUTPUT_CAPTURE_BYTES)));
        let shared = Arc::clone(&captured);
        let thread = std::thread::Builder::new()
            .name("cc-detect-pipe".to_owned())
            .spawn(move || {
                if let Some(pipe) = pipe {
                    drain_pipe(pipe, &shared);
                }
            })?;
        Ok(Self { captured, thread })
    }

    fn output(&self) -> String {
        let captured = self.captured.lock().unwrap_or_else(PoisonError::into_inner);
        captured_to_string(&captured)
    }
}

fn captured_to_string(captured: &[u8]) -> String {
    debug_assert!(captured.len() <= OUTPUT_CAPTURE_BYTES);
    let mut output = String::from_utf8_lossy(captured).into_owned();
    limit_output_length(&mut output);
    output.trim().to_string()
}

fn spawn_pipe_readers(child: &mut Child) -> io::Result<(PipeReader, PipeReader)> {
    let stdout = PipeReader::spawn(child.stdout.take())?;
    let stderr = PipeReader::spawn(child.stderr.take())?;
    Ok((stdout, stderr))
}

/// Reads a pipe to its end, keeping the start of it. The rest is read and dropped, as the child
/// would block on a full pipe.
fn drain_pipe(mut pipe: impl Read, captured: &Mutex<Vec<u8>>) {
    let mut chunk = [0_u8; PIPE_READ_CHUNK_BYTES];
    loop {
        let read_bytes = match pipe.read(&mut chunk) {
            Ok(0) => return,
            Ok(read_bytes) => read_bytes,
            Err(err) if err.kind() == ErrorKind::Interrupted => continue,
            Err(err) => {
                debug!("Error reading command output: {err}");
                return;
            }
        };
        let mut captured = captured.lock().unwrap_or_else(PoisonError::into_inner);
        let room_bytes = OUTPUT_CAPTURE_BYTES.saturating_sub(captured.len());
        captured.extend_from_slice(&chunk[..read_bytes.min(room_bytes)]);
    }
}

/// `Ok(None)` if `deadline` passes first.
fn wait_for_exit(child: &mut Child, deadline: Instant) -> io::Result<Option<ExitStatus>> {
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(EXIT_POLL_INTERVAL);
    }
}

fn wait_for_readers(stdout: &PipeReader, stderr: &PipeReader, deadline: Instant) {
    while Instant::now() < deadline {
        if stdout.thread.is_finished() && stderr.thread.is_finished() {
            return;
        }
        std::thread::sleep(PIPE_POLL_INTERVAL);
    }
}

fn limit_output_length(output: &mut String) {
    if output.len() > MAX_OUTPUT_LENGTH_BYTES {
        // `truncate` panics inside a char. `floor_char_boundary` needs a newer Rust than this
        // crate's minimum.
        let end_index = (0..=MAX_OUTPUT_LENGTH_BYTES)
            .rev()
            .find(|index| output.is_char_boundary(*index))
            .unwrap_or(0);
        output.truncate(end_index);
        output.push_str("...[truncated]");
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn limit_cuts_on_a_char_boundary() {
        // Goal: a limit that lands inside a char does not panic. `truncate` would.
        // Method: offset a run of four byte chars through every alignment against the limit.
        for padding_bytes in 0..4 {
            let mut output = "a".repeat(padding_bytes);
            output.push_str(&"\u{1F600}".repeat(MAX_OUTPUT_LENGTH_BYTES));
            limit_output_length(&mut output);
            assert!(output.ends_with("...[truncated]"));
            assert!(output.len() <= MAX_OUTPUT_LENGTH_BYTES + "...[truncated]".len());
        }
    }

    #[test]
    fn whitespace_does_not_hide_a_cut() {
        // Goal: output cut at the capture size is marked, even if trimming brings it within the
        // limit.
        // Method: fill a capture with whitespace at its end, and with a newline at its start.
        let mut trailing = vec![b'x'; MAX_OUTPUT_LENGTH_BYTES];
        trailing.push(b' ');
        assert!(captured_to_string(&trailing).ends_with("...[truncated]"));
        let mut leading = vec![b'\n'];
        leading.extend_from_slice(&[b'x'; MAX_OUTPUT_LENGTH_BYTES]);
        assert!(captured_to_string(&leading).ends_with("...[truncated]"));
    }

    #[test]
    fn drain_keeps_the_start_of_a_pipe_and_reads_all_of_it() {
        // Goal: memory stays bounded, and the child is never left blocked on a full pipe.
        // Method: drain far more than the capture holds and check nothing is left to read.
        let captured = Mutex::new(Vec::new());
        let mut pipe = Cursor::new(vec![b'x'; OUTPUT_CAPTURE_BYTES * 8]);
        drain_pipe(&mut pipe, &captured);
        assert_eq!(captured.lock().unwrap().len(), OUTPUT_CAPTURE_BYTES);
        let mut left = Vec::new();
        pipe.read_to_end(&mut left).unwrap();
        assert!(left.is_empty());
    }

    #[test]
    fn a_missing_pipe_captures_nothing() {
        // Goal: a child without a pipe gives empty output, and is not an error.
        // Method: spawn a reader on `None` and read what it kept.
        let reader = PipeReader::spawn(None::<Cursor<Vec<u8>>>).unwrap();
        assert!(reader.output().is_empty());
    }
}

// Gated: these spawn the host's /bin/sh and its tools, and bound wall-clock time.
#[cfg(all(test, feature = "gated-tests"))]
mod tests {
    use super::*;
    use std::ops::Not;

    const TRUNCATED_OUTPUT_BYTES_MAX: usize = MAX_OUTPUT_LENGTH_BYTES + "...[truncated]".len();
    const PIPE_BUFFER_BYTES: usize = 64 * 1024;
    const TIMEOUT_GENEROUS: Duration = Duration::from_secs(10);
    const TIMEOUT_SHORT: Duration = Duration::from_secs(1);
    const TIMEOUT_TINY: Duration = Duration::from_millis(100);
    /// Under the 5 s the scripts sleep for, and loose enough for a loaded CI machine.
    const ELAPSED_MAX: Duration = Duration::from_secs(4);
    const POLL_INTERVAL: Duration = Duration::from_millis(20);
    const POLL_COUNT_MAX: usize = 200;

    fn fill_script(size_bytes: usize) -> String {
        format!("head -c {size_bytes} /dev/zero | tr '\\0' x")
    }

    fn path_appears(path: &std::path::Path) -> bool {
        for _ in 0..POLL_COUNT_MAX {
            if path.exists() {
                return true;
            }
            std::thread::sleep(POLL_INTERVAL);
        }
        false
    }

    #[test]
    fn output_past_the_pipe_buffer_does_not_hang() {
        // Goal: a child that writes more than a pipe holds exits instead of blocking until the
        // timeout, and what comes back stays within the limit.
        // Method: write just past the pipe buffer and then 1 MiB, to each stream.
        for size_bytes in [PIPE_BUFFER_BYTES + 1024, 1024 * 1024] {
            for redirect in ["", ">&2"] {
                let script = format!("{{ {}; }} {redirect}", fill_script(size_bytes));
                let start = Instant::now();
                let result = ShellCommand::new(&script, TIMEOUT_GENEROUS).run();
                assert!(start.elapsed() < ELAPSED_MAX, "{script}");
                let ShellCommandResult::Success { stdout, stderr } = result else {
                    panic!("expected success: {script}");
                };
                let written = if redirect.is_empty() { stdout } else { stderr };
                assert_eq!(written.len(), TRUNCATED_OUTPUT_BYTES_MAX);
            }
        }
    }

    #[test]
    fn neither_pipe_starves_the_other() {
        // Goal: both pipes are read at once. A reader that finished one pipe before starting the
        // other deadlocks with a child blocked on the other pipe.
        // Method: overfill stdout, then stderr, then stdout, so either read order would block.
        let fill = fill_script(PIPE_BUFFER_BYTES + 1024);
        let script = format!("{fill}; {{ {fill}; }} >&2; {fill}");
        let start = Instant::now();
        let result = ShellCommand::new(&script, TIMEOUT_GENEROUS).run();
        assert!(start.elapsed() < ELAPSED_MAX);
        assert!(matches!(result, ShellCommandResult::Success { .. }));
    }

    #[test]
    fn long_non_ascii_output_stays_within_the_limit() {
        // Goal: a limit that lands inside a char neither panics nor overruns.
        // Method: overfill a pipe with a two byte char, offset so the limit lands inside one.
        for padding in ["", "a"] {
            let script = format!(
                "printf '{padding}'; yes é | tr -d '\\n' | head -c {}",
                PIPE_BUFFER_BYTES + 1024
            );
            let result = ShellCommand::new(&script, TIMEOUT_GENEROUS).run();
            let ShellCommandResult::Success { stdout, .. } = result else {
                panic!("expected success: {script}");
            };
            assert!(stdout.len() <= TRUNCATED_OUTPUT_BYTES_MAX);
            assert!(stdout.ends_with("...[truncated]"));
        }
    }

    #[test]
    fn timeout_kills_the_child_and_keeps_output_read_so_far() {
        // Goal: a child that overruns is killed at the timeout, and its output is not lost.
        // Method: the child writes its pid to stderr then becomes a long sleep. The error must
        // carry that pid, and that process must be gone.
        let start = Instant::now();
        let result = ShellCommand::new("echo $$ >&2; exec sleep 60", TIMEOUT_SHORT).run();
        assert!(start.elapsed() < ELAPSED_MAX);
        let ShellCommandResult::Error(message) = result else {
            panic!("expected timeout error");
        };
        let pid = message.split_whitespace().last().unwrap();
        assert!(pid.parse::<u32>().is_ok(), "{message}");
        assert!(std::path::Path::new(&format!("/proc/{pid}")).exists().not());
    }

    #[test]
    fn exit_status_is_trusted_when_a_descendant_holds_the_pipes() {
        // Goal: a child that exits 0 is a success, even though a background process it left
        // behind still holds its pipes, and that process is not killed.
        // Method: background a sleep past the timeout and report its pid on stdout.
        let start = Instant::now();
        let result = ShellCommand::new("sleep 5 & echo $!", TIMEOUT_SHORT).run();
        assert!(start.elapsed() < ELAPSED_MAX);
        let ShellCommandResult::Success { stdout, .. } = result else {
            panic!("expected success");
        };
        assert!(std::path::Path::new(&format!("/proc/{stdout}")).exists());
    }

    #[test]
    fn timeout_covers_reads_held_open_by_a_descendant() {
        // Goal: killing the child ends the run at the timeout, even with a process of its own
        // still holding the pipes.
        // Method: the shell waits on a sleep past the timeout. Killing the shell leaves the sleep.
        let start = Instant::now();
        let result = ShellCommand::new("sleep 5; true", TIMEOUT_SHORT).run();
        assert!(start.elapsed() < ELAPSED_MAX);
        assert!(matches!(result, ShellCommandResult::Error(_)));
    }

    #[test]
    fn a_descendant_that_keeps_writing_cannot_hold_the_caller() {
        // Goal: the wait ends soon after the child exits, even if a pipe never goes quiet.
        // Method: leave a writer that outlasts ELAPSED_MAX, then kill it. `timeout` ends it if an
        // assert fails first.
        let start = Instant::now();
        let result = ShellCommand::new("timeout 5 yes & echo $! >&2", TIMEOUT_GENEROUS).run();
        let elapsed = start.elapsed();
        let ShellCommandResult::Success { stderr: pid, .. } = result else {
            panic!("expected success");
        };
        let killed = Command::new("kill").arg(&pid).status().unwrap();
        assert!(killed.success(), "the writer should still run");
        assert!(elapsed < ELAPSED_MAX);
    }

    #[test]
    fn a_descendant_can_still_write_after_the_caller_returns() {
        // Goal: nothing here ends a descendant. Closing the read end would kill it with SIGPIPE at
        // its next write.
        // Method: background a job that writes twice after the grace, as a reader that gives up
        // does so only once the first write wakes it. It then touches a marker it never reaches
        // if a write kills it. One child exits, the other is killed at the timeout.
        for (foreground, timeout) in [("", TIMEOUT_GENEROUS), ("exec sleep 60", TIMEOUT_TINY)] {
            let dir = tempfile::tempdir().unwrap();
            let marker = dir.path().join("survived");
            let job = format!(
                "(sleep 0.3; echo out; sleep 0.2; echo out; touch '{}')",
                marker.display()
            );
            let _ = ShellCommand::new(&format!("{job} & {foreground}"), timeout).run();
            assert!(path_appears(&marker), "{foreground}");
        }
    }

    #[test]
    fn closing_the_pipes_does_not_end_the_wait() {
        // Goal: the status is still the child's own when its pipes reach their end first.
        // Method: close both pipes, outlive that, then exit with a known code.
        let failing = ShellCommand::new("exec >&- 2>&-; sleep 0.2; exit 3", TIMEOUT_GENEROUS);
        assert!(matches!(failing.run(), ShellCommandResult::Error(_)));
        let passing = ShellCommand::new("exec >&- 2>&-; sleep 0.2; exit 0", TIMEOUT_GENEROUS);
        assert!(matches!(passing.run(), ShellCommandResult::Success { .. }));
    }

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
