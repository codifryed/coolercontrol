// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `/proc/cpuinfo`, read whole every poll for the package frequencies. One held descriptor and
//! one buffer sized at init make that read allocation-free.

use std::cell::RefCell;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::os::unix::fs::FileExt;
use std::path::Path;

use anyhow::Result;

use crate::repositories::cpu::percent::MAX_LOGICAL_CPUS;

/// Room added when a read finds the buffer full. The kernel hands out cpuinfo about a page per
/// read, so any step past a page works.
const GROWTH_BYTES: usize = 16 * 1024;

/// Bound on the buffer: 8 KiB for each of `MAX_LOGICAL_CPUS`, well above the about 2 KiB each
/// processor takes on x86 today.
const MAX_BYTES: usize = MAX_LOGICAL_CPUS * 8 * 1024;

/// Reads a signal may interrupt before the read gives up for this poll.
const MAX_INTERRUPTED_READS: u8 = 8;

pub struct CpuInfoFile {
    path: Box<Path>,
    /// Opened on the first read and held from then on, so a poll costs no open or close. A failed
    /// read drops it, so the next poll opens the file fresh.
    file: RefCell<Option<File>>,
    /// Holds the last read. Grows only when cpuinfo does, as when processors come online.
    buf: RefCell<Vec<u8>>,
}

impl CpuInfoFile {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.into(),
            file: RefCell::new(None),
            buf: RefCell::new(Vec::new()),
        }
    }

    /// Sizes the buffer for a cpuinfo of `len` bytes, so polls read it without growing.
    pub fn reserve(&self, len: usize) {
        let mut buf = self.buf.borrow_mut();
        // One growth step of headroom lets the file run a little longer than it did at init.
        let size = (len + GROWTH_BYTES).min(MAX_BYTES);
        if buf.len() < size {
            // Only the missing bytes: the buffer is held for the daemon's lifetime, so amortized
            // slack would never be used.
            let missing = size - buf.len();
            buf.reserve_exact(missing);
            buf.resize(size, 0);
        }
    }

    /// Reads the whole file and hands its text to `parse`.
    ///
    /// # Errors
    ///
    /// When the file cannot be opened or read, is larger than `MAX_BYTES`, or is not UTF-8.
    pub fn read_with<T>(&self, parse: impl FnOnce(&str) -> T) -> Result<T> {
        let file = match self.file.take() {
            Some(file) => file,
            None => File::open(&self.path)?,
        };
        let mut buf = self.buf.borrow_mut();
        // On an error the descriptor is dropped here, so the next poll opens the file fresh.
        let filled = read_all(&file, &mut buf)?;
        self.file.replace(Some(file));
        Ok(parse(std::str::from_utf8(&buf[..filled])?))
    }
}

/// Reads `file` from its start into `buf`, growing `buf` only when a read finds it full, and
/// returns how many bytes the file held.
fn read_all(file: &File, buf: &mut Vec<u8>) -> io::Result<usize> {
    let mut filled = 0;
    let mut interrupts_left = MAX_INTERRUPTED_READS;
    // Bounded: each pass returns, adds at least one byte to `filled` (capped by `MAX_BYTES`), or
    // spends one of `MAX_INTERRUPTED_READS`.
    loop {
        debug_assert!(filled <= buf.len());
        // Full: the file may hold more, so make room before asking for it.
        if filled == buf.len() {
            if buf.len() >= MAX_BYTES {
                return Err(ErrorKind::FileTooLarge.into());
            }
            buf.reserve_exact(GROWTH_BYTES);
            buf.resize(buf.len() + GROWTH_BYTES, 0);
        }
        // Each read starts where the last one ended, so the descriptor's own offset never
        // matters and every poll starts from the top.
        match file.read_at(&mut buf[filled..], filled as u64) {
            Ok(0) => return Ok(filled),
            Ok(read) => filled += read,
            Err(err) if err.kind() == ErrorKind::Interrupted && interrupts_left > 0 => {
                interrupts_left -= 1;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: a read returns exactly what the file holds now, across growth steps and after the
    /// file shrinks. Method: a file longer than two growth steps, then the same file rewritten
    /// shorter, read through one held descriptor.
    #[test]
    fn a_read_returns_the_whole_current_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cpuinfo");
        let long = "cpu MHz\t\t: 4433.776\n".repeat(2 * GROWTH_BYTES / 20 + 50);
        std::fs::write(&path, &long).unwrap();
        let cpuinfo = CpuInfoFile::new(&path);

        assert_eq!(cpuinfo.read_with(str::to_owned).unwrap(), long);

        std::fs::write(&path, "cpu MHz\t\t: 3590.008\n").unwrap();
        assert_eq!(
            cpuinfo.read_with(str::to_owned).unwrap(),
            "cpu MHz\t\t: 3590.008\n"
        );
    }

    /// Goal: once sized at init, polls read without allocating, the first one's open included.
    /// Method: reserve for the file, then count the allocations of two reads.
    #[test]
    fn a_reserved_read_allocates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cpuinfo");
        let content = "physical id\t: 0\ncpu MHz\t\t: 4433.776\n".repeat(1_000);
        std::fs::write(&path, &content).unwrap();
        let cpuinfo = CpuInfoFile::new(&path);
        cpuinfo.reserve(content.len());

        let ((first, second), allocations) = crate::alloc_count::count_allocations(|| {
            let first = cpuinfo.read_with(str::len).unwrap();
            (first, cpuinfo.read_with(str::len).unwrap())
        });

        assert_eq!((first, second), (content.len(), content.len()));
        assert_eq!(allocations, 0);
    }

    /// Goal: a file that could not be opened is tried again on the next poll. Method: read a
    /// missing path, create it, read again.
    #[test]
    fn a_failed_open_is_retried_on_the_next_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cpuinfo");
        let cpuinfo = CpuInfoFile::new(&path);

        assert!(cpuinfo.read_with(str::len).is_err());

        let content = "cpu MHz\t\t: 800.000\n";
        std::fs::write(&path, content).unwrap();
        assert_eq!(cpuinfo.read_with(str::to_owned).unwrap(), content);
    }
}
