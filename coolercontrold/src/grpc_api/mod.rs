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

mod cc_device_service;
mod grpc_limiter;

use crate::api::actor::{DeviceHandle, StatusHandle};
use crate::grpc_api::cc_device_service::CCDeviceService;
use crate::grpc_api::device_service::v1::device_service_server::DeviceServiceServer;
use crate::grpc_api::grpc_limiter::{GRPCLimiterConfig, GRPCLimiterLayer};
use anyhow::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tonic::transport::Server;
const API_RATE_BURST: u32 = 80;
const API_RATE_REQ_PER_SEC: u64 = 50; // Status requests per device per second

// Note: the rust module relational hierarchy MUST follow the proto package hierarchy
pub mod models {
    pub mod v1 {
        tonic::include_proto!("coolercontrol.models.v1");
    }
}
pub mod device_service {
    pub mod v1 {
        tonic::include_proto!("coolercontrol.device_service.v1");
    }
    pub const FILE_DESCRIPTOR_SET: &[u8] =
        tonic::include_file_descriptor_set!("device_service_descriptor");
}

pub async fn create_grpc_api_server(
    addr: SocketAddr,
    device_handle: DeviceHandle,
    status_handle: StatusHandle,
    cancel_token: CancellationToken,
) -> Result<()> {
    let service = CCDeviceService::new(device_handle, status_handle);
    let limiter_layer = GRPCLimiterLayer {
        config: Arc::new(GRPCLimiterConfig::new(
            Duration::from_millis(1000 / API_RATE_REQ_PER_SEC),
            API_RATE_BURST,
        )),
    };
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<DeviceServiceServer<CCDeviceService>>()
        .await;
    let reflection_service = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(device_service::FILE_DESCRIPTOR_SET)
        .register_encoded_file_descriptor_set(tonic_health::pb::FILE_DESCRIPTOR_SET)
        .build_v1()?;
    Server::builder()
        .layer(limiter_layer)
        .add_service(reflection_service)
        .add_service(health_service)
        .add_service(DeviceServiceServer::new(service))
        .serve_with_shutdown(addr, cancel_token.cancelled())
        .await?;
    Ok(())
}
