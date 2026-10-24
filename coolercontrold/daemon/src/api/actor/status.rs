// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::status::{DeviceChannelStatusDto, DeviceStatusDto};
use crate::api::CCError;
use crate::device::{ChannelName, UID};
use crate::repositories::repository::DeviceLock;
use crate::AllDevices;
use anyhow::Result;
use chrono::{DateTime, Local};
use moro_local::Scope;
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
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

    // The trait declares this async; a body that happens not to await cannot drop it.
    #[allow(clippy::unused_async_trait_impl)]
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
    DeviceStatusDto {
        d_type: device.d_type,
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history: device.snapshot_since(timestamp_limit),
    }
}

fn get_most_recent_status(device_lock: &DeviceLock) -> DeviceStatusDto {
    let device = device_lock.borrow();
    DeviceStatusDto {
        d_type: device.d_type,
        type_index: device.type_index,
        uid: device.uid.clone(),
        status_history: device.snapshot_recent(),
    }
}

fn get_all_statuses_for_channel(
    device_lock: &DeviceLock,
    channel_name: &ChannelName,
) -> DeviceChannelStatusDto {
    DeviceChannelStatusDto {
        status_history: device_lock.borrow().snapshot_name(channel_name),
    }
}

fn get_most_recent_status_for_channel(
    device_lock: &DeviceLock,
    channel_name: &ChannelName,
) -> DeviceChannelStatusDto {
    DeviceChannelStatusDto {
        status_history: device_lock.borrow().snapshot_name_recent(channel_name),
    }
}

#[derive(Clone)]
pub struct StatusHandle {
    sender: mpsc::Sender<StatusMessage>,
    /// Shared, so each SSE subscriber's receive is a reference count instead of a deep copy.
    broadcaster: broadcast::Sender<Arc<[DeviceStatusDto]>>,
    cancel_token: CancellationToken,
}

impl StatusHandle {
    pub fn new<'s>(
        all_devices: AllDevices,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let (broadcaster, _) = broadcast::channel::<Arc<[DeviceStatusDto]>>(2);
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

    pub fn broadcaster(&self) -> &broadcast::Sender<Arc<[DeviceStatusDto]>> {
        &self.broadcaster
    }

    pub async fn broadcast_status(&self) {
        // only collect statuses if we have listeners:
        if self.broadcaster.receiver_count() == 0 {
            return;
        }
        let recent_status = self.recent().await.unwrap_or_default();
        let _ = self.broadcaster.send(Arc::from(recent_status));
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
