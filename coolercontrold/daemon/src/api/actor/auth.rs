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

use crate::admin;
use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::CCError;
use crate::sidecar::SidecarHandle;
use anyhow::Result;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const MAX_FAILED_ATTEMPTS: u32 = 6;
const BASE_LOCKOUT_SECS: u64 = 60;
const MAX_LOCKOUT_SECS: u64 = 900; // 15 minutes
const ATTEMPT_DECAY_SECS: u64 = 900; // 15 minutes

struct AuthActor {
    receiver: mpsc::Receiver<AuthMessage>,
    failed_attempts: u32,
    lockout_count: u32,
    locked_until: Option<Instant>,
    last_attempt: Option<Instant>,
}

enum AuthMessage {
    AdminSavePasswd {
        passwd: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    AdminMatchPasswd {
        passwd: String,
        respond_to: oneshot::Sender<Result<bool>>,
    },
}

impl AuthActor {
    pub fn new(receiver: mpsc::Receiver<AuthMessage>) -> Self {
        Self {
            receiver,
            failed_attempts: 0,
            lockout_count: 0,
            locked_until: None,
            last_attempt: None,
        }
    }

    /// Resets failure counters if no consumed login attempt has occurred within the decay period.
    /// Locked-out requests are not "consumed" and therefore do not extend this window,
    /// so a client stuck retrying during a lockout will eventually see counters reset.
    fn decay_if_stale(&mut self) {
        if let Some(last) = self.last_attempt {
            if last.elapsed().as_secs() >= ATTEMPT_DECAY_SECS {
                self.failed_attempts = 0;
                self.lockout_count = 0;
                self.locked_until = None;
            }
        }
    }

    /// Returns the remaining lockout seconds if currently locked out.
    fn check_lockout(&self) -> Option<u64> {
        self.locked_until.and_then(|until| {
            let now = Instant::now();
            if now < until {
                Some((until - now).as_secs() + 1)
            } else {
                None
            }
        })
    }

    /// Gates a login attempt against the active lockout.
    /// Returns the remaining lockout seconds if the request must be rejected.
    /// Otherwise records this attempt's timestamp and returns `None`.
    ///
    /// Why the order matters: `last_attempt` is only updated for consumed
    /// (non-locked-out) attempts, so the decay window can fire even if a
    /// client retries continuously during a lockout. Updating it before the
    /// lockout check would let any persistent retry loop indefinitely
    /// extend `lockout_count` beyond recovery.
    fn try_consume_attempt(&mut self) -> Option<u64> {
        self.decay_if_stale();
        if let Some(remaining_secs) = self.check_lockout() {
            return Some(remaining_secs);
        }
        self.last_attempt = Some(Instant::now());
        None
    }

    fn record_failure(&mut self) {
        self.failed_attempts += 1;
        if self.failed_attempts >= MAX_FAILED_ATTEMPTS {
            let duration_secs = (BASE_LOCKOUT_SECS << self.lockout_count).min(MAX_LOCKOUT_SECS);
            self.locked_until =
                Some(Instant::now() + std::time::Duration::from_secs(duration_secs));
            self.lockout_count = self.lockout_count.saturating_add(1);
            self.failed_attempts = 0;
        }
    }

    fn record_success(&mut self) {
        self.failed_attempts = 0;
        self.lockout_count = 0;
        self.locked_until = None;
    }
}

impl ApiActor<AuthMessage> for AuthActor {
    fn name(&self) -> &'static str {
        "AuthActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<AuthMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: AuthMessage) {
        match msg {
            AuthMessage::AdminSavePasswd { passwd, respond_to } => {
                let response = admin::save_passwd(&passwd).await;
                if response.is_ok() {
                    self.record_success();
                }
                let _ = respond_to.send(response);
            }
            AuthMessage::AdminMatchPasswd { passwd, respond_to } => {
                if let Some(remaining_secs) = self.try_consume_attempt() {
                    let _ = respond_to.send(Err(CCError::TooManyAttempts {
                        msg: format!(
                            "Too many failed login attempts. Try again in {remaining_secs}s."
                        ),
                    }
                    .into()));
                    return;
                }
                let matched = admin::match_passwd(&passwd).await;
                if matched {
                    self.record_success();
                } else {
                    self.record_failure();
                }
                let _ = respond_to.send(Ok(matched));
            }
        }
    }
}

#[derive(Clone)]
pub struct AuthHandle {
    sender: mpsc::Sender<AuthMessage>,
}

impl AuthHandle {
    pub fn new(cancel_token: CancellationToken, sidecar: &SidecarHandle) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let actor = AuthActor::new(receiver);
        // The auth actor does password file IO via `sidecar_fs` (always Tokio), so it must run on
        // the sidecar Tokio runtime, not the main thread.
        sidecar.spawn(move || run_api_actor(actor, cancel_token));
        Self { sender }
    }
    pub async fn save_passwd(&self, passwd: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = AuthMessage::AdminSavePasswd {
            passwd,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn match_passwd(&self, passwd: String) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        let msg = AuthMessage::AdminMatchPasswd {
            passwd,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn new_test_actor() -> AuthActor {
        let (_tx, rx) = mpsc::channel(1);
        AuthActor::new(rx)
    }

    fn drive_failures(actor: &mut AuthActor, count: u32) {
        for _ in 0..count {
            assert!(
                actor.try_consume_attempt().is_none(),
                "consume must succeed when not locked out"
            );
            actor.record_failure();
        }
    }

    #[test]
    fn try_consume_attempt_records_timestamp_when_open() {
        // Goal: when not locked out, try_consume_attempt updates last_attempt
        // so the decay clock starts ticking from the most recent activity.
        let mut actor = new_test_actor();
        assert_eq!(actor.last_attempt, None);
        let result = actor.try_consume_attempt();
        assert!(result.is_none());
        assert!(actor.last_attempt.is_some());
    }

    #[test]
    fn try_consume_attempt_blocks_without_extending_decay_clock() {
        // Goal: regression for the lockout-cascade bug. While locked out, every
        // try_consume_attempt call must report remaining seconds and leave
        // last_attempt untouched, so a stuck retry loop cannot prevent the
        // 15-min decay window from firing.
        let mut actor = new_test_actor();
        drive_failures(&mut actor, MAX_FAILED_ATTEMPTS);
        assert!(
            actor.locked_until.is_some(),
            "lockout must engage after MAX_FAILED_ATTEMPTS"
        );
        let last_attempt_at_lockout = actor.last_attempt;
        assert!(last_attempt_at_lockout.is_some());

        for _ in 0..10 {
            std::thread::sleep(Duration::from_millis(1));
            let result = actor.try_consume_attempt();
            assert!(
                result.is_some(),
                "locked-out request must report remaining seconds"
            );
            assert_eq!(
                actor.last_attempt, last_attempt_at_lockout,
                "last_attempt must not be updated by a locked-out request"
            );
        }
    }

    #[test]
    fn record_failure_engages_lockout_at_threshold() {
        // Goal: confirm the lockout threshold and that the failure counter
        // resets to zero (and lockout_count increments) when the lockout fires.
        let mut actor = new_test_actor();
        for _ in 0..(MAX_FAILED_ATTEMPTS - 1) {
            actor.record_failure();
            assert!(actor.locked_until.is_none());
        }
        actor.record_failure();
        assert!(actor.locked_until.is_some());
        assert_eq!(actor.failed_attempts, 0);
        assert_eq!(actor.lockout_count, 1);
    }

    #[test]
    fn record_failure_doubles_lockout_duration_up_to_cap() {
        // Goal: confirm the exponential backoff: each lockout doubles the
        // base duration via the left-shift formula, capped at MAX_LOCKOUT_SECS.
        let mut actor = new_test_actor();

        for expected_count in 1..=4 {
            for _ in 0..MAX_FAILED_ATTEMPTS {
                actor.record_failure();
            }
            assert_eq!(actor.lockout_count, expected_count);
            let until = actor.locked_until.expect("lockout must be set");
            let remaining_secs = (until - Instant::now()).as_secs();
            let expected_duration =
                (BASE_LOCKOUT_SECS << (expected_count - 1)).min(MAX_LOCKOUT_SECS);
            assert!(
                remaining_secs <= expected_duration,
                "lockout duration must respect the doubling formula (got {remaining_secs}s, expected <= {expected_duration}s)"
            );
        }
    }

    #[test]
    fn record_success_clears_all_counters() {
        // Goal: a successful login (correct password) clears every counter,
        // including a pending lockout window and the lockout escalation count.
        let mut actor = new_test_actor();
        drive_failures(&mut actor, MAX_FAILED_ATTEMPTS);
        assert!(actor.locked_until.is_some());
        actor.record_success();
        assert_eq!(actor.failed_attempts, 0);
        assert_eq!(actor.lockout_count, 0);
        assert_eq!(actor.locked_until, None);
    }

    #[test]
    fn check_lockout_returns_none_after_expiry() {
        // Goal: once locked_until is in the past, check_lockout reports None
        // so the next attempt can pass through to password verification.
        let mut actor = new_test_actor();
        actor.locked_until = Instant::now().checked_sub(Duration::from_secs(1));
        assert!(
            actor.locked_until.is_some(),
            "test setup needs a monotonic clock that can step back"
        );
        assert_eq!(actor.check_lockout(), None);
    }

    #[test]
    fn decay_if_stale_resets_after_window() {
        // Goal: when last_attempt is older than ATTEMPT_DECAY_SECS, decay_if_stale
        // clears all counters. Use a manually-aged Instant to simulate elapsed time
        // without sleeping.
        let mut actor = new_test_actor();
        actor.failed_attempts = 3;
        actor.lockout_count = 2;
        actor.locked_until = Some(Instant::now() + Duration::from_secs(10));
        actor.last_attempt =
            Instant::now().checked_sub(Duration::from_secs(ATTEMPT_DECAY_SECS + 1));
        assert!(
            actor.last_attempt.is_some(),
            "test setup needs a monotonic clock that can step back"
        );
        actor.decay_if_stale();
        assert_eq!(actor.failed_attempts, 0);
        assert_eq!(actor.lockout_count, 0);
        assert_eq!(actor.locked_until, None);
    }
}
