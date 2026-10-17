// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::path::Path;

use crate::cc_fs;
use crate::cc_fs::sidecar_fs;
use anyhow::{anyhow, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use log::{debug, error, info};
use sha2::{Digest, Sha512};
use std::fs::Permissions;
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use subtle::ConstantTimeEq;
use tower_sessions::cookie::Key;

use crate::paths;
pub const DEFAULT_PASS: &str = "coolAdmin";
const DEFAULT_PERMISSIONS: u32 = 0o600;

pub async fn match_passwd(passwd: &str) -> bool {
    let stored_hash = match load_passwd().await {
        Ok(Some(stored_hash)) => stored_hash,
        // Nothing stored, so only the default matches. It is public, so comparing against it
        // needs no argon2.
        Ok(None) => return passwd.as_bytes().ct_eq(DEFAULT_PASS.as_bytes()).into(),
        Err(err) => {
            error!("Error loading password: {err}");
            return false;
        }
    };
    let is_legacy = is_legacy_hash(&stored_hash);
    let is_match = verify_stored_hash(passwd, stored_hash).await;
    if is_legacy {
        if is_match {
            if let Err(err) = migrate_to_argon2(passwd).await {
                error!("Failed to migrate password hash to Argon2id: {err}");
            }
        }
    }
    is_match
}

/// Whether `passwd` matches `stored_hash`, a legacy SHA-512 hash or an argon2 one.
async fn verify_stored_hash(passwd: &str, stored_hash: String) -> bool {
    if is_legacy_hash(&stored_hash) {
        let legacy_hash = hash_passwd_sha512(passwd.as_bytes());
        let input_bytes = legacy_hash.as_bytes();
        let stored_bytes = stored_hash.as_bytes();
        return input_bytes.len() == stored_bytes.len() && input_bytes.ct_eq(stored_bytes).into();
    }
    // Argon2 verify is CPU-bound (~8 ms). Run it on the sidecar's blocking pool so it does not
    // stall the sidecar's single async reactor, and so a burst of login attempts cannot
    // monopolize it. All callers already run on the sidecar runtime.
    let passwd = passwd.to_owned();
    tokio::task::spawn_blocking(move || verify_password_argon2(&passwd, &stored_hash))
        .await
        .unwrap_or_else(|err| {
            error!("Argon2 verify task failed to join: {err}");
            false
        })
}

/// Answers whether the admin password is still the default, which `/verify-session` asks on
/// every UI load. Remembers the verdict for one stored hash, so argon2 runs once per hash, not
/// per call. Every save or reset writes a fresh salt, so a changed file, the CLI's
/// `--reset-password` included, is verified again.
#[derive(Default)]
pub struct DefaultPasswdCheck {
    verdict: Option<DefaultVerdict>,
}

struct DefaultVerdict {
    stored_hash: String,
    is_default: bool,
}

impl DefaultPasswdCheck {
    /// What `match_passwd(DEFAULT_PASS)` answers, without migrating a legacy hash: the next
    /// login with the default does that.
    pub async fn is_default(&mut self, passwd_path: &Path) -> bool {
        let stored_hash = match read_stored_hash(passwd_path).await {
            Ok(Some(stored_hash)) => stored_hash,
            // Nothing stored, so the default applies.
            Ok(None) => return true,
            Err(err) => {
                error!("Error loading password: {err}");
                return false;
            }
        };
        if let Some(verdict) = self
            .verdict
            .as_ref()
            .filter(|verdict| verdict.stored_hash == stored_hash)
        {
            return verdict.is_default;
        }
        let is_default = verify_stored_hash(DEFAULT_PASS, stored_hash.clone()).await;
        self.verdict = Some(DefaultVerdict {
            stored_hash,
            is_default,
        });
        is_default
    }
}

/// The stored hash, or `None` when there is none to read and the default password applies.
pub async fn load_passwd() -> Result<Option<String>> {
    read_stored_hash(paths::passwd_file()).await
}

async fn read_stored_hash(passwd_path: &Path) -> Result<Option<String>> {
    if passwd_path.exists() {
        if let Ok(contents) = sidecar_fs::read_txt(passwd_path).await {
            sidecar_fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS))
                .await?;
            return Ok(Some(contents.trim().to_owned()));
        }
    }
    Ok(None)
}

pub async fn save_passwd(password: &str) -> Result<()> {
    let passwd_path = paths::passwd_file();
    let passwd = hash_password(password).await?;
    let _ = sidecar_fs::remove_file(passwd_path).await;
    sidecar_fs::write_string(passwd_path, passwd).await?;
    sidecar_fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
    Ok(())
}

pub async fn reset_passwd() -> Result<()> {
    let passwd_path = paths::passwd_file();
    let passwd = hash_password(DEFAULT_PASS).await?;
    let _ = sidecar_fs::remove_file(passwd_path).await;
    sidecar_fs::write_string(passwd_path, passwd).await?;
    sidecar_fs::set_permissions(passwd_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
    clear_sessions().await;
    Ok(())
}

/// Clears all persisted session files so that old sessions authenticated with
/// the previous password are invalidated.
async fn clear_sessions() {
    clear_session_files(paths::sessions_dir()).await;
}

pub async fn clear_session_files(sessions_dir: &Path) {
    if let Ok(entries) = cc_fs::read_dir(sessions_dir) {
        for entry in entries.flatten() {
            let _ = sidecar_fs::remove_file(entry.path()).await;
        }
    }
    debug!("Cleared all persisted sessions");
}

/// Loads or generates a persistent session encryption key.
///
/// The key is stored at `/etc/coolercontrol/.session_key` as base64-encoded
/// 64 bytes. On first run, a new key is generated and saved. On subsequent
/// runs, the existing key is loaded. This ensures session cookies survive
/// daemon restarts.
pub async fn load_or_generate_session_key() -> Result<Key> {
    let key_path = paths::session_key_file();
    if key_path.exists() {
        let encoded = sidecar_fs::read_txt(key_path).await?;
        let bytes = BASE64.decode(encoded.trim())?;
        debug!("Session key loaded.");
        Ok(Key::from(&bytes))
    } else {
        let key = Key::generate();
        let encoded = BASE64.encode(key.master());
        sidecar_fs::write_string(key_path, encoded).await?;
        sidecar_fs::set_permissions(key_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
        debug!("Session key generated and saved.");
        Ok(key)
    }
}

/// Hashes `passwd` with Argon2id off the async reactor, on the sidecar's blocking pool. Argon2 is
/// intentionally CPU-bound (~8 ms) and the sidecar runs a single async thread, so hashing inline
/// would stall the REST API/SSE. Must be awaited on the sidecar Tokio runtime (every caller is).
async fn hash_password(passwd: &str) -> Result<String> {
    let passwd = passwd.to_owned();
    tokio::task::spawn_blocking(move || hash_password_argon2(&passwd))
        .await
        .map_err(|err| anyhow!("Argon2 hash task failed to join: {err}"))?
}

/// Sync Argon2id hash. CPU-bound; prefer `hash_password` in async contexts. Used directly by tests.
fn hash_password_argon2(passwd: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(passwd.as_bytes(), &salt)
        .map_err(|e| anyhow!("Failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

/// Sync Argon2id verify; a malformed stored hash is logged and treated as a non-match. CPU-bound,
/// so `match_passwd` runs it via `spawn_blocking`.
fn verify_password_argon2(passwd: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(passwd.as_bytes(), &parsed)
            .is_ok(),
        Err(err) => {
            error!("Failed to parse stored password hash: {err}");
            false
        }
    }
}

fn is_legacy_hash(stored: &str) -> bool {
    stored.starts_with("$argon2").not()
}

fn hash_passwd_sha512(passwd: &[u8]) -> String {
    let mut hasher = Sha512::new();
    hasher.update(passwd);
    crate::hashutil::to_lower_hex(&hasher.finalize())
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
    fn verify_password_argon2_matches_rejects_and_survives_garbage() {
        // Goal: the extracted sync verify accepts the right password, rejects a wrong one, and
        // treats a malformed stored hash as a non-match (not a panic), since `match_passwd` feeds it
        // whatever is on disk.
        let hash = hash_password_argon2("password").unwrap();
        assert!(verify_password_argon2("password", &hash));
        assert!(!verify_password_argon2("wrong", &hash));
        assert!(!verify_password_argon2(
            "password",
            "not-a-valid-argon2-hash"
        ));
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
    #[serial]
    fn default_check_without_a_file_is_default() {
        // Goal: with no password file the default applies, with nothing hashed or remembered.
        // Method: point the check at a path that does not exist.
        sidecar_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let mut check = DefaultPasswdCheck::default();
            assert!(check.is_default(&dir.path().join(".passwd")).await);
            assert!(check.verdict.is_none());
        });
    }

    #[test]
    #[serial]
    fn default_check_verifies_each_stored_hash_once() {
        // Goal: a remembered verdict answers for the same stored hash, and a changed file is
        // verified again. Method: plant a wrong verdict for the stored hash; getting it back
        // shows the call skipped argon2. Then rewrite the file as a save or reset would.
        sidecar_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let passwd_path = dir.path().join(".passwd");
            let custom_hash = hash_password_argon2("custom").unwrap();
            std::fs::write(&passwd_path, &custom_hash).unwrap();
            let mut check = DefaultPasswdCheck::default();
            assert!(check.is_default(&passwd_path).await.not());

            check.verdict = Some(DefaultVerdict {
                stored_hash: custom_hash,
                is_default: true,
            });
            assert!(check.is_default(&passwd_path).await);

            std::fs::write(&passwd_path, hash_password_argon2("custom").unwrap()).unwrap();
            assert!(check.is_default(&passwd_path).await.not());
            std::fs::write(&passwd_path, hash_password_argon2(DEFAULT_PASS).unwrap()).unwrap();
            assert!(check.is_default(&passwd_path).await);
        });
    }

    #[test]
    #[serial]
    fn default_check_matches_a_legacy_hash_without_migrating() {
        // Goal: a legacy SHA-512 hash of the default is recognized and left as stored, since
        // the next login with the default migrates it. Method: check it, then compare the file.
        sidecar_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let passwd_path = dir.path().join(".passwd");
            let legacy_hash = hash_passwd_sha512(DEFAULT_PASS.as_bytes());
            std::fs::write(&passwd_path, &legacy_hash).unwrap();
            let mut check = DefaultPasswdCheck::default();
            assert!(check.is_default(&passwd_path).await);
            assert_eq!(std::fs::read_to_string(&passwd_path).unwrap(), legacy_hash);
        });
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
    fn test_clear_session_files_removes_all() {
        sidecar_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let sessions_dir = dir.path().join("sessions");
            std::fs::create_dir_all(&sessions_dir).unwrap();

            // Create some fake session files
            std::fs::write(sessions_dir.join("session1"), "data1").unwrap();
            std::fs::write(sessions_dir.join("session2"), "data2").unwrap();
            assert_eq!(std::fs::read_dir(&sessions_dir).unwrap().count(), 2);

            clear_session_files(&sessions_dir).await;

            assert_eq!(std::fs::read_dir(&sessions_dir).unwrap().count(), 0);
        });
    }

    #[test]
    #[serial]
    fn test_clear_session_files_handles_missing_dir() {
        sidecar_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let sessions_dir = dir.path().join("nonexistent");
            // Should not panic when directory doesn't exist
            clear_session_files(&sessions_dir).await;
        });
    }

    #[test]
    #[serial]
    fn test_load_or_generate_session_key_creates_file() {
        sidecar_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let key_path = dir.path().join(".session_key");

            // Simulate by writing a key manually, then reading
            let key = Key::generate();
            let encoded = BASE64.encode(key.master());
            sidecar_fs::write_string(&key_path, encoded).await.unwrap();

            let loaded_encoded = sidecar_fs::read_txt(&key_path).await.unwrap();
            let bytes = BASE64.decode(loaded_encoded.trim()).unwrap();
            let loaded_key = Key::from(&bytes);
            assert_eq!(key.master(), loaded_key.master());
        });
    }
}
