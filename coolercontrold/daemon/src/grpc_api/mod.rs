// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

mod cc_device_service;

use crate::api::actor::{DeviceHandle, StatusHandle};
use crate::grpc_api::cc_device_service::CCDeviceService;
use crate::grpc_api::device_service::v1::device_service_server::DeviceServiceServer;
use std::ops::Not;
use tonic::server::NamedService;
use tonic_health::pb::health_server::{Health, HealthServer};
use tonic_health::server::HealthService;

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

/// Route pattern for a generated gRPC service: its fully qualified name, then any method.
///
/// gRPC method paths are absolute and fully qualified (`/<package>.<Service>/<Method>`),
/// so this cannot collide with a REST route or a served asset. Axum matches explicit
/// routes before the static-asset fallback, so nothing else can shadow it either.
///
/// The name is the codegen's own `NamedService::NAME`, the one tonic routes on and
/// `health_service` registers, so renaming the proto package or service moves the route
/// with it rather than leaving it to answer `Unimplemented`. Built once per listener when
/// the router is assembled; requests match against axum's parsed route table.
fn service_route<S: NamedService>() -> String {
    // A blank name or one containing a slash would change the shape of the route.
    debug_assert!(S::NAME.is_empty().not());
    debug_assert!(S::NAME.contains('/').not());
    format!("/{}/{{*method}}", S::NAME)
}

/// Route pattern for the device service.
pub fn device_service_route() -> String {
    service_route::<DeviceServiceServer<CCDeviceService>>()
}

/// Route pattern for the standard gRPC health service. `NAME` does not depend on the
/// implementation, so naming tonic's own `HealthService` gives the right route for the
/// opaque type `health_service` returns.
pub fn health_service_route() -> String {
    service_route::<HealthServer<HealthService>>()
}

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
    use crate::grpc_api::device_service::v1::device_service_client::DeviceServiceClient;
    use crate::grpc_api::device_service::v1::HealthRequest;
    use std::convert::Infallible;
    use tokio::net::TcpListener;
    use tonic::{Code, Status};

    /// Goal: the generated client's paths land on the route the device service is mounted
    /// at. The client takes its paths from the proto through codegen, and the route takes
    /// its name from `NamedService`, so this is what notices the two parting ways. It is
    /// also the only test that mounts the device-service route: the real service needs
    /// live actor handles, so a stub stands in and answers with a marker status. A missed
    /// route would 404, which tonic reports as `Unimplemented`, not the marker.
    #[tokio::test]
    async fn the_generated_client_reaches_the_device_service_route() {
        let marker = tower::service_fn(|_: axum::extract::Request| async {
            Ok::<_, Infallible>(Status::already_exists("routed").into_http::<axum::body::Body>())
        });
        let router = axum::Router::new().route_service(&device_service_route(), marker);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });

        let channel = tonic::transport::Endpoint::new(format!("http://{address}"))
            .unwrap()
            .connect()
            .await
            .unwrap();
        let status = DeviceServiceClient::new(channel)
            .health(HealthRequest {})
            .await
            .expect_err("the stub answers every call with the marker status");

        assert_eq!(status.code(), Code::AlreadyExists, "{status}");
        assert_eq!(status.message(), "routed");
    }
}
