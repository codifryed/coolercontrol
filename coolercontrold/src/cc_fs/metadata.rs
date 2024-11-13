/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

//! This module contains wrappers around `std::fs` functions so they should be called sparkly.
//! That being said all these functions should be very quick and save having to clone
//! the path which Tokio needs to do to pass between threads.

use anyhow::Result;
use std::fs::{Metadata, Permissions};
use std::path::{Path, PathBuf};

/// Returns the canonical, absolute form of a path.
///
/// This function is equivalent to `std::fs::canonicalize` and will return an error if the
/// path does not exist or if there is an error resolving the path.
pub fn canonicalize(path: impl AsRef<Path>) -> Result<PathBuf> {
    Ok(std::fs::canonicalize(path)?)
}

/// Returns metadata for the given path.
///
/// This function is equivalent to `std::fs::metadata` and will return an error if the
/// path does not exist or if there is an error resolving the path.
pub fn metadata(path: impl AsRef<Path>) -> Result<Metadata> {
    Ok(std::fs::metadata(path)?)
}

/// Sets the permissions for the given path.
///
/// This function is equivalent to `std::fs::set_permissions` and will return an error if the
/// path does not exist or if there is an error resolving the path.
pub fn set_permissions(path: impl AsRef<Path>, perm: Permissions) -> Result<()> {
    Ok(std::fs::set_permissions(path, perm)?)
}
