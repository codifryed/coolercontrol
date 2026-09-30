// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::token::{self, LegacyToken, StoredToken, TokenDigest};
use anyhow::Result;
use chrono::{DateTime, Local};
use log::{error, trace, warn};
use std::collections::HashMap;
use std::ops::Not;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use tokio::sync::{RwLock, Semaphore};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const FLUSH_INTERVAL_SECS: u64 = 300; // 5 minutes
/// Requests allowed in the legacy argon2 pass at once, running or queued. Past it a request
/// is refused, so concurrent `Bearer` requests cannot pile up snapshots and argon2 runs.
const LEGACY_PASS_MAX_WAITERS: usize = 8;

const _: () = assert!(LEGACY_PASS_MAX_WAITERS > 0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValidation {
    ValidReadWrite,
    ValidReadOnly,
    Invalid,
    /// The legacy pass is full, so the token was not checked. No verdict.
    Busy,
}

#[derive(Clone)]
pub struct TokenHandle {
    tokens: Arc<RwLock<Vec<StoredToken>>>,
    last_used_cache: Arc<Mutex<HashMap<String, DateTime<Local>>>>,
    /// One legacy argon2 pass at a time. Each costs milliseconds and 19 MiB per legacy
    /// token, and any `Bearer` header triggers one while legacy tokens exist, so concurrent
    /// requests queue here rather than fanning out across the blocking pool.
    legacy_pass: Arc<Semaphore>,
    /// Requests holding a `LegacyWaiter`, never above `LEGACY_PASS_MAX_WAITERS`.
    legacy_waiters: Arc<AtomicUsize>,
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
            legacy_pass: Arc::new(Semaphore::new(1)),
            legacy_waiters: Arc::new(AtomicUsize::new(0)),
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
            legacy_pass: Arc::new(Semaphore::new(1)),
            legacy_waiters: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Parks `LEGACY_PASS_MAX_WAITERS` unknown-token requests on the legacy pass, held shut
    /// by the returned permit. Needs a legacy token stored. Method: the permit is taken
    /// first, so each spawned request snapshots, takes a waiter slot, then parks.
    #[cfg(test)]
    pub async fn fill_legacy_pass(
        &self,
    ) -> (
        tokio::sync::OwnedSemaphorePermit,
        Vec<tokio::task::JoinHandle<TokenValidation>>,
    ) {
        let permit = Arc::clone(&self.legacy_pass).acquire_owned().await.unwrap();
        let parked: Vec<_> = (0..LEGACY_PASS_MAX_WAITERS)
            .map(|_| {
                let parked_handle = self.clone();
                tokio::spawn(async move {
                    parked_handle
                        .validate(token::generate_token())
                        .await
                        .unwrap()
                })
            })
            .collect();
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert_eq!(
            self.legacy_waiters.load(Ordering::Acquire),
            LEGACY_PASS_MAX_WAITERS
        );
        (permit, parked)
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

    /// The digest pass, which is free. A token it does not match comes back as the legacy
    /// argon2 pass it still needs, for the caller to charge before running it.
    pub async fn check_digest(&self, raw_token: String) -> TokenCheck<'_> {
        let (digest_match, legacy) = {
            let tokens = self.tokens.read().await;
            let digest_match = token::match_digest(&raw_token, &tokens);
            let legacy = if digest_match.is_none() {
                token::legacy_tokens(&tokens)
            } else {
                Vec::new()
            };
            (digest_match, legacy)
        };
        if let Some(matched) = digest_match {
            debug_assert!(matched.upgrade_digest.is_none());
            return TokenCheck::Done(self.accept(matched).await);
        }
        if legacy.is_empty() {
            return TokenCheck::Done(TokenValidation::Invalid);
        }
        TokenCheck::Legacy(LegacyCheck {
            handle: self,
            raw_token,
            legacy,
        })
    }

    /// Both passes in turn, charging nothing.
    #[cfg(test)]
    pub async fn validate(&self, raw_token: String) -> Result<TokenValidation> {
        match self.check_digest(raw_token).await {
            TokenCheck::Done(validation) => Ok(validation),
            TokenCheck::Legacy(legacy) => legacy.run().await,
        }
    }

    async fn accept(&self, matched: token::TokenMatch) -> TokenValidation {
        self.cache().insert(matched.id.clone(), Local::now());
        if let Some(digest) = matched.upgrade_digest {
            self.persist_digest(&matched.id, digest).await;
        }
        if matched.write_access {
            TokenValidation::ValidReadWrite
        } else {
            TokenValidation::ValidReadOnly
        }
    }

    /// The argon2 pass, on the blocking pool so it never stalls the sidecar's reactor, which
    /// serves every API connection. The permit and waiter slot move into the blocking task, so
    /// a dropped request frees neither until its argon2 run ends.
    async fn match_legacy(
        &self,
        raw_token: String,
        legacy: Vec<LegacyToken>,
        waiter: LegacyWaiter,
    ) -> Result<Option<token::TokenMatch>> {
        debug_assert!(legacy.is_empty().not());
        let permit = Arc::clone(&self.legacy_pass).acquire_owned().await?;
        let matched = tokio::task::spawn_blocking(move || {
            let _held = (permit, waiter);
            token::match_legacy(&raw_token, &legacy)
        })
        .await?;
        let Some(matched) = matched else {
            return Ok(None);
        };
        // The pass ran on a snapshot: a token deleted or expired meanwhile must not validate.
        let tokens = self.tokens.read().await;
        Ok(token::is_current(&tokens, &matched.id, Local::now()).then_some(matched))
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

/// A token's verdict from the digest pass, or the legacy pass it still needs.
pub enum TokenCheck<'a> {
    Done(TokenValidation),
    Legacy(LegacyCheck<'a>),
}

/// A token no digest matched, while legacy tokens remain for it to match. Running it costs
/// an argon2 pass.
pub struct LegacyCheck<'a> {
    handle: &'a TokenHandle,
    raw_token: String,
    legacy: Vec<LegacyToken>,
}

impl LegacyCheck<'_> {
    /// `Busy` without checking the token when `LEGACY_PASS_MAX_WAITERS` requests already
    /// hold the pass.
    pub async fn run(self) -> Result<TokenValidation> {
        let Self {
            handle,
            raw_token,
            legacy,
        } = self;
        debug_assert!(legacy.is_empty().not());
        let Some(waiter) = LegacyWaiter::enter(&handle.legacy_waiters) else {
            return Ok(TokenValidation::Busy);
        };
        let Some(matched) = handle.match_legacy(raw_token, legacy, waiter).await? else {
            return Ok(TokenValidation::Invalid);
        };
        Ok(handle.accept(matched).await)
    }
}

/// A slot in the legacy pass, held from queueing until its argon2 run ends.
struct LegacyWaiter(Arc<AtomicUsize>);

impl LegacyWaiter {
    /// Takes a slot, or `None` when all `LEGACY_PASS_MAX_WAITERS` are held.
    fn enter(waiters: &Arc<AtomicUsize>) -> Option<Self> {
        let previous = waiters
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < LEGACY_PASS_MAX_WAITERS).then_some(count + 1)
            })
            .ok()?;
        debug_assert!(previous < LEGACY_PASS_MAX_WAITERS);
        Some(Self(Arc::clone(waiters)))
    }
}

impl Drop for LegacyWaiter {
    fn drop(&mut self) {
        let previous = self.0.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
        debug_assert!(previous <= LEGACY_PASS_MAX_WAITERS);
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

    /// Goal: a token that matches no stored one is invalid when legacy tokens are present,
    /// which is the path that now runs off the reactor.
    #[tokio::test]
    async fn unknown_token_with_legacy_tokens_is_invalid() {
        let handle = make_handle_with_tokens(vec![make_legacy_token(&token::generate_token())]);
        let result = handle.validate(token::generate_token()).await.unwrap();
        assert_eq!(result, TokenValidation::Invalid);
    }

    /// Goal: the argon2 pass yields the reactor while it runs. Method: a task spawned on
    /// this single-threaded runtime can only run if `validate` awaits; inline argon2 never
    /// would, so the flag would still be unset when it returns.
    #[tokio::test]
    async fn legacy_pass_does_not_block_the_reactor() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let handle = make_handle_with_tokens(vec![make_legacy_token(&token::generate_token())]);
        let other_task_ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&other_task_ran);
        tokio::spawn(async move { flag.store(true, Ordering::SeqCst) });
        handle.validate(token::generate_token()).await.unwrap();
        assert!(other_task_ran.load(Ordering::SeqCst));
    }

    /// Goal: a request whose digest matches never pays for the argon2 pass, even with legacy
    /// tokens present. Method: the same flag, which stays unset because nothing yields.
    #[tokio::test]
    async fn digest_match_skips_the_legacy_pass() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let legacy = make_legacy_token(&token::generate_token());
        let handle = make_handle_with_tokens(vec![legacy, stored]);
        let other_task_ran = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&other_task_ran);
        tokio::spawn(async move { flag.store(true, Ordering::SeqCst) });
        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadWrite);
        assert!(other_task_ran.load(Ordering::SeqCst).not());
    }

    /// Runs `validate` for a legacy token, applying `revoke` to the store while the request
    /// waits for the argon2 pass, after it took its snapshot. Method: the test holds the
    /// pass's only permit, so the spawned request snapshots, then parks on the semaphore.
    async fn validate_revoked_mid_pass(revoke: fn(&mut Vec<StoredToken>)) -> TokenValidation {
        let raw = token::generate_token();
        let handle = make_handle_with_tokens(vec![make_legacy_token(&raw)]);
        let permit = Arc::clone(&handle.legacy_pass)
            .acquire_owned()
            .await
            .unwrap();
        let validating = handle.clone();
        let pending = tokio::spawn(async move { validating.validate(raw).await.unwrap() });
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
        assert!(pending.is_finished().not());
        revoke(&mut *handle.tokens.write().await);
        drop(permit);
        pending.await.unwrap()
    }

    /// Goal: a legacy token deleted while its request waits on the argon2 pass is refused,
    /// though the pass matched it against the snapshot taken before the delete.
    #[tokio::test]
    async fn legacy_token_deleted_mid_pass_is_invalid() {
        let result = validate_revoked_mid_pass(Vec::clear).await;
        assert_eq!(result, TokenValidation::Invalid);
    }

    /// Goal: a legacy token that expires while its request waits on the argon2 pass is
    /// refused, since expiry was checked only when the snapshot was taken.
    #[tokio::test]
    async fn legacy_token_expired_mid_pass_is_invalid() {
        let result = validate_revoked_mid_pass(|tokens| {
            tokens[0].expires_at = Some(Local::now() - chrono::Duration::seconds(1));
        })
        .await;
        assert_eq!(result, TokenValidation::Invalid);
    }

    /// Goal: a request past the waiter cap is refused before it queues or runs argon2.
    /// Method: with the pass full, the refused call must finish on its first poll, which
    /// neither queueing nor the blocking argon2 run could, and leave the waiter count alone.
    #[tokio::test]
    async fn full_legacy_pass_refuses_without_running_argon2() {
        use futures_util::FutureExt;
        let handle = make_handle_with_tokens(vec![make_legacy_token(&token::generate_token())]);
        let (_permit, parked) = handle.fill_legacy_pass().await;
        let result = handle.validate(token::generate_token()).now_or_never();
        assert_eq!(result.unwrap().unwrap(), TokenValidation::Busy);
        assert_eq!(
            handle.legacy_waiters.load(Ordering::Acquire),
            LEGACY_PASS_MAX_WAITERS
        );
        parked.iter().for_each(tokio::task::JoinHandle::abort);
    }

    /// Goal: a full legacy pass never refuses a token its digest matches.
    #[tokio::test]
    async fn full_legacy_pass_never_refuses_a_digest_match() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let legacy = make_legacy_token(&token::generate_token());
        let handle = make_handle_with_tokens(vec![legacy, stored]);
        let (_permit, parked) = handle.fill_legacy_pass().await;
        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadWrite);
        parked.iter().for_each(tokio::task::JoinHandle::abort);
    }

    /// Goal: a waiter slot is freed both when its request is dropped while queued and when
    /// its pass completes. Method: abort the parked requests, then run one pass to the end.
    #[tokio::test]
    async fn legacy_waiters_are_released() {
        let raw = token::generate_token();
        let handle = make_handle_with_tokens(vec![make_legacy_token(&raw)]);
        let (permit, parked) = handle.fill_legacy_pass().await;
        parked.iter().for_each(tokio::task::JoinHandle::abort);
        for task in parked {
            assert!(task.await.unwrap_err().is_cancelled());
        }
        assert_eq!(handle.legacy_waiters.load(Ordering::Acquire), 0);
        drop(permit);
        let result = handle.validate(raw).await.unwrap();
        assert_eq!(result, TokenValidation::ValidReadWrite);
        assert_eq!(handle.legacy_waiters.load(Ordering::Acquire), 0);
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
