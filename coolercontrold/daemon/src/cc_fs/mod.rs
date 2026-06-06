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

//! File utilities for `CoolerControl`.
//!
//! Specific to `CoolerControl`'s use cases and intended only for ordinary files. Async reads and
//! writes go through the active runtime: Tokio's file utilities (a blocking-thread pool) by
//! default, or compio (completion-based, with `read_sysfs` using the runtime buffer pool) under the
//! `compio-rt` feature. Directory and metadata helpers fall back to `std` where appropriate and
//! should be used sparingly.

mod metadata;
pub use self::metadata::*;
mod read;
pub use self::read::*;
mod write;
pub use self::write::*;
mod open;
pub use self::open::*;

/// Always-Tokio fs for the auth/session/token subsystem (the REST API lives on the Tokio sidecar).
pub mod sidecar_fs;

// The runtime entry lives in `crate::rt`. Re-exported here so the many fs-touching tests can keep
// calling `cc_fs::test_runtime`.
#[cfg(test)]
pub use crate::rt::test_runtime;
