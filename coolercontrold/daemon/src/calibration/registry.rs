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

//! Per-channel `CancellationToken` registry. The REST cancel handler
//! looks up the token and triggers it; the diagnoser polls for it
//! between sweep steps. Parallel to `FanStateMap::under_diagnosis`
//! but exposes the handle so external callers can interrupt.

use super::ChannelKey;
use std::cell::RefCell;
use std::collections::HashMap;
use tokio_util::sync::CancellationToken;

pub struct DiagnosisRegistry {
    in_flight: RefCell<HashMap<ChannelKey, CancellationToken>>,
}

impl Default for DiagnosisRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl DiagnosisRegistry {
    pub fn new() -> Self {
        Self {
            in_flight: RefCell::new(HashMap::new()),
        }
    }

    /// Replaces any existing token for the key. Callers wanting to
    /// reject duplicates should check `is_in_flight` first.
    pub fn register(&self, key: ChannelKey) -> CancellationToken {
        let token = CancellationToken::new();
        self.in_flight.borrow_mut().insert(key, token.clone());
        token
    }

    /// Returns `true` if a diagnosis was found and cancelled.
    pub fn cancel(&self, key: &ChannelKey) -> bool {
        let token = self.in_flight.borrow().get(key).cloned();
        if let Some(token) = token {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Idempotent.
    pub fn clear(&self, key: &ChannelKey) {
        self.in_flight.borrow_mut().remove(key);
    }

    pub fn is_in_flight(&self, key: &ChannelKey) -> bool {
        self.in_flight.borrow().contains_key(key)
    }

    #[allow(dead_code)] // test-only; useful production API.
    pub fn len(&self) -> usize {
        self.in_flight.borrow().len()
    }

    #[allow(dead_code)] // test-only; useful production API.
    pub fn is_empty(&self) -> bool {
        self.in_flight.borrow().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(dev: &str, chan: &str) -> ChannelKey {
        (dev.to_string(), chan.to_string())
    }

    #[test]
    fn fresh_registry_is_empty() {
        // Goal: a brand new registry reports zero in-flight and not
        // in flight for any key.
        let registry = DiagnosisRegistry::new();
        assert!(registry.is_empty());
        assert_eq!(registry.len(), 0);
        assert!(!registry.is_in_flight(&key("dev-a", "fan1")));
    }

    #[test]
    fn register_inserts_and_marks_in_flight() {
        // Goal: registering a key returns a token, reports
        // is_in_flight = true, and len = 1.
        let registry = DiagnosisRegistry::new();
        let _token = registry.register(key("dev-a", "fan1"));
        assert!(registry.is_in_flight(&key("dev-a", "fan1")));
        assert_eq!(registry.len(), 1);
    }

    #[test]
    fn cancel_triggers_token_and_returns_true() {
        // Goal: cancel(key) on an in-flight diagnosis triggers the
        // returned token and returns true. The token signal is what
        // the diagnoser polls between sweep steps to bail.
        let registry = DiagnosisRegistry::new();
        let token = registry.register(key("dev-a", "fan1"));
        assert!(!token.is_cancelled());
        assert!(registry.cancel(&key("dev-a", "fan1")));
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancel_unknown_returns_false() {
        // Goal: cancelling a channel with no in-flight diagnosis
        // returns false so the REST handler can return a 404.
        let registry = DiagnosisRegistry::new();
        assert!(!registry.cancel(&key("dev-a", "fan1")));
    }

    #[test]
    fn clear_removes_registration() {
        // Goal: clear(key) removes the entry; subsequent is_in_flight
        // returns false. Called by the diagnoser cleanup after the
        // sweep terminates (success or failure).
        let registry = DiagnosisRegistry::new();
        let _token = registry.register(key("dev-a", "fan1"));
        registry.clear(&key("dev-a", "fan1"));
        assert!(!registry.is_in_flight(&key("dev-a", "fan1")));
        assert!(registry.is_empty());
    }

    #[test]
    fn clear_idempotent_on_unknown_key() {
        // Goal: clearing an unknown key is a no-op. Useful since the
        // diagnoser cleanup runs even on early-failure paths where
        // register() may not have been called.
        let registry = DiagnosisRegistry::new();
        registry.clear(&key("dev-a", "fan1"));
        assert!(registry.is_empty());
    }

    #[test]
    fn register_replaces_existing_token() {
        // Goal: registering twice with the same key replaces the
        // existing token. The old token reference held by the caller
        // is now orphaned (not in the map). This matches the engine
        // contract: only one in-flight diagnosis per channel.
        let registry = DiagnosisRegistry::new();
        let first = registry.register(key("dev-a", "fan1"));
        let second = registry.register(key("dev-a", "fan1"));
        // Cancelling via the registry hits `second`, not `first`.
        assert!(registry.cancel(&key("dev-a", "fan1")));
        assert!(second.is_cancelled());
        assert!(!first.is_cancelled());
    }

    #[test]
    fn multiple_channels_are_independent() {
        // Goal: the parallel-diagnosis use case. Different channels
        // get independent tokens; cancelling one does not affect the
        // other.
        let registry = DiagnosisRegistry::new();
        let token_a = registry.register(key("dev-a", "fan1"));
        let token_b = registry.register(key("dev-b", "fan1"));
        registry.cancel(&key("dev-a", "fan1"));
        assert!(token_a.is_cancelled());
        assert!(!token_b.is_cancelled());
        assert_eq!(registry.len(), 2);
    }
}
