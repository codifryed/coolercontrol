// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelKind, ChannelStatus, Device, DeviceInfo, DeviceType,
    DeviceUID, DriverInfo, DriverType, Duty, LcdInfo, LcdMode, LcdModeType, LightingMode,
    LightingModeType, SpeedOptions, Temp, TempInfo, TempStatus,
};
use crate::grpc_api::device_service::v1::{
    device_service_client, CustomFunctionOneRequest, EnableManualFanControlRequest,
    FixedDutyRequest, HealthRequest, HealthResponse, InitializeDeviceRequest, LcdRequest,
    LcdSetting, LightingRequest, LightingSetting, ListDevicesRequest, ListDevicesResponse,
    ResetChannelRequest, Rgb, ShutdownRequest, SpeedProfilePoint, SpeedProfileRequest,
    StatusRequest, StatusResponse,
};
use crate::grpc_api::models;
use crate::grpc_api::models::v1::channel_info::Options;
use crate::grpc_api::models::v1::status::Metric;
use crate::grpc_api::models::v1::ChannelExtensionName;
use crate::repositories::service_plugin::service_management::ServiceId;
use crate::repositories::service_plugin::service_manifest::{ConnectionType, ServiceManifest};
use crate::repositories::service_plugin::service_plugin_repo::ServiceDeviceID;
use crate::repositories::service_plugin::transport::PluginChannel;
use crate::repositories::service_plugin::{transport, trust};
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Result};
use log::error;
use std::cell::RefCell;
use std::collections::HashMap;
use std::default::Default;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tonic::metadata::{Ascii, MetadataValue};
use tonic::Request;

use crate::repositories::failsafe::MISSING_STATUS_THRESHOLD;

/// Derives the service-plugin wait timeout from `poll_rate`. Pure
/// helper so the formula is testable without a live gRPC client.
/// `MISSING_STATUS_THRESHOLD` is a small `usize` (8) that fits within
/// `u8::MAX`, so the cast to `f64` is lossless.
#[allow(clippy::cast_precision_loss)]
fn service_wait_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * MISSING_STATUS_THRESHOLD as f64)
}

/// Our client wrapper for Device Service plugins.
/// This handles CC's device service contract by only allowing a single request at a time per device,
/// handling the permit/locking system and timeouts. It also maps CC's models to the generated
/// device service models.
/// The generated client over the daemon's own channel. Named once so a change to the
/// channel stack does not ripple through every field and signature below.
type PluginClient = device_service_client::DeviceServiceClient<PluginChannel>;

#[derive(Debug)]
pub struct DeviceServiceClient {
    service_id: ServiceId,

    /// The `authorization` header value for the remote, parsed once at connect time so a
    /// malformed token fails where it is read rather than on every request. Absent for a
    /// plugin on a Unix socket, a remote that predates authentication, or a link that
    /// cannot carry the token safely.
    credentials: Option<MetadataValue<Ascii>>,

    /// Snapshot of the service-plugin wait timeout. `poll_rate` only
    /// changes on daemon restart, so this value is constant for the
    /// client's lifetime and is computed once in `new` to avoid per-
    /// retry f64 math on the hot path.
    service_wait_timeout: Duration,

    /// Using a `tokio::Mutex` has the advantage of being able to hold a lock over an await point,
    /// and for the small amount of requests we make, performance is shown to be on par with std.
    service_client: Mutex<PluginClient>,

    /// For each device present, we clone the client for it, so we can handle requests per device
    /// concurrently. Clone is a cheap operation for the tonic Client. Each per-device client sits
    /// behind its own `Mutex` (per-device serialization, essential for hardware safety) and an `Rc`
    /// so a caller can clone the `Rc<Mutex>` out of the map and lock it without holding a `RefCell`
    /// borrow across the `.await`. The `RefCell` makes the map interior-mutable so `with_device_ids`
    /// can populate it once at setup while the client is shared (`Rc`) on the sidecar.
    device_clients: RefCell<HashMap<DeviceUID, Rc<Mutex<PluginClient>>>>,

    /// Maps the device UID to the service device ID, so we can pass the correct ID to the device service.
    device_ids: RefCell<HashMap<DeviceUID, ServiceDeviceID>>,
}

impl DeviceServiceClient {
    pub async fn connect(
        service_manifest: &ServiceManifest,
        plan: &trust::LinkPlan,
        poll_rate: f64,
        tls_strict: bool,
    ) -> Result<Self> {
        let address = Self::address_from_manifest(service_manifest, plan)?;
        let credentials = Self::outbound_credentials(service_manifest, plan)?;
        let channel = transport::connect(service_manifest, &address, plan, tls_strict).await?;
        let channel = transport::ExplainRefusals::new(channel, service_manifest.id.clone());
        let grpc_client = device_service_client::DeviceServiceClient::new(channel);
        Ok(Self::new(
            service_manifest.id.clone(),
            poll_rate,
            grpc_client,
            credentials,
        ))
    }

    /// The parsed `authorization` value, or `None` when there is no token to send.
    ///
    /// Parsing here rather than per request means a token with characters illegal in a
    /// header refuses the connection outright. Logging and sending the request anyway
    /// would reach the user as the remote refusing credentials that were never sent.
    fn outbound_credentials(
        service_manifest: &ServiceManifest,
        plan: &trust::LinkPlan,
    ) -> Result<Option<MetadataValue<Ascii>>> {
        let Some(token) = Self::outbound_token(service_manifest, plan) else {
            return Ok(None);
        };
        let value = format!("Bearer {token}").parse().map_err(|err| {
            anyhow!(
                "Device service '{}' has an unusable access token: {err}. Rewrite the '{}' \
                 file in that plugin's directory with the token exactly as the remote \
                 daemon issued it.",
                service_manifest.id,
                trust::TOKEN_FILE_NAME
            )
        })?;
        Ok(Some(value))
    }

    /// The token to send, withheld when the link would carry it in the clear off this
    /// machine.
    ///
    /// `LinkPlan` already ties a token to TLS, but a plugin author can override
    /// that with `tls = false` in their own manifest. Since the manifest is the plugin's
    /// to write and the token is the user's, a plugin must not be able to turn the user's
    /// credential into a plaintext broadcast.
    fn outbound_token(
        service_manifest: &ServiceManifest,
        plan: &trust::LinkPlan,
    ) -> Option<String> {
        let token = plan.token()?;
        if Self::link_protects_a_token(service_manifest, plan) {
            return Some(token.to_string());
        }
        error!(
            "Device service '{}' declares an unencrypted connection, so its access token \
             will not be sent. Remove 'tls = false' from that plugin's manifest, or remove \
             the '{}' file if the remote needs no token.",
            service_manifest.id,
            trust::TOKEN_FILE_NAME
        );
        None
    }

    /// Whether a token can travel this link safely: TLS encrypts it, and a link that
    /// never leaves this machine has nothing to eavesdrop on.
    fn link_protects_a_token(service_manifest: &ServiceManifest, plan: &trust::LinkPlan) -> bool {
        if plan.encrypted() {
            return true;
        }
        match &service_manifest.address {
            ConnectionType::Uds(_) => true,
            ConnectionType::Tcp(tcp_address) => {
                trust::is_loopback_host(&transport::host_of(tcp_address))
            }
            ConnectionType::None => false,
        }
    }

    /// Derives the gRPC connection address from a manifest. Shared by `connect` and the main-side
    /// proxy handle (which needs the address to map device locations without holding the client).
    ///
    /// A TCP address is `https` exactly when the link is encrypted, which `LinkPlan`
    /// decides. Unix sockets stay plain, since the kernel already scopes them to this
    /// machine.
    pub fn address_from_manifest(
        service_manifest: &ServiceManifest,
        plan: &trust::LinkPlan,
    ) -> Result<String> {
        match &service_manifest.address {
            ConnectionType::Uds(uds) => Ok(format!("unix://{}", uds.display())),
            ConnectionType::Tcp(tcp_addr) => {
                let scheme = if plan.encrypted() { "https" } else { "http" };
                Ok(format!("{scheme}://{tcp_addr}"))
            }
            ConnectionType::None => Err(anyhow!("Invalid Connection Type: NONE!")),
        }
    }

    /// Wraps a message in a request carrying this client's credentials.
    ///
    /// Every outbound call goes through here, so a new RPC cannot accidentally ship
    /// without the token.
    fn request<T>(&self, message: T) -> Request<T> {
        let mut request = Request::new(message);
        if let Some(credentials) = &self.credentials {
            request
                .metadata_mut()
                .insert("authorization", credentials.clone());
        }
        request
    }

    fn new(
        service_id: ServiceId,
        poll_rate: f64,
        client: PluginClient,
        credentials: Option<MetadataValue<Ascii>>,
    ) -> Self {
        let service_client = Mutex::new(client);
        let service_wait_timeout = service_wait_timeout_for(poll_rate);
        Self {
            service_id,
            credentials,
            service_wait_timeout,
            service_client,
            device_clients: RefCell::new(HashMap::new()),
            device_ids: RefCell::new(HashMap::new()),
        }
    }

    /// This allows us to make a few basic requests with our timeout logic before we have the device
    /// IDs. Populates the per-device clients once at setup (sequential, before any steady-state
    /// call), so the `RefCell` borrow never overlaps a read.
    pub async fn with_device_ids(&self, device_ids: Vec<(DeviceUID, ServiceDeviceID)>) {
        let mut device_clients = HashMap::new();
        let mut device_id_map = HashMap::new();
        for (device_uid, service_device_id) in device_ids {
            device_clients.insert(
                device_uid.clone(),
                Rc::new(Mutex::new(self.service_client.lock().await.clone())),
            );
            device_id_map.insert(device_uid, service_device_id);
        }
        *self.device_clients.borrow_mut() = device_clients;
        *self.device_ids.borrow_mut() = device_id_map;
    }

    /// For service-wide requests where we want to make sure all devices clients are inactive. Clones
    /// the `Rc<Mutex>`es out of the map first so the `RefCell` borrow is not held across the locks.
    async fn wait_till_all_clients_are_free(&self) {
        let device_clients: Vec<_> = self.device_clients.borrow().values().cloned().collect();
        for device_client in device_clients {
            let _ = device_client.lock().await;
        }
    }

    fn get_device_client(&self, device_uid: &DeviceUID) -> Result<Rc<Mutex<PluginClient>>> {
        self.device_clients
            .borrow()
            .get(device_uid)
            .cloned()
            .ok_or_else(|| anyhow!("Device {device_uid} not found"))
    }

    fn get_service_device_id(&self, device_uid: &DeviceUID) -> Result<ServiceDeviceID> {
        self.device_ids
            .borrow()
            .get(device_uid)
            .cloned()
            .ok_or_else(|| anyhow!("Service Device {device_uid} ID not found"))
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to get health status. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            // Health endpoint need not wait for all clients:
            mut service_client = self.service_client.lock() => {
                let request = self.request(HealthRequest{});
                service_client.health(request).await
                .map(tonic::Response::into_inner)
                .map_err(|status| anyhow!("Failed to get health status: {status}"))
            }
        }
    }

    /// Performs the gRPC list-devices call and returns the raw (`Send`) response. Mapping to
    /// `Device` (which is `!Send`) is done by the caller on the main thread via `map_devices`.
    pub async fn list_devices_raw(&self) -> Result<ListDevicesResponse> {
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to list devices. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            () = self.wait_till_all_clients_are_free() => {
                let request = self.request(ListDevicesRequest{});
                let mut service_client = self.service_client.lock().await;
                service_client.list_devices(request).await
                .map(tonic::Response::into_inner)
                .map_err(|status| anyhow!("Failed to list devices: {status}"))
            }
        }
    }

    pub async fn initialize_device(&self, device_uid: &DeviceUID) -> Result<()> {
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to initialize devices. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            () = self.wait_till_all_clients_are_free() => {
                let request = self.request(InitializeDeviceRequest{
                    device_id: self.get_service_device_id(device_uid)?
                });
                let mut service_client = self.service_client.lock().await;
                service_client.initialize_device(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to initialize devices: {s}"))
            }
        }
    }

    pub async fn shutdown(&self) -> Result<()> {
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to shutdown service. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            () = self.wait_till_all_clients_are_free() => {
                let request = self.request(ShutdownRequest{});
                let mut service_client = self.service_client.lock().await;
                match service_client.shutdown(request).await {
                    Ok(_) => Ok(()),
                    // Service already down (e.g. systemd shut it down before us).
                    Err(s) if s.code() == tonic::Code::Unavailable => Ok(()),
                    Err(s) => Err(anyhow!("Failed to shutdown service: {s}")),
                }
            }
        }
    }

    /// Maps a raw list-devices response into `Device`s. Static (takes `client_address`/`poll_rate`)
    /// so the main-side proxy handle can map without holding the sidecar-resident client, since
    /// `Device` is `!Send` and cannot be built on the sidecar and sent back.
    pub fn map_devices(
        client_address: &str,
        poll_rate: f64,
        devices_response: ListDevicesResponse,
    ) -> Vec<(ServiceDeviceID, Device)> {
        let mut devices = vec![];
        for (index, device_res) in devices_response.devices.into_iter().enumerate() {
            let device_info = device_res
                .info
                .map_or_else(DeviceInfo::default, |info_res| DeviceInfo {
                    channels: Self::map_device_info_channels(info_res.channels),
                    temps: Self::map_device_info_temps(info_res.temps),
                    lighting_speeds: info_res.lighting_speeds,
                    temp_min: info_res.temp_min.map_or(0, safe_u8_temp),
                    temp_max: info_res.temp_max.map_or(100, safe_u8_temp),
                    profile_max_length: info_res.profile_max_length.map_or(17, safe_u8),
                    profile_min_length: info_res.profile_min_length.map_or(2, safe_u8),
                    model: info_res.model,
                    thinkpad_fan_control: None,
                    amd_gpu_overdrive: None,
                    driver_info: info_res.driver_info.map_or_else(
                        || DriverInfo {
                            drv_type: DriverType::External,
                            ..Default::default()
                        },
                        |d| DriverInfo {
                            drv_type: DriverType::External,
                            name: d.name,
                            version: d.version,
                            locations: Self::add_address_to_locations(client_address, d.locations),
                        },
                    ),
                });
            #[allow(clippy::cast_possible_truncation)]
            devices.push((
                device_res.id,
                Device::new(
                    device_res.name,
                    DeviceType::ServicePlugin,
                    index as u8,
                    None,
                    device_info,
                    device_res.uid_info,
                    poll_rate,
                ),
            ));
        }
        devices
    }

    fn map_device_info_channels(
        channel_info_res: HashMap<String, models::v1::ChannelInfo>,
    ) -> HashMap<String, ChannelInfo> {
        channel_info_res
            .into_iter()
            .map(|(channel_name, channel_info_res)| {
                (channel_name, Self::map_channel_info(channel_info_res))
            })
            .collect()
    }

    fn map_channel_info(channel_info_res: models::v1::ChannelInfo) -> ChannelInfo {
        let kind = match channel_info_res.options {
            Some(Options::SpeedOptions(speed_options)) => ChannelKind::Speed(SpeedOptions {
                min_duty: safe_u8(speed_options.min_duty),
                max_duty: safe_u8(speed_options.max_duty),
                fixed_enabled: speed_options.fixed_enabled,
                extension: Self::map_channel_extension_names(speed_options.extension),
            }),
            Some(Options::LightingModes(lighting_modes)) => ChannelKind::Lighting(
                lighting_modes
                    .lighting_mode
                    .into_iter()
                    .map(|mode| LightingMode {
                        frontend_name: mode.frontend_name.unwrap_or_else(|| mode.name.clone()),
                        name: mode.name,
                        min_colors: safe_u8(mode.min_colors),
                        max_colors: safe_u8(mode.max_colors),
                        speed_enabled: mode.speed_enabled,
                        backward_enabled: mode.backward_enabled,
                        type_: LightingModeType::Custom,
                    })
                    .collect(),
            ),
            Some(Options::LcdInfo(lcd_info)) => {
                let modes = lcd_info
                    .lcd_modes
                    .into_iter()
                    .map(|mode| LcdMode {
                        frontend_name: mode.frontend_name.unwrap_or_else(|| mode.name.clone()),
                        name: mode.name,
                        brightness: mode.brightness,
                        orientation: mode.orientation,
                        image: mode.image,
                        colors_min: 0,
                        colors_max: 0,
                        type_: LcdModeType::None,
                    })
                    .collect();
                let info = Some(LcdInfo {
                    screen_width: lcd_info.screen_width,
                    screen_height: lcd_info.screen_height,
                    max_image_size_bytes: lcd_info.max_image_size_bytes,
                    // The plugin protocol carries no gif field, and every screen but one can.
                    gif_supported: true,
                });
                ChannelKind::Lcd { modes, info }
            }
            None => ChannelKind::InfoOnly,
        };
        ChannelInfo {
            label: channel_info_res.label,
            kind,
        }
    }

    fn map_channel_extension_names(extension_res: Option<i32>) -> Option<ChannelExtensionNames> {
        extension_res.and_then(|ext| match ChannelExtensionName::try_from(ext) {
            Ok(ChannelExtensionName::AmdRdnaGpu) => Some(ChannelExtensionNames::AmdRdnaGpu),
            Ok(ChannelExtensionName::AutoHwCurve) => Some(ChannelExtensionNames::AutoHWCurve),
            Ok(ChannelExtensionName::Unspecified) | Err(_) => None,
        })
    }

    fn map_device_info_temps(
        temp_info_res: HashMap<String, models::v1::TempInfo>,
    ) -> HashMap<String, TempInfo> {
        temp_info_res
            .into_iter()
            .map(|(temp_name, temp_info)| {
                (
                    temp_name,
                    TempInfo {
                        label: temp_info.label,
                        number: safe_u8(temp_info.number),
                    },
                )
            })
            .collect()
    }

    fn add_address_to_locations(client_address: &str, mut locations: Vec<String>) -> Vec<String> {
        locations.push(client_address.to_owned());
        locations
    }

    pub async fn status(
        &self,
        device_uid: &DeviceUID,
    ) -> Result<(Vec<ChannelStatus>, Vec<TempStatus>)> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to get device: {device_uid} status. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let request = self.request(StatusRequest{
                    device_id: self.get_service_device_id(device_uid)?
                });
                device_client.status(request).await
                .map(Self::map_status)
                .map_err(|s| anyhow!("Failed to get device: {device_uid} status: {s}"))
            }
        }
    }

    fn map_status(
        status_response: tonic::Response<StatusResponse>,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut channel_status = vec![];
        let mut temp_status = vec![];
        for status in status_response.into_inner().status {
            let Some(metric) = status.metric else {
                error!("Status Metric is missing for {}", status.id);
                continue;
            };
            match metric {
                Metric::Temp(temp) => {
                    temp_status.push(TempStatus {
                        name: status.id,
                        temp,
                    });
                }
                Metric::Speed(speed) => {
                    channel_status.push(ChannelStatus {
                        name: status.id,
                        rpm: speed.rpm,
                        duty: speed.duty,
                        ..Default::default()
                    });
                }
                Metric::Mhz(freq) => {
                    channel_status.push(ChannelStatus {
                        name: status.id,
                        freq: Some(freq),
                        ..Default::default()
                    });
                }
                Metric::Watts(watts) => {
                    channel_status.push(ChannelStatus {
                        name: status.id,
                        watts: Some(watts),
                        ..Default::default()
                    });
                }
            }
        }
        (channel_status, temp_status)
    }

    pub async fn reset_channel(&self, device_uid: &DeviceUID, channel_name: &str) -> Result<()> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to reset device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let request = self.request(ResetChannelRequest{
                    device_id: self.get_service_device_id(device_uid)?,
                    channel_id: channel_name.to_owned(),
                });
                device_client.reset_channel(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to reset device: {device_uid} channel: {channel_name}: {s}"))
            }
        }
    }
    pub async fn enable_manual_fan_control(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
    ) -> Result<()> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to enable manual fan control for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let request = self.request(EnableManualFanControlRequest{
                    device_id: self.get_service_device_id(device_uid)?,
                    channel_id: channel_name.to_owned(),
                });
                device_client.enable_manual_fan_control(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to enable manual fan control for device: {device_uid} channel: {channel_name}: {s}"))
            }
        }
    }
    pub async fn fixed_duty(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        duty: Duty,
    ) -> Result<()> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set fixed duty for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let request = self.request(FixedDutyRequest{
                    device_id: self.get_service_device_id(device_uid)?,
                    channel_id: channel_name.to_owned(),
                    duty: i32::from(duty),
                });
                device_client.fixed_duty(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to set fixed duty for device: {device_uid} channel: {channel_name}: {s}"))
            }
        }
    }
    pub async fn speed_profile(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(Temp, Duty)],
    ) -> Result<()> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set speed profile for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let mut req_speed_profile = Vec::new();
                for (temp, duty) in speed_profile {
                    req_speed_profile.push(SpeedProfilePoint {
                        temp: *temp,
                        duty: u32::from(*duty),
                    });
                }
                let request = self.request(SpeedProfileRequest{
                    device_id: self.get_service_device_id(device_uid)?,
                    channel_id: channel_name.to_owned(),
                    temp_source_id: Some(temp_source.temp_name.clone()),
                    speed_profile: req_speed_profile,
                });
                device_client.speed_profile(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to set speed profile for device: {device_uid} channel: {channel_name}: {s}"))
            }
        }
    }

    pub async fn lighting(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        lighting: &LightingSettings,
    ) -> Result<()> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set lighting for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let mut colors = vec![];
                for (r, g, b) in &lighting.colors {
                    colors.push(Rgb {
                        r: u32::from(*r),
                        g: u32::from(*g),
                        b: u32::from(*b),
                    });
                }
                let lighting_setting = LightingSetting {
                    mode: lighting.mode.clone(),
                    speed: lighting.speed.clone(),
                    backward: lighting.backward,
                    colors,
                };
                let request = self.request(LightingRequest {
                    device_id: self.get_service_device_id(device_uid)?,
                    channel_id: channel_name.to_owned(),
                    setting: Some(lighting_setting),
                });
                device_client.lighting(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to set lighting for device: {device_uid} channel: {channel_name}: {s}"))
            }
        }
    }
    pub async fn lcd(
        &self,
        device_uid: &DeviceUID,
        channel_name: &str,
        lcd: &LcdSettings,
    ) -> Result<()> {
        let device_client = self.get_device_client(device_uid)?;
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set LCD for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = device_client.lock() => {
                let lcd_setting = LcdSetting {
                    mode: lcd.mode.to_string(),
                    brightness: lcd.brightness.map(u32::from),
                    orientation: lcd.orientation.map(u32::from),
                    image_path: lcd.image_file_processed().cloned(),
                };
                let request = self.request(LcdRequest {
                    device_id: self.get_service_device_id(device_uid)?,
                    channel_id: channel_name.to_owned(),
                    setting: Some(lcd_setting),
                });
                device_client.lcd(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to set LCD for device: {device_uid} channel: {channel_name}: {s}"))
            }
        }
    }

    #[allow(dead_code)]
    pub async fn custom_function_one(&self) -> Result<()> {
        tokio::select! {
            () = sleep(self.service_wait_timeout) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to apply custom function. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut service_client = self.service_client.lock() => {
                let request = self.request(CustomFunctionOneRequest{});
                service_client.custom_function_one(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to apply custom function: {s}"))
            }
        }
    }
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn safe_u8_temp(value: f64) -> u8 {
    if value < 0. {
        error!("Negative f64 temp number from Device Service detected.");
        return 0;
    } else if value > 200. {
        error!("High f64 temp number from Device Service detected.");
        return 200;
    }
    value as u8
}

#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn safe_u8(value: u32) -> u8 {
    // if value < 0 {
    //     error!("Negative f64 temp number from Device Service detected.");
    //     return 0;
    if value > 255 {
        error!("High u32 temp number from Device Service detected.");
        return 255;
    }
    value as u8
}

#[cfg(test)]
mod wait_timeout_tests {
    use super::*;

    #[test]
    fn wait_timeout_matches_legacy_at_default_poll_rate() {
        // Regression: at the default poll_rate = 1.0 s the formula
        // must reproduce the previous hard-coded 8 s value.
        assert_eq!(service_wait_timeout_for(1.0), Duration::from_secs(8));
    }

    #[test]
    fn wait_timeout_scales_with_poll_rate() {
        // The wait must track MISSING_STATUS_THRESHOLD * poll_rate so
        // a stalled service bail is coincident with a remote client
        // being declared unresponsive.
        assert_eq!(service_wait_timeout_for(0.5), Duration::from_secs(4));
        assert_eq!(service_wait_timeout_for(5.0), Duration::from_secs(40));
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    use crate::repositories::service_plugin::service_manifest::ServiceType;
    use std::ops::Not;
    use std::path::PathBuf;

    fn manifest(address: ConnectionType) -> ServiceManifest {
        manifest_in(
            address,
            PathBuf::from("/var/lib/coolercontrol/plugins/test_service"),
        )
    }

    /// A manifest whose directory really exists, so the token file can be created or left
    /// absent and `uses_tls` reads the same thing the daemon would.
    fn manifest_in(address: ConnectionType, path: PathBuf) -> ServiceManifest {
        ServiceManifest {
            id: "test_service".to_string(),
            service_type: ServiceType::Device,
            description: None,
            version: None,
            url: None,
            executable: None,
            args: Vec::new(),
            envs: Vec::new(),
            address,
            tls: None,
            privileged: false,
            proxy: None,
            path,
        }
    }

    fn write_token(dir: &std::path::Path) {
        std::fs::write(dir.join(trust::TOKEN_FILE_NAME), "cc_secret\n").unwrap();
    }

    /// The plan the daemon would resolve for this manifest, read from the real files.
    async fn plan_for(manifest: &ServiceManifest) -> trust::LinkPlan {
        trust::LinkPlan::resolve(manifest).await
    }

    const REMOTE: &str = "192.168.1.100:11987";

    /// Goal: the compatibility rule. Without a token the link is plain `http`, so an
    /// older daemon and every third-party plugin serving plain h2c on TCP keep working
    /// after this daemon is upgraded.
    #[test]
    fn a_tcp_service_without_a_token_stays_plaintext() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let manifest = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            let plan = plan_for(&manifest).await;
            assert!(plan.encrypted().not());
            assert_eq!(
                DeviceServiceClient::address_from_manifest(&manifest, &plan).unwrap(),
                format!("http://{REMOTE}")
            );
        });
    }

    /// Goal: the other half of the rule. Placing a token is the act that says the remote
    /// is an upgraded daemon, so it switches the link to `https`. The token must never
    /// cross a network in the clear, and this is what guarantees it.
    #[test]
    fn a_token_switches_the_link_to_tls() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            write_token(dir.path());
            let manifest = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            let plan = plan_for(&manifest).await;
            assert!(plan.encrypted());
            assert_eq!(
                DeviceServiceClient::address_from_manifest(&manifest, &plan).unwrap(),
                format!("https://{REMOTE}")
            );
            assert!(DeviceServiceClient::outbound_token(&manifest, &plan).is_some());
        });
    }

    /// Goal: the escape hatch, both ways. A plugin author knows whether their server
    /// terminates TLS, so their declaration overrides the token-derived default. This is
    /// what a remote with `tls_enabled = false` needs.
    #[test]
    fn the_manifest_tls_field_overrides_the_token() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let mut forced_on = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            forced_on.tls = Some(true);
            let on_plan = plan_for(&forced_on).await;
            assert!(on_plan.encrypted());
            assert_eq!(
                DeviceServiceClient::address_from_manifest(&forced_on, &on_plan).unwrap(),
                format!("https://{REMOTE}")
            );

            write_token(dir.path());
            let mut forced_off = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            forced_off.tls = Some(false);
            let off_plan = plan_for(&forced_off).await;
            assert!(off_plan.encrypted().not());
            assert_eq!(
                DeviceServiceClient::address_from_manifest(&forced_off, &off_plan).unwrap(),
                format!("http://{REMOTE}")
            );
        });
    }

    /// Goal: the manifest is the plugin's to write and the token is the user's, so a
    /// plugin declaring `tls = false` must not be able to turn that credential into a
    /// plaintext broadcast. The token is withheld rather than sent.
    #[test]
    fn a_plaintext_remote_never_receives_the_token() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            write_token(dir.path());
            let mut manifest = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            manifest.tls = Some(false);
            let plan = plan_for(&manifest).await;
            assert!(DeviceServiceClient::outbound_token(&manifest, &plan).is_none());
        });
    }

    /// Goal: the withholding must not fire where there is nothing to eavesdrop on. A
    /// loopback peer and a Unix socket never leave this machine, so a token declared
    /// plaintext there is still sent, which is what a local plugin needs.
    #[test]
    fn a_local_plaintext_link_still_carries_the_token() {
        for address in [
            ConnectionType::Tcp("127.0.0.1:11987".to_string()),
            ConnectionType::Tcp("[::1]:11987".to_string()),
            ConnectionType::Tcp("localhost:11987".to_string()),
            ConnectionType::Uds(PathBuf::from("/run/test.sock")),
        ] {
            let dir = tempfile::tempdir().unwrap();
            write_token(dir.path());
            let mut manifest = manifest_in(address.clone(), dir.path().to_path_buf());
            manifest.tls = Some(false);
            let plan = crate::rt::test_runtime(plan_for(&manifest));
            assert!(
                DeviceServiceClient::outbound_token(&manifest, &plan).is_some(),
                "{address:?} should still carry the token"
            );
        }
    }

    /// Goal: a token that cannot become a header must fail the connection, not go out as
    /// an anonymous request. Sending it unauthenticated earns an `Unauthenticated` from
    /// the remote, which reads to the user as credentials being refused when in fact none
    /// were sent, and it repeats on every single RPC instead of once.
    #[test]
    fn an_unusable_token_refuses_the_connection() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            // Survives the trim in `read_token`, but is illegal in a header value.
            std::fs::write(dir.path().join(trust::TOKEN_FILE_NAME), "cc_\u{7f}bad\n").unwrap();
            let manifest = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            let plan = plan_for(&manifest).await;
            assert!(plan.token().is_some(), "the token must reach the parser");

            let error = DeviceServiceClient::outbound_credentials(&manifest, &plan)
                .expect_err("an unusable token must refuse the connection");
            let message = error.to_string();
            assert!(message.contains("test_service"), "{message}");
            assert!(message.contains(trust::TOKEN_FILE_NAME), "{message}");
        });
    }

    /// Goal: the ordinary case still yields a header, so the refusal above is not simply
    /// rejecting everything.
    #[test]
    fn a_usable_token_becomes_a_bearer_header() {
        crate::rt::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            write_token(dir.path());
            let manifest = manifest_in(
                ConnectionType::Tcp(REMOTE.to_string()),
                dir.path().to_path_buf(),
            );
            let plan = plan_for(&manifest).await;
            let credentials = DeviceServiceClient::outbound_credentials(&manifest, &plan)
                .unwrap()
                .expect("a token on a TLS link is sent");
            assert_eq!(credentials.to_str().unwrap(), "Bearer cc_secret");
        });
    }

    /// Goal: Unix sockets stay plain. The kernel already scopes them to this machine, so
    /// TLS would add a certificate to manage for no gain.
    #[test]
    fn uds_services_stay_plaintext() {
        crate::rt::test_runtime(async {
            let uds = manifest(ConnectionType::Uds(PathBuf::from("/run/test.sock")));
            let plan = plan_for(&uds).await;
            let address = DeviceServiceClient::address_from_manifest(&uds, &plan).unwrap();
            assert_eq!(address, "unix:///run/test.sock");
        });
    }

    #[test]
    fn missing_address_is_an_error() {
        crate::rt::test_runtime(async {
            let none = manifest(ConnectionType::None);
            let plan = plan_for(&none).await;
            assert!(DeviceServiceClient::address_from_manifest(&none, &plan).is_err());
        });
    }
}
