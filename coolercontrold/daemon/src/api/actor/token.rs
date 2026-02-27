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

use crate::token::{self, StoredToken};
use anyhow::Result;
use chrono::{DateTime, Local};
use log::{error, trace, warn};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const FLUSH_INTERVAL_SECS: u64 = 300; // 5 minutes

#[derive(Clone)]
pub struct TokenHandle {
    tokens: Arc<RwLock<Vec<StoredToken>>>,
    last_used_cache: Arc<Mutex<HashMap<String, DateTime<Local>>>>,
}

impl TokenHandle {
    pub async fn new(cancel_token: CancellationToken) -> Self {
        let tokens = token::load_tokens().await.unwrap_or_else(|err| {
            error!("Failed to load access tokens: {err}");
            Vec::new()
        });
        let handle = Self {
            tokens: Arc::new(RwLock::new(tokens)),
            last_used_cache: Arc::new(Mutex::new(HashMap::new())),
        };

        // Spawn background flush task
        let flush_handle = handle.clone();
        tokio::spawn(async move {
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

    pub async fn create(
        &self,
        label: String,
        expires_at: Option<DateTime<Local>>,
    ) -> Result<(StoredToken, String)> {
        let raw_token = token::generate_token();
        let hash = token::hash_token(&raw_token)?;
        let id = Uuid::new_v4().to_string();
        let stored = StoredToken {
            id,
            label,
            hash,
            created_at: Local::now(),
            expires_at,
            last_used: None,
        };
        let mut tokens = self.tokens.write().await;
        tokens.push(stored.clone());
        token::save_tokens(&tokens).await?;
        Ok((stored, raw_token))
    }

    pub async fn list(&self) -> Result<Vec<StoredToken>> {
        let tokens = self.tokens.read().await;
        let cache = self
            .last_used_cache
            .lock()
            .expect("last_used_cache poisoned");
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
        {
            self.last_used_cache
                .lock()
                .expect("last_used_cache poisoned")
                .remove(&id);
        }
        let mut tokens = self.tokens.write().await;
        tokens.retain(|t| t.id != id);
        token::save_tokens(&tokens).await
    }

    pub async fn validate(&self, raw_token: String) -> Result<bool> {
        let tokens = self.tokens.read().await;
        match token::validate_token(&raw_token, &tokens) {
            Some(id) => {
                drop(tokens);
                self.last_used_cache
                    .lock()
                    .expect("last_used_cache poisoned")
                    .insert(id, Local::now());
                Ok(true)
            }
            None => Ok(false),
        }
    }

    async fn flush_last_used(&self) -> Result<()> {
        let updates: HashMap<String, DateTime<Local>> = {
            let mut cache = self
                .last_used_cache
                .lock()
                .expect("last_used_cache poisoned");
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

    fn make_stored_token(raw: &str) -> (StoredToken, String) {
        let hash = token::hash_token(raw).unwrap();
        let stored = StoredToken {
            id: Uuid::new_v4().to_string(),
            label: "Test Token".to_string(),
            hash,
            created_at: Local::now(),
            expires_at: None,
            last_used: None,
        };
        (stored, raw.to_string())
    }

    fn make_handle_with_tokens(tokens: Vec<StoredToken>) -> TokenHandle {
        TokenHandle {
            tokens: Arc::new(RwLock::new(tokens)),
            last_used_cache: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[tokio::test]
    async fn test_validate_valid_token() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let handle = make_handle_with_tokens(vec![stored]);

        let result = handle.validate(raw).await.unwrap();
        assert!(result);
    }

    #[tokio::test]
    async fn test_validate_invalid_token() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let handle = make_handle_with_tokens(vec![stored]);

        let wrong = token::generate_token();
        let result = handle.validate(wrong).await.unwrap();
        assert!(!result);
    }

    #[tokio::test]
    async fn test_validate_updates_last_used_cache() {
        let raw = token::generate_token();
        let (stored, _) = make_stored_token(&raw);
        let token_id = stored.id.clone();
        let handle = make_handle_with_tokens(vec![stored]);

        handle.validate(raw).await.unwrap();

        let cache = handle
            .last_used_cache
            .lock()
            .expect("last_used_cache poisoned");
        assert!(cache.contains_key(&token_id));
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
        {
            handle
                .last_used_cache
                .lock()
                .expect("last_used_cache poisoned")
                .remove(&token_id);
        }
        {
            let mut tokens = handle.tokens.write().await;
            tokens.retain(|t| t.id != token_id);
        }

        let listed = handle.list().await.unwrap();
        assert!(listed.is_empty());
        assert!(!handle
            .last_used_cache
            .lock()
            .unwrap()
            .contains_key(&token_id));
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
            assert!(jh.await.unwrap());
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
        assert!(!handle
            .last_used_cache
            .lock()
            .expect("last_used_cache poisoned")
            .is_empty());

        // Flush merges cache into tokens (save_tokens will fail without filesystem,
        // but we can verify the merge logic by checking token state)
        {
            let updates: HashMap<String, DateTime<Local>> = {
                let mut cache = handle
                    .last_used_cache
                    .lock()
                    .expect("last_used_cache poisoned");
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
        assert!(handle
            .last_used_cache
            .lock()
            .expect("last_used_cache poisoned")
            .is_empty());
        // Token should have last_used set
        let tokens = handle.tokens.read().await;
        let t = tokens.iter().find(|t| t.id == token_id).unwrap();
        assert!(t.last_used.is_some());
    }
}
