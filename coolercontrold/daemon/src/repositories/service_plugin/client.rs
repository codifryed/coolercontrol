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
use tonic::transport::Channel;
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
#[derive(Debug)]
pub struct DeviceServiceClient {
    service_id: ServiceId,

    /// Bearer token for the remote, when the user placed one in the plugin directory.
    /// Attached to every outbound request; absent for a plugin on a Unix socket or a
    /// remote that predates authentication.
    token: Option<String>,

    /// Snapshot of the service-plugin wait timeout. `poll_rate` only
    /// changes on daemon restart, so this value is constant for the
    /// client's lifetime and is computed once in `new` to avoid per-
    /// retry f64 math on the hot path.
    service_wait_timeout: Duration,

    /// Using a `tokio::Mutex` has the advantage of being able to hold a lock over an await point,
    /// and for the small amount of requests we make, performance is shown to be on par with std.
    service_client: Mutex<device_service_client::DeviceServiceClient<Channel>>,

    /// For each device present, we clone the client for it, so we can handle requests per device
    /// concurrently. Clone is a cheap operation for the tonic Client. Each per-device client sits
    /// behind its own `Mutex` (per-device serialization, essential for hardware safety) and an `Rc`
    /// so a caller can clone the `Rc<Mutex>` out of the map and lock it without holding a `RefCell`
    /// borrow across the `.await`. The `RefCell` makes the map interior-mutable so `with_device_ids`
    /// can populate it once at setup while the client is shared (`Rc`) on the sidecar.
    device_clients:
        RefCell<HashMap<DeviceUID, Rc<Mutex<device_service_client::DeviceServiceClient<Channel>>>>>,

    /// Maps the device UID to the service device ID, so we can pass the correct ID to the device service.
    device_ids: RefCell<HashMap<DeviceUID, ServiceDeviceID>>,
}

impl DeviceServiceClient {
    pub async fn connect(
        service_manifest: &ServiceManifest,
        poll_rate: f64,
        tls_strict: bool,
    ) -> Result<Self> {
        let address = Self::address_from_manifest(service_manifest)?;
        let channel = transport::connect(service_manifest, &address, tls_strict).await?;
        let grpc_client = device_service_client::DeviceServiceClient::new(channel);
        Ok(Self::new(
            service_manifest.id.clone(),
            poll_rate,
            grpc_client,
            trust::read_token(&service_manifest.path),
        ))
    }

    /// Derives the gRPC connection address from a manifest. Shared by `connect` and the main-side
    /// proxy handle (which needs the address to map device locations without holding the client).
    ///
    /// TCP is `https`: a remote device service is reached over a network, and the token
    /// this client now sends must not cross it in the clear. Unix sockets stay plain,
    /// since the kernel already scopes them to this machine.
    pub fn address_from_manifest(service_manifest: &ServiceManifest) -> Result<String> {
        match &service_manifest.address {
            ConnectionType::Uds(uds) => Ok(format!("unix://{}", uds.display())),
            ConnectionType::Tcp(tcp_addr) => Ok(format!("https://{tcp_addr}")),
            ConnectionType::None => Err(anyhow!("Invalid Connection Type: NONE!")),
        }
    }

    /// Turns a gRPC failure into an error a user can act on.
    ///
    /// `Unauthenticated` is the one worth special-casing: it means the remote is up and
    /// answering, and refused us. Reported as a bare status it reads like a network
    /// fault, and the retry loop above would then blame startup time for what is
    /// actually a missing or stale token.
    fn call_failed(&self, context: &str, status: &tonic::Status) -> anyhow::Error {
        if status.code() == tonic::Code::Unauthenticated {
            return anyhow!(
                "{context}: device service '{}' refused our credentials. Put a valid access \
                 token from that daemon into a '{}' file in this plugin's directory.",
                self.service_id,
                trust::TOKEN_FILE_NAME
            );
        }
        anyhow!("{context}: {status}")
    }

    /// Wraps a message in a request carrying this client's credentials.
    ///
    /// Every outbound call goes through here, so a new RPC cannot accidentally ship
    /// without the token.
    fn request<T>(&self, message: T) -> Request<T> {
        let mut request = Request::new(message);
        if let Some(token) = &self.token {
            match format!("Bearer {token}").parse() {
                Ok(value) => {
                    request.metadata_mut().insert("authorization", value);
                }
                Err(err) => {
                    // A token with characters illegal in a header is a user error in the
                    // token file, not something to retry: say so once per request rather
                    // than failing with an opaque `Unauthenticated` from the remote.
                    error!(
                        "Device service '{}' has an unusable access token: {err}",
                        self.service_id
                    );
                }
            }
        }
        request
    }

    fn new(
        service_id: ServiceId,
        poll_rate: f64,
        client: device_service_client::DeviceServiceClient<Channel>,
        token: Option<String>,
    ) -> Self {
        let service_client = Mutex::new(client);
        let service_wait_timeout = service_wait_timeout_for(poll_rate);
        Self {
            service_id,
            token,
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

    fn get_device_client(
        &self,
        device_uid: &DeviceUID,
    ) -> Result<Rc<Mutex<device_service_client::DeviceServiceClient<Channel>>>> {
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
                .map_err(|status| self.call_failed("Failed to get health status", &status))
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
                .map_err(|status| self.call_failed("Failed to list devices", &status))
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
    use std::path::PathBuf;

    fn manifest(address: ConnectionType) -> ServiceManifest {
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
            privileged: false,
            proxy: None,
            path: PathBuf::from("/var/lib/coolercontrol/plugins/test_service"),
        }
    }

    /// Goal: a remote reached over the network must be addressed as `https`. Sending the
    /// bearer token over plaintext `http` would hand it to anyone on the path, which is
    /// the whole reason the token exists.
    #[test]
    fn tcp_services_are_addressed_over_tls() {
        let address = DeviceServiceClient::address_from_manifest(&manifest(ConnectionType::Tcp(
            "192.168.1.100:11987".to_string(),
        )))
        .unwrap();
        assert_eq!(address, "https://192.168.1.100:11987");
    }

    /// Goal: Unix sockets stay plain. The kernel already scopes them to this machine, so
    /// TLS would add a certificate to manage for no gain.
    #[test]
    fn uds_services_stay_plaintext() {
        let address = DeviceServiceClient::address_from_manifest(&manifest(ConnectionType::Uds(
            PathBuf::from("/run/test.sock"),
        )))
        .unwrap();
        assert_eq!(address, "unix:///run/test.sock");
    }

    #[test]
    fn missing_address_is_an_error() {
        assert!(
            DeviceServiceClient::address_from_manifest(&manifest(ConnectionType::None)).is_err()
        );
    }
}
