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

//! `io_uring` file utilities for `CoolerControl`.
//!
//! This module contains utility methods for working with the Linux `io_uring` file system
//! asynchronously. This includes reading/writing to files, and working with
//! directories.
//!
//! Be aware that `io_uring` is a relatively new Linux Kernel feature requiring at least
//! 5.11, and for this implementation 5.19.
//!
//! This module should only be used for ordinary files and is specific to `CoolerControl`'s use cases.
//!
//! The standard Tokio file utilities will always use a pool of blocking threads, but `io_uring`
//! has asynchronous file system APIs that work well for a single-threaded runtime. Note that the
//! `tokio_uring` runtime does spawn a single thread to handle `io_uring` queue polling operations.
//!
//! Unfortunately not all file system operations are supported yet by `tokio_uring`, so some
//! operations will use std or tokio file utilities where appropriate. Those functions
//! will be marked as such and should be used sparingly.

mod metadata;
pub use self::metadata::*;
mod read;
pub use self::read::*;
mod write;
pub use self::write::*;
mod open;
pub use self::open::*;

use anyhow::{Context, Result};
use std::cell::LazyCell;
use std::future::Future;
use std::iter;
use std::ops::Deref;
use tokio_uring::buf::fixed::FixedBufPool;

const SENSORS_POOL_SIZE: usize = 50;
const OTHER_POOL_SIZE: usize = 2;

static POOL: BufferPool = BufferPool::create();

/// Initialize and run the Tokio `io_uring` runtime.
///
/// `io_uring` requires at least Kernel 5.11, and our optimization flags require 5.19.
pub fn uring_runtime<F: Future>(future: F) -> F::Output {
    tokio_uring::builder()
        .entries(256)
        .uring_builder(
            tokio_uring::uring_builder()
                .setup_coop_taskrun()
                .setup_taskrun_flag()
                .dontfork(),
        )
        .start(future)
}

/// Registers a pool of fixed buffers with varying sizes for use in `io_uring` operations.
/// This should be called once during the initialization of the program right after the start
/// of the `tokio_uring` runtime.
///
/// The function creates a `FixedBufPool` with three different buffer sizes:
/// - `Small`: Optimized for standard sysfs sensors, with a pool size of `SENSORS_POOL_SIZE`.
/// - `Medium`: Optimized for other system files, with a pool size of `OTHER_POOL_SIZE`.
/// - `Large`: Optimized for large files like images, with a pool size of `OTHER_POOL_SIZE`.
///
/// Returns:
///
/// - `Result<()>`: Returns `Ok(())` if the buffer pool is successfully registered,
///   otherwise returns an error indicating that the registration failed.
pub fn register_uring_buffers() -> Result<()> {
    POOL.force_init();
    POOL.register()
        .context("Failed to register fixed buffer pool")
}

/// The buffer sizes registered in our fixed buffer pool for `io_uring`.
#[derive(Default, Debug)]
enum BufferSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl BufferSize {
    pub fn value(&self) -> usize {
        match self {
            // optimized for standard sysfs sensors (bytes)
            BufferSize::Small => 64,
            // optimized for other system files
            BufferSize::Medium => 4 * 1024,
            // optimized for large files like images
            BufferSize::Large => 32 * 1024,
        }
    }
}

/// This is a thread-local only buffer pool for `io_uring`.
struct BufferPool(LazyCell<FixedBufPool<Vec<u8>>>);

impl BufferPool {
    /// Creates a new `BufferPool` with a thread-local, lazily initialized
    /// `FixedBufPool` containing three different buffer sizes.
    ///
    /// The `FixedBufPool` is lazily initialized when the `BufferPool` is first accessed.
    pub const fn create() -> Self {
        Self(LazyCell::new(|| {
            FixedBufPool::new(
                iter::repeat_with(|| Vec::with_capacity(BufferSize::Small.value()))
                    .take(SENSORS_POOL_SIZE)
                    .chain(
                        iter::repeat_with(|| Vec::with_capacity(BufferSize::Medium.value()))
                            .take(OTHER_POOL_SIZE),
                    )
                    .chain(
                        iter::repeat_with(|| Vec::with_capacity(BufferSize::Large.value()))
                            .take(OTHER_POOL_SIZE),
                    ),
            )
        }))
    }

    /// Forces the initialization of the `FixedBufPool` if it hasn't been
    /// initialized yet.
    ///
    /// This is useful when you want to ensure that the buffer pool is
    /// initialized before any async operations are spawned.
    pub fn force_init(&self) {
        LazyCell::force(&self.0);
    }
}

unsafe impl Send for BufferPool {}
unsafe impl Sync for BufferPool {}

impl Deref for BufferPool {
    type Target = FixedBufPool<Vec<u8>>;

    #[inline]
    fn deref(&self) -> &FixedBufPool<Vec<u8>> {
        &self.0
    }
}
