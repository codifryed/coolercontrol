// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::rt;
use anyhow::Result;
use log::trace;
use std::fmt::Display;
use std::fs::ReadDir;
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

/// Upper bound for one sysfs value read by `read_sysfs_value`. Every numeric sysfs value the
/// daemon reads is a short number or flag; 64 bytes covers all of them with headroom.
pub const SYSFS_VALUE_MAX_BYTES: usize = 64;

/// Attempts at a read the kernel aborted before it produced anything.
///
/// `io_uring` converts the kernel's internal `-ERESTARTSYS` into `EINTR` instead of restarting the
/// read the way a plain `read(2)` does (`io_uring/rw.c:io_fixup_restart_res`: "We can't just
/// restart the syscall, since previously submitted sqes may already be in progress. Just fail this
/// IO with EINTR."), so the restart is ours to make. Drivers that sleep interruptibly inside their
/// sysfs read hit this; see `cc_fs::is_transient`.
///
/// compio already re-issues for us inside `read_to_end_at`, `read_exact_at` and `write_all_at`, so
/// `read_txt` and every `cc_fs::write` are covered. The fixed-buffer readers here issue a bare
/// `read_at` and have to do it themselves. Only `EINTR` is retried: this is the per-tick path, and
/// a slow device must not pay a doubled read every tick.
pub const INTERRUPTED_READ_ATTEMPTS: u8 = 3;
const _: () = assert!(INTERRUPTED_READ_ATTEMPTS > 0);

/// Base pause before re-issuing an interrupted read, multiplied by the attempts already spent.
///
/// An immediate re-issue mostly fails again, because the interrupting condition is not a one-off.
/// `io_uring` issues these reads inline on the ring thread (kernfs advertises a `.poll`, so
/// `io_file_supports_nowait` says yes and the read is never punted to `io_wq`), and it re-arms
/// `TIF_NOTIFY_SIGNAL` on that same thread for every completion carrying `task_work`. A tick that
/// reads several devices therefore keeps the flag set for as long as the other reads take to
/// drain, and every re-issue inside that window aborts again. Observed on a Ryujin II with six
/// hwmon devices: the first three reads of the device failed on all attempts across about 5 ms,
/// then every later read in the same tick succeeded, once the other devices had finished.
///
/// Awaiting here is what fixes it rather than merely delaying: the sleep yields the runtime
/// instead of parking it, so the very completions that clear the flag get to run. It also stops
/// us spending a USB round trip per doomed attempt, since the driver sends its request before
/// waiting.
pub const INTERRUPTED_READ_BACKOFF: Duration = Duration::from_millis(2);

/// Whether a failed read should be re-issued, given the attempts left after this one.
///
/// Only `EINTR`, deliberately. This is the per-tick path: a device that is merely slow must not
/// pay a doubled read every tick, which is what the detection re-probe and its wider
/// `cc_fs::is_transient` set are for.
pub fn should_reissue(attempts_left: u8, err: &Error) -> bool {
    attempts_left > 0 && err.kind() == ErrorKind::Interrupted
}

/// How long to wait before the next re-issue, growing with the attempts already spent.
#[must_use]
pub fn reissue_backoff(attempts_left: u8) -> Duration {
    debug_assert!(attempts_left < INTERRUPTED_READ_ATTEMPTS);
    let spent = INTERRUPTED_READ_ATTEMPTS - attempts_left;
    debug_assert!(spent > 0);
    INTERRUPTED_READ_BACKOFF * u32::from(spent)
}

/// One sysfs value in a fixed stack buffer. Avoids the per-read heap ceremony of `read_sysfs`
/// (Vec growth + UTF-8 pass + String) on the per-tick sensor path.
#[derive(Debug)]
pub struct SysfsValue {
    buf: [u8; SYSFS_VALUE_MAX_BYTES],
    len: usize,
}

impl SysfsValue {
    /// Exactly the bytes read from the file.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        debug_assert!(self.len <= SYSFS_VALUE_MAX_BYTES);
        &self.buf[..self.len]
    }

    /// The value bytes with ascii whitespace trimmed from both ends.
    #[must_use]
    pub fn trimmed(&self) -> &[u8] {
        self.as_bytes().trim_ascii()
    }

    /// The trimmed value as UTF-8. Non-UTF-8 contents error as `InvalidData`.
    pub fn trimmed_str(&self) -> Result<&str> {
        std::str::from_utf8(self.trimmed())
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()).into())
    }

    /// Trims and parses the value. Errors are `InvalidData` `io::Error`s, matching the previous
    /// `check_parsing_*` helpers so caller downcasts keep working. A full buffer errors: the
    /// value may be cut off and must not parse to a wrong number.
    pub fn parse<T: FromStr>(&self) -> Result<T>
    where
        T::Err: Display,
    {
        if self.len == SYSFS_VALUE_MAX_BYTES {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!("sysfs value exceeds {SYSFS_VALUE_MAX_BYTES} bytes, possibly truncated"),
            )
            .into());
        }
        self.trimmed_str()?
            .parse::<T>()
            .map_err(|err| Error::new(ErrorKind::InvalidData, err.to_string()).into())
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Only tests call this today; it exists because a pub `len` demands it (clippy).
    #[allow(dead_code)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Wraps a filled read buffer. `len` is the byte count the read reported.
    #[must_use]
    pub fn from_read(buf: [u8; SYSFS_VALUE_MAX_BYTES], len: usize) -> Self {
        assert!(len <= SYSFS_VALUE_MAX_BYTES);
        Self { buf, len }
    }

    /// Test-only constructor so pure parsing tests need no file round-trip.
    #[cfg(test)]
    #[must_use]
    pub fn from_bytes(contents: &[u8]) -> Self {
        assert!(contents.len() <= SYSFS_VALUE_MAX_BYTES);
        let mut buf = [0u8; SYSFS_VALUE_MAX_BYTES];
        buf[..contents.len()].copy_from_slice(contents);
        Self {
            buf,
            len: contents.len(),
        }
    }
}

/// Reads one small sysfs value into a fixed stack buffer.
///
/// This is the hot numeric read path (every sensor, every tick). See `read_sysfs` for why a
/// shared buffer pool is not an option here; a per-call stack buffer sidesteps that class of
/// corruption entirely while avoiding the Vec + UTF-8 + String ceremony per scalar. Contents
/// beyond `SYSFS_VALUE_MAX_BYTES` are cut off; `SysfsValue::parse` rejects a full buffer.
pub async fn read_sysfs_value(path: impl AsRef<Path>) -> Result<SysfsValue> {
    use compio::buf::{IntoInner, IoBuf};
    use compio::io::AsyncReadAt;
    let file = compio::fs::File::open(path.as_ref()).await?;
    let mut buf = [0u8; SYSFS_VALUE_MAX_BYTES];
    let mut len = 0;
    let mut attempts = INTERRUPTED_READ_ATTEMPTS;
    // Bounded on both axes: every pass reads at least one byte, ends the read, or spends one of
    // `INTERRUPTED_READ_ATTEMPTS`, so passes are capped at the sum of the two. The array
    // round-trips through the op by value; no buffer allocation.
    while len < SYSFS_VALUE_MAX_BYTES {
        let compio::BufResult(result, slice) = file.read_at(buf.slice(len..), len as u64).await;
        buf = slice.into_inner();
        match result {
            Ok(0) => break,
            Ok(bytes_read) => len += bytes_read,
            Err(err) => {
                debug_assert!(attempts > 0);
                attempts -= 1;
                if should_reissue(attempts, &err).not() {
                    return Err(err.into());
                }
                trace!(
                    "sysfs read interrupted, re-issuing: {}",
                    path.as_ref().display()
                );
                rt::sleep(reissue_backoff(attempts)).await;
            }
        }
    }
    debug_assert!(len <= SYSFS_VALUE_MAX_BYTES);
    Ok(SysfsValue { buf, len })
}

/// Reads the entire contents of a sysfs file into a UTF-8 encoded string.
///
/// Tailored for sysfs files, which are typically small and contain few values. Returns an error if
/// the file cannot be opened or read, or if the contents are not valid UTF-8. Numeric hot-path
/// reads belong on `read_sysfs_value`; this remains for genuine text reads (names, labels,
/// serials).
///
/// The idle-CPU win comes from compio's completion-based IO. A managed buffer pool was tried here
/// for registered buffers, but it corrupts/fails the per-tick concurrent fan-out (many reads share
/// one pool over the `io_uring` buffer ring: cross-contaminated data or "flags are invalid"). The
/// plain read is correct and still completion-based.
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    {
        Ok(String::from_utf8(compio::fs::read(path.as_ref()).await?)?)
    }
}

/// Reads the entire contents of a text file into a UTF-8 encoded string.
///
/// Returns an error if the file cannot be opened or read, or if the contents are not valid UTF-8.
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    {
        Ok(String::from_utf8(compio::fs::read(path.as_ref()).await?)?)
    }
}

/// For small sysfs attributes that are not valid UTF-8, so cannot go through `read_sysfs`.
/// SCSI VPD page 0x80 is the motivating case: it opens with `0x80`, an invalid lead byte.
pub async fn read_bytes(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    {
        Ok(compio::fs::read(path.as_ref()).await?)
    }
}

/// Reads the entire contents of a file into a vector of bytes. Tailored for reading images, which
/// are typically larger than other files.
///
/// Returns an error if the file cannot be opened or read.
pub async fn read_image(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    {
        Ok(compio::fs::read(path.as_ref()).await?)
    }
}

/// Reads the contents of a directory.
///
/// Returns an iterator over the entries of the directory at `path`, or an error if `path` is not a
/// directory or any I/O fails. As a wrapper for `std::fs::read_dir` it should be called sparingly,
/// but is generally very fast and only used during application startup.
pub fn read_dir(path: impl AsRef<Path>) -> Result<ReadDir> {
    Ok(std::fs::read_dir(path)?)
}

/// Reads a symbolic link, returning the path it points to.
///
/// Sync `std::fs` wrapper: compio exposes no async `read_link`. Used by the AMD DRM fdinfo scan to
/// resolve `/proc/<pid>/fd` entries to their targets.
pub fn read_link(path: impl AsRef<Path>) -> Result<PathBuf> {
    Ok(std::fs::read_link(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "gated-tests")]
    use std::ops::Not;

    /// Goal: an interrupted read must be re-issued. `io_uring` hands us `EINTR` for a read the
    /// kernel aborted before it produced anything, instead of restarting it the way a plain
    /// `read(2)` does, so giving up loses a sensor value that was never actually read. Method:
    /// exercise the policy directly; a real `EINTR` needs a driver that sleeps interruptibly
    /// inside its sysfs read and cannot be produced from a regular file.
    #[test]
    fn an_interrupted_read_is_reissued_while_attempts_remain() {
        let interrupted = Error::from(ErrorKind::Interrupted);
        assert!(should_reissue(INTERRUPTED_READ_ATTEMPTS - 1, &interrupted));
        assert!(should_reissue(1, &interrupted));
    }

    /// Goal: the wait before a re-issue must grow, and must never be zero. An immediate re-issue
    /// lands inside the same burst of completions that caused the interrupt, so it fails again;
    /// the wait is what lets that burst drain. Method: walk every reachable attempt count.
    #[test]
    fn the_reissue_backoff_grows_and_is_never_zero() {
        let mut previous = Duration::ZERO;
        for attempts_left in (0..INTERRUPTED_READ_ATTEMPTS).rev() {
            let wait = reissue_backoff(attempts_left);
            assert!(
                wait > previous,
                "backoff must grow, got {wait:?} after {previous:?}"
            );
            previous = wait;
        }
        assert_eq!(
            reissue_backoff(INTERRUPTED_READ_ATTEMPTS - 1),
            INTERRUPTED_READ_BACKOFF
        );
    }

    /// Goal: the re-issue must be bounded, or a persistently interrupted read spins forever on
    /// the per-tick path. Method: spend the last attempt and assert the policy gives up.
    #[test]
    fn the_last_attempt_does_not_reissue() {
        let interrupted = Error::from(ErrorKind::Interrupted);
        assert!(should_reissue(0, &interrupted).not());
    }

    /// Goal: only `EINTR`. A slow or absent device must not pay a doubled read every tick; the
    /// one-shot detection re-probe is what covers the wider transient set. Method: one error per
    /// kind these sysfs paths actually see.
    #[test]
    fn other_failures_are_not_reissued() {
        for kind in [
            ErrorKind::NotFound,
            ErrorKind::Unsupported,
            ErrorKind::PermissionDenied,
            ErrorKind::TimedOut,
            ErrorKind::WouldBlock,
            ErrorKind::InvalidData,
        ] {
            let err = Error::from(kind);
            assert!(
                should_reissue(INTERRUPTED_READ_ATTEMPTS, &err).not(),
                "{kind:?} must not be re-issued"
            );
        }
    }

    /// Goal: `read_sysfs` must return EXACTLY the file's bytes, no stale/garbage tail. A wrong
    /// length leaves trailing bytes that break numeric parsing ("invalid digit found in string").
    /// Method: write a longer value first, then shorter ones, and read each back asserting exact
    /// equality. A reused buffer with bad length tracking would leak the longer value's tail into a
    /// later short read.
    #[test]
    fn read_sysfs_returns_exact_contents() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let cases = [
                ("long", "1234567890\n"),
                ("short", "42\n"),
                ("no_newline", "0"),
                ("byte", "255\n"),
            ];
            for (name, contents) in cases {
                std::fs::write(dir.path().join(name), contents).unwrap();
            }
            for (name, contents) in cases {
                let read = read_sysfs(dir.path().join(name)).await.unwrap();
                assert_eq!(
                    read,
                    contents,
                    "read_sysfs returned wrong bytes for {name} (len {} vs {})",
                    read.len(),
                    contents.len()
                );
            }
        });
    }

    /// Goal: reading a real sysfs file (kernfs, reports size 4096, generated per read) must return
    /// only its actual bytes. This reproduces the live-daemon corruption that regular tempfiles do
    /// not. Method: find a readable numeric hwmon input and assert the value parses cleanly and is
    /// short. Skips when /sys has no readable hwmon input (e.g. a CI sandbox). Gated because its
    /// coverage varies with the build host's hwmon; CI runs it via `--features gated-tests`.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn read_sysfs_real_file_has_no_stale_tail() {
        crate::rt::test_runtime(async {
            let Some(path) = first_readable_hwmon_input() else {
                eprintln!("no readable hwmon *_input found; skipping sysfs read test");
                return;
            };
            let read = read_sysfs(&path).await.unwrap();
            assert!(
                read.len() < 64,
                "sysfs read returned a stale/garbage tail: len {} for {}",
                read.len(),
                path.display()
            );
            assert!(
                read.trim().parse::<i64>().is_ok(),
                "sysfs value did not parse cleanly: {read:?} for {}",
                path.display()
            );
        });
    }

    /// Goal: concurrent `read_sysfs` calls (as the hwmon hot path does each tick) must each return
    /// their own file's bytes. Method: fan many simultaneous reads of distinct, varied-length files
    /// through a moro scope and assert exact content, which catches cross-contamination between
    /// in-flight completion-based reads.
    #[test]
    fn read_sysfs_concurrent_reads_are_not_cross_contaminated() {
        crate::rt::test_runtime(async {
            const N: usize = 256; // high concurrency to stress io_uring ring reuse
            let dir = tempfile::tempdir().unwrap();
            let mut expected = Vec::with_capacity(N);
            for i in 0..N {
                let contents = format!("{}\n", "9".repeat((i % 17) + 1));
                std::fs::write(dir.path().join(i.to_string()), &contents).unwrap();
                expected.push(contents);
            }
            let results: std::rc::Rc<std::cell::RefCell<Vec<Option<String>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(vec![None; N]));
            moro_local::async_scope!(|scope| {
                for i in 0..N {
                    let base = dir.path().to_path_buf();
                    let results = std::rc::Rc::clone(&results);
                    scope.spawn(async move {
                        let read = read_sysfs(base.join(i.to_string())).await.unwrap();
                        results.borrow_mut()[i] = Some(read);
                    });
                }
            })
            .await;
            let results = results.borrow();
            for i in 0..N {
                assert_eq!(
                    results[i].as_deref(),
                    Some(expected[i].as_str()),
                    "concurrent read {i} mismatched (cross-contaminated buffer?)"
                );
            }
        });
    }

    /// Goal: `read_sysfs_value` must return EXACTLY the file's bytes, no stale/garbage tail.
    /// Method: same case table as the `read_sysfs` test; a fill loop with bad length tracking
    /// would leak a longer value's tail into a later short read.
    #[test]
    fn read_sysfs_value_returns_exact_contents() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let cases = [
                ("long", "1234567890\n"),
                ("short", "42\n"),
                ("no_newline", "0"),
                ("byte", "255\n"),
            ];
            for (name, contents) in cases {
                std::fs::write(dir.path().join(name), contents).unwrap();
            }
            for (name, contents) in cases {
                let value = read_sysfs_value(dir.path().join(name)).await.unwrap();
                assert_eq!(
                    value.as_bytes(),
                    contents.as_bytes(),
                    "read_sysfs_value returned wrong bytes for {name} (len {} vs {})",
                    value.len(),
                    contents.len()
                );
            }
        });
    }

    /// Goal: `SysfsValue::parse` must trim ascii whitespace and parse into every numeric type the
    /// hwmon path uses, and reject junk as `InvalidData`. Method: a positive case table over the
    /// migrated target types, then negative cases (empty file, garbage, non-UTF-8) asserting the
    /// io error kind that `is_kernel_refusal`-style downcasts rely on.
    #[test]
    fn read_sysfs_value_parse_trims_and_types() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let write = |name: &str, contents: &[u8]| {
                std::fs::write(dir.path().join(name), contents).unwrap();
                dir.path().join(name)
            };
            let value = read_sysfs_value(write("millideg", b"45000\n"))
                .await
                .unwrap();
            assert_eq!(value.parse::<i32>().unwrap(), 45000);
            assert_eq!(value.parse::<u32>().unwrap(), 45000);
            assert_eq!(value.parse::<u64>().unwrap(), 45000);
            assert!((value.parse::<f64>().unwrap() - 45000.0).abs() < f64::EPSILON);
            let value = read_sysfs_value(write("spaced", b" 42 \n")).await.unwrap();
            assert_eq!(value.parse::<u8>().unwrap(), 42);
            let value = read_sysfs_value(write("zero", b"0")).await.unwrap();
            assert_eq!(value.parse::<u8>().unwrap(), 0);
            let value = read_sysfs_value(write("negative", b"-2000\n"))
                .await
                .unwrap();
            assert_eq!(value.parse::<i32>().unwrap(), -2000);
            for (name, contents) in [
                ("empty", b"".as_slice()),
                ("garbage", b"not-a-number\n".as_slice()),
                ("non_utf8", &[0xFF, 0xFE, 0x0A]),
            ] {
                let value = read_sysfs_value(write(name, contents)).await.unwrap();
                assert_eq!(value.is_empty(), contents.is_empty());
                let err = value.parse::<i32>().unwrap_err();
                let io_err = err.downcast_ref::<Error>().unwrap();
                assert_eq!(
                    io_err.kind(),
                    ErrorKind::InvalidData,
                    "wrong error kind for {name}"
                );
            }
        });
    }

    /// Goal: values at the buffer boundary must behave predictably: 63 bytes parses, a full
    /// buffer (64 or more bytes in the file) is rejected as possibly truncated instead of parsing
    /// a cut-off number. Method: files of exactly 63, 64, and 100 bytes.
    #[test]
    fn read_sysfs_value_boundary_lengths() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let cases = [
                ("fits", 62, true),
                ("exact", SYSFS_VALUE_MAX_BYTES, false),
                ("over", 100, false),
            ];
            for (name, digit_count, parses) in cases {
                let contents = format!("{}\n", "7".repeat(digit_count));
                std::fs::write(dir.path().join(name), &contents).unwrap();
                let value = read_sysfs_value(dir.path().join(name)).await.unwrap();
                assert!(value.len() <= SYSFS_VALUE_MAX_BYTES);
                // f64 accepts any digit-string length, so only the buffer boundary decides.
                assert_eq!(
                    value.parse::<f64>().is_ok(),
                    parses,
                    "unexpected parse result for {name} ({digit_count} digits)"
                );
            }
        });
    }

    /// Goal: a missing file must surface `io::ErrorKind::NotFound` through the anyhow chain.
    /// `verify_file_exists` (custom sensors) and the kernel-refusal check (fans) downcast for it.
    /// Method: read a nonexistent path and walk the error chain.
    #[test]
    fn read_sysfs_value_notfound_downcasts() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let err = read_sysfs_value(dir.path().join("missing"))
                .await
                .unwrap_err();
            let found = err.chain().any(|cause| {
                cause
                    .downcast_ref::<Error>()
                    .is_some_and(|io_err| io_err.kind() == ErrorKind::NotFound)
            });
            assert!(found, "NotFound io::Error not in chain: {err:?}");
        });
    }

    /// Goal: concurrent `read_sysfs_value` calls (as the hwmon hot path does each tick) must each
    /// return their own file's bytes. Stack buffers make cross-contamination impossible by
    /// construction, but this guards io_uring ring regressions. Method: port of the `read_sysfs`
    /// fan-out test.
    #[test]
    fn read_sysfs_value_concurrent_reads_are_not_cross_contaminated() {
        crate::rt::test_runtime(async {
            const N: usize = 256; // high concurrency to stress io_uring ring reuse
            let dir = tempfile::tempdir().unwrap();
            let mut expected = Vec::with_capacity(N);
            for i in 0..N {
                let contents = format!("{}\n", "9".repeat((i % 17) + 1));
                std::fs::write(dir.path().join(i.to_string()), &contents).unwrap();
                expected.push(contents);
            }
            let results: std::rc::Rc<std::cell::RefCell<Vec<Option<Vec<u8>>>>> =
                std::rc::Rc::new(std::cell::RefCell::new(vec![None; N]));
            moro_local::async_scope!(|scope| {
                for i in 0..N {
                    let base = dir.path().to_path_buf();
                    let results = std::rc::Rc::clone(&results);
                    scope.spawn(async move {
                        let value = read_sysfs_value(base.join(i.to_string())).await.unwrap();
                        results.borrow_mut()[i] = Some(value.as_bytes().to_vec());
                    });
                }
            })
            .await;
            let results = results.borrow();
            for i in 0..N {
                assert_eq!(
                    results[i].as_deref(),
                    Some(expected[i].as_bytes()),
                    "concurrent read {i} mismatched (cross-contaminated buffer?)"
                );
            }
        });
    }

    /// Goal: reading a real sysfs file (kernfs, reports size 4096, generated per read) must
    /// return only its actual bytes through the fixed-buffer path. Method and gating identical to
    /// the `read_sysfs` variant above.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn read_sysfs_value_real_file_has_no_stale_tail() {
        crate::rt::test_runtime(async {
            let Some(path) = first_readable_hwmon_input() else {
                eprintln!("no readable hwmon *_input found; skipping sysfs value read test");
                return;
            };
            let value = read_sysfs_value(&path).await.unwrap();
            assert!(
                value.len() < SYSFS_VALUE_MAX_BYTES,
                "sysfs value read returned a stale/garbage tail: len {} for {}",
                value.len(),
                path.display()
            );
            assert!(
                value.parse::<i64>().is_ok(),
                "sysfs value did not parse cleanly: {:?} for {}",
                value.as_bytes(),
                path.display()
            );
        });
    }

    #[cfg(feature = "gated-tests")]
    fn first_readable_hwmon_input() -> Option<std::path::PathBuf> {
        let hwmons = std::fs::read_dir("/sys/class/hwmon").ok()?;
        for hwmon in hwmons.flatten() {
            let Ok(files) = std::fs::read_dir(hwmon.path()) else {
                continue;
            };
            for file in files.flatten() {
                let name = file.file_name();
                let name = name.to_string_lossy();
                let is_input = (name.starts_with("fan") || name.starts_with("temp"))
                    && name.ends_with("_input");
                if is_input.not() {
                    continue;
                }
                // Many inputs open but return ENODATA on read (idle fan tach, disabled
                // sensor). Require a clean numeric stdlib read so the test targets a
                // genuinely readable input, not merely an openable one.
                let Ok(contents) = std::fs::read_to_string(file.path()) else {
                    continue;
                };
                if contents.len() < 64 && contents.trim().parse::<i64>().is_ok() {
                    return Some(file.path());
                }
            }
        }
        None
    }
}
