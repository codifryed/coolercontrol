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

//! Nearly every async Web Framework out there is designed for multithreaded work-stealing
//! asynchronous runtimes. But our use case is different and requires a different approach.
//! This Actor module contains actor models to handle our application state and communicate
//! with the Web Framework over channels. This enables easy integration into those frameworks
//! which require Send + Sync, while keeping our internal logic and state !Send + !Sync.
//!
//! Note: we may want to use more Actors in the future in other parts of the application,
//! but shared state is sometimes preferred.

mod auth;

use log::info;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
mod custom_sensor;
mod device;
mod function;
mod mode;
mod profile;
mod setting;
mod status;

pub use self::auth::*;
pub use self::custom_sensor::*;
pub use self::device::*;
pub use self::function::*;
pub use self::mode::*;
pub use self::profile::*;
pub use self::setting::*;
pub use self::status::*;

pub trait ApiActor<M> {
    fn name(&self) -> &str;
    fn receiver(&mut self) -> &mut mpsc::Receiver<M>;
    async fn handle_message(&mut self, msg: M);
}

/// This is the async function to run any `ApiActor` in its own spawned task.
async fn run_api_actor<M>(mut api_actor: impl ApiActor<M>, cancel_token: CancellationToken) {
    loop {
        tokio::select! {
            // This *might* be unnecessary, but guarantees that this task is shut down.
            () = cancel_token.cancelled() => {
                break;
            }
            Some(msg) = api_actor.receiver().recv() => {
                api_actor.handle_message(msg).await;
            }
            else => break,
        }
    }
    info!("{} is shutting down", api_actor.name());
}
