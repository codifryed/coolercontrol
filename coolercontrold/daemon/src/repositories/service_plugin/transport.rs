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
use axum::http::{header, HeaderMap, HeaderValue, Request, Response, StatusCode};
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
use tonic::Status;

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
    info!(
        "{}",
        describe_link(&manifest.id, &manifest.address, plan.encrypted())
    );
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

/// How the link to a plugin is protected, for the log.
///
/// The pin message is the only other place TLS shows up, and it appears on first contact
/// alone, so from the second start onwards nothing distinguishes an encrypted link from a
/// plaintext one.
fn describe_link(service_id: &str, address: &ConnectionType, encrypted: bool) -> String {
    debug_assert!(service_id.is_empty().not());
    match address {
        ConnectionType::Uds(path) => format!(
            "Connecting to device service '{service_id}' over the local socket {}",
            path.display()
        ),
        ConnectionType::Tcp(tcp_address) if encrypted => {
            format!("Connecting to device service '{service_id}' at {tcp_address} over TLS")
        }
        ConnectionType::Tcp(tcp_address) => {
            format!("Connecting to device service '{service_id}' at {tcp_address} without TLS")
        }
        ConnectionType::None => {
            format!("Device service '{service_id}' has no address to connect to")
        }
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

/// The channel a plugin client talks over: a `Channel` that explains refusals.
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

/// Whether the remote answered with a redirect to HTTPS.
///
/// That is how a `CoolerControl` daemon with TLS enabled answers an unencrypted request from
/// another machine, before authentication ever runs.
fn redirects_to_https<B>(response: &Response<B>) -> bool {
    if response.status().is_redirection().not() {
        return false;
    }
    response
        .headers()
        .get(header::LOCATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|location| location.get(.."https://".len()))
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
}

/// Turns a redirect to HTTPS into a refusal the client reads and can act on.
///
/// gRPC cannot follow a redirect, so left alone it surfaces as an unexplained failure that
/// the connection loop retries until it reports a startup timeout, for a remote that is up
/// and answering.
///
/// Reported as `Unauthenticated` because off this machine a link is only unencrypted when
/// it carries no token (see `trust::LinkPlan`), and, as with a refused token, no retry can
/// fix it. Shaped as the trailers-only response a gRPC server sends for an error, so
/// reading it does not depend on tonic checking `grpc-status` before the HTTP status.
fn explain_unencrypted_refusal<B>(response: &mut Response<B>, service_id: &str) {
    debug_assert!(response.status().is_redirection());
    debug_assert!(service_id.is_empty().not());
    let message = format!(
        "device service '{service_id}' only accepts encrypted connections. Put an access token \
         from that daemon into a '{}' file in this plugin's directory, which switches the \
         connection to TLS, and remove 'tls = false' from its manifest if it is set.",
        trust::TOKEN_FILE_NAME
    );
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/grpc"),
    );
    let written = Status::unauthenticated(message).add_header(response.headers_mut());
    // Percent-encoding makes any message a valid header value, so this cannot fail.
    debug_assert!(written.is_ok());
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
            // Exclusive: the rewritten redirect is itself an `Unauthenticated` status, and
            // `explain_refusal` would replace its message with the wrong advice.
            if redirects_to_https(&response) {
                explain_unencrypted_refusal(&mut response, &service_id);
            } else {
                explain_refusal(response.headers_mut(), &service_id);
            }
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
    use crate::repositories::service_plugin::client::credential_refusal;
    use tonic::Code;

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

    /// Goal: the connect line has to state whether the link is encrypted. Inferring it
    /// from the pin message only works on first contact, and inferring it from a remote
    /// address not at all.
    #[test]
    fn the_link_description_states_whether_it_is_encrypted() {
        let remote = ConnectionType::Tcp("10.1.1.11:11987".into());

        let encrypted = describe_link("Gerver", &remote, true);
        assert!(encrypted.contains("Gerver"), "{encrypted}");
        assert!(encrypted.contains("10.1.1.11:11987"), "{encrypted}");
        assert!(encrypted.contains("over TLS"), "{encrypted}");
        assert!(encrypted.contains("without TLS").not(), "{encrypted}");

        let plain = describe_link("Gerver", &remote, false);
        assert!(plain.contains("without TLS"), "{plain}");

        let local = describe_link("local", &ConnectionType::Uds("/run/cc.sock".into()), false);
        assert!(local.contains("/run/cc.sock"), "{local}");
        // A Unix socket is scoped to this machine by the kernel, so naming the missing
        // TLS would read as a problem to go and fix.
        assert!(local.contains("TLS").not(), "{local}");
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

    fn redirect_to(location: &str) -> Response<()> {
        let mut response = Response::new(());
        *response.status_mut() = StatusCode::MOVED_PERMANENTLY;
        response
            .headers_mut()
            .insert(header::LOCATION, HeaderValue::from_str(location).unwrap());
        response
    }

    /// Goal: a remote that insists on TLS is reported as a refusal the connection loop
    /// gives up on at once, with advice that fixes it. Checked the way the client reads
    /// it: tonic parses the status from the headers, then the loop classifies it through
    /// `credential_refusal`. Left as a bare redirect, the loop retried it into a startup
    /// timeout.
    #[test]
    fn an_https_redirect_becomes_an_explained_refusal() {
        let mut response = redirect_to(
            "https://10.1.1.11:11987/coolercontrol.device_service.v1.DeviceService/Health",
        );
        assert!(redirects_to_https(&response));

        explain_unencrypted_refusal(&mut response, "my_plugin");

        assert_eq!(response.status(), StatusCode::OK);
        let status = Status::from_header_map(response.headers()).expect("a status tonic reads");
        assert_eq!(status.code(), Code::Unauthenticated);
        let message = status.message().to_string();
        assert!(message.contains("my_plugin"), "{message}");
        assert!(message.contains("encrypted"), "{message}");
        assert!(message.contains(trust::TOKEN_FILE_NAME), "{message}");
        assert!(message.contains("tls = false"), "{message}");
        // A dropped line continuation leaves a run of spaces mid-sentence.
        assert!(message.contains("  ").not(), "{message}");

        let health_error = anyhow::Error::new(status).context("Failed to get health status");
        assert_eq!(
            credential_refusal(&health_error).as_deref(),
            Some(message.as_str())
        );
    }

    /// Goal: only a redirect to HTTPS is recognised. Any other redirect, or a response
    /// that is no redirect at all, says nothing about encryption, and calling it one would
    /// send the user to fix a token that is not the problem.
    #[test]
    fn only_a_redirect_to_https_is_recognised() {
        assert!(redirects_to_https(&redirect_to("HTTPS://10.1.1.11:11987/")));

        assert!(redirects_to_https(&redirect_to("http://10.1.1.11:11987/")).not());
        assert!(redirects_to_https(&redirect_to("/relative/path")).not());
        assert!(redirects_to_https(&redirect_to("https:")).not());

        let mut not_a_redirect = redirect_to("https://10.1.1.11:11987/");
        *not_a_redirect.status_mut() = StatusCode::OK;
        assert!(redirects_to_https(&not_a_redirect).not());

        let mut no_location = Response::new(());
        *no_location.status_mut() = StatusCode::MOVED_PERMANENTLY;
        assert!(redirects_to_https(&no_location).not());
    }

    /// Goal: the wrapper gives a redirect its own explanation, not the credentials one.
    /// The rewritten redirect is an `Unauthenticated` status too, so running both would
    /// replace the right advice with advice about a token the link never carried.
    #[tokio::test]
    async fn the_wrapper_explains_a_redirect_as_unencrypted() {
        use tower::ServiceExt;

        let inner = tower::service_fn(|_: Request<()>| async {
            Ok::<_, std::convert::Infallible>(redirect_to("https://10.1.1.11:11987/"))
        });
        let service = ExplainRefusals::new(inner, "my_plugin".to_string());

        let response = service.oneshot(Request::new(())).await.unwrap();

        let status = Status::from_header_map(response.headers()).expect("a status tonic reads");
        let message = status.message();
        assert_eq!(status.code(), Code::Unauthenticated);
        assert!(message.contains("encrypted"), "{message}");
        assert!(
            message.contains("refused our credentials").not(),
            "{message}"
        );
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
