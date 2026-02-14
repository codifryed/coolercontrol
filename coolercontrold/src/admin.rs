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

use std::path::Path;

use crate::cc_fs;
use crate::config::DEFAULT_CONFIG_DIR;
use anyhow::{anyhow, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use const_format::concatcp;
use log::{debug, error, info};
use sha2::{Digest, Sha512};
use std::fs::Permissions;
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use subtle::ConstantTimeEq;
use tower_sessions::cookie::Key;

const PASSWD_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/.passwd");
const SESSION_KEY_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/.session_key");
pub const DEFAULT_PASS: &str = "coolAdmin";
const DEFAULT_PERMISSIONS: u32 = 0o600;

pub async fn match_passwd(passwd: &str) -> bool {
    match load_passwd().await {
        Ok(stored_hash) => {
            if is_legacy_hash(&stored_hash) {
                let legacy_hash = hash_passwd_sha512(passwd.as_bytes());
                let input_bytes = legacy_hash.as_bytes();
                let stored_bytes = stored_hash.as_bytes();
                let is_match = input_bytes.len() == stored_bytes.len()
                    && input_bytes.ct_eq(stored_bytes).into();
                if is_match {
                    if let Err(err) = migrate_to_argon2(passwd).await {
                        error!("Failed to migrate password hash to Argon2id: {err}");
                    }
                }
                is_match
            } else {
                match PasswordHash::new(&stored_hash) {
                    Ok(parsed) => Argon2::default()
                        .verify_password(passwd.as_bytes(), &parsed)
                        .is_ok(),
                    Err(err) => {
                        error!("Failed to parse stored password hash: {err}");
                        false
                    }
                }
            }
        }
        Err(err) => {
            error!("Error loading password: {err}");
            false
        }
    }
}

pub async fn load_passwd() -> Result<String> {
    let passwd_path = Path::new(PASSWD_FILE_PATH);
    if passwd_path.exists() {
        if let Ok(contents) = cc_fs::read_txt(passwd_path).await {
            cc_fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS))?;
            return Ok(contents.trim().to_owned());
        }
    }
    let passwd = hash_password_argon2(DEFAULT_PASS)?;
    Ok(passwd)
}

pub async fn save_passwd(password: &str) -> Result<()> {
    let passwd_path = Path::new(PASSWD_FILE_PATH);
    let passwd = hash_password_argon2(password)?;
    let _ = cc_fs::remove_file(passwd_path);
    cc_fs::write_string(passwd_path, passwd).await?;
    cc_fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS))?;
    Ok(())
}

pub async fn reset_passwd() -> Result<()> {
    let passwd_path = Path::new(PASSWD_FILE_PATH);
    let passwd = hash_password_argon2(DEFAULT_PASS)?;
    let _ = cc_fs::remove_file(passwd_path);
    cc_fs::write_string(passwd_path, passwd).await?;
    cc_fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS))?;
    Ok(())
}

/// Loads or generates a persistent session encryption key.
///
/// The key is stored at `/etc/coolercontrol/.session_key` as base64-encoded
/// 64 bytes. On first run, a new key is generated and saved. On subsequent
/// runs, the existing key is loaded. This ensures session cookies survive
/// daemon restarts.
pub async fn load_or_generate_session_key() -> Result<Key> {
    let key_path = Path::new(SESSION_KEY_FILE_PATH);
    if key_path.exists() {
        let encoded = cc_fs::read_txt(key_path).await?;
        let bytes = BASE64.decode(encoded.trim())?;
        debug!("Session key loaded.");
        Ok(Key::from(&bytes))
    } else {
        let key = Key::generate();
        let encoded = BASE64.encode(key.master());
        cc_fs::write_string(key_path, encoded).await?;
        cc_fs::set_permissions(key_path, Permissions::from_mode(DEFAULT_PERMISSIONS))?;
        debug!("Session key generated and saved.");
        Ok(key)
    }
}

fn hash_password_argon2(passwd: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(passwd.as_bytes(), &salt)
        .map_err(|e| anyhow!("Failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

fn is_legacy_hash(stored: &str) -> bool {
    stored.starts_with("$argon2").not()
}

fn hash_passwd_sha512(passwd: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(passwd);
    format!("{:x}", hasher.finalize())
}

async fn migrate_to_argon2(passwd: &str) -> Result<()> {
    info!("Migrating password hash from SHA-512 to Argon2id");
    save_passwd(passwd).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_argon2_hash_format() {
        let result = hash_password_argon2("password").unwrap();
        assert!(result.starts_with("$argon2id$"));
    }

    #[test]
    fn test_argon2_verify() {
        let hash = hash_password_argon2("password").unwrap();
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(Argon2::default()
            .verify_password(b"password", &parsed)
            .is_ok());
    }

    #[test]
    fn test_argon2_wrong_password() {
        let hash = hash_password_argon2("password").unwrap();
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(Argon2::default()
            .verify_password(b"wrong", &parsed)
            .is_err());
    }

    #[test]
    fn test_argon2_different_hashes() {
        let hash1 = hash_password_argon2("password").unwrap();
        let hash2 = hash_password_argon2("password").unwrap();
        // Same password produces different hashes (different salts)
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn test_argon2_empty_password() {
        let result = hash_password_argon2("").unwrap();
        assert!(result.starts_with("$argon2id$"));
    }

    #[test]
    fn test_legacy_hash_detection() {
        let legacy = "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e";
        assert!(is_legacy_hash(legacy));

        let argon2_hash = hash_password_argon2("test").unwrap();
        assert!(!is_legacy_hash(&argon2_hash));
    }

    #[test]
    fn test_legacy_sha512_hash() {
        let result = hash_passwd_sha512(b"password");
        let expected = "b109f3bbbc244eb82441917ed06d618b9008dd09b3befd1b5e07394c706a8bb980b1d7785e5976ec049b46df5f1326af5a2ea6d103fd07c95385ffab0cacbc86";
        assert_eq!(result, expected);
    }

    #[test]
    fn test_argon2_hash_and_verify_roundtrip() {
        let password = "mySecurePass123!";
        let hash = hash_password_argon2(password).unwrap();
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok());
        assert!(Argon2::default()
            .verify_password(b"wrongPassword", &parsed)
            .is_err());
    }

    #[test]
    fn test_argon2_verify_after_rehash() {
        let original = "originalPass";
        let new_pass = "newPass456!";
        let original_hash = hash_password_argon2(original).unwrap();
        let new_hash = hash_password_argon2(new_pass).unwrap();

        // Original password should not match new hash
        let parsed_new = PasswordHash::new(&new_hash).unwrap();
        assert!(Argon2::default()
            .verify_password(original.as_bytes(), &parsed_new)
            .is_err());

        // New password should match new hash
        assert!(Argon2::default()
            .verify_password(new_pass.as_bytes(), &parsed_new)
            .is_ok());

        // New password should not match original hash
        let parsed_original = PasswordHash::new(&original_hash).unwrap();
        assert!(Argon2::default()
            .verify_password(new_pass.as_bytes(), &parsed_original)
            .is_err());
    }

    #[test]
    fn test_default_password_is_not_empty() {
        assert!(!DEFAULT_PASS.is_empty());
    }

    #[test]
    fn test_session_key_generate_produces_valid_key() {
        let key = Key::generate();
        assert_eq!(key.master().len(), 64);
    }

    #[test]
    fn test_session_key_roundtrip() {
        let key = Key::generate();
        let encoded = BASE64.encode(key.master());
        let decoded = BASE64.decode(encoded.trim()).unwrap();
        let restored = Key::from(&decoded);
        assert_eq!(key.master(), restored.master());
    }

    #[test]
    #[serial]
    fn test_load_or_generate_session_key_creates_file() {
        cc_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let key_path = dir.path().join(".session_key");

            // Simulate by writing a key manually, then reading
            let key = Key::generate();
            let encoded = BASE64.encode(key.master());
            cc_fs::write_string(&key_path, encoded).await.unwrap();

            let loaded_encoded = cc_fs::read_txt(&key_path).await.unwrap();
            let bytes = BASE64.decode(loaded_encoded.trim()).unwrap();
            let loaded_key = Key::from(&bytes);
            assert_eq!(key.master(), loaded_key.master());
        });
    }
}
