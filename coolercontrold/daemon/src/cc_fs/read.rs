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

/// Reads the entire contents of a sysfs file into a UTF-8 encoded string.
///
/// Tailored for sysfs files, which are typically small and contain few values. Returns an error if
/// the file cannot be opened or read, or if the contents are not valid UTF-8.
pub async fn read_sysfs(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Reads the entire contents of a text file into a UTF-8 encoded string.
///
/// Returns an error if the file cannot be opened or read, or if the contents are not valid UTF-8.
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
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
