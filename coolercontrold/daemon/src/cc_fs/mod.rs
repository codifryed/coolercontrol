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
//! Specific to `CoolerControl`'s use cases and intended only for ordinary files. Reads and writes
//! use the async Tokio file utilities (a pool of blocking threads under the hood); directory and
//! metadata helpers fall back to `std` where appropriate and should be used sparingly.

mod metadata;
pub use self::metadata::*;
mod read;
pub use self::read::*;
mod write;
pub use self::write::*;
mod open;
pub use self::open::*;

use std::future::Future;
use std::time::Duration;
use tokio::runtime::Builder;
use tokio::task::LocalSet;

/// Initialize and run the Tokio runtime.
pub fn runtime<F: Future>(future: F) -> F::Output {
    let rt_builder = Builder::new_current_thread()
        .enable_io()
        .enable_time()
        // By default, this pool can grow large and fluctuate over time.
        // A large thread pool is less efficient for us, but we want more than a single
        // thread in case a device has severe latency:
        .max_blocking_threads(4)
        .thread_keep_alive(Duration::from_secs(60))
        .thread_name("cc-wrk")
        .event_interval(200)
        .global_queue_interval(200)
        .build();
    // requires tokio unstable: (but would make all our spawns !Send by default)
    // .build_local(&Default::default());
    // ^ until then, this allows us to use spawn_local:
    let rt = rt_builder.unwrap();
    let output = rt.block_on(LocalSet::new().run_until(future));
    // should a background thread still be running, this will force the runtime process to stop:
    rt.shutdown_timeout(Duration::from_secs(3));
    output
}

/// Initialize and run a Tokio runtime for tests.
///
/// Important: cargo tests need to be run single threaded, i.e. `-- --test-threads=1`, as cargo
/// runs test in parallel by default. We use the `serial_test` crate to explicitly ensure this.
#[allow(dead_code)]
pub fn test_runtime<F: Future>(future: F) -> F::Output {
    let rt = Builder::new_current_thread().enable_all().build();
    rt.unwrap().block_on(LocalSet::new().run_until(future))
}
