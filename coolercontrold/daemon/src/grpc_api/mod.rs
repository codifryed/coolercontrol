// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod cc_device_service;

use crate::api::actor::{DeviceHandle, StatusHandle};
use crate::grpc_api::cc_device_service::CCDeviceService;
use crate::grpc_api::device_service::v1::device_service_server::DeviceServiceServer;
use tonic_health::pb::health_server::{Health, HealthServer};

// Note: the rust module relational hierarchy MUST follow the proto package hierarchy
pub mod models {
    pub mod v1 {
        #![allow(clippy::pedantic)]
        tonic::include_proto!("coolercontrol.models.v1");
    }
}
pub mod device_service {
    pub mod v1 {
        #![allow(clippy::pedantic)]
        tonic::include_proto!("coolercontrol.device_service.v1");
    }
}

/// Route pattern for the device service.
///
/// gRPC method paths are absolute and fully qualified (`/<package>.<Service>/<Method>`),
/// so this cannot collide with a REST route or a served asset. Axum matches explicit
/// routes before the static-asset fallback, so nothing else can shadow it either.
pub const DEVICE_SERVICE_PATH: &str = "/coolercontrol.device_service.v1.DeviceService/{*method}";

/// Route pattern for the standard gRPC health service.
pub const HEALTH_SERVICE_PATH: &str = "/grpc.health.v1.Health/{*method}";

/// The device service, ready to mount into the REST router.
///
/// It is served from the same listener and behind the same authentication as the REST
/// API rather than from a second `tonic::transport::Server`. That listener already
/// speaks HTTP/2 (hyper's auto builder handles h2c prior-knowledge, and the rustls
/// config advertises `h2` via ALPN), so gRPC needs no transport of its own, and folding
/// it in is what gives it TLS and auth instead of leaving both absent.
///
/// Server reflection is deliberately not registered: nothing in the project consumes it,
/// the protos are public in the repository anyway, and it only widens what an unproven
/// peer can enumerate.
pub fn device_service(
    device_handle: DeviceHandle,
    status_handle: StatusHandle,
    calibration_handle: crate::api::actor::CalibrationHandle,
) -> DeviceServiceServer<CCDeviceService> {
    let service = CCDeviceService::new(device_handle, status_handle, calibration_handle);
    DeviceServiceServer::new(service)
}

/// The standard gRPC health service, reporting the device service as serving.
pub async fn health_service() -> HealthServer<impl Health> {
    let (health_reporter, health_service) = tonic_health::server::health_reporter();
    health_reporter
        .set_serving::<DeviceServiceServer<CCDeviceService>>()
        .await;
    health_service
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: the route patterns stay fully qualified gRPC paths. A typo here would either
    /// shadow a REST route or silently fall through to the static-asset fallback, which
    /// answers 200 with HTML and would look like a broken client rather than a bad route.
    #[test]
    fn service_paths_are_fully_qualified() {
        assert!(DEVICE_SERVICE_PATH.starts_with("/coolercontrol.device_service.v1.DeviceService/"));
        assert!(HEALTH_SERVICE_PATH.starts_with("/grpc.health.v1.Health/"));
        assert!(DEVICE_SERVICE_PATH.ends_with("/{*method}"));
        assert!(HEALTH_SERVICE_PATH.ends_with("/{*method}"));
    }
}
