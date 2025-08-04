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
use crate::api::status::{DeviceChannelStatusDto, DeviceStatusDto};
use crate::api::CCError;
use crate::device::{ChannelName, Status, UID};
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
    AllDevice {
        device_uid: UID,
        respond_to: oneshot::Sender<Result<DeviceStatusDto>>,
    },
    RecentDevice {
        device_uid: UID,
        respond_to: oneshot::Sender<Result<DeviceStatusDto>>,
    },
    AllDeviceChannel {
        device_uid: UID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Result<DeviceChannelStatusDto>>,
    },
    RecentDeviceChannel {
        device_uid: UID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Result<DeviceChannelStatusDto>>,
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
    fn name(&self) -> &'static str {
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
            StatusMessage::AllDevice {
                device_uid,
                respond_to,
            } => {
                for device_lock in self.all_devices.values() {
                    if device_lock.borrow().uid == device_uid {
                        let _ = respond_to.send(Ok(get_all_statuses(device_lock)));
                        return;
                    }
                }
                let _ = respond_to.send(Err(CCError::NotFound {
                    msg: "Device not found".to_string(),
                }
                .into()));
            }
            StatusMessage::RecentDevice {
                device_uid,
                respond_to,
            } => {
                for device_lock in self.all_devices.values() {
                    if device_lock.borrow().uid == device_uid {
                        let _ = respond_to.send(Ok(get_most_recent_status(device_lock)));
                        return;
                    }
                }
                let _ = respond_to.send(Err(CCError::NotFound {
                    msg: "Device not found".to_string(),
                }
                .into()));
            }
            StatusMessage::AllDeviceChannel {
                device_uid,
                channel_name,
                respond_to,
            } => {
                for device_lock in self.all_devices.values() {
                    if device_lock.borrow().uid == device_uid {
                        let _ = respond_to
                            .send(Ok(get_all_statuses_for_channel(device_lock, &channel_name)));
                        return;
                    }
                }
                let _ = respond_to.send(Err(CCError::NotFound {
                    msg: "Device not found".to_string(),
                }
                .into()));
            }
            StatusMessage::RecentDeviceChannel {
                device_uid,
                channel_name,
                respond_to,
            } => {
                for device_lock in self.all_devices.values() {
                    if device_lock.borrow().uid == device_uid {
                        let _ = respond_to.send(Ok(get_most_recent_status_for_channel(
                            device_lock,
                            &channel_name,
                        )));
                        return;
                    }
                }
                let _ = respond_to.send(Err(CCError::NotFound {
                    msg: "Device not found".to_string(),
                }
                .into()));
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

fn get_all_statuses_for_channel(
    device_lock: &DeviceLock,
    channel_name: &ChannelName,
) -> DeviceChannelStatusDto {
    let device = device_lock.borrow();
    let status_history = device
        .status_history
        .iter()
        .map(|status| {
            let mut filtered_status = Status {
                timestamp: status.timestamp,
                ..Default::default()
            };
            status
                .temps
                .iter()
                .filter(|status| &status.name == channel_name)
                .for_each(|status| {
                    filtered_status.temps.push(status.clone());
                });
            status
                .channels
                .iter()
                .filter(|status| &status.name == channel_name)
                .for_each(|status| {
                    filtered_status.channels.push(status.clone());
                });
            filtered_status
        })
        .collect();
    DeviceChannelStatusDto { status_history }
}

fn get_most_recent_status_for_channel(
    device_lock: &DeviceLock,
    channel_name: &ChannelName,
) -> DeviceChannelStatusDto {
    let mut status_history: Vec<Status> = Vec::with_capacity(1);
    let device = device_lock.borrow();
    if let Some(mut most_recent_status) = device.status_current() {
        most_recent_status
            .channels
            .retain(|status| &status.name == channel_name);
        most_recent_status
            .temps
            .retain(|status| &status.name == channel_name);
        status_history.push(most_recent_status);
    }
    DeviceChannelStatusDto { status_history }
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

    pub async fn all_device(&self, device_uid: UID) -> Result<DeviceStatusDto> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::AllDevice {
            device_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn recent_device(&self, device_uid: UID) -> Result<DeviceStatusDto> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::RecentDevice {
            device_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn all_device_channel(
        &self,
        device_uid: UID,
        channel_name: ChannelName,
    ) -> Result<DeviceChannelStatusDto> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::AllDeviceChannel {
            device_uid,
            channel_name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn recent_device_channel(
        &self,
        device_uid: UID,
        channel_name: ChannelName,
    ) -> Result<DeviceChannelStatusDto> {
        let (tx, rx) = oneshot::channel();
        let msg = StatusMessage::RecentDeviceChannel {
            device_uid,
            channel_name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
