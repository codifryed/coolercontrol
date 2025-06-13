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

#[cfg(feature = "io_uring")]
use crate::cc_fs::{BufferSize, POOL};
use anyhow::Result;
use std::fs::ReadDir;
use std::path::Path;
#[cfg(feature = "io_uring")]
use tokio_uring::fs::File;

/// Reads the entire contents of a sysfs file into a UTF-8 encoded string.
///
/// This function is tailored for sysfs files, which are typically small and
/// contain few values. It will return an error if the file cannot be
/// opened, read, or if any I/O operations fail during the process.
#[cfg(feature = "io_uring")]
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    let data = read(path, BufferSize::Small).await?;
    Ok(unsafe { String::from_utf8_unchecked(data) })
}

#[cfg(not(feature = "io_uring"))]
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Reads the entire contents of a text file into a UTF-8 encoded string.
///
/// This function will return an error if the file cannot be opened, read, or
/// if any I/O operations fail during the process. It will also return an error
/// if the file's contents are not valid UTF-8.
#[cfg(feature = "io_uring")]
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    read_to_string(path, BufferSize::Medium).await
}

#[cfg(not(feature = "io_uring"))]
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Reads the entire contents of a file into a vector of bytes. This function
/// is tailored for reading images, which are typically larger than other
/// files.
///
/// This function will return an error if the file cannot be opened, read, or
/// if any I/O operations fail during the process.
#[cfg(feature = "io_uring")]
pub async fn read_image(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    read(path, BufferSize::Large).await
}

#[cfg(not(feature = "io_uring"))]
pub async fn read_image(path: impl AsRef<Path>) -> Result<Vec<u8>> {
    Ok(tokio::fs::read(path).await?)
}

/// Reads the contents of a directory.
///
/// This function returns an iterator over the entries of the directory
/// at the given `path`. It will return an error if `path` does not point
/// to a directory, or if any I/O operations fail during the process.
///
/// As this function is a wrapper for `std::fs::read_dir` it should be called sparingly,
/// but is generally very fast and only used during application startup.
/// # Returns
///
/// Returns a `Result` containing a `ReadDir` iterator if successful, or an error
/// if the operation fails.
pub fn read_dir(path: impl AsRef<Path>) -> Result<ReadDir> {
    Ok(std::fs::read_dir(path)?)
}

/// Asynchronously reads the entire contents of a file into a vector of bytes.
///
/// This function opens a file at the given `path` and reads its contents in
/// chunks of size specified by the `BufferSize`. It continues reading until
/// the end of the file is reached, accumulating all data into a `Vec<u8>`.
///
/// # Parameters
///
/// - `path`: The file path to read from. It can be any type that implements
///   `AsRef<Path>`.
/// - `bs`: The buffer size to use for each read operation.
///
/// # Returns
///
/// Returns a `Result` containing a `Vec<u8>` with the file's contents if
/// successful, or an error if the operation fails.
///
/// # Errors
///
/// This function will return an error if the file cannot be opened, read, or
/// if any I/O operations fail during the process.
///
/// # Examples
///
/// ```
/// use my_crate::fs::{read, BufferSize};
///
/// # async fn example() -> anyhow::Result<()> {
/// let contents = read("path/to/file.txt", BufferSize::default()).await?;
/// println!("File contents: {:?}", contents);
/// # Ok(())
/// # }
/// ```
#[cfg(feature = "io_uring")]
async fn read(path: impl AsRef<Path>, bs: BufferSize) -> Result<Vec<u8>> {
    let file = File::open(path).await?;
    let mut all_data: Vec<u8> = Vec::new();
    let mut pos = 0;
    loop {
        let buf = POOL.next(bs.value()).await;
        let (read_result, b) = file.read_fixed_at(buf, pos).await;
        let read_size = read_result?;
        if read_size == 0 {
            break;
        }
        pos += read_size as u64;
        all_data.extend_from_slice(&b[..read_size]);
    }
    file.close().await?;
    Ok(all_data)
}

/// Asynchronously reads the entire contents of a file into a UTF-8 encoded
/// string.
///
/// # Parameters
///
/// - `path`: The file path to read from. It can be any type that implements
///   `AsRef<Path>`.
/// - `bs`: The buffer size to use for each read operation.
///
/// # Returns
///
/// Returns a `Result` containing a `String` with the file's contents if
/// successful, or an error if the operation fails.
///
/// # Errors
///
/// This function will return an error if the file cannot be opened, read, or
/// if any I/O operations fail during the process. It will also return an error
/// if the file's contents are not valid UTF-8.
#[cfg(feature = "io_uring")]
async fn read_to_string(path: impl AsRef<Path>, bs: BufferSize) -> Result<String> {
    Ok(String::from_utf8(read(path, bs).await?)?)
}
