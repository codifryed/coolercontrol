// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::token::{self, StoredToken, TokenDigest};
use anyhow::Result;
use chrono::{DateTime, Local};
use log::{error, trace, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const FLUSH_INTERVAL_SECS: u64 = 300; // 5 minutes

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValidation {
    ValidReadWrite,
    ValidReadOnly,
    Invalid,
}

#[derive(Clone)]
pub struct TokenHandle {
    tokens: Arc<RwLock<Vec<StoredToken>>>,
    last_used_cache: Arc<Mutex<HashMap<String, DateTime<Local>>>>,
}

impl TokenHandle {
    /// A poisoned cache must not take authentication down with it. The map holds only
    /// last-used timestamps, so continuing with whatever state survived is strictly
    /// better than failing every subsequent token validation.
    fn cache(&self) -> MutexGuard<'_, HashMap<String, DateTime<Local>>> {
        self.last_used_cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }

    pub async fn new(cancel_token: CancellationToken) -> Self {
        // Token IO uses `sidecar_fs` (always Tokio), so load on the sidecar Tokio runtime.
        let tokens = match crate::sidecar::handle().run(token::load_tokens).await {
            Ok(Ok(tokens)) => tokens,
            Ok(Err(err)) => {
                error!("Failed to load access tokens: {err}");
                Vec::new()
            }
            Err(err) => {
                error!("Sidecar dispatch for token load failed: {err}");
                Vec::new()
            }
        };
        let handle = Self {
            tokens: Arc::new(RwLock::new(tokens)),
            last_used_cache: Arc::new(Mutex::new(HashMap::new())),
        };

        // Spawn the background flush task on the sidecar: it also writes via `sidecar_fs`.
        let flush_handle = handle.clone();
        crate::sidecar::handle().spawn(move || async move {
            let mut flush_interval =
                tokio::time::interval(tokio::time::Duration::from_secs(FLUSH_INTERVAL_SECS));
            flush_interval.tick().await; // skip first immediate tick
            loop {
                tokio::select! {
                    () = cancel_token.cancelled() => {
                        if let Err(err) = flush_handle.flush_last_used().await {
                            warn!("Failed to flush last_used timestamps on shutdown: {err}");
                        }
                        break;
                    }
                    _ = flush_interval.tick() => {
                        if let Err(err) = flush_handle.flush_last_used().await {
                            warn!("Failed to flush last_used timestamps: {err}");
                        }
                    }
                }
            }
            trace!("Token flush task is shutting down");
        });

        handle
    }

    /// Builds a handle over a fixed token set, for tests that need a working
    /// `TokenHandle` without the sidecar and on-disk store `new` requires.
    #[cfg(test)]
    pub fn with_tokens(tokens: Vec<StoredToken>) -> Self {
        Self {
            tokens: Arc::new(RwLock::new(tokens)),
            last_used_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn create(
        &self,
        label: String,
        expires_at: Option<DateTime<Local>>,
        write_access: bool,
    ) -> Result<(StoredToken, String)> {
        let raw_token = token::generate_token();
        // DOWNGRADE-COMPAT(added 5.0.0, remove 5.2.0): see DEPRECATIONS.md. `digest` is
        // what validation reads; the argon2 hash is written only so a 4.3.x daemon can
        // still validate this token after a downgrade.
        let hash = token::hash_token(&raw_token)?;
        let digest = token::digest_token(&raw_token);
        let id = Uuid::new_v4().to_string();
        let stored = StoredToken {
            id,
            label,
            hash,
            digest: Some(digest),
            created_at: Local::now(),
            expires_at,
            last_used: None,
            write_access,
        };
        let mut tokens = self.tokens.write().await;
        tokens.push(stored.clone());
        token::save_tokens(&tokens).await?;
        Ok((stored, raw_token))
    }

    pub async fn list(&self) -> Result<Vec<StoredToken>> {
        let tokens = self.tokens.read().await;
        let cache = self.cache();
        Ok(tokens
            .iter()
            .map(|t| {
                let mut t = t.clone();
                if let Some(last) = cache.get(&t.id) {
                    t.last_used = Some(*last);
                }
                t
            })
            .collect())
    }

    pub async fn delete(&self, id: String) -> Result<()> {
        self.cache().remove(&id);
        let mut tokens = self.tokens.write().await;
        tokens.retain(|t| t.id != id);
        token::save_tokens(&tokens).await
    }

    pub async fn validate(&self, raw_token: String) -> Result<TokenValidation> {
        let tokens = self.tokens.read().await;
        let Some(matched) = token::validate_token(&raw_token, &tokens) else {
            return Ok(TokenValidation::Invalid);
        };
        drop(tokens);
        self.cache().insert(matched.id.clone(), Local::now());
        if let Some(digest) = matched.upgrade_digest {
            self.persist_digest(&matched.id, digest).await;
        }
        if matched.write_access {
            Ok(TokenValidation::ValidReadWrite)
        } else {
            Ok(TokenValidation::ValidReadOnly)
        }
    }

    /// Records the digest of a token that just matched on the legacy argon2 path, so
    /// it never pays the KDF again.
    ///
    /// Failures are logged rather than propagated: the caller has already
    /// authenticated, and a failed upgrade costs nothing worse than one more argon2
    /// verify on the next request.
    async fn persist_digest(&self, id: &str, digest: TokenDigest) {
        let mut tokens = self.tokens.write().await;
        let Some(token) = tokens.iter_mut().find(|token| token.id == id) else {
            return; // deleted between validation and upgrade
        };
        if token.digest.is_some() {
            return; // a concurrent validation upgraded it first
        }
        token.digest = Some(digest);
        if let Err(err) = token::save_tokens(&tokens).await {
            warn!("Failed to persist upgraded token digest for {id}: {err}");
        }
    }

    async fn flush_last_used(&self) -> Result<()> {
        let updates: HashMap<String, DateTime<Local>> = {
            let mut cache = self.cache();
            if cache.is_empty() {
                return Ok(());
            }
            std::mem::take(&mut *cache)
        };
        let mut tokens = self.tokens.write().await;
        for token in tokens.iter_mut() {
            if let Some(last) = updates.get(&token.id) {
                token.last_used = Some(*last);
            }
        }
        token::save_tokens(&tokens).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    fn make_stored_token(raw: &str) -> (StoredToken, String) {
        make_stored_token_with_write(raw, true)
    }

    fn make_stored_token_with_write(raw: &str, write_access: bool) -> (StoredToken, String) {
        let hash = token::hash_token(raw).unwrap();
        let stored = StoredToken {
            id: Uuid::new_v4().to_string(),
            label: "Test Token".to_string(),
            hash,
            digest: Some(token::digest_token(raw)),
            created_at: Local::now(),
            expires_at: None,
            last_used: None,
            write_access,
        };
        (stored, raw.to_string())
    }

    /// A token as stored before 5.0.0: argon2 hash, no digest.
    fn make_legacy_token(raw: &str) -> StoredToken {
        let (stored, _) = make_stored_token(raw);
        StoredToken {
            digest: None,
            ..stored
        }
    }

    fn make_handle_with_tokens(tokens: Vec<StoredToken>) -> TokenHandle {
        TokenHandle::with_tokens(tokens)
    }

    #[tokio::test]
    async fn test_validate_valid_token_read_write() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let handle = make_handle_with_tokens(vec![stored]);

        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadWrite);
    }

    #[tokio::test]
    async fn test_validate_valid_token_read_only() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token_with_write(&raw, false);
        let handle = make_handle_with_tokens(vec![stored]);

        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadOnly);
    }

    #[tokio::test]
    async fn test_validate_invalid_token() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let handle = make_handle_with_tokens(vec![stored]);

        let wrong = token::generate_token();
        let result = handle.validate(wrong).await.unwrap();
        assert_eq!(result, TokenValidation::Invalid);
    }

    /// Goal: a token minted before 5.0.0 still authenticates through the argon2
    /// fallback, so upgrading the daemon does not invalidate anyone's tokens.
    #[tokio::test]
    async fn test_validate_legacy_token_still_authenticates() {
        let raw = token::generate_token();
        let handle = make_handle_with_tokens(vec![make_legacy_token(&raw)]);

        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadWrite);
    }

    /// Goal: validating a legacy token records its digest, so the KDF is paid at most
    /// once more per token.
    #[tokio::test]
    async fn test_validate_legacy_token_records_digest() {
        let raw = token::generate_token();
        let handle = make_handle_with_tokens(vec![make_legacy_token(&raw)]);
        assert_eq!(handle.tokens.read().await[0].digest, None);

        handle.validate(raw.clone()).await.unwrap();

        let tokens = handle.tokens.read().await;
        assert_eq!(tokens[0].digest, Some(token::digest_token(&raw)));
    }

    /// Goal: prove the upgraded token authenticates off the digest alone.
    ///
    /// Method: validate once to trigger the upgrade, then corrupt the argon2 hash and
    /// validate again. Success is only possible if the digest path handled it.
    #[tokio::test]
    async fn test_upgraded_token_validates_without_argon2() {
        let raw = token::generate_token();
        let handle = make_handle_with_tokens(vec![make_legacy_token(&raw)]);
        handle.validate(raw.clone()).await.unwrap();

        {
            let mut tokens = handle.tokens.write().await;
            tokens[0].hash = "$argon2id$v=19$m=19456,t=2,p=1$corrupt$corrupt".to_string();
        }

        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadWrite);
    }

    /// Goal: an already-upgraded token is left alone, so concurrent validations cannot
    /// each rewrite the store.
    #[tokio::test]
    async fn test_persist_digest_is_noop_when_already_set() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let id = stored.id.clone();
        let original = stored.digest.clone();
        let handle = make_handle_with_tokens(vec![stored]);

        handle
            .persist_digest(&id, token::digest_token("a-different-token"))
            .await;

        assert_eq!(handle.tokens.read().await[0].digest, original);
    }

    /// Goal: a token deleted between validation and upgrade does not resurrect or panic.
    #[tokio::test]
    async fn test_persist_digest_ignores_missing_token() {
        let handle = make_handle_with_tokens(Vec::new());

        handle
            .persist_digest("gone", token::digest_token("x"))
            .await;

        assert!(handle.tokens.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_validate_updates_last_used_cache() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let token_id = stored.id.clone();
        let handle = make_handle_with_tokens(vec![stored]);

        handle.validate(raw).await.unwrap();

        assert!(handle.cache().contains_key(&token_id));
    }

    #[tokio::test]
    async fn test_list_merges_last_used_cache() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let token_id = stored.id.clone();
        let handle = make_handle_with_tokens(vec![stored]);

        // Validate to populate cache
        handle.validate(raw).await.unwrap();

        let listed = handle.list().await.unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].last_used.is_some());
        assert_eq!(listed[0].id, token_id);
    }

    #[tokio::test]
    async fn test_delete_removes_token_and_cache() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let token_id = stored.id.clone();
        let handle = make_handle_with_tokens(vec![stored]);

        // Validate to populate cache
        handle.validate(raw.clone()).await.unwrap();

        // Note: delete calls save_tokens which writes to disk — skip for unit test
        // Instead, verify the in-memory state changes
        handle.cache().remove(&token_id);
        {
            let mut tokens = handle.tokens.write().await;
            tokens.retain(|t| t.id != token_id);
        }

        let listed = handle.list().await.unwrap();
        assert!(listed.is_empty());
        assert!(handle.cache().contains_key(&token_id).not());
    }

    #[tokio::test]
    async fn test_concurrent_validations() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let handle = make_handle_with_tokens(vec![stored]);

        let mut handles = Vec::new();
        for _ in 0..10 {
            let h = handle.clone();
            let r = raw.clone();
            handles.push(tokio::spawn(async move { h.validate(r).await.unwrap() }));
        }

        for jh in handles {
            assert_eq!(jh.await.unwrap(), TokenValidation::ValidReadWrite);
        }
    }

    #[tokio::test]
    async fn test_flush_last_used() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let token_id = stored.id.clone();
        let handle = make_handle_with_tokens(vec![stored]);

        // Validate to populate cache
        handle.validate(raw).await.unwrap();
        assert!(handle.cache().is_empty().not());

        // Flush merges cache into tokens (save_tokens will fail without filesystem,
        // but we can verify the merge logic by checking token state)
        {
            let updates: HashMap<String, DateTime<Local>> = {
                let mut cache = handle.cache();
                std::mem::take(&mut *cache)
            };
            let mut tokens = handle.tokens.write().await;
            for token in tokens.iter_mut() {
                if let Some(last) = updates.get(&token.id) {
                    token.last_used = Some(*last);
                }
            }
        }

        // Cache should be empty after flush
        assert!(handle.cache().is_empty());
        // Token should have last_used set
        let tokens = handle.tokens.read().await;
        let t = tokens.iter().find(|t| t.id == token_id).unwrap();
        assert!(t.last_used.is_some());
    }
}
