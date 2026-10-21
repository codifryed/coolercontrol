// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::rt;
use anyhow::Result;
use log::debug;
use std::fmt::Display;
use std::fs::ReadDir;
use std::io::{Error, ErrorKind};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Upper bound for one sysfs value read by `read_sysfs_value`. Every numeric sysfs value the
/// daemon reads is a short number or flag; 64 bytes covers all of them with headroom.
pub const SYSFS_VALUE_MAX_BYTES: usize = 64;

/// Attempts at a read the kernel aborted before it produced anything. A blocking `pread` sees
/// `EINTR` from a signal handler without `SA_RESTART`, and the restart is ours to make.
pub const INTERRUPTED_READ_ATTEMPTS: u8 = 3;
const _: () = assert!(INTERRUPTED_READ_ATTEMPTS > 0);

/// Whether a failed read should be re-issued, given the attempts left after this one.
///
/// Only `EINTR`, deliberately. This is the per-tick path: a device that is merely slow must not
/// pay a doubled read every tick, which is what the detection re-probe and its wider
/// `cc_fs::is_transient` set are for.
pub fn should_reissue(attempts_left: u8, err: &Error) -> bool {
    attempts_left > 0 && err.kind() == ErrorKind::Interrupted
}

/// Records that an interrupted read is going round again, so a debug log shows the retry rather
/// than only the eventual failure.
pub fn log_reissue(path: &Path, attempts_left: u8) {
    debug!(
        "sysfs read at {} was interrupted, re-issuing ({attempts_left} of \
         {INTERRUPTED_READ_ATTEMPTS} attempts left)",
        path.display()
    );
}

/// Records that the attempts are spent, so the caller's own message is not the first hint that
/// anything was retried.
pub fn log_reissue_exhausted(path: &Path) {
    debug!(
        "sysfs read at {} still interrupted after {INTERRUPTED_READ_ATTEMPTS} attempts, giving up",
        path.display()
    );
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

/// Reads one small sysfs value into a fixed stack buffer, on the blocking pool.
///
/// Contents beyond `SYSFS_VALUE_MAX_BYTES` are cut off; `SysfsValue::parse` rejects a full buffer.
/// The daemon's own reads go through a device worker; this serves the inline `DeviceIo` tests use.
pub async fn read_sysfs_value(path: impl AsRef<Path>) -> Result<SysfsValue> {
    let path = path.as_ref().to_path_buf();
    rt::spawn_blocking(move || read_sysfs_value_blocking(&path)).await?
}

/// Reads one small sysfs value into a fixed stack buffer, as blocking calls, for a device's IO
/// thread, which has no runtime.
pub fn read_sysfs_value_blocking(path: &Path) -> Result<SysfsValue> {
    let file = std::fs::File::open(path)?;
    let mut buf = [0u8; SYSFS_VALUE_MAX_BYTES];
    let mut len = 0;
    let mut attempts = INTERRUPTED_READ_ATTEMPTS;
    // Bounded on both axes: every pass reads at least one byte, ends the read, or spends one of
    // `INTERRUPTED_READ_ATTEMPTS`, so passes are capped at the sum of the two.
    while len < SYSFS_VALUE_MAX_BYTES {
        match pread_reissuing(path, &file, &mut buf[len..], len as u64, &mut attempts)? {
            0 => break,
            bytes_read => len += bytes_read,
        }
    }
    debug_assert!(len <= SYSFS_VALUE_MAX_BYTES);
    Ok(SysfsValue { buf, len })
}

/// Most bytes a one-shot attribute read keeps. sysfs hands out at most one page per attribute, and
/// the largest page size Linux runs with is 64 KiB.
pub const ATTRIBUTE_MAX_BYTES: usize = 64 * 1024;

/// Reads a whole attribute as blocking calls, for a device's IO thread, which has no runtime: text
/// such as labels and `uevent`, or a binary page such as SCSI VPD.
///
/// # Errors
///
/// The open or read error, or `FileTooLarge` past `ATTRIBUTE_MAX_BYTES`.
pub fn read_bytes_blocking(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let file = std::fs::File::open(path)?;
    let mut contents = Vec::new();
    // One past the cap, so a file longer than it is told apart from one exactly at it. `read_to_end`
    // re-issues an interrupted read itself.
    let limit = u64::try_from(ATTRIBUTE_MAX_BYTES)? + 1;
    let read = file.take(limit).read_to_end(&mut contents)?;
    debug_assert_eq!(read, contents.len());
    if read > ATTRIBUTE_MAX_BYTES {
        return Err(Error::new(
            ErrorKind::FileTooLarge,
            format!("{} exceeds {ATTRIBUTE_MAX_BYTES} bytes", path.display()),
        )
        .into());
    }
    Ok(contents)
}

/// One blocking `pread`, re-issued while `should_reissue` allows, spending from `attempts`.
///
/// An interrupted `pread` read nothing, so it is re-issued at once.
///
/// # Errors
///
/// The read's error, once it is not worth re-issuing or `attempts` is spent.
pub fn pread_reissuing(
    path: &Path,
    file: &std::fs::File,
    buf: &mut [u8],
    offset: u64,
    attempts: &mut u8,
) -> std::io::Result<usize> {
    use std::os::unix::fs::FileExt;
    // Bounded by `attempts`, which drops by one per failure and returns the error at zero.
    loop {
        match file.read_at(buf, offset) {
            Ok(len) => {
                debug_assert!(len <= buf.len());
                return Ok(len);
            }
            Err(err) => {
                debug_assert!(*attempts > 0);
                *attempts -= 1;
                if should_reissue(*attempts, &err).not() {
                    if err.kind() == ErrorKind::Interrupted {
                        log_reissue_exhausted(path);
                    }
                    return Err(err);
                }
                log_reissue(path, *attempts);
            }
        }
    }
}

/// Reads the entire contents of a sysfs file into a UTF-8 encoded string.
///
/// Tests only, to check what a write left behind: the daemon's own text reads (names, labels,
/// serials) go through a device worker, `DeviceIo::read_text`.
#[cfg(test)]
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Reads the entire contents of a text file into a UTF-8 encoded string.
///
/// Returns an error if the file cannot be opened or read, or if the contents are not valid UTF-8.
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// For small sysfs attributes that are not valid UTF-8, so cannot go through `read_sysfs`.
/// SCSI VPD page 0x80 is the motivating case: it opens with `0x80`, an invalid lead byte.
pub async fn read_bytes(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    Ok(tokio::fs::read(path).await?)
}

/// Reads the entire contents of a file into a vector of bytes. Tailored for reading images, which
/// are typically larger than other files.
///
/// Returns an error if the file cannot be opened or read.
pub async fn read_image(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    Ok(tokio::fs::read(path).await?)
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

    /// Goal: a one-shot attribute read on a worker returns the whole file, past what a
    /// `SysfsValue` holds, binary bytes included. Method: read back a multi-line text file longer
    /// than `SYSFS_VALUE_MAX_BYTES`, a file holding a non-UTF-8 byte, and one exactly at the cap.
    #[test]
    fn a_blocking_attribute_read_returns_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let uevent = "DRIVER=nvme\nPCI_ID=144D:A80A\nPCI_SUBSYS_ID=144D:A801\nMODALIAS=pci:v0\n";
        assert!(uevent.len() > SYSFS_VALUE_MAX_BYTES);
        let text = dir.path().join("uevent");
        std::fs::write(&text, uevent).unwrap();
        let binary = dir.path().join("vpd_pg80");
        std::fs::write(&binary, [0x80, 0x00, 0x01, b'S']).unwrap();
        let full = dir.path().join("full");
        std::fs::write(&full, vec![b'x'; ATTRIBUTE_MAX_BYTES]).unwrap();

        assert_eq!(read_bytes_blocking(&text).unwrap(), uevent.as_bytes());
        assert_eq!(
            read_bytes_blocking(&binary).unwrap(),
            [0x80, 0x00, 0x01, b'S']
        );
        assert_eq!(
            read_bytes_blocking(&full).unwrap().len(),
            ATTRIBUTE_MAX_BYTES
        );
    }

    /// Goal: the read is bounded, and a missing file is an error rather than empty contents.
    /// Method: read a file one byte past the cap, and a path that does not exist.
    #[test]
    fn a_blocking_attribute_read_refuses_past_the_cap() {
        let dir = tempfile::tempdir().unwrap();
        let long = dir.path().join("long");
        std::fs::write(&long, vec![b'x'; ATTRIBUTE_MAX_BYTES + 1]).unwrap();

        let too_large = read_bytes_blocking(&long).unwrap_err();
        let missing = read_bytes_blocking(&dir.path().join("absent")).unwrap_err();

        assert_eq!(
            too_large.downcast_ref::<Error>().unwrap().kind(),
            ErrorKind::FileTooLarge
        );
        assert_eq!(
            missing.downcast_ref::<Error>().unwrap().kind(),
            ErrorKind::NotFound
        );
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
    #[cfg(feature = "gated-tests")]
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

    /// Goal: the blocking reader serves the device threads' one-shot reads, so it must return
    /// exactly what `read_sysfs_value` returns, at the buffer boundary and for a missing file.
    /// Method: read short, boundary and oversize files through both readers and compare bytes,
    /// then read a missing path through both and compare error kinds.
    #[test]
    fn read_sysfs_value_blocking_matches_the_async_reader() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let cases = [
                ("short", "42\n".to_owned()),
                ("no_newline", "0".to_owned()),
                ("fits", format!("{}\n", "7".repeat(62))),
                ("exact", format!("{}\n", "7".repeat(SYSFS_VALUE_MAX_BYTES))),
                ("over", format!("{}\n", "7".repeat(100))),
            ];
            for (name, contents) in &cases {
                let path = dir.path().join(name);
                std::fs::write(&path, contents).unwrap();
                let blocking = read_sysfs_value_blocking(&path).unwrap();
                let compio = read_sysfs_value(&path).await.unwrap();
                assert_eq!(
                    blocking.as_bytes(),
                    compio.as_bytes(),
                    "bytes differ for {name}"
                );
            }
            let missing = dir.path().join("missing");
            let blocking = read_sysfs_value_blocking(&missing).unwrap_err();
            let compio = read_sysfs_value(&missing).await.unwrap_err();
            let kind = |err: &anyhow::Error| err.downcast_ref::<Error>().map(Error::kind);
            assert_eq!(kind(&blocking), Some(ErrorKind::NotFound));
            assert_eq!(kind(&blocking), kind(&compio));
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
    #[cfg(feature = "gated-tests")]
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
