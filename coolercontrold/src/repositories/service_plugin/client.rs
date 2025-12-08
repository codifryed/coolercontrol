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

use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, DeviceUID,
    DriverInfo, DriverType, Duty, LcdInfo, LcdMode, LcdModeType, LightingMode, LightingModeType,
    SpeedOptions, Temp, TempInfo, TempStatus,
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
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Result};
use log::error;
use std::collections::HashMap;
use std::default::Default;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio::time::sleep;
use tonic::transport::Channel;
use tonic::Request;

static DEVICE_SERVICE_WAIT_TIMEOUT: LazyLock<Duration> = LazyLock::new(|| Duration::from_secs(8));

/// Our client wrapper for Device Service plugins.
/// This handles CC's device service contract by only allowing a single request at a time per device,
/// handling the permit/locking system and timeouts. It also maps CC's models to the generated
/// device service models.
#[derive(Debug)]
pub struct DeviceServiceClient {
    service_id: ServiceId,
    client_address: String,
    poll_rate: f64,

    /// Using a `tokio::Mutex` has the advantage of being able to hold a lock over an await point,
    /// and for the small amount of requests we make, performance is shown to be on par with std.
    service_client: Mutex<device_service_client::DeviceServiceClient<Channel>>,

    /// For each device present, we clone the client for it, so we can handle requests per device
    /// concurrently. Clone is a cheap operation for the tonic Client.
    device_clients: HashMap<DeviceUID, Mutex<device_service_client::DeviceServiceClient<Channel>>>,

    /// Maps the device UID to the service device ID, so we can pass the correct ID to the device service.
    device_ids: HashMap<DeviceUID, ServiceDeviceID>,
}

impl DeviceServiceClient {
    pub async fn connect(service_manifest: &ServiceManifest, poll_rate: f64) -> Result<Self> {
        let address = match &service_manifest.address {
            ConnectionType::Uds(uds) => {
                format!("unix://{}", uds.display())
            }
            ConnectionType::Tcp(tcp_addr) => {
                format!("http://{tcp_addr}")
            }
            ConnectionType::None => return Err(anyhow!("Invalid Connection Type: NONE!")),
        };
        let grpc_client =
            device_service_client::DeviceServiceClient::connect(address.clone()).await?;
        Ok(Self::new(
            service_manifest.id.clone(),
            address,
            poll_rate,
            grpc_client,
        ))
    }

    fn new(
        service_id: ServiceId,
        client_address: String,
        poll_rate: f64,
        client: device_service_client::DeviceServiceClient<Channel>,
    ) -> Self {
        let service_client = Mutex::new(client);
        Self {
            service_id,
            client_address,
            poll_rate,
            service_client,
            device_clients: HashMap::new(),
            device_ids: HashMap::new(),
        }
    }

    /// This allows us to make a few basic requests with our timeout logic before we have the device IDs.
    pub async fn with_device_ids(&mut self, device_ids: Vec<(DeviceUID, ServiceDeviceID)>) {
        let mut device_clients = HashMap::new();
        let mut device_id_map = HashMap::new();
        for (device_uid, service_device_id) in device_ids {
            device_clients.insert(
                device_uid.clone(),
                Mutex::new(self.service_client.lock().await.clone()),
            );
            device_id_map.insert(device_uid, service_device_id);
        }
        self.device_clients = device_clients;
        self.device_ids = device_id_map;
    }

    /// For service-wide requests where we want to make sure all devices clients are inactive.
    async fn wait_till_all_clients_are_free(&self) {
        for device_client in self.device_clients.values() {
            let _ = device_client.lock().await;
        }
    }

    fn get_device_client(
        &self,
        device_uid: &DeviceUID,
    ) -> Result<&Mutex<device_service_client::DeviceServiceClient<Channel>>> {
        self.device_clients
            .get(device_uid)
            .ok_or_else(|| anyhow!("Device {device_uid} not found"))
    }

    fn get_service_device_id(&self, device_uid: &DeviceUID) -> Result<ServiceDeviceID> {
        self.device_ids
            .get(device_uid)
            .cloned()
            .ok_or_else(|| anyhow!("Service Device {device_uid} ID not found"))
    }

    pub async fn health(&self) -> Result<HealthResponse> {
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to get health status. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            // Health endpoint need not wait for all clients:
            mut service_client = self.service_client.lock() => {
                let request = Request::new(HealthRequest{});
                service_client.health(request).await
                .map(tonic::Response::into_inner)
                .map_err(|s| anyhow!("Failed to get health status: {s}"))
            }
        }
    }

    pub async fn list_devices(&self) -> Result<Vec<(ServiceDeviceID, Device)>> {
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to list devices. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            () = self.wait_till_all_clients_are_free() => {
                let request = Request::new(ListDevicesRequest{});
                let mut service_client = self.service_client.lock().await;
                service_client.list_devices(request).await
                .map(|r| self.map_devices(r))
                .map_err(|s| anyhow!("Failed to list devices: {s}"))
            }
        }
    }

    pub async fn initialize_device(&self, device_uid: &DeviceUID) -> Result<()> {
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to initialize devices. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            () = self.wait_till_all_clients_are_free() => {
                let request = Request::new(InitializeDeviceRequest{
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
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to shutdown service. \
                There may be significant issues handling this device service plugin due to extreme lag.",
                self.service_id
            )),
            () = self.wait_till_all_clients_are_free() => {
                let request = Request::new(ShutdownRequest{});
                let mut service_client = self.service_client.lock().await;
                service_client.shutdown(request).await
                .map(|_| ())
                .map_err(|s| anyhow!("Failed to shutdown service: {s}"))
            }
        }
    }

    fn map_devices(
        &self,
        devices_response: tonic::Response<ListDevicesResponse>,
    ) -> Vec<(ServiceDeviceID, Device)> {
        let mut devices = vec![];
        for (index, device_res) in devices_response
            .into_inner()
            .devices
            .into_iter()
            .enumerate()
        {
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
                    driver_info: info_res.driver_info.map_or_else(
                        || DriverInfo {
                            drv_type: DriverType::External,
                            ..Default::default()
                        },
                        |d| DriverInfo {
                            drv_type: DriverType::External,
                            name: d.name,
                            version: d.version,
                            locations: self.add_address_to_locations(d.locations),
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
                    self.poll_rate,
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
                let mut channel_info = ChannelInfo {
                    label: channel_info_res.label,
                    ..Default::default()
                };
                if let Some(options) = channel_info_res.options {
                    match options {
                        Options::SpeedOptions(speed_options) => {
                            channel_info.speed_options = Some(SpeedOptions {
                                min_duty: safe_u8(speed_options.min_duty),
                                max_duty: safe_u8(speed_options.max_duty),
                                fixed_enabled: speed_options.fixed_enabled,
                                extension: Self::map_channel_extension_names(
                                    speed_options.extension,
                                ),
                            });
                        }
                        Options::LightingModes(lighting_modes) => {
                            channel_info.lighting_modes = lighting_modes
                                .lighting_mode
                                .into_iter()
                                .map(|mode| LightingMode {
                                    frontend_name: mode
                                        .frontend_name
                                        .unwrap_or_else(|| mode.name.clone()),
                                    name: mode.name,
                                    min_colors: safe_u8(mode.min_colors),
                                    max_colors: safe_u8(mode.max_colors),
                                    speed_enabled: mode.speed_enabled,
                                    backward_enabled: mode.backward_enabled,
                                    type_: LightingModeType::None,
                                })
                                .collect();
                        }
                        Options::LcdInfo(lcd_info) => {
                            channel_info.lcd_modes = lcd_info
                                .lcd_modes
                                .into_iter()
                                .map(|mode| LcdMode {
                                    frontend_name: mode
                                        .frontend_name
                                        .unwrap_or_else(|| mode.name.clone()),
                                    name: mode.name,
                                    brightness: mode.brightness,
                                    orientation: mode.orientation,
                                    image: mode.image,
                                    colors_min: 0,
                                    colors_max: 0,
                                    type_: LcdModeType::None,
                                })
                                .collect();
                            channel_info.lcd_info = Some(LcdInfo {
                                screen_width: lcd_info.screen_width,
                                screen_height: lcd_info.screen_height,
                                max_image_size_bytes: lcd_info.max_image_size_bytes,
                            });
                        }
                    }
                }
                (channel_name, channel_info)
            })
            .collect()
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

    fn add_address_to_locations(&self, mut locations: Vec<String>) -> Vec<String> {
        locations.push(self.client_address.clone());
        locations
    }

    pub async fn status(
        &self,
        device_uid: &DeviceUID,
    ) -> Result<(Vec<ChannelStatus>, Vec<TempStatus>)> {
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to get device: {device_uid} status. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
                let request = Request::new(StatusRequest{
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
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to reset device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
                let request = Request::new(ResetChannelRequest{
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
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to enable manual fan control for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
                let request = Request::new(EnableManualFanControlRequest{
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
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set fixed duty for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
                let request = Request::new(FixedDutyRequest{
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
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set speed profile for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
                let mut req_speed_profile = Vec::new();
                for (temp, duty) in speed_profile {
                    req_speed_profile.push(SpeedProfilePoint {
                        temp: *temp,
                        duty: u32::from(*duty),
                    });
                }
                let request = Request::new(SpeedProfileRequest{
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
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set lighting for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
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
                let request = Request::new(LightingRequest {
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
        tokio::select! {
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to set LCD for device: {device_uid} channel: {channel_name}. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut device_client = self.get_device_client(device_uid)?.lock() => {
                let lcd_setting = LcdSetting {
                    mode: lcd.mode.clone(),
                    brightness: lcd.brightness.map(u32::from),
                    orientation: lcd.orientation.map(u32::from),
                    image_path: lcd.image_file_processed.clone(),
                };
                let request = Request::new(LcdRequest {
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
            () = sleep(*DEVICE_SERVICE_WAIT_TIMEOUT) => Err(anyhow!(
                "TIMEOUT Device Service Plugin {}; waiting to apply custom function. \
                There may be significant issues handling this device due to lag.",
                self.service_id,
            )),
            mut service_client = self.service_client.lock() => {
                let request = Request::new(CustomFunctionOneRequest{});
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
