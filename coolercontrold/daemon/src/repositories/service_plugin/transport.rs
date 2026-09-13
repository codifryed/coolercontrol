// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Connecting to a device-service plugin.
//!
//! Unix sockets connect as they always did. TCP goes over TLS when the link carries a
//! token, because a remote device service is reached across a network and a bearer token
//! must not travel in the clear. A plugin that serves plain h2c, which is every
//! third-party plugin and every daemon older than this change, is reached exactly as
//! before. See `trust::LinkPlan`.
//!
//! tonic is built here without its own TLS features, so the channel is built from a
//! connector over the `tokio-rustls` stack the daemon already carries for the server
//! side. That is also the only way to install [`trust::PinnedCertVerifier`], since no
//! public CA is involved and identity has to come from the certificate fingerprint.

use crate::repositories::service_plugin::service_management::ServiceId;
use crate::repositories::service_plugin::service_manifest::{ConnectionType, ServiceManifest};
use crate::repositories::service_plugin::trust::{self, PinnedCertVerifier};
use anyhow::{anyhow, Context, Result};
use axum::http::{HeaderMap, HeaderValue, Request, Response};
use hyper_util::rt::TokioIo;
use log::{error, info, warn};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use std::future::Future;
use std::ops::Not;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context as TaskContext, Poll};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;
use tonic::transport::{Channel, Endpoint, Uri};

/// Builds a channel to the service described by `manifest`.
///
/// On first contact with an unpinned TCP peer this records the peer's fingerprint, so a
/// later change is refused rather than silently accepted.
pub async fn connect(
    manifest: &ServiceManifest,
    address: &str,
    plan: &trust::LinkPlan,
    tls_strict: bool,
) -> Result<Channel> {
    match &manifest.address {
        ConnectionType::Tcp(tcp_address) if plan.encrypted() => {
            connect_tls(manifest, address, tcp_address, tls_strict).await
        }
        // Plaintext h2c. A Unix socket is already scoped to this machine by the kernel;
        // a TCP peer reaching here has no token to protect.
        ConnectionType::Uds(_) | ConnectionType::Tcp(_) => Endpoint::try_from(address.to_string())?
            .connect()
            .await
            .with_context(|| format!("Connecting to device service at {address}")),
        ConnectionType::None => Err(anyhow!("Invalid Connection Type: NONE!")),
    }
}

async fn connect_tls(
    manifest: &ServiceManifest,
    address: &str,
    tcp_address: &str,
    tls_strict: bool,
) -> Result<Channel> {
    let host = host_of(tcp_address);
    let is_loopback = trust::is_loopback_host(&host);
    let pinned = trust::read_pin(&manifest.path).await;
    let provider = rustls::crypto::ring::default_provider();
    let verifier = Arc::new(PinnedCertVerifier::new(
        Arc::new(provider.clone()),
        is_loopback,
        tls_strict,
        pinned.clone(),
    ));

    let mut config = ClientConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()
        .context("Building the TLS client configuration")?
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    // The remote serves gRPC over HTTP/2, so the handshake has to offer it by name or the
    // server settles on HTTP/1.1 and every RPC fails.
    config.alpn_protocols = vec![b"h2".to_vec()];

    let channel = build_channel(address, tcp_address, Arc::new(config)).await;
    let channel = match channel {
        Ok(channel) => channel,
        Err(err) => {
            if let Some(reason) = verifier.rejection() {
                error!("{}", reason.describe(&manifest.id, tcp_address));
                return Err(anyhow!(
                    "TLS trust check failed for device service '{}'",
                    manifest.id
                ));
            }
            return Err(err);
        }
    };

    if let Some(pin) = verifier.pin_to_persist() {
        info!(
            "Device service '{}' at {tcp_address} pinned on first contact with certificate \
             fingerprint {pin}. A later change will be refused; delete the '{}' file in that \
             plugin's directory if the remote legitimately regenerates its certificate.",
            manifest.id,
            trust::PIN_FILE_NAME
        );
        if let Err(err) = trust::write_pin(&manifest.path, &pin).await {
            // Not fatal: the connection is up. It just means the next start pins again,
            // so the user loses change detection until the write succeeds.
            warn!(
                "Could not record the TLS pin for device service '{}': {err}",
                manifest.id
            );
        }
    }
    Ok(channel)
}

async fn build_channel(
    address: &str,
    tcp_address: &str,
    config: Arc<ClientConfig>,
) -> Result<Channel> {
    let endpoint = Endpoint::try_from(address.to_string())
        .with_context(|| format!("Parsing device service address {address}"))?;
    let host = host_of(tcp_address);
    let target = tcp_address.to_string();
    endpoint
        .connect_with_connector(tower::service_fn(move |_: Uri| {
            let config = config.clone();
            let host = host.clone();
            let target = target.clone();
            async move {
                let stream = TcpStream::connect(&target).await?;
                // The pin is the peer's identity here, so this name only satisfies
                // rustls' API; `PinnedCertVerifier` ignores it.
                let server_name = ServerName::try_from(host).unwrap_or_else(|_| {
                    ServerName::try_from("localhost")
                        .expect("'localhost' is a valid DNS name, so this parse cannot fail")
                });
                let tls = TlsConnector::from(config)
                    .connect(server_name, stream)
                    .await?;
                Ok::<_, std::io::Error>(TokioIo::new(tls))
            }
        }))
        .await
        .with_context(|| format!("Connecting to device service at {address}"))
}

/// The channel a plugin client talks over: a `Channel` that explains credential refusals.
pub type PluginChannel = ExplainRefusals<Channel>;

const GRPC_STATUS: &str = "grpc-status";
const GRPC_MESSAGE: &str = "grpc-message";
/// `tonic::Code::Unauthenticated` on the wire.
const UNAUTHENTICATED: &str = "16";

/// Wraps a channel so a refusal from the remote says what to do about it.
///
/// Deliberately at the HTTP layer, not at the call sites. The RPC methods are generated
/// from the proto, so anything wired in per call site loses coverage the moment a method
/// is added: whoever writes the new one copies the neighbouring `map_err` and the
/// explanation quietly stops applying. Here every RPC on the channel is covered, present
/// and future, and nothing in this file knows a single method name.
#[derive(Clone, Debug)]
pub struct ExplainRefusals<S> {
    inner: S,
    service_id: ServiceId,
}

impl<S> ExplainRefusals<S> {
    pub fn new(inner: S, service_id: ServiceId) -> Self {
        Self { inner, service_id }
    }
}

/// Replaces the remote's `grpc-message` when it refused our credentials.
///
/// Only the trailers-only shape is rewritten, which is what an auth rejection is: both
/// our own `grpc_error_middleware` and a tonic interceptor answer before any message, so
/// `grpc-status` lands in the headers. A refusal raised mid-stream would carry its status
/// in the trailers instead and is left alone, since no device-service RPC streams.
fn explain_refusal(headers: &mut HeaderMap, service_id: &str) {
    let refused = headers
        .get(GRPC_STATUS)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value == UNAUTHENTICATED);
    if refused.not() {
        return;
    }
    let message = format!(
        "device service '{service_id}' refused our credentials. Put a valid access token \
         from that daemon into a '{}' file in this plugin's directory.",
        trust::TOKEN_FILE_NAME
    );
    if let Ok(value) = HeaderValue::from_str(&message) {
        headers.insert(GRPC_MESSAGE, value);
    }
}

impl<S, ReqBody, ResBody> tower::Service<Request<ReqBody>> for ExplainRefusals<S>
where
    S: tower::Service<Request<ReqBody>, Response = Response<ResBody>>,
    S::Future: Send + 'static,
    S::Error: Send + 'static,
    ResBody: Send + 'static,
{
    type Response = Response<ResBody>;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut TaskContext<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let service_id = self.service_id.clone();
        let future = self.inner.call(request);
        Box::pin(async move {
            let mut response = future.await?;
            explain_refusal(response.headers_mut(), &service_id);
            Ok(response)
        })
    }
}

/// The host part of an `address:port`, tolerating bracketed IPv6 literals.
pub fn host_of(tcp_address: &str) -> String {
    let trimmed = tcp_address.trim();
    // The result feeds both the loopback exemption and the TLS server name, so an empty
    // or bracket-only host would silently widen trust.
    debug_assert!(trimmed.is_empty().not());
    if let Some(rest) = trimmed.strip_prefix('[') {
        if let Some((host, _)) = rest.split_once(']') {
            return host.to_string();
        }
    }
    match trimmed.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host.to_string(),
        _ => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: the host is what the loopback exemption and the TLS server name are derived
    /// from, so splitting it off the port must handle IPv6 literals. Reading "::1" as a
    /// hostname would skip the loopback exemption for every IPv6 local plugin.
    #[test]
    fn host_is_split_from_the_port() {
        assert_eq!(host_of("127.0.0.1:11987"), "127.0.0.1");
        assert_eq!(host_of("192.168.1.100:11987"), "192.168.1.100");
        assert_eq!(host_of("example.com:11987"), "example.com");
        assert_eq!(host_of("[::1]:11987"), "::1");
        assert_eq!(host_of("[fe80::1]:11987"), "fe80::1");
        assert_eq!(host_of(" 127.0.0.1:11987 "), "127.0.0.1");
    }

    fn refusal_headers(status: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(GRPC_STATUS, HeaderValue::from_str(status).unwrap());
        headers.insert(GRPC_MESSAGE, HeaderValue::from_static("unauthenticated"));
        headers
    }

    /// Goal: a remote refusing our credentials must say so in terms a user can act on.
    /// Reported as the bare status it reads like a network fault, and the client's retry
    /// loop then blames startup lag for a missing token.
    #[test]
    fn a_refusal_is_explained() {
        let mut headers = refusal_headers(UNAUTHENTICATED);
        explain_refusal(&mut headers, "my_plugin");
        let message = headers.get(GRPC_MESSAGE).unwrap().to_str().unwrap();
        assert!(message.contains("my_plugin"), "{message}");
        assert!(message.contains(trust::TOKEN_FILE_NAME), "{message}");
        // A dropped line continuation in the literal leaves a run of spaces mid-sentence,
        // which the compiler is happy with and the user reads.
        assert!(message.contains("  ").not(), "{message}");
    }

    /// Goal: only a refusal is rewritten. Relabelling an unrelated failure would hide the
    /// real fault behind a credentials message and send the user editing the wrong file.
    #[test]
    fn other_outcomes_are_left_alone() {
        for status in ["0", "14", "4", "12"] {
            let mut headers = refusal_headers(status);
            explain_refusal(&mut headers, "my_plugin");
            assert_eq!(
                headers.get(GRPC_MESSAGE).unwrap(),
                "unauthenticated",
                "status {status} should not be rewritten"
            );
        }

        let mut no_status = HeaderMap::new();
        no_status.insert(GRPC_MESSAGE, HeaderValue::from_static("unauthenticated"));
        explain_refusal(&mut no_status, "my_plugin");
        assert_eq!(no_status.get(GRPC_MESSAGE).unwrap(), "unauthenticated");
    }

    /// Goal: the wrapper covers whatever goes through the channel, which is the point of
    /// putting it here rather than at the call sites. This drives it as a real
    /// `tower::Service`, so the response actually passes through `call`.
    #[tokio::test]
    async fn the_wrapper_explains_every_response_on_the_channel() {
        use tower::ServiceExt;

        let inner = tower::service_fn(|_: Request<()>| async {
            let mut response = Response::new(());
            *response.headers_mut() = refusal_headers(UNAUTHENTICATED);
            Ok::<_, std::convert::Infallible>(response)
        });
        let service = ExplainRefusals::new(inner, "my_plugin".to_string());

        let response = service.oneshot(Request::new(())).await.unwrap();

        let message = response
            .headers()
            .get(GRPC_MESSAGE)
            .unwrap()
            .to_str()
            .unwrap();
        assert!(message.contains("my_plugin"), "{message}");
    }

    /// Goal: an address with no port, or a trailing colon that is not one, is not
    /// silently truncated into a different host.
    #[test]
    fn host_without_a_port_is_left_alone() {
        assert_eq!(host_of("example.com"), "example.com");
        assert_eq!(host_of("127.0.0.1"), "127.0.0.1");
        assert_eq!(host_of("example.com:notaport"), "example.com:notaport");
    }
}
