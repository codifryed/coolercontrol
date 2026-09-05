// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Connecting to a device-service plugin.
//!
//! Unix sockets connect as they always did. TCP now goes over TLS, because a remote
//! device service is reached across a network and the bearer token this daemon sends
//! must not travel in the clear.
//!
//! tonic is built here without its own TLS features, so the channel is built from a
//! connector over the `tokio-rustls` stack the daemon already carries for the server
//! side. That is also the only way to install [`trust::PinnedCertVerifier`], since no
//! public CA is involved and identity has to come from the certificate fingerprint.

use crate::repositories::service_plugin::service_manifest::{ConnectionType, ServiceManifest};
use crate::repositories::service_plugin::trust::{self, PinnedCertVerifier};
use anyhow::{anyhow, Context, Result};
use hyper_util::rt::TokioIo;
use log::{error, info, warn};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use std::sync::Arc;
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
    tls_strict: bool,
) -> Result<Channel> {
    match &manifest.address {
        ConnectionType::Uds(_) => Endpoint::try_from(address.to_string())?
            .connect()
            .await
            .with_context(|| format!("Connecting to device service at {address}")),
        ConnectionType::Tcp(tcp_address) => {
            connect_tls(manifest, address, tcp_address, tls_strict).await
        }
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
    let pinned = trust::read_pin(&manifest.path);
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
        if let Err(err) = trust::write_pin(&manifest.path, &pin) {
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
                let server_name = ServerName::try_from(host)
                    .unwrap_or_else(|_| ServerName::try_from("localhost").expect("static name"));
                let tls = TlsConnector::from(config)
                    .connect(server_name, stream)
                    .await?;
                Ok::<_, std::io::Error>(TokioIo::new(tls))
            }
        }))
        .await
        .with_context(|| format!("Connecting to device service at {address}"))
}

/// The host part of an `address:port`, tolerating bracketed IPv6 literals.
fn host_of(tcp_address: &str) -> String {
    let trimmed = tcp_address.trim();
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

    /// Goal: an address with no port, or a trailing colon that is not one, is not
    /// silently truncated into a different host.
    #[test]
    fn host_without_a_port_is_left_alone() {
        assert_eq!(host_of("example.com"), "example.com");
        assert_eq!(host_of("127.0.0.1"), "127.0.0.1");
        assert_eq!(host_of("example.com:notaport"), "example.com:notaport");
    }
}
