// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! This module contains wrappers around `std::fs` functions so they should be called sparingly.
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

/// Returns whether the path exists.
/// This is a convenience function that wraps `std::fs::exists` and returns a boolean.
pub fn exists(path: impl AsRef<Path>) -> bool {
    std::fs::exists(path).is_ok_and(|b| b)
}

/// Sets the permissions for the given path.
///
/// This function will return an error if the path does not exist or if there
/// is an error resolving the path.
pub async fn set_permissions(path: impl AsRef<Path>, perm: Permissions) -> Result<()> {
    {
        // compio has its own distinct `Permissions` type, so rebuild it from the mode bits.
        use std::os::unix::fs::PermissionsExt;
        let compio_perm = compio::fs::Permissions::from_mode(perm.mode());
        Ok(compio::fs::set_permissions(path.as_ref(), compio_perm).await?)
    }
}
