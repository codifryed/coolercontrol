/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2026  Guy Boldon, Eren Simsek and contributors
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

use crate::cc_fs;
use crate::config::DEFAULT_CONFIG_DIR;
use anyhow::{anyhow, Context, Result};
use log::info;
use rcgen::{CertificateParams, CertifiedKey, DistinguishedName, DnType, KeyPair};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const DEFAULT_CERT_FILE: &str = "coolercontrol.crt";
const DEFAULT_KEY_FILE: &str = "coolercontrol.key";
const DEFAULT_PERMISSIONS: u32 = 0o600;

pub fn default_cert_path() -> String {
    format!("{DEFAULT_CONFIG_DIR}/{DEFAULT_CERT_FILE}")
}

pub fn default_key_path() -> String {
    format!("{DEFAULT_CONFIG_DIR}/{DEFAULT_KEY_FILE}")
}

pub async fn ensure_certificates(
    cert_path: Option<String>,
    key_path: Option<String>,
) -> Result<(PathBuf, PathBuf)> {
    let cert_path = PathBuf::from(cert_path.unwrap_or_else(default_cert_path));
    let key_path = PathBuf::from(key_path.unwrap_or_else(default_key_path));
    let cert_exists = cert_path.exists();
    let key_exists = key_path.exists();
    if cert_exists && key_exists {
        info!("Using existing TLS certificates");
        return Ok((cert_path, key_path));
    }
    if cert_exists != key_exists {
        return Err(anyhow!(
            "TLS configuration error: certificate and key files must both exist or both be absent. \
             cert exists: {cert_exists}, key exists: {key_exists}"
        ));
    }

    info!("Generating self-signed TLS certificate...");
    let CertifiedKey { cert, signing_key } = generate_self_signed_cert()?;

    cc_fs::write_string(&cert_path, cert.pem())
        .await
        .with_context(|| format!("Writing TLS certificate to {}", cert_path.display()))?;
    cc_fs::write_string(&key_path, signing_key.serialize_pem())
        .await
        .with_context(|| format!("Writing TLS private key to {}", key_path.display()))?;
    cc_fs::set_permissions(&key_path, Permissions::from_mode(DEFAULT_PERMISSIONS))
        .await
        .with_context(|| {
            format!(
                "Setting permissions on TLS private key {}",
                key_path.display()
            )
        })?;

    info!(
        "Generated self-signed TLS certificate: {}",
        cert_path.display()
    );
    info!("Generated TLS private key: {}", key_path.display());
    Ok((cert_path, key_path))
}

fn generate_self_signed_cert() -> Result<CertifiedKey<KeyPair>> {
    let mut distinguished_name = DistinguishedName::new();
    distinguished_name.push(DnType::CommonName, "CoolerControl self signed cert");
    distinguished_name.push(DnType::OrganizationName, "CoolerControl");
    let subject_alt_names = vec![
        // standard localhost DNS names
        "localhost".to_string(),
        "localhost4".to_string(),
        "localhost6".to_string(),
        "localhost.localdomain".to_string(),
        "localhost4.localdomain4".to_string(),
        "localhost6.localdomain6".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
        // catch-all IPs
        "0.0.0.0".to_string(),
        "::".to_string(),
    ];
    let mut params = CertificateParams::new(subject_alt_names)?;
    params.distinguished_name = distinguished_name;
    let signing_key = KeyPair::generate()?;
    let cert = params
        .self_signed(&signing_key)
        .context("Failed to generate self-signed certificate")?;
    Ok(CertifiedKey { cert, signing_key })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_generate_certificates() {
        let temp_dir = tempdir().unwrap();
        let cert_path = temp_dir.path().join("test.crt");
        let key_path = temp_dir.path().join("test.key");

        let (result_cert, result_key) = ensure_certificates(
            Some(cert_path.to_str().unwrap().to_string()),
            Some(key_path.to_str().unwrap().to_string()),
        )
        .await
        .unwrap();

        assert!(result_cert.exists());
        assert!(&result_key.exists());

        let cert_content = std::fs::read_to_string(&result_cert).unwrap();
        let key_content = std::fs::read_to_string(&result_key).unwrap();

        assert!(cert_content.contains("BEGIN CERTIFICATE"));
        assert!(key_content.contains("BEGIN PRIVATE KEY"));

        let key_permissions = std::fs::metadata(&result_key).unwrap().permissions();
        assert_eq!(key_permissions.mode() & 0o777, DEFAULT_PERMISSIONS);
    }
}
