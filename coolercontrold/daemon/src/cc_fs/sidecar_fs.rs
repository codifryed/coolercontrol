// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! File utilities that always run on Tokio, for the auth/session/token subsystem.
//!
//! Unlike the rest of `cc_fs` (which runs on the main-thread compio runtime), these are
//! unconditionally `tokio::fs` and therefore produce `Send` futures. They
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
/// Tokio reactor, which the shared `cc_fs::test_runtime` (a compio runtime) does not provide. As
/// with the other runtimes, tests must run single-threaded (`serial_test`).
#[allow(dead_code)]
pub fn test_runtime<F: std::future::Future>(future: F) -> F::Output {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio test runtime builds");
    rt.block_on(tokio::task::LocalSet::new().run_until(future))
}
