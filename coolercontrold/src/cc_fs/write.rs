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
#[cfg(feature = "io_uring")]
use tokio_uring::fs::File;

/// Writes the given string `txt` to a file at the given `path`.
///
/// This function writes the bytes of `txt` to the file at `path`. If any of
/// the operations fail, the function will return an error.
pub async fn write_string(path: impl AsRef<Path>, txt: String) -> Result<()> {
    write(path, txt.into_bytes()).await
}

/// Writes the given `data` to a file at the given `path`.
///
/// This function opens the file at `path` and writes all of `data` to it.
/// If any of the operations fail, the function will return an error.
#[cfg(feature = "io_uring")]
pub async fn write(path: impl AsRef<Path>, data: Vec<u8>) -> Result<()> {
    let file = File::create(path).await?;
    let (res, _) = file.write_all_at(data, 0).await;
    res?;
    file.close().await?;
    Ok(())
}

#[cfg(not(feature = "io_uring"))]
pub async fn write(path: impl AsRef<Path>, data: Vec<u8>) -> Result<()> {
    tokio::fs::write(path, data).await?;
    Ok(())
}

/// Recursively creates a directory and all of its parent components if they
/// are missing.
///
/// This function creates all directories in the specified path that do not
/// already exist. If the directory already exists, this function does nothing.
///
/// This is a wrapper for `std::fs::create_dir_all`.
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
pub fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    Ok(std::fs::create_dir_all(path)?)
}

/// Removes a file from the filesystem.
///
/// This function removes the specified file from the filesystem. If the
/// file does not exist, this function does nothing.
///
/// This is a wrapper for `std::fs::remove_file`.
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
pub fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    Ok(std::fs::remove_file(path)?)
}

/// Recursively removes a directory and all of its contents.
///
/// This function removes the specified directory and all of its contents from
/// the filesystem. If the directory does not exist, this function does nothing.
///
/// This is a wrapper for `std::fs::remove_dir_all`.
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
/// Currently only used in tests, hence the allow `dead_code`.
#[allow(dead_code)]
pub fn remove_dir_all(path: impl AsRef<Path>) -> Result<()> {
    Ok(std::fs::remove_dir_all(path)?)
}
