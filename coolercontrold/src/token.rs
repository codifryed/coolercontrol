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

use crate::cc_fs;
use crate::config::DEFAULT_CONFIG_DIR;
use anyhow::{anyhow, Result};
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::SaltString;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use chrono::{DateTime, Local};
use const_format::concatcp;
use log::error;
use serde::{Deserialize, Serialize};
use std::fs::Permissions;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use uuid::Uuid;

const TOKENS_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/.tokens");
const DEFAULT_PERMISSIONS: u32 = 0o600;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct StoredToken {
    pub id: String,
    pub label: String,
    pub hash: String,
    pub created_at: DateTime<Local>,
    pub expires_at: Option<DateTime<Local>>,
    pub last_used: Option<DateTime<Local>>,
}

pub fn generate_token() -> String {
    format!("cc_{}", Uuid::new_v4().simple())
}

pub fn hash_token(token: &str) -> Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(token.as_bytes(), &salt)
        .map_err(|e| anyhow!("Failed to hash token: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_token(raw: &str, hash: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(raw.as_bytes(), &parsed)
            .is_ok(),
        Err(err) => {
            error!("Failed to parse stored token hash: {err}");
            false
        }
    }
}

pub async fn load_tokens() -> Result<Vec<StoredToken>> {
    let tokens_path = Path::new(TOKENS_FILE_PATH);
    if tokens_path.exists() {
        let contents = cc_fs::read_txt(tokens_path).await?;
        let trimmed = contents.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }
        let tokens: Vec<StoredToken> = serde_json::from_str(trimmed)?;
        cc_fs::set_permissions(tokens_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
        Ok(tokens)
    } else {
        Ok(Vec::new())
    }
}

pub async fn save_tokens(tokens: &[StoredToken]) -> Result<()> {
    let tokens_path = Path::new(TOKENS_FILE_PATH);
    let json = serde_json::to_string_pretty(tokens)?;
    let _ = cc_fs::remove_file(tokens_path).await;
    cc_fs::write_string(tokens_path, json).await?;
    cc_fs::set_permissions(tokens_path, Permissions::from_mode(DEFAULT_PERMISSIONS)).await?;
    Ok(())
}

pub fn validate_token(raw_token: &str, tokens: &[StoredToken]) -> Option<String> {
    let now = Local::now();
    for token in tokens {
        if let Some(expires_at) = token.expires_at {
            if now >= expires_at {
                continue;
            }
        }
        if verify_token(raw_token, &token.hash) {
            return Some(token.id.clone());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token_format() {
        let token = generate_token();
        assert!(token.starts_with("cc_"));
        assert_eq!(token.len(), 35); // "cc_" (3) + uuid simple (32) = 35
                                     // Verify the UUID part is valid hex
        let uuid_part = &token[3..];
        assert!(uuid_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_generate_token_uniqueness() {
        let token1 = generate_token();
        let token2 = generate_token();
        assert_ne!(token1, token2);
    }

    #[test]
    fn test_hash_verify_roundtrip() {
        let token = generate_token();
        let hash = hash_token(&token).unwrap();
        assert!(hash.starts_with("$argon2id$"));
        assert!(verify_token(&token, &hash));
    }

    #[test]
    fn test_wrong_token_fails_verification() {
        let token = generate_token();
        let hash = hash_token(&token).unwrap();
        let wrong_token = generate_token();
        assert!(!verify_token(&wrong_token, &hash));
    }

    #[test]
    fn test_validate_token_finds_match() {
        let raw = generate_token();
        let hash = hash_token(&raw).unwrap();
        let stored = StoredToken {
            id: "test-id".to_string(),
            label: "Test Token".to_string(),
            hash,
            created_at: Local::now(),
            expires_at: None,
            last_used: None,
        };
        let result = validate_token(&raw, &[stored]);
        assert_eq!(result, Some("test-id".to_string()));
    }

    #[test]
    fn test_validate_token_rejects_expired() {
        let raw = generate_token();
        let hash = hash_token(&raw).unwrap();
        let stored = StoredToken {
            id: "test-id".to_string(),
            label: "Expired Token".to_string(),
            hash,
            created_at: Local::now(),
            expires_at: Some(Local::now() - chrono::Duration::hours(1)),
            last_used: None,
        };
        let result = validate_token(&raw, &[stored]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_validate_token_accepts_non_expired() {
        let raw = generate_token();
        let hash = hash_token(&raw).unwrap();
        let stored = StoredToken {
            id: "test-id".to_string(),
            label: "Valid Token".to_string(),
            hash,
            created_at: Local::now(),
            expires_at: Some(Local::now() + chrono::Duration::hours(1)),
            last_used: None,
        };
        let result = validate_token(&raw, &[stored]);
        assert_eq!(result, Some("test-id".to_string()));
    }

    #[test]
    fn test_validate_token_no_match() {
        let raw = generate_token();
        let other_raw = generate_token();
        let hash = hash_token(&other_raw).unwrap();
        let stored = StoredToken {
            id: "test-id".to_string(),
            label: "Other Token".to_string(),
            hash,
            created_at: Local::now(),
            expires_at: None,
            last_used: None,
        };
        let result = validate_token(&raw, &[stored]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_load_save_roundtrip() {
        cc_fs::test_runtime(async {
            let dir = tempfile::tempdir().unwrap();
            let tokens_path = dir.path().join(".tokens");

            let raw = generate_token();
            let hash = hash_token(&raw).unwrap();
            let tokens = vec![StoredToken {
                id: "id1".to_string(),
                label: "Test".to_string(),
                hash: hash.clone(),
                created_at: Local::now(),
                expires_at: None,
                last_used: None,
            }];

            let json = serde_json::to_string_pretty(&tokens).unwrap();
            cc_fs::write_string(&tokens_path, json).await.unwrap();

            let contents = cc_fs::read_txt(&tokens_path).await.unwrap();
            let loaded: Vec<StoredToken> = serde_json::from_str(contents.trim()).unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].id, "id1");
            assert_eq!(loaded[0].label, "Test");
            assert!(verify_token(&raw, &loaded[0].hash));
        });
    }
}
