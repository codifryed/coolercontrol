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

//! File utilities that always run on Tokio, for the auth/session/token subsystem.
//!
//! Unlike the rest of `cc_fs` (which follows the active main-thread runtime, compio under
//! `compio-rt`), these are unconditionally `tokio::fs` and therefore produce `Send` futures. They
//! exist because that subsystem serves the REST API, which lives on the Tokio sidecar: tower-sessions
//! requires `Send` futures, and the axum handlers run there. So these must only ever be awaited
//! inside the Tokio runtime (the sidecar). Main-thread callers (a few startup paths) dispatch onto
//! the sidecar via `SidecarHandle::run`. The contents are tiny and rarely written, so staying on
//! Tokio's blocking-thread pool here costs nothing the migration cares about.

use anyhow::Result;
use std::fs::Permissions;
use std::path::Path;

/// Reads the entire contents of a text file into a UTF-8 encoded string.
pub async fn read_txt(path: impl AsRef<Path>) -> Result<String> {
    Ok(tokio::fs::read_to_string(path).await?)
}

/// Writes the given string `txt` to a file at the given `path`.
pub async fn write_string(path: impl AsRef<Path>, txt: String) -> Result<()> {
    write(path, txt.into_bytes()).await
}

/// Opens (or creates/truncates) the file at `path` and writes all of `data` to it.
pub async fn write(path: impl AsRef<Path>, data: Vec<u8>) -> Result<()> {
    tokio::fs::write(path, data).await?;
    Ok(())
}

/// Recursively creates a directory and all of its missing parent components.
pub async fn create_dir_all(path: impl AsRef<Path>) -> Result<()> {
    Ok(tokio::fs::create_dir_all(path).await?)
}

/// Removes a file from the filesystem.
pub async fn remove_file(path: impl AsRef<Path>) -> Result<()> {
    Ok(tokio::fs::remove_file(path).await?)
}

/// Sets the permissions for the given path.
pub async fn set_permissions(path: impl AsRef<Path>, perm: Permissions) -> Result<()> {
    Ok(tokio::fs::set_permissions(path, perm).await?)
}

/// Initialize and run a Tokio runtime for tests of sidecar-resident code.
///
/// The auth/session/token tests exercise code that uses these always-Tokio helpers, so they need a
/// Tokio reactor regardless of the `compio-rt` feature (the shared `cc_fs::test_runtime` becomes a
/// compio runtime under that feature, which has no Tokio reactor). As with the other runtimes,
/// tests must run single-threaded (`serial_test`).
#[allow(dead_code)]
pub fn test_runtime<F: std::future::Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio test runtime builds");
    rt.block_on(tokio::task::LocalSet::new().run_until(future))
}
