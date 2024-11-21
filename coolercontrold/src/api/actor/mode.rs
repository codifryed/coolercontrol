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

use crate::api::actor::{run_api_actor, ApiActor};
use crate::device::UID;
use crate::modes::{Mode, ModeController};
use anyhow::Result;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct ModeActor {
    receiver: mpsc::Receiver<ModeMessage>,
    modes_controller: Rc<ModeController>,
}

enum ModeMessage {
    Get {
        mode_uid: UID,
        respond_to: oneshot::Sender<Result<Option<Mode>>>,
    },
    GetAll {
        respond_to: oneshot::Sender<Result<Vec<Mode>>>,
    },
    SaveOrder {
        order: Vec<UID>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Create {
        mode_name: String,
        respond_to: oneshot::Sender<Result<Mode>>,
    },
    Update {
        mode_uid: UID,
        mode_name: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Delete {
        mode_uid: UID,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Duplicate {
        mode_uid: UID,
        respond_to: oneshot::Sender<Result<Mode>>,
    },
    UpdateSettings {
        mode_uid: UID,
        respond_to: oneshot::Sender<Result<Mode>>,
    },
    GetAllActive {
        respond_to: oneshot::Sender<Result<Vec<UID>>>,
    },
    Activate {
        mode_uid: UID,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl ModeActor {
    pub fn new(
        receiver: mpsc::Receiver<ModeMessage>,
        modes_controller: Rc<ModeController>,
    ) -> Self {
        Self {
            receiver,
            modes_controller,
        }
    }
}

impl ApiActor<ModeMessage> for ModeActor {
    fn name(&self) -> &str {
        "ModeActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<ModeMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: ModeMessage) {
        match msg {
            ModeMessage::Get {
                mode_uid,
                respond_to,
            } => {
                let mode = self.modes_controller.get_mode(&mode_uid).await;
                let _ = respond_to.send(Ok(mode));
            }
            ModeMessage::GetAll { respond_to } => {
                let modes = self.modes_controller.get_modes().await;
                let _ = respond_to.send(Ok(modes));
            }
            ModeMessage::SaveOrder { order, respond_to } => {
                let result = self.modes_controller.update_mode_order(order).await;
                let _ = respond_to.send(result);
            }
            ModeMessage::Create {
                mode_name,
                respond_to,
            } => {
                let result = self.modes_controller.create_mode(mode_name).await;
                let _ = respond_to.send(result);
            }
            ModeMessage::Update {
                mode_uid,
                mode_name,
                respond_to,
            } => {
                let result = self
                    .modes_controller
                    .update_mode(&mode_uid, mode_name)
                    .await;
                let _ = respond_to.send(result);
            }
            ModeMessage::Delete {
                mode_uid,
                respond_to,
            } => {
                let result = self.modes_controller.delete_mode(&mode_uid).await;
                let _ = respond_to.send(result);
            }
            ModeMessage::Duplicate {
                mode_uid,
                respond_to,
            } => {
                let result = self.modes_controller.duplicate_mode(&mode_uid).await;
                let _ = respond_to.send(result);
            }
            ModeMessage::UpdateSettings {
                mode_uid,
                respond_to,
            } => {
                let result = self
                    .modes_controller
                    .update_mode_with_current_settings(&mode_uid)
                    .await;
                let _ = respond_to.send(result);
            }
            ModeMessage::GetAllActive { respond_to } => {
                let mode_uids = self.modes_controller.determine_active_modes_uids().await;
                let _ = respond_to.send(Ok(mode_uids));
            }
            ModeMessage::Activate {
                mode_uid,
                respond_to,
            } => {
                let result = self.modes_controller.activate_mode(&mode_uid).await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct ModeHandle {
    sender: mpsc::Sender<ModeMessage>,
}

impl ModeHandle {
    pub fn new<'s>(
        modes_controller: Rc<ModeController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = ModeActor::new(receiver, modes_controller);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get(&self, mode_uid: UID) -> Result<Option<Mode>> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::Get {
            mode_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_all(&self) -> Result<Vec<Mode>> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::GetAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn save_order(&self, order: Vec<UID>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::SaveOrder {
            order,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn create(&self, mode_name: String) -> Result<Mode> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::Create {
            mode_name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update(&self, mode_uid: UID, mode_name: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::Update {
            mode_uid,
            mode_name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn delete(&self, mode_uid: UID) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::Delete {
            mode_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn duplicate(&self, mode_uid: UID) -> Result<Mode> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::Duplicate {
            mode_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_settings(&self, mode_uid: UID) -> Result<Mode> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::UpdateSettings {
            mode_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_all_active(&self) -> Result<Vec<UID>> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::GetAllActive { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn activate(&self, mode_uid: UID) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ModeMessage::Activate {
            mode_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}