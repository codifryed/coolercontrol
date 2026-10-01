// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Serializes password verification and storage.
//!
//! Guessing is throttled upstream, per peer and by the remote breaker in
//! `api::auth_throttle`. This actor used to keep its own lockout, but that one was global:
//! any single peer could trip it and lock every login out, the local admin's included.

use crate::admin;
use crate::api::actor::{run_api_actor, ApiActor};
use anyhow::Result;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct AuthActor {
    receiver: mpsc::Receiver<AuthMessage>,
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
        Self { receiver }
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
                let _ = respond_to.send(admin::save_passwd(&passwd).await);
            }
            AuthMessage::AdminMatchPasswd { passwd, respond_to } => {
                let _ = respond_to.send(Ok(admin::match_passwd(&passwd).await));
            }
        }
    }
}

#[derive(Clone)]
pub struct AuthHandle {
    sender: mpsc::Sender<AuthMessage>,
}

impl AuthHandle {
    /// Capacity 1: one argon2 job runs at a time (about 8 ms and 19 MiB each), and callers
    /// wait their turn on the channel. The password throttle bounds how many can be waiting.
    pub fn new(cancel_token: CancellationToken) -> Self {
        let (sender, receiver) = mpsc::channel(1);
        let actor = AuthActor::new(receiver);
        // The auth actor does password file IO via `sidecar_fs` (always Tokio), so it must run on
        // the sidecar Tokio runtime, not the main thread.
        crate::sidecar::handle().spawn(move || run_api_actor(actor, cancel_token));
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
