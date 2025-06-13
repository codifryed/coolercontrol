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
use anyhow::Result;
use moro_local::Scope;
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
                let response = admin::save_passwd(&passwd).await;
                let _ = respond_to.send(response);
            }
            AuthMessage::AdminMatchPasswd { passwd, respond_to } => {
                let response = admin::match_passwd(&passwd).await;
                let _ = respond_to.send(Ok(response));
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
        let (sender, receiver) = mpsc::channel(10);
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
