// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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

    // The trait declares this async; a body that happens not to await cannot drop it.
    #[allow(clippy::unused_async_trait_impl)]
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
    cancel_token: CancellationToken,
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
            cancel_token: cancel_token.clone(),
        };
        controller.set_handle(handle.clone());
        let actor = DeviceHealthActor::new(receiver, controller);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        handle
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    pub async fn get_all(&self) -> DeviceHealthDto {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceHealthMessage::GetAll { respond_to: tx };
        if self.sender.send(msg).await.is_err() {
            return DeviceHealthDto {
                unreachable: Vec::new(),
                failsafe: Vec::new(),
                missing: Vec::new(),
                stale_source: Vec::new(),
                firmware_overrides: Vec::new(),
                channel_capabilities: Vec::new(),
                system_findings: Vec::new(),
            };
        }
        rx.await.unwrap_or(DeviceHealthDto {
            unreachable: Vec::new(),
            failsafe: Vec::new(),
            missing: Vec::new(),
            stale_source: Vec::new(),
            firmware_overrides: Vec::new(),
            channel_capabilities: Vec::new(),
            system_findings: Vec::new(),
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
