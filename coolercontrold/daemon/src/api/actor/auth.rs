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
use anyhow::Result;
use moro_local::Scope;
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

const MAX_FAILED_ATTEMPTS: u32 = 5;
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

    /// Resets failure counters if no login attempt has occurred within the decay period.
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
                self.decay_if_stale();
                self.last_attempt = Some(Instant::now());
                if let Some(remaining_secs) = self.check_lockout() {
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
    pub fn new<'s>(
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let actor = AuthActor::new(receiver);
        main_scope.spawn(run_api_actor(actor, cancel_token));
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
