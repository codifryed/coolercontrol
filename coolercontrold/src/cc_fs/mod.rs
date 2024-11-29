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

//! File utilities for `CoolerControl`.
//! This module comes in two flavors: `io_uring` and Tokio.
//!
//! For the `io_uring` flavor, this module contains utility methods for working with the Linux `io_uring` file system
//! asynchronously. This includes reading/writing to files, and working with directories.
//!
//! Be aware that `io_uring` is a relatively new Linux Kernel feature requiring at least
//! 5.11, and for this implementation 5.19.
//!
//! This module should only be used for ordinary files and is specific to `CoolerControl`'s use cases.
//!
//! The standard Tokio file utilities will always use a pool of blocking threads, but `io_uring`
//! has asynchronous file system APIs that work well for a single-threaded runtime. Note that the
//! `tokio_uring` runtime does spawn threads to handle `io_uring` queue polling operations.
//!
//! Unfortunately not all file system operations are supported yet by `tokio_uring`, so some
//! operations will use std or tokio file utilities where appropriate. Those functions
//! will be marked as such and should be used sparingly.
//!
//! When using the Tokio flavor, a customer Tokio runtime is created and most of the Tokio
//! async file operations are used under the hood.

mod metadata;
pub use self::metadata::*;
mod read;
pub use self::read::*;
mod write;
pub use self::write::*;
mod open;
pub use self::open::*;

#[cfg(feature = "io_uring")]
use anyhow::{Context, Result};
#[cfg(feature = "io_uring")]
use std::cell::LazyCell;
use std::future::Future;
#[cfg(feature = "io_uring")]
use std::iter;
#[cfg(feature = "io_uring")]
use std::ops::Deref;
use std::time::Duration;
#[cfg(not(feature = "io_uring"))]
use tokio::runtime::Builder;
#[cfg(not(feature = "io_uring"))]
use tokio::task::LocalSet;
#[cfg(feature = "io_uring")]
use tokio_uring::buf::fixed::FixedBufPool;

#[cfg(feature = "io_uring")]
const SENSORS_POOL_SIZE: usize = 50;
#[cfg(feature = "io_uring")]
const OTHER_POOL_SIZE: usize = 2;

#[cfg(feature = "io_uring")]
static POOL: BufferPool = BufferPool::create();

/// Initialize and run the `io_uring` runtime.
///
/// `io_uring` requires at least Kernel 5.11, and our optimization flags require 5.19.
#[cfg(feature = "io_uring")]
pub fn runtime<F: Future>(future: F) -> F::Output {
    tokio_uring::builder()
        // This will limit the number of io-wkr threads, as when the queue is full they will wait:
        .entries(4)
        .uring_builder(
            tokio_uring::uring_builder()
                .setup_coop_taskrun()
                .setup_taskrun_flag()
                .dontfork(),
        )
        .start(future)
}

/// Initialize and run the Tokio runtime.
#[cfg(not(feature = "io_uring"))]
pub fn runtime<F: Future>(future: F) -> F::Output {
    let rt = Builder::new_current_thread()
        .enable_io()
        .max_io_events_per_tick(1024)
        .enable_time()
        // These intervals prioritize local tasks over IO polling. A bit more efficient for our use case.
        .event_interval(121)
        .global_queue_interval(61)
        // By default, this pool can grow large and fluctuate. We want efficiency over speed.
        .max_blocking_threads(2)
        .thread_keep_alive(Duration::from_secs(5))
        .thread_name("coolercontrold-wrk")
        .build();
    // requires tokio unstable: (but would make all our spawns !Send by default)
    // .build_local(&Default::default());
    // ^ until then, this allows us to use spawn_local:
    rt.unwrap().block_on(LocalSet::new().run_until(future))
}

/// A variant of `uring_runtime` that also registers the fixed buffers with the
/// Tokio `io_uring` runtime. This is useful for testing purposes, but should
/// not be used in production code.
///
/// Important: cargo tests need to be run single threaded, i.e. `-- --test-threads=1`, as cargo
/// runs test in parallel by default. We use the `serial_test` crate to explicitly ensure this.
#[allow(dead_code)]
#[cfg(feature = "io_uring")]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    tokio_uring::builder()
        .entries(4)
        .uring_builder(
            tokio_uring::uring_builder()
                .setup_coop_taskrun()
                .setup_taskrun_flag()
                .dontfork(),
        )
        .start(async {
            let _ = POOL.register();
            future.await
        })
}

#[allow(dead_code)]
#[cfg(not(feature = "io_uring"))]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    let rt = Builder::new_current_thread().enable_all().build();
    rt.unwrap().block_on(LocalSet::new().run_until(future))
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
#[cfg(feature = "io_uring")]
pub fn register_uring_buffers() -> Result<()> {
    POOL.force_init();
    POOL.register()
        .context("Failed to register fixed buffer pool")
}

/// The buffer sizes registered in our fixed buffer pool for `io_uring`.
#[cfg(feature = "io_uring")]
#[derive(Default, Debug)]
enum BufferSize {
    Small,
    #[default]
    Medium,
    Large,
}

#[cfg(feature = "io_uring")]
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
#[cfg(feature = "io_uring")]
struct BufferPool(LazyCell<FixedBufPool<Vec<u8>>>);

#[cfg(feature = "io_uring")]
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

#[cfg(feature = "io_uring")]
unsafe impl Send for BufferPool {}
#[cfg(feature = "io_uring")]
unsafe impl Sync for BufferPool {}

#[cfg(feature = "io_uring")]
impl Deref for BufferPool {
    type Target = FixedBufPool<Vec<u8>>;

    #[inline]
    fn deref(&self) -> &FixedBufPool<Vec<u8>> {
        &self.0
    }
}
