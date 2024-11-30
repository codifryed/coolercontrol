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

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::status::DeviceStatusDto;
use crate::device::Status;
use crate::repositories::repository::DeviceLock;
use crate::AllDevices;
use anyhow::Result;
use chrono::{DateTime, Local};
use moro_local::Scope;
use std::ops::Deref;
use std::sync::LazyLock;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

// possible scheduled update variance (<100ms) + all devices updated avg timespan (~80ms)
static MAX_UPDATE_TIMESTAMP_VARIATION: LazyLock<chrono::Duration> =
    LazyLock::new(|| chrono::Duration::milliseconds(200));

struct StatusActor {
    receiver: mpsc::Receiver<StatusMessage>,
    all_devices: AllDevices,
}

enum StatusMessage {
    All {
        respond_to: oneshot::Sender<Result<Vec<DeviceStatusDto>>>,
    },
    Recent {
        respond_to: oneshot::Sender<Result<Vec<DeviceStatusDto>>>,
    },
    Since {
        since: DateTime<Local>,
        respond_to: oneshot::Sender<Result<Vec<DeviceStatusDto>>>,
    },
}

impl StatusActor {
    pub fn new(receiver: mpsc::Receiver<StatusMessage>, all_devices: AllDevices) -> Self {
        Self {
            receiver,
            all_devices,
        }
    }
}

impl ApiActor<StatusMessage> for StatusActor {
    fn name(&self) -> &str {
        "StatusActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<StatusMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: StatusMessage) {
        match msg {
            StatusMessage::All { respond_to } => {
                let mut all_devices = vec![];
                for device_lock in self.all_devices.values() {
                    all_devices.push(get_all_statuses(device_lock));
                }
                let _ = respond_to.send(Ok(all_devices));
            }
            StatusMessage::Recent { respond_to } => {
                let mut all_devices = vec![];
                for device_lock in self.all_devices.values() {
                    all_devices.push(get_most_recent_status(device_lock));
                }
                let _ = respond_to.send(Ok(all_devices));
            }
            StatusMessage::Since { since, respond_to } => {
                let mut all_devices = vec![];
                for device_lock in self.all_devices.values() {
                    all_devices.push(get_statuses_since(since, device_lock));
                }
                let _ = respond_to.send(Ok(all_devices));
            }
        }
    }
}

fn get_all_statuses(device_lock: &DeviceLock) -> DeviceStatusDto {
    device_lock.borrow().deref().into()
}

fn get_statuses_since(
    since_timestamp: DateTime<Local>,
    device_lock: &DeviceLock,
) -> DeviceStatusDto {
    let timestamp_limit = since_timestamp + *MAX_UPDATE_TIMESTAMP_VARIATION;
    let device = device_lock.borrow();
    let filtered_history = device
        .status_history
        .iter()
        .filter(|device_status| device_status.timestamp > timestamp_limit)
        .cloned()
        .collect();
    DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history: filtered_history,
    }
}

fn get_most_recent_status(device_lock: &DeviceLock) -> DeviceStatusDto {
    let mut status_history: Vec<Status> = Vec::with_capacity(1);
    let device = device_lock.borrow();
    if let Some(most_recent_status) = device.status_current() {
        status_history.push(most_recent_status);
    }
    DeviceStatusDto {
        d_type: device.d_type.clone(),
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history,
    }
}

#[derive(Clone)]
pub struct StatusHandle {
    sender: mpsc::Sender<StatusMessage>,
    broadcaster: broadcast::Sender<Vec<DeviceStatusDto>>,
    cancel_token: CancellationToken,
}

impl StatusHandle {
    pub fn new<'s>(
        all_devices: AllDevices,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let (broadcaster, _) = broadcast::channel::<Vec<DeviceStatusDto>>(2);
        let actor = StatusActor::new(receiver, all_devices);
        main_scope.spawn(run_api_actor(actor, cancel_token.clone()));
        Self {
            sender,
            broadcaster,
            cancel_token,
        }
    }

    pub async fn all(&self) -> Result<Vec<DeviceStatusDto>> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::All { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn recent(&self) -> Result<Vec<DeviceStatusDto>> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::Recent { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<Vec<DeviceStatusDto>> {
        &self.broadcaster
    }

    pub async fn broadcast_status(&self) {
        // only collect statuses if we have listeners:
        if self.broadcaster.receiver_count() == 0 {
            return;
        }
        let recent_status = self.recent().await.unwrap_or_default();
        let _ = self.broadcaster.send(recent_status);
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }

    pub async fn since(&self, since: DateTime<Local>) -> Result<Vec<DeviceStatusDto>> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::Since {
            since,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
