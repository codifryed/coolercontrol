// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Trust policy for outbound device-service TLS connections.
//!
//! A device-service plugin over TCP is usually another `CoolerControl` daemon, which serves
//! a certificate it generated itself (`api/tls.rs`). No public CA is involved and none
//! ever will be, so chain validation has nothing to validate against and the daemon
//! carries no root store. Identity therefore comes from the certificate fingerprint,
//! the same way `coolercontrol/tls_trust.cpp` does it for the desktop app.
//!
//! The desktop app can ask a human on first contact. The daemon connects at startup with
//! nobody watching, so first contact instead behaves like SSH's `accept-new`: trust it,
//! record the fingerprint, and refuse loudly if it ever changes.
//!
//! `tls_strict` means something narrower here than it does in the desktop app. There it
//! demands a CA-valid chain; here, with no roots to check against, it demands a pin that
//! the user placed deliberately, and refuses to trust anything on first contact.

use crate::hashutil;
use anyhow::{Context, Result};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::CryptoProvider;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{CertificateError, DigitallySignedStruct, Error as TlsError, SignatureScheme};
use std::net::IpAddr;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

/// Fixed filename holding the bearer token for a remote device service, beside the
/// manifest in the plugin's own directory.
///
/// Deliberately not a `manifest.toml` field: `plugin_controller` forces that file to
/// 0644 root-owned and re-applies it on every init, because a plugin-writable manifest
/// lets a plugin grant itself `privileged = true`. A world-readable manifest is the wrong
/// home for a credential, so the token lives in a sibling the existing
/// `secure_config_file` path can lock down to 0600.
pub const TOKEN_FILE_NAME: &str = "token";

/// Fixed filename holding the pinned certificate fingerprint. Kept per plugin so that
/// removing a plugin removes its pin, leaving nothing stale behind.
pub const PIN_FILE_NAME: &str = "tls_pin";

/// Bytes in a SHA-256 digest.
const FINGERPRINT_BYTES: usize = 32;

/// What to do with a certificate the peer presented.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustDecision {
    /// Trusted, with nothing new to record.
    Accept,
    /// First contact: trust it and persist this fingerprint as the pin.
    AcceptAndPin,
    Reject(RejectReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    /// The peer presented a different certificate than the one pinned. Either the remote
    /// regenerated its certificate or someone is in the middle; the user has to say which.
    PinMismatch { pinned: String, presented: String },
    /// Strict mode declines to trust anything it was not told about in advance.
    UnpinnedInStrictMode { presented: String },
}

impl RejectReason {
    pub fn describe(&self, service_id: &str, address: &str) -> String {
        match self {
            Self::PinMismatch { pinned, presented } => format!(
                "Device service '{service_id}' at {address} presented a different TLS certificate \
                 than the one pinned. Expected {pinned}, got {presented}. Either the remote \
                 regenerated its certificate, in which case delete the '{PIN_FILE_NAME}' file in \
                 that plugin's directory, or the connection is being intercepted."
            ),
            Self::UnpinnedInStrictMode { presented } => format!(
                "Device service '{service_id}' at {address} is not pinned and tls_strict is \
                 enabled, so it will not be trusted on first contact. Its certificate \
                 fingerprint is {presented}. Write it to the '{PIN_FILE_NAME}' file in that \
                 plugin's directory to allow the connection."
            ),
        }
    }
}

/// The trust decision, as a pure function of the inputs so it can be reasoned about and
/// tested without a TLS handshake.
///
/// Loopback is exempt, matching both `tls_trust.cpp` and the `DualProtocolAcceptor`'s
/// localhost exemption on the serving side. It is not pinned either: a local plugin that
/// regenerates its certificate would otherwise start failing for no security gain, since
/// an attacker who can bind loopback on this machine has already won.
pub fn decide(
    is_loopback: bool,
    strict: bool,
    pinned: Option<&str>,
    presented: &str,
) -> TrustDecision {
    if strict {
        return match pinned {
            Some(pin) if pin == presented => TrustDecision::Accept,
            Some(pin) => TrustDecision::Reject(RejectReason::PinMismatch {
                pinned: pin.to_string(),
                presented: presented.to_string(),
            }),
            None => TrustDecision::Reject(RejectReason::UnpinnedInStrictMode {
                presented: presented.to_string(),
            }),
        };
    }
    if is_loopback {
        return TrustDecision::Accept;
    }
    match pinned {
        Some(pin) if pin == presented => TrustDecision::Accept,
        Some(pin) => TrustDecision::Reject(RejectReason::PinMismatch {
            pinned: pin.to_string(),
            presented: presented.to_string(),
        }),
        None => TrustDecision::AcceptAndPin,
    }
}

/// SHA-256 of the certificate, as colon-separated byte pairs.
pub fn fingerprint(certificate: &[u8]) -> String {
    let printed = hashutil::to_fingerprint(certificate);
    debug_assert_eq!(printed.split(':').count(), FINGERPRINT_BYTES);
    printed
}

/// True when the host part of an address refers to this machine.
pub fn is_loopback_host(host: &str) -> bool {
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    host.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

pub fn read_pin(plugin_dir: &Path) -> Option<String> {
    let pin = std::fs::read_to_string(plugin_dir.join(PIN_FILE_NAME)).ok()?;
    let pin = pin.trim().to_string();
    if pin.is_empty() {
        return None;
    }
    Some(pin)
}

pub fn write_pin(plugin_dir: &Path, pin: &str) -> Result<()> {
    let path = plugin_dir.join(PIN_FILE_NAME);
    std::fs::write(&path, format!("{pin}\n"))
        .with_context(|| format!("Writing TLS pin to {}", path.display()))
}

/// The bearer token for a remote device service, if the user placed one.
///
/// Absent is not an error: a plugin on a Unix socket, or an older remote with no auth,
/// simply has no token to send.
pub fn read_token(plugin_dir: &Path) -> Option<String> {
    let token = std::fs::read_to_string(plugin_dir.join(TOKEN_FILE_NAME)).ok()?;
    let token = token.trim().to_string();
    if token.is_empty() {
        return None;
    }
    Some(token)
}

/// Applies [`decide`] during the TLS handshake.
///
/// Hostname and expiry are deliberately not checked. The pin covers the whole
/// certificate, so it *is* the peer's identity here; a self-signed certificate carries no
/// name a CA ever vouched for, and rejecting it for an expired validity window would
/// break a working link without making it safer.
#[derive(Debug)]
pub struct PinnedCertVerifier {
    provider: Arc<CryptoProvider>,
    is_loopback: bool,
    strict: bool,
    pinned: Option<String>,
    /// Fingerprint seen during the handshake, so the caller can persist a new pin without
    /// re-parsing the certificate.
    observed: Mutex<Option<String>>,
    /// Why the handshake was refused, so the caller can report which peer and what to do
    /// rather than surfacing an opaque TLS error.
    rejection: Mutex<Option<RejectReason>>,
}

impl PinnedCertVerifier {
    pub fn new(
        provider: Arc<CryptoProvider>,
        is_loopback: bool,
        strict: bool,
        pinned: Option<String>,
    ) -> Self {
        Self {
            provider,
            is_loopback,
            strict,
            pinned,
            observed: Mutex::new(None),
            rejection: Mutex::new(None),
        }
    }

    /// The fingerprint to persist, set only when this was first contact.
    pub fn pin_to_persist(&self) -> Option<String> {
        if self.pinned.is_some() {
            return None;
        }
        if self.is_loopback {
            return None;
        }
        lock(&self.observed).clone()
    }

    pub fn rejection(&self) -> Option<RejectReason> {
        lock(&self.rejection).clone()
    }
}

/// A poisoned lock here holds only diagnostic state, so recovering beats failing every
/// later connection.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl ServerCertVerifier for PinnedCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, TlsError> {
        let presented = fingerprint(end_entity.as_ref());
        *lock(&self.observed) = Some(presented.clone());
        match decide(
            self.is_loopback,
            self.strict,
            self.pinned.as_deref(),
            &presented,
        ) {
            TrustDecision::Accept | TrustDecision::AcceptAndPin => {
                Ok(ServerCertVerified::assertion())
            }
            TrustDecision::Reject(reason) => {
                *lock(&self.rejection) = Some(reason);
                Err(TlsError::InvalidCertificate(
                    CertificateError::ApplicationVerificationFailure,
                ))
            }
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, TlsError> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;
    use tempfile::tempdir;

    const PIN_A: &str = "aa:bb:cc";
    const PIN_B: &str = "dd:ee:ff";

    /// Goal: first contact with an unknown remote is trusted and recorded, which is what
    /// lets a headless daemon connect at all without a human to ask.
    #[test]
    fn unpinned_remote_is_trusted_on_first_contact() {
        assert_eq!(
            decide(false, false, None, PIN_A),
            TrustDecision::AcceptAndPin
        );
    }

    /// Goal: the pin is the peer's identity, so a matching one is accepted and a changed
    /// one is refused. This is the whole point of recording it.
    #[test]
    fn pinned_remote_must_present_the_pinned_certificate() {
        assert_eq!(
            decide(false, false, Some(PIN_A), PIN_A),
            TrustDecision::Accept
        );
        assert_eq!(
            decide(false, false, Some(PIN_A), PIN_B),
            TrustDecision::Reject(RejectReason::PinMismatch {
                pinned: PIN_A.to_string(),
                presented: PIN_B.to_string(),
            })
        );
    }

    /// Goal: loopback is exempt and is not pinned, matching `tls_trust.cpp` and the
    /// serving side's localhost exemption. A local plugin that regenerates its
    /// certificate must not start failing.
    #[test]
    fn loopback_is_trusted_without_pinning() {
        assert_eq!(decide(true, false, None, PIN_A), TrustDecision::Accept);
        assert_eq!(
            decide(true, false, Some(PIN_B), PIN_A),
            TrustDecision::Accept
        );
    }

    /// Goal: strict mode refuses to trust anything it was not told about first, loopback
    /// included. It is the opt-in for users who want no first-use trust at all.
    #[test]
    fn strict_mode_requires_a_pin_everywhere() {
        assert_eq!(
            decide(false, true, None, PIN_A),
            TrustDecision::Reject(RejectReason::UnpinnedInStrictMode {
                presented: PIN_A.to_string(),
            })
        );
        assert_eq!(
            decide(true, true, None, PIN_A),
            TrustDecision::Reject(RejectReason::UnpinnedInStrictMode {
                presented: PIN_A.to_string(),
            })
        );
        assert_eq!(
            decide(true, true, Some(PIN_A), PIN_A),
            TrustDecision::Accept
        );
        assert!(matches!(
            decide(false, true, Some(PIN_A), PIN_B),
            TrustDecision::Reject(RejectReason::PinMismatch { .. })
        ));
    }

    /// Goal: the fingerprint format matches `tls_trust.cpp::fingerprint` so a user can
    /// compare the daemon's output against the desktop app's without transcribing.
    #[test]
    fn fingerprint_is_colon_separated_lowercase_hex() {
        let printed = fingerprint(b"");
        // SHA-256 of the empty input, the standard vector.
        assert!(printed.starts_with("e3:b0:c4:42:98:fc:1c:14"));
        assert_eq!(printed.split(':').count(), FINGERPRINT_BYTES);
        for group in printed.split(':') {
            assert_eq!(group.len(), 2);
            assert!(group.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(group.chars().any(char::is_uppercase).not());
        }
        assert_ne!(fingerprint(b"a"), fingerprint(b"b"));
    }

    /// Goal: the loopback exemption must not be widened by a lookalike address. A remote
    /// host wrongly read as loopback would skip pinning entirely.
    #[test]
    fn loopback_detection_is_exact() {
        assert!(is_loopback_host("127.0.0.1"));
        assert!(is_loopback_host("127.4.5.6"));
        assert!(is_loopback_host("localhost"));
        assert!(is_loopback_host("LocalHost"));
        assert!(is_loopback_host("::1"));
        assert!(is_loopback_host("[::1]"));

        assert!(is_loopback_host("192.168.1.100").not());
        assert!(is_loopback_host("localhost.evil.com").not());
        assert!(is_loopback_host("127.0.0.1.evil.com").not());
        assert!(is_loopback_host("example.com").not());
        assert!(is_loopback_host("").not());
    }

    /// Goal: a pin round-trips through the file the user is told to edit, tolerating the
    /// trailing newline any editor adds.
    #[test]
    fn pin_file_round_trips() {
        let dir = tempdir().unwrap();
        assert_eq!(read_pin(dir.path()), None);

        write_pin(dir.path(), PIN_A).unwrap();
        assert_eq!(read_pin(dir.path()).as_deref(), Some(PIN_A));

        std::fs::write(dir.path().join(PIN_FILE_NAME), format!("  {PIN_B}  \n\n")).unwrap();
        assert_eq!(read_pin(dir.path()).as_deref(), Some(PIN_B));
    }

    /// Goal: an empty pin file reads as no pin rather than as a pin that can never match,
    /// which would wedge the connection with a confusing mismatch every time.
    #[test]
    fn empty_pin_file_reads_as_absent() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(PIN_FILE_NAME), "   \n").unwrap();
        assert_eq!(read_pin(dir.path()), None);
    }

    /// Goal: a token is optional and whitespace-tolerant. A missing file is the normal
    /// case for a Unix-socket plugin, not an error.
    #[test]
    fn token_file_is_optional_and_trimmed() {
        let dir = tempdir().unwrap();
        assert_eq!(read_token(dir.path()), None);

        std::fs::write(dir.path().join(TOKEN_FILE_NAME), "  cc_abc123  \n").unwrap();
        assert_eq!(read_token(dir.path()).as_deref(), Some("cc_abc123"));

        std::fs::write(dir.path().join(TOKEN_FILE_NAME), "\n \n").unwrap();
        assert_eq!(read_token(dir.path()), None);
    }

    /// Goal: the rejection messages name the plugin and say what to do about it. These
    /// are what a user sees when a remote's devices vanish, so they must be actionable.
    #[test]
    fn rejection_messages_are_actionable() {
        let mismatch = RejectReason::PinMismatch {
            pinned: PIN_A.to_string(),
            presented: PIN_B.to_string(),
        }
        .describe("my_server", "192.168.1.100:11987");
        assert!(mismatch.contains("my_server"));
        assert!(mismatch.contains("192.168.1.100:11987"));
        assert!(mismatch.contains(PIN_A));
        assert!(mismatch.contains(PIN_B));
        assert!(mismatch.contains(PIN_FILE_NAME));

        let unpinned = RejectReason::UnpinnedInStrictMode {
            presented: PIN_B.to_string(),
        }
        .describe("my_server", "192.168.1.100:11987");
        assert!(unpinned.contains("tls_strict"));
        assert!(unpinned.contains(PIN_B));
        assert!(unpinned.contains(PIN_FILE_NAME));
    }
}
