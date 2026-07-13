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
use std::path::Path;

/// Writes the given string `txt` to a file at the given `path`.
///
/// This function writes the bytes of `txt` to the file at `path`. If any of
/// the operations fail, the function will return an error.
pub async fn write_string(path: impl AsRef<Path>, txt: String) -> Result<()> {
    write(path, txt.into_bytes()).await
}

/// Writes the given `data` to a file at the given `path`.
///
/// Opens (or creates/truncates) the file at `path` and writes all of `data` to it. Returns an error
/// if any of the operations fail.
pub async fn write(path: impl AsRef<Path>, data: Vec<u8>) -> Result<()> {
    #[cfg(not(feature = "compio-rt"))]
    {
        tokio::fs::write(path, data).await?;
        Ok(())
    }
    #[cfg(feature = "compio-rt")]
    {
        // compio takes ownership of the buffer for the duration of the write and hands it back in
        // the `BufResult`; we only care about the io result.
        let compio::BufResult(res, _buf) = compio::fs::write(path.as_ref(), data).await;
        res?;
        Ok(())
    }
}

/// Atomically renames `from` to `to` on the same filesystem (a metadata-only
/// operation). Used to publish a fully written temp file over its target so a
/// crash mid-write cannot leave a partial file at `to`.
pub async fn rename(from: impl AsRef<Path>, to: impl AsRef<Path>) -> Result<()> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::rename(from, to).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(compio::fs::rename(from.as_ref(), to.as_ref()).await?)
    }
}

/// Recursively creates a directory and all of its parent components if they
/// are missing.
///
/// This function creates all directories in the specified path that do not
/// already exist. If the directory already exists, this function does nothing.
///
/// # Returns
///
/// Returns a `Result` indicating success or failure. If the directory creation
/// fails, an error is returned.
///
/// # Errors
///
/// This function will return an error if the directory or any parent component
/// cannot be created. Possible reasons include lack of permissions, or if a
/// non-directory file exists at one of the parent component paths.
pub async fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::create_dir_all(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(compio::fs::create_dir_all(path.as_ref()).await?)
    }
}

/// Removes a file from the filesystem.
///
/// This function removes the specified file from the filesystem. If the
/// file does not exist, this function does nothing.
///
/// # Returns
///
/// Returns a `Result` indicating success or failure. If the removal fails, an
/// error is returned.
///
/// # Errors
///
/// This function will return an error if the file cannot be removed. Possible
/// reasons include lack of permissions, or if the file is a directory.
pub async fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::remove_file(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        Ok(compio::fs::remove_file(path.as_ref()).await?)
    }
}

/// Recursively removes a directory and all of its contents.
///
/// This function removes the specified directory and all of its contents from
/// the filesystem. If the directory does not exist, this function does nothing.
///
/// # Returns
///
/// Returns a `Result` indicating success or failure. If the removal fails, an
/// error is returned.
///
/// # Errors
///
/// This function will return an error if the directory cannot be removed.
/// Possible reasons include lack of permissions, or if a non-directory file
/// exists at the specified path.
///
// `async` for parity with the Tokio backend; the compio branch's body is a sync std call.
#[allow(clippy::unused_async)]
pub async fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    #[cfg(not(feature = "compio-rt"))]
    {
        Ok(tokio::fs::remove_dir_all(path).await?)
    }
    #[cfg(feature = "compio-rt")]
    {
        // compio has no async remove_dir_all; this is a test-only helper, so a direct std call
        // (briefly blocking the single thread during cleanup) is acceptable.
        Ok(std::fs::remove_dir_all(path.as_ref())?)
    }
}
