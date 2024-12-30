/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use crate::alerts::{Alert, AlertController, AlertLog};
use crate::api::actor::{run_api_actor, ApiActor};
use crate::device::UID;
use anyhow::Result;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct AlertActor {
    receiver: mpsc::Receiver<AlertMessage>,
    alert_controller: Rc<AlertController>,
}

enum AlertMessage {
    GetAll {
        respond_to: oneshot::Sender<Result<(Vec<Alert>, Vec<AlertLog>)>>,
    },
    Create {
        alert: Alert,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Update {
        alert: Alert,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Delete {
        alert_uid: UID,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl AlertActor {
    pub fn new(
        receiver: mpsc::Receiver<AlertMessage>,
        alert_controller: Rc<AlertController>,
    ) -> Self {
        Self {
            receiver,
            alert_controller,
        }
    }
}

impl ApiActor<AlertMessage> for AlertActor {
    fn name(&self) -> &str {
        "AlertActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<AlertMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: AlertMessage) {
        match msg {
            AlertMessage::GetAll { respond_to } => {
                let result = self.alert_controller.get_all();
                respond_to.send(Ok(result)).unwrap();
            }
            AlertMessage::Create { alert, respond_to } => {
                let result = self.alert_controller.create(alert).await;
                respond_to.send(result).unwrap();
            }
            AlertMessage::Update { alert, respond_to } => {
                let result = self.alert_controller.update(alert).await;
                respond_to.send(result).unwrap();
            }
            AlertMessage::Delete {
                alert_uid,
                respond_to,
            } => {
                let result = self.alert_controller.delete(alert_uid).await;
                respond_to.send(result).unwrap();
            }
        }
    }
}

#[derive(Clone)]
pub struct AlertHandle {
    sender: mpsc::Sender<AlertMessage>,
    broadcaster: broadcast::Sender<AlertLog>,
    cancel_token: CancellationToken,
}

impl AlertHandle {
    pub fn new<'s>(
        alert_controller: Rc<AlertController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let (broadcaster, _) = broadcast::channel::<AlertLog>(2);
        let alert_handle = Self {
            sender: sender.clone(),
            broadcaster: broadcaster.clone(),
            cancel_token: cancel_token.clone(),
        };
        alert_controller.set_alert_handle(alert_handle.clone());
        let actor = AlertActor::new(receiver, alert_controller);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        alert_handle
    }

    pub async fn get_all(&self) -> Result<(Vec<Alert>, Vec<AlertLog>)> {
        let (tx, rx) = oneshot::channel();
        let msg = AlertMessage::GetAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn create(&self, alert: Alert) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = AlertMessage::Create {
            alert,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update(&self, alert: Alert) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = AlertMessage::Update {
            alert,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn delete(&self, alert_uid: UID) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = AlertMessage::Delete {
            alert_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub fn broadcaster(&self) -> &broadcast::Sender<AlertLog> {
        &self.broadcaster
    }

    pub fn broadcast_alert_state_change(&self, log: AlertLog) {
        // only create messages if we have listeners:
        if self.broadcaster.receiver_count() == 0 {
            return;
        }
        let _ = self.broadcaster.send(log);
    }

    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel_token.clone()
    }
}
