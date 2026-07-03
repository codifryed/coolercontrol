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

use crate::api::actor::{run_api_actor, ApiActor};
use crate::device_health::{DeviceHealthController, DeviceHealthDto, HealthEvent};
use anyhow::Result;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// Broadcast capacity. Each tick broadcasts at most one batched event per
/// subject (`missing`, `stale-source`, `failsafe`), so this holds two full ticks; a
/// consumer that still lags is resynced with a full snapshot by the SSE stream.
const BROADCAST_CAPACITY: usize = 6;

struct DeviceHealthActor {
    receiver: mpsc::Receiver<DeviceHealthMessage>,
    controller: Rc<DeviceHealthController>,
}

enum DeviceHealthMessage {
    GetAll {
        respond_to: oneshot::Sender<DeviceHealthDto>,
    },
}

impl DeviceHealthActor {
    pub fn new(
        receiver: mpsc::Receiver<DeviceHealthMessage>,
        controller: Rc<DeviceHealthController>,
    ) -> Self {
        Self {
            receiver,
            controller,
        }
    }
}

impl ApiActor<DeviceHealthMessage> for DeviceHealthActor {
    fn name(&self) -> &'static str {
        "DeviceHealthActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<DeviceHealthMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: DeviceHealthMessage) {
        match msg {
            DeviceHealthMessage::GetAll { respond_to } => {
                let _ = respond_to.send(self.controller.get_all());
            }
        }
    }
}

#[derive(Clone)]
pub struct DeviceHealthHandle {
    sender: mpsc::Sender<DeviceHealthMessage>,
    broadcaster: broadcast::Sender<HealthEvent>,
}

impl DeviceHealthHandle {
    pub fn new<'s>(
        controller: Rc<DeviceHealthController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let (broadcaster, _) = broadcast::channel::<HealthEvent>(BROADCAST_CAPACITY);
        let handle = Self {
            sender,
            broadcaster,
        };
        controller.set_handle(handle.clone());
        let actor = DeviceHealthActor::new(receiver, controller);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        handle
    }

    pub async fn get_all(&self) -> DeviceHealthDto {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceHealthMessage::GetAll { respond_to: tx };
        if self.sender.send(msg).await.is_err() {
            return DeviceHealthDto {
                failsafe: Vec::with_capacity(0),
                missing: Vec::with_capacity(0),
                stale_source: Vec::with_capacity(0),
            };
        }
        rx.await.unwrap_or(DeviceHealthDto {
            failsafe: Vec::with_capacity(0),
            missing: Vec::with_capacity(0),
            stale_source: Vec::with_capacity(0),
        })
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<HealthEvent> {
        &self.broadcaster
    }

    /// Broadcasts a transition only when there are listeners.
    pub fn broadcast(&self, event: HealthEvent) {
        if self.broadcaster.receiver_count() == 0 {
            return;
        }
        let _ = self.broadcaster.send(event);
    }
}
