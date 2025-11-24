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

use crate::api::actor::{DeviceHandle, StatusHandle};
use crate::api::devices::DeviceDto;
use crate::api::status::DeviceStatusDto;
use crate::device::{ChannelInfo, ChannelStatus, TempInfo, TempStatus};
use crate::grpc_api::device_service::v1::device_service_server::DeviceService;
use crate::grpc_api::device_service::v1::{health_response, HealthRequest, HealthResponse};
use crate::grpc_api::device_service::v1::{CustomFunctionOneRequest, CustomFunctionOneResponse};
use crate::grpc_api::device_service::v1::{
    EnableManualFanControlRequest, EnableManualFanControlResponse,
};
use crate::grpc_api::device_service::v1::{FixedDutyRequest, FixedDutyResponse};
use crate::grpc_api::device_service::v1::{InitializeDeviceRequest, InitializeDeviceResponse};
use crate::grpc_api::device_service::v1::{LcdRequest, LcdResponse};
use crate::grpc_api::device_service::v1::{LightingRequest, LightingResponse};
use crate::grpc_api::device_service::v1::{ListDevicesRequest, ListDevicesResponse};
use crate::grpc_api::device_service::v1::{ResetChannelRequest, ResetChannelResponse};
use crate::grpc_api::device_service::v1::{ShutdownRequest, ShutdownResponse};
use crate::grpc_api::device_service::v1::{SpeedProfileRequest, SpeedProfileResponse};
use crate::grpc_api::device_service::v1::{StatusRequest, StatusResponse};
use crate::grpc_api::models;
use crate::grpc_api::models::v1::status::FanSpeed;
use crate::VERSION;
use nix::unistd::Pid;
use tonic::{Request, Response, Status};

pub struct CCDeviceService {
    service_id: String,
    version: String,
    hostname: String,
    device_handle: DeviceHandle,
    status_handle: StatusHandle,
}

impl CCDeviceService {
    pub fn new(device_handle: DeviceHandle, status_handle: StatusHandle) -> Self {
        let hostname = sysinfo::System::host_name().unwrap_or_default();
        Self {
            service_id: format!("coolercontrold-{hostname}"),
            version: VERSION.to_string(),
            hostname,
            device_handle,
            status_handle,
        }
    }

    fn map_device_model(&self, device_dto: DeviceDto) -> models::v1::Device {
        let channels = device_dto
            .info
            .channels
            .into_iter()
            .map(|(name, info)| (name, info.into()))
            .collect();
        let temps = device_dto
            .info
            .temps
            .into_iter()
            .map(|(name, info)| (name, info.into()))
            .collect();
        let driver_info = models::v1::DriverInfo {
            name: device_dto.info.driver_info.name,
            version: device_dto.info.driver_info.version,
            locations: vec![self.service_id.clone()],
        };
        let device_info = models::v1::DeviceInfo {
            channels,
            temps,
            lighting_speeds: vec![],
            temp_min: Some(f64::from(device_dto.info.temp_min)),
            temp_max: Some(f64::from(device_dto.info.temp_max)),
            profile_min_length: None,
            profile_max_length: None,
            model: device_dto.info.model,
            driver_info: Some(driver_info),
        };
        models::v1::Device {
            id: device_dto.uid.clone(),
            name: device_dto.name,
            uid_info: Some(device_dto.uid),
            info: Some(device_info),
        }
    }
}

#[tonic::async_trait]
#[allow(clippy::cast_sign_loss)]
impl DeviceService for CCDeviceService {
    /// Used to confirm service connection and retrieve service health information.
    async fn health(
        &self,
        _request: Request<HealthRequest>,
    ) -> Result<Response<HealthResponse>, Status> {
        let pid = sysinfo::Pid::from_u32(Pid::this().as_raw() as u32);
        let uptime_seconds = sysinfo::System::new_with_specifics(
            sysinfo::RefreshKind::nothing().with_processes(sysinfo::ProcessRefreshKind::nothing()),
        )
        .process(pid)
        .map_or(0, sysinfo::Process::run_time);
        let reply = HealthResponse {
            name: self.service_id.clone(),
            version: self.version.clone(),
            status: health_response::Status::Ok.into(),
            uptime_seconds,
        };
        Ok(Response::new(reply))
    }

    /// This is the first message sent to the device service after establishing a connection
    /// and is used to detect the service's devices and capabilities.
    /// The device models should be filled out for each device and all of their
    /// available channels. This information is used to populate the `CoolerControl` device
    /// list and available features in the UI.
    async fn list_devices(
        &self,
        _request: Request<ListDevicesRequest>,
    ) -> Result<Response<ListDevicesResponse>, Status> {
        // The CC Device Service Plugin for itself is read-only, and returned DeviceInfo reflects this
        self.device_handle
            .devices_get()
            .await
            .map(|devices| {
                Response::new(ListDevicesResponse {
                    devices: devices
                        .into_iter()
                        .map(|device| self.map_device_model(device))
                        .collect(),
                })
            })
            .map_err(|err| Status::internal(format!("Error listing devices: {err}")))
    }

    /// This is called and used by some devices to initialize hardware, before starting to send
    /// commands to it. It is also be called after resuming from sleep, as many firmwares are rest.
    async fn initialize_device(
        &self,
        _request: Request<InitializeDeviceRequest>,
    ) -> Result<Response<InitializeDeviceResponse>, Status> {
        // handled internally
        Ok(Response::new(InitializeDeviceResponse {}))
    }

    async fn shutdown(
        &self,
        _request: Request<ShutdownRequest>,
    ) -> Result<Response<ShutdownResponse>, Status> {
        // Upstream shutdown should not shut this service down.
        Ok(Response::new(ShutdownResponse {}))
    }

    /// This is called to retrieve the status of all devices and their respective channels
    /// and is called at a regular intervals (default 1 second).
    ///
    /// Note that multiple devices should be polled concurrently to speed up request time.
    /// Device _channels_ usually can not be done concurrently, but that depends on the hardware and drivers.
    async fn status(
        &self,
        request: Request<StatusRequest>,
    ) -> Result<Response<StatusResponse>, Status> {
        self.status_handle
            .recent_device(request.into_inner().device_id)
            .await
            .map(|status| {
                Response::new(StatusResponse {
                    status: status.into(),
                })
            })
            .map_err(|err| Status::internal(format!("Error listing devices status: {err}")))
    }

    /// Reset the device channel to it's default state if applicable. (Auto)
    async fn reset_channel(
        &self,
        _request: Request<ResetChannelRequest>,
    ) -> Result<Response<ResetChannelResponse>, Status> {
        // Not supported
        Ok(Response::new(ResetChannelResponse {}))
    }

    async fn enable_manual_fan_control(
        &self,
        _request: Request<EnableManualFanControlRequest>,
    ) -> Result<Response<EnableManualFanControlResponse>, Status> {
        Err(Status::unimplemented("Not supported"))
    }

    async fn fixed_duty(
        &self,
        _request: Request<FixedDutyRequest>,
    ) -> Result<Response<FixedDutyResponse>, Status> {
        Err(Status::unimplemented("Not supported"))
    }

    async fn speed_profile(
        &self,
        _request: Request<SpeedProfileRequest>,
    ) -> Result<Response<SpeedProfileResponse>, Status> {
        Err(Status::unimplemented("Not supported"))
    }

    async fn lighting(
        &self,
        _request: Request<LightingRequest>,
    ) -> Result<Response<LightingResponse>, Status> {
        Err(Status::unimplemented("Not supported"))
    }

    async fn lcd(&self, _request: Request<LcdRequest>) -> Result<Response<LcdResponse>, Status> {
        Err(Status::unimplemented("Not supported"))
    }

    /// This is a placeholder for any custom functions that the device service might expose.
    async fn custom_function_one(
        &self,
        _request: Request<CustomFunctionOneRequest>,
    ) -> Result<Response<CustomFunctionOneResponse>, Status> {
        Err(Status::unimplemented("No Custom Function"))
    }
}

impl From<ChannelInfo> for models::v1::ChannelInfo {
    fn from(value: ChannelInfo) -> Self {
        // We only expose Speed ChannelInfo, as all channels are read-only anyway.
        let options = if let Some(speed_opt) = value.speed_options {
            Some(models::v1::channel_info::Options::SpeedOptions(
                models::v1::SpeedOptions {
                    min_duty: u32::from(speed_opt.min_duty),
                    max_duty: u32::from(speed_opt.max_duty),
                    fixed_enabled: false,
                    extension: None,
                },
            ))
        } else {
            None
        };
        Self {
            label: value.label,
            options,
        }
    }
}

impl From<TempInfo> for models::v1::TempInfo {
    fn from(value: TempInfo) -> Self {
        Self {
            label: value.label,
            number: u32::from(value.number),
        }
    }
}

impl From<DeviceStatusDto> for Vec<models::v1::Status> {
    fn from(mut value: DeviceStatusDto) -> Self {
        value.status_history.pop().map_or_else(Vec::new, |s| {
            let mut status: Vec<models::v1::Status> = s.temps.into_iter().map(Into::into).collect();
            s.channels
                .into_iter()
                .map(Into::into)
                .for_each(|s| status.push(s));
            status
        })
    }
}

impl From<TempStatus> for models::v1::Status {
    fn from(value: TempStatus) -> Self {
        Self {
            id: value.name,
            metric: Some(models::v1::status::Metric::Temp(value.temp)),
        }
    }
}

impl From<ChannelStatus> for models::v1::Status {
    fn from(value: ChannelStatus) -> Self {
        let metric = if value.duty.is_some() || value.rpm.is_some() {
            Some(models::v1::status::Metric::Speed(FanSpeed {
                duty: value.duty,
                rpm: value.rpm,
            }))
        } else if value.watts.is_some() {
            Some(models::v1::status::Metric::Watts(value.watts.unwrap()))
        } else if value.freq.is_some() {
            Some(models::v1::status::Metric::Mhz(value.freq.unwrap()))
        } else {
            None
        };
        Self {
            id: value.name,
            metric,
        }
    }
}
