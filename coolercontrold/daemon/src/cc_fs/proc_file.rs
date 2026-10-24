// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Files the kernel generates from memory, with no device behind them, such as `/proc/stat` and
//! `/proc/cpuinfo`.
//!
//! A read of one of these cannot wait on hardware the way a hwmon attribute can, so it needs no
//! device thread. It is read on the runtime thread with plain positional reads, yielding to the
//! runtime after each one so other ready tasks run in between. tokio's file utilities would send
//! each read to the blocking pool, allocating a job and waking a thread for it. A held descriptor
//! and a buffer sized once keep a read free of allocations. Device-backed sysfs attributes go
//! through the hwmon repo's `DeviceIo` instead.

use std::cell::RefCell;
use std::fs::File;
use std::io::{self, ErrorKind};
use std::os::unix::fs::FileExt;
use std::path::Path;

use anyhow::Result;

use crate::rt;

/// Room added when a read finds the buffer full. procfs hands out about a page per read, so any
/// step past a page works.
const GROWTH_BYTES: usize = 16 * 1024;
const _: () = assert!(GROWTH_BYTES > 4096);

/// Reads a signal may interrupt before a read of the file gives up.
const MAX_INTERRUPTED_READS: u8 = 8;

pub struct ProcFile {
    path: Box<Path>,
    /// Bound on the buffer, from what the caller expects the file to hold.
    max_bytes: usize,
    /// Opened on the first read and held from then on, so a read costs no open or close. A failed
    /// read drops it, so the next read opens the file fresh.
    file: RefCell<Option<File>>,
    /// Holds the last read. Grows only when the file does.
    buf: RefCell<Vec<u8>>,
}

impl ProcFile {
    pub fn new(path: &Path, max_bytes: usize) -> Self {
        assert!(max_bytes > 0);
        Self {
            path: path.into(),
            max_bytes,
            file: RefCell::new(None),
            buf: RefCell::new(Vec::new()),
        }
    }

    /// Reads the whole file and hands its text to `parse`.
    ///
    /// Reads are expected one at a time. An overlapping read still works, but grows a buffer of
    /// its own.
    ///
    /// # Errors
    ///
    /// When the file cannot be opened or read, is larger than its bound, or is not UTF-8.
    pub async fn read_with<T>(&self, parse: impl FnOnce(&str) -> T) -> Result<T> {
        let file = match self.file.take() {
            Some(file) => file,
            None => File::open(&self.path)?,
        };
        // Taken out rather than borrowed, since the read yields between positional reads.
        let mut buf = self.buf.take();
        let filled = match read_all(&file, &mut buf, self.max_bytes).await {
            Ok(filled) => filled,
            Err(err) => {
                // The buffer keeps its room; the descriptor is dropped, so the next read opens
                // the file fresh.
                self.buf.replace(buf);
                return Err(err.into());
            }
        };
        debug_assert!(filled <= buf.len());
        debug_assert!(filled <= self.max_bytes);
        self.file.replace(Some(file));
        let parsed = std::str::from_utf8(&buf[..filled]).map(parse);
        self.buf.replace(buf);
        Ok(parsed?)
    }

    /// Sizes the buffer for a file of `len` bytes, so reads do not grow it.
    pub fn reserve(&self, len: usize) {
        let mut buf = self.buf.borrow_mut();
        // One growth step of headroom lets the file run a little longer than it does now.
        let size = (len + GROWTH_BYTES).min(self.max_bytes);
        if buf.len() < size {
            grow_to(&mut buf, size);
        }
        assert!(buf.len() >= size);
        assert!(buf.len() <= self.max_bytes);
    }
}

/// Reads an open file the kernel generates from memory into the caller's `buf`, and hands its text
/// to `parse`. Yields to the runtime after each positional read, as `ProcFile` does.
///
/// For a caller holding many such files, such as other processes' fdinfo, that shares one buffer
/// between them rather than keeping one per file. `buf` keeps its room between reads, so a read
/// allocates only when a file outgrows every one before it.
///
/// # Errors
///
/// When the file cannot be read, is larger than `max_bytes`, or is not UTF-8.
pub async fn read_proc_held<T>(
    file: &File,
    buf: &mut Vec<u8>,
    max_bytes: usize,
    parse: impl FnOnce(&str) -> T,
) -> Result<T> {
    assert!(max_bytes > 0);
    debug_assert!(buf.len() <= max_bytes);
    let filled = read_all(file, buf, max_bytes).await?;
    debug_assert!(filled <= buf.len());
    let parsed = std::str::from_utf8(&buf[..filled]).map(parse);
    Ok(parsed?)
}

/// Reads `file` from its start into `buf`, yielding to the runtime after each read, and returns
/// how many bytes the file held. Grows `buf` only when a read finds it full.
async fn read_all(file: &File, buf: &mut Vec<u8>, max_bytes: usize) -> io::Result<usize> {
    let mut filled = 0;
    let mut interrupts_left = MAX_INTERRUPTED_READS;
    // Bounded: each pass returns, adds at least one byte to `filled` (capped by `max_bytes`), or
    // spends one of `MAX_INTERRUPTED_READS`.
    loop {
        debug_assert!(filled <= buf.len());
        debug_assert!(buf.len() <= max_bytes);
        // Full: the file may hold more, so make room before asking for it.
        if filled == buf.len() {
            if buf.len() >= max_bytes {
                return Err(ErrorKind::FileTooLarge.into());
            }
            // One growth step, never past the bound.
            let size = (buf.len() + GROWTH_BYTES).min(max_bytes);
            grow_to(buf, size);
        }
        // Each read starts where the last one ended, so the descriptor's own offset never
        // matters and every read of the file starts from the top.
        match file.read_at(&mut buf[filled..], filled as u64) {
            Ok(0) => return Ok(filled),
            Ok(read) => filled += read,
            Err(err) if err.kind() == ErrorKind::Interrupted => {
                if interrupts_left > 0 {
                    interrupts_left -= 1;
                } else {
                    return Err(err);
                }
            }
            Err(err) => return Err(err),
        }
        rt::yield_now().await;
    }
}

/// Grows `buf` to `size` bytes, zero filled. Only the missing bytes are reserved: the buffer is
/// kept for good, so amortized slack would never be used.
fn grow_to(buf: &mut Vec<u8>, size: usize) {
    debug_assert!(buf.len() < size);
    let missing = size - buf.len();
    buf.reserve_exact(missing);
    buf.resize(size, 0);
    debug_assert_eq!(buf.len(), size);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rt::poll_to_end;

    /// Goal: a read returns exactly what the file holds now, across growth steps and after the
    /// file shrinks. Method: a file longer than two growth steps, then the same file rewritten
    /// shorter, read through one held descriptor.
    #[test]
    fn a_read_returns_the_whole_current_file() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("cpuinfo");
            let long = "cpu MHz\t\t: 4433.776\n".repeat(2 * GROWTH_BYTES / 20 + 50);
            std::fs::write(&path, &long).unwrap();
            let file = ProcFile::new(&path, 1024 * 1024);

            assert_eq!(file.read_with(str::to_owned).await.unwrap(), long);

            let short = "cpu MHz\t\t: 3590.008\n";
            std::fs::write(&path, short).unwrap();
            assert_eq!(file.read_with(str::to_owned).await.unwrap(), short);
        });
    }

    /// Goal: the runtime gets a turn after every positional read, so a long file never holds the
    /// thread for its whole read. Method: poll the read of a file that takes three reads into an
    /// unsized buffer, and count the polls.
    #[test]
    fn a_read_yields_after_each_positional_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cpuinfo");
        std::fs::write(&path, vec![b'x'; 40_000]).unwrap();
        let file = ProcFile::new(&path, 1024 * 1024);

        let (read, polls) = poll_to_end(file.read_with(str::len));

        assert_eq!(read.unwrap(), 40_000);
        // One read per growth step the file fills, each followed by a yield, then the last poll
        // finds the end.
        let reads = 40_000_u32.div_ceil(u32::try_from(GROWTH_BYTES).unwrap());
        assert_eq!(polls, reads + 1);
    }

    /// Goal: once sized, reads allocate nothing, the first one's open included. Method: reserve
    /// for the file, then count the allocations of two reads.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_reserved_read_allocates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cpuinfo");
        let content = "physical id\t: 0\ncpu MHz\t\t: 4433.776\n".repeat(1_000);
        std::fs::write(&path, &content).unwrap();
        let file = ProcFile::new(&path, 1024 * 1024);
        file.reserve(content.len());

        let ((first, second), allocations) = crate::alloc_count::count_allocations(|| {
            let (first, _) = poll_to_end(file.read_with(str::len));
            let (second, _) = poll_to_end(file.read_with(str::len));
            (first.unwrap(), second.unwrap())
        });

        assert_eq!((first, second), (content.len(), content.len()));
        assert_eq!(allocations, 0);
    }

    /// Goal: held reads through a shared buffer allocate nothing once it has room, and each
    /// returns only its own file. Method: read a longer file to size the buffer, then count the
    /// allocations of reading a shorter, different file through it.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_held_read_through_a_sized_buffer_allocates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let long_path = dir.path().join("long");
        let short_path = dir.path().join("short");
        std::fs::write(&long_path, "drm-client-id:\t1\n".repeat(200)).unwrap();
        std::fs::write(&short_path, "drm-client-id:\t2\n").unwrap();
        let long = File::open(&long_path).unwrap();
        let short = File::open(&short_path).unwrap();
        let mut buf = Vec::new();
        let (sized, _) = poll_to_end(read_proc_held(&long, &mut buf, 64 * 1024, str::len));
        assert!(sized.is_ok());

        let ((matched, _), allocations) = crate::alloc_count::count_allocations(|| {
            poll_to_end(read_proc_held(&short, &mut buf, 64 * 1024, |text| {
                text == "drm-client-id:\t2\n"
            }))
        });

        assert!(matched.unwrap());
        assert_eq!(allocations, 0);
    }

    /// Goal: a held read that fails is an error, and the shared buffer survives it. Method: read a
    /// directory, which opens but cannot be read, through a buffer with room.
    #[test]
    fn a_held_read_that_fails_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let unreadable = File::open(dir.path()).unwrap();
        let mut buf = vec![0; 1024];

        let (read, _) = poll_to_end(read_proc_held(&unreadable, &mut buf, 64 * 1024, str::len));

        assert!(read.is_err());
        assert_eq!(buf.len(), 1024);
    }

    /// Goal: a held file is read afresh each time, which is what lets a caller keep it open
    /// across ticks. Method: read, rewrite the file in place, and read again through the same
    /// descriptor.
    #[test]
    fn a_held_read_sees_the_files_current_contents() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fdinfo");
        std::fs::write(&path, "drm-engine-gfx:\t1 ns\n").unwrap();
        let file = File::open(&path).unwrap();
        let mut buf = Vec::new();
        let (first, _) = poll_to_end(read_proc_held(&file, &mut buf, 64 * 1024, str::to_owned));

        std::fs::write(&path, "drm-engine-gfx:\t22 ns\n").unwrap();
        let (second, _) = poll_to_end(read_proc_held(&file, &mut buf, 64 * 1024, str::to_owned));

        assert_eq!(first.unwrap(), "drm-engine-gfx:\t1 ns\n");
        assert_eq!(second.unwrap(), "drm-engine-gfx:\t22 ns\n");
    }

    /// Goal: a file larger than its bound is an error, not an unbounded buffer. Method: a 4 KiB
    /// file read with a 1 KiB bound.
    #[test]
    fn a_file_past_its_bound_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("stat");
        std::fs::write(&path, vec![b'x'; 4096]).unwrap();
        let file = ProcFile::new(&path, 1024);

        let (read, _) = poll_to_end(file.read_with(str::len));

        assert!(read.is_err());
    }

    /// Goal: a file that could not be opened is tried again on the next read. Method: read a
    /// missing path, create it, read again.
    #[test]
    fn a_failed_open_is_retried_on_the_next_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cpuinfo");
        let file = ProcFile::new(&path, 1024 * 1024);

        assert!(poll_to_end(file.read_with(str::len)).0.is_err());

        let content = "cpu MHz\t\t: 800.000\n";
        std::fs::write(&path, content).unwrap();
        assert_eq!(
            poll_to_end(file.read_with(str::to_owned)).0.unwrap(),
            content
        );
    }
}
