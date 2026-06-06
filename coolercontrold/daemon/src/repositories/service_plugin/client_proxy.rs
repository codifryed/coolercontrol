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

//! Main-side proxy for the service-plugin gRPC transport, which runs on the Tokio sidecar.
//!
//! tonic is hard-wired to Tokio, so the `DeviceServiceClient` lives on the sidecar while
//! `ServicePluginRepo` and its device state stay on the main thread. They are bridged by a typed
//! request channel: `DeviceServiceClientHandle` (main side) mirrors the client's methods so repo
//! call sites are unchanged; a thin dispatcher on the sidecar owns the client and `spawn_local`s
//! each request.
//!
//! Concurrency and safety: the dispatcher spawns each request, so different devices run
//! concurrently; the client's per-device `Mutex` (held on the sidecar across the call) serializes
//! requests to the same device. That per-device serialization is essential for hardware safety
//! (a device, especially a plugin doing direct EC I/O, cannot take concurrent commands), and
//! holding the lock where the call runs makes it immune to main-side cancellation.

use super::client::DeviceServiceClient;
use super::service_manifest::ServiceManifest;
use super::service_plugin_repo::ServiceDeviceID;
use crate::device::{ChannelStatus, Device, DeviceUID, Duty, Temp, TempStatus};
use crate::grpc_api::device_service::v1::{HealthResponse, ListDevicesResponse};
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Result};
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};

const REQUEST_CHANNEL_CAP: usize = 16;

/// One request to a service plugin's gRPC client. Each carries owned (`Send`) args and a `oneshot`
/// for the reply, so it crosses the channel to the sidecar dispatcher.
enum DeviceServiceRequest {
    Health {
        respond_to: oneshot::Sender<Result<HealthResponse>>,
    },
    // The raw (Send) response crosses the channel; `Device` (which is !Send) is built on the main
    // thread by the handle via `DeviceServiceClient::map_devices`.
    ListDevices {
        respond_to: oneshot::Sender<Result<ListDevicesResponse>>,
    },
    WithDeviceIds {
        device_ids: Vec<(DeviceUID, ServiceDeviceID)>,
        respond_to: oneshot::Sender<()>,
    },
    InitializeDevice {
        device_uid: DeviceUID,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Shutdown {
        respond_to: oneshot::Sender<Result<()>>,
    },
    Status {
        device_uid: DeviceUID,
        respond_to: oneshot::Sender<Result<(Vec<ChannelStatus>, Vec<TempStatus>)>>,
    },
    ResetChannel {
        device_uid: DeviceUID,
        channel_name: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    EnableManualFanControl {
        device_uid: DeviceUID,
        channel_name: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    FixedDuty {
        device_uid: DeviceUID,
        channel_name: String,
        duty: Duty,
        respond_to: oneshot::Sender<Result<()>>,
    },
    SpeedProfile {
        device_uid: DeviceUID,
        channel_name: String,
        temp_source: TempSource,
        speed_profile: Vec<(Temp, Duty)>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Lighting {
        device_uid: DeviceUID,
        channel_name: String,
        lighting: LightingSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Lcd {
        device_uid: DeviceUID,
        channel_name: String,
        lcd: LcdSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

/// Main-side handle that mirrors `DeviceServiceClient`. Routes each call to the sidecar dispatcher
/// that owns the real tonic client.
#[derive(Debug)]
pub struct DeviceServiceClientHandle {
    request_tx: mpsc::Sender<DeviceServiceRequest>,
    /// Kept so `list_devices` can map the raw response into `!Send` `Device`s on the main thread.
    client_address: String,
    poll_rate: f64,
}

impl DeviceServiceClientHandle {
    /// Connects on the sidecar (tonic needs a Tokio reactor), starts the dispatcher there, and
    /// returns the main-side handle. Errors if the connection fails or the sidecar is gone.
    pub async fn connect(service_manifest: &ServiceManifest, poll_rate: f64) -> Result<Self> {
        let client_address = DeviceServiceClient::address_from_manifest(service_manifest)?;
        let (request_tx, request_rx) = mpsc::channel(REQUEST_CHANNEL_CAP);
        let manifest = service_manifest.clone();
        crate::sidecar::handle()
            .run(move || async move {
                let client = DeviceServiceClient::connect(&manifest, poll_rate).await?;
                tokio::task::spawn_local(run_dispatcher(Rc::new(client), request_rx));
                Ok::<(), anyhow::Error>(())
            })
            .await??;
        Ok(Self {
            request_tx,
            client_address,
            poll_rate,
        })
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::Health { respond_to: tx })
            .await?;
        rx.await?
    }

    pub async fn list_devices(&self) -> Result<Vec<(ServiceDeviceID, Device)>> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::ListDevices { respond_to: tx })
            .await?;
        let response = rx.await??;
        // `Device` is !Send, so map the raw response here on the main thread.
        Ok(DeviceServiceClient::map_devices(
            &self.client_address,
            self.poll_rate,
            response,
        ))
    }

    pub async fn with_device_ids(&self, device_ids: Vec<(DeviceUID, ServiceDeviceID)>) {
        let (tx, rx) = oneshot::channel();
        if self
            .send(DeviceServiceRequest::WithDeviceIds {
                device_ids,
                respond_to: tx,
            })
            .await
            .is_ok()
        {
            let _ = rx.await;
        }
    }

    pub async fn initialize_device(&self, device_uid: &DeviceUID) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::InitializeDevice {
            device_uid: device_uid.clone(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn shutdown(&self) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::Shutdown { respond_to: tx })
            .await?;
        rx.await?
    }

    pub async fn status(
        &self,
        device_uid: &DeviceUID,
    ) -> Result<(Vec<ChannelStatus>, Vec<TempStatus>)> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::Status {
            device_uid: device_uid.clone(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn reset_channel(&self, device_uid: &DeviceUID, channel_name: &str) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::ResetChannel {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_owned(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn enable_manual_fan_control(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::EnableManualFanControl {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_owned(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn fixed_duty(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        duty: Duty,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::FixedDuty {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_owned(),
            duty,
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn speed_profile(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(Temp, Duty)],
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::SpeedProfile {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_owned(),
            temp_source: temp_source.clone(),
            speed_profile: speed_profile.to_vec(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn lighting(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        lighting: &LightingSettings,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::Lighting {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_owned(),
            lighting: lighting.clone(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    pub async fn lcd(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        lcd: &LcdSettings,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.send(DeviceServiceRequest::Lcd {
            device_uid: device_uid.clone(),
            channel_name: channel_name.to_owned(),
            lcd: lcd.clone(),
            respond_to: tx,
        })
        .await?;
        rx.await?
    }

    async fn send(&self, request: DeviceServiceRequest) -> Result<()> {
        self.request_tx
            .send(request)
            .await
            .map_err(|_| anyhow!("service-plugin dispatcher for this service is gone"))
    }
}

/// Runs on the sidecar: owns the tonic client and spawns each request so different devices run
/// concurrently (the client's per-device `Mutex` serializes same-device calls). Exits when the
/// handle is dropped and the request channel closes.
async fn run_dispatcher(
    client: Rc<DeviceServiceClient>,
    mut request_rx: mpsc::Receiver<DeviceServiceRequest>,
) {
    while let Some(request) = request_rx.recv().await {
        tokio::task::spawn_local(handle_request(Rc::clone(&client), request));
    }
}

async fn handle_request(client: Rc<DeviceServiceClient>, request: DeviceServiceRequest) {
    match request {
        DeviceServiceRequest::Health { respond_to } => {
            let _ = respond_to.send(client.health().await);
        }
        DeviceServiceRequest::ListDevices { respond_to } => {
            let _ = respond_to.send(client.list_devices_raw().await);
        }
        DeviceServiceRequest::WithDeviceIds {
            device_ids,
            respond_to,
        } => {
            client.with_device_ids(device_ids).await;
            let _ = respond_to.send(());
        }
        DeviceServiceRequest::InitializeDevice {
            device_uid,
            respond_to,
        } => {
            let _ = respond_to.send(client.initialize_device(&device_uid).await);
        }
        DeviceServiceRequest::Shutdown { respond_to } => {
            let _ = respond_to.send(client.shutdown().await);
        }
        DeviceServiceRequest::Status {
            device_uid,
            respond_to,
        } => {
            let _ = respond_to.send(client.status(&device_uid).await);
        }
        DeviceServiceRequest::ResetChannel {
            device_uid,
            channel_name,
            respond_to,
        } => {
            let _ = respond_to.send(client.reset_channel(&device_uid, &channel_name).await);
        }
        DeviceServiceRequest::EnableManualFanControl {
            device_uid,
            channel_name,
            respond_to,
        } => {
            let result = client
                .enable_manual_fan_control(&device_uid, &channel_name)
                .await;
            let _ = respond_to.send(result);
        }
        DeviceServiceRequest::FixedDuty {
            device_uid,
            channel_name,
            duty,
            respond_to,
        } => {
            let _ = respond_to.send(client.fixed_duty(&device_uid, &channel_name, duty).await);
        }
        DeviceServiceRequest::SpeedProfile {
            device_uid,
            channel_name,
            temp_source,
            speed_profile,
            respond_to,
        } => {
            let result = client
                .speed_profile(&device_uid, &channel_name, &temp_source, &speed_profile)
                .await;
            let _ = respond_to.send(result);
        }
        DeviceServiceRequest::Lighting {
            device_uid,
            channel_name,
            lighting,
            respond_to,
        } => {
            let result = client.lighting(&device_uid, &channel_name, &lighting).await;
            let _ = respond_to.send(result);
        }
        DeviceServiceRequest::Lcd {
            device_uid,
            channel_name,
            lcd,
            respond_to,
        } => {
            let _ = respond_to.send(client.lcd(&device_uid, &channel_name, &lcd).await);
        }
    }
}
