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

use anyhow::Result;
use std::fs::ReadDir;
use std::path::Path;

#[cfg(feature = "compio-rt")]
thread_local! {
    /// Reused buffer pool for sysfs reads: registered buffers plus completion IO are where the
    /// idle-CPU win comes from. Compio's pool is caller-managed, so we keep one per thread (the
    /// main runtime is single-threaded) and reuse it for every read. A 4 KiB buffer covers a full
    /// sysfs page; the count bounds how many reads can be in flight at once during the per-tick
    /// fan-out (extra reads simply wait for a free buffer).
    static SYSFS_BUFFER_POOL: std::rc::Rc<compio::runtime::BufferPool> = std::rc::Rc::new(
        compio::runtime::BufferPool::new(SYSFS_POOL_BUFFER_COUNT, SYSFS_POOL_BUFFER_BYTES)
            .expect("sysfs buffer pool"),
    );
}

/// Number of buffers in the sysfs pool. Must be a power of two (`io_uring` buffer-ring constraint).
#[cfg(feature = "compio-rt")]
const SYSFS_POOL_BUFFER_COUNT: u16 = 64;
/// Bytes per sysfs pool buffer. One memory page, which bounds any single sysfs read.
#[cfg(feature = "compio-rt")]
const SYSFS_POOL_BUFFER_BYTES: usize = 4096;

/// Reads the entire contents of a sysfs file into a UTF-8 encoded string.
///
/// Tailored for sysfs files, which are typically small and contain few values. Returns an error if
/// the file cannot be opened or read, or if the contents are not valid UTF-8.
///
/// This is the hot read path (every sensor, every tick). On the compio backend it does a single
/// managed read from a reused buffer pool: registered buffers plus completion-based IO are where the
/// idle-CPU win comes from. sysfs single-value files are far smaller than a pool buffer, so one read
/// captures the whole file.
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::read_to_string(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        use compio::io::AsyncReadManagedAt;
        let pool = SYSFS_BUFFER_POOL.with(std::rc::Rc::clone);
        let file = compio::fs::File::open(path.as_ref()).await?;
        // len 0 reads up to one pool buffer; sysfs single-value files fit easily.
        let buf = file.read_managed_at(&pool, 0, 0).await?;
        Ok(std::str::from_utf8(&buf)?.to_owned())
    }
}

/// Reads the entire contents of a text file into a UTF-8 encoded string.
///
/// Returns an error if the file cannot be opened or read, or if the contents are not valid UTF-8.
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::read_to_string(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(String::from_utf8(compio::fs::read(path.as_ref()).await?)?)
    }
}

/// Reads the entire contents of a file into a vector of bytes. Tailored for reading images, which
/// are typically larger than other files.
///
/// Returns an error if the file cannot be opened or read.
pub async fn read_image(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::read(path).await?)
    }
    #[cfg(feature = "compio-rt")]
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
