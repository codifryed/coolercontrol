// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! File utilities for `CoolerControl`.
//!
//! Specific to `CoolerControl`'s use cases and intended only for ordinary files. Async reads and
//! writes go through tokio's file utilities, which run on its blocking pool. Files the kernel
//! generates from memory, like `/proc/stat`, are the exception: see `ProcFile`. So are the device
//! IO threads, which have no runtime and use the `_blocking` helpers and `SysfsFdCache`. Directory
//! and metadata helpers fall back to `std` where appropriate and should be used sparingly.
//!
//! tokio replaced compio as the main runtime once device IO left the main thread: `io_uring`
//! then had nothing to batch, so wakeups measured even and compio allocated about 80 more per tick.

mod errno;
pub use self::errno::*;
mod fd_cache;
pub use self::fd_cache::*;
mod metadata;
pub use self::metadata::*;
mod read;
pub use self::read::*;
mod write;
pub use self::write::*;
mod open;
pub use self::open::*;
mod proc_file;
pub use self::proc_file::*;

/// Always-Tokio fs for the auth/session/token subsystem (the REST API lives on the Tokio sidecar).
pub mod sidecar_fs;

// The runtime entry lives in `crate::rt`. Re-exported here so the many fs-touching tests can keep
// calling `cc_fs::test_runtime`.
#[cfg(test)]
pub use crate::rt::test_runtime;
