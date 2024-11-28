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
use crate::config::Config;
use crate::processing::settings::SettingsController;
use crate::setting::{Function, FunctionUID};
use anyhow::Result;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct FunctionActor {
    receiver: mpsc::Receiver<FunctionMessage>,
    settings_controller: Rc<SettingsController>,
    config: Rc<Config>,
}

enum FunctionMessage {
    GetAll {
        respond_to: oneshot::Sender<Result<Vec<Function>>>,
    },
    SaveOrder {
        order: Vec<Function>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Create {
        function: Function,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Update {
        function: Function,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Delete {
        function_uid: FunctionUID,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl FunctionActor {
    pub fn new(
        receiver: mpsc::Receiver<FunctionMessage>,
        settings_controller: Rc<SettingsController>,
        config: Rc<Config>,
    ) -> Self {
        Self {
            receiver,
            settings_controller,
            config,
        }
    }
}

impl ApiActor<FunctionMessage> for FunctionActor {
    fn name(&self) -> &str {
        "FunctionActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<FunctionMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: FunctionMessage) {
        match msg {
            FunctionMessage::GetAll { respond_to } => {
                let result = self.config.get_functions().await;
                let _ = respond_to.send(result);
            }
            FunctionMessage::SaveOrder { order, respond_to } => {
                let result = async {
                    self.config.set_functions_order(&order)?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            FunctionMessage::Create {
                function,
                respond_to,
            } => {
                let result = async {
                    self.config.set_function(function)?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            FunctionMessage::Update {
                function,
                respond_to,
            } => {
                let result = async {
                    let function_uid = function.uid.clone();
                    self.config.update_function(function)?;
                    self.settings_controller
                        .function_updated(&function_uid)
                        .await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            FunctionMessage::Delete {
                function_uid,
                respond_to,
            } => {
                let result = async {
                    self.config.delete_function(&function_uid)?;
                    self.settings_controller
                        .function_deleted(&function_uid)
                        .await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct FunctionHandle {
    sender: mpsc::Sender<FunctionMessage>,
}

impl FunctionHandle {
    pub fn new<'s>(
        settings_controller: Rc<SettingsController>,
        config: Rc<Config>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = FunctionActor::new(receiver, settings_controller, config);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get_all(&self) -> Result<Vec<Function>> {
        let (tx, rx) = oneshot::channel();
        let msg = FunctionMessage::GetAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn save_order(&self, order: Vec<Function>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = FunctionMessage::SaveOrder {
            order,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn create(&self, function: Function) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = FunctionMessage::Create {
            function,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update(&self, function: Function) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = FunctionMessage::Update {
            function,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn delete(&self, function_uid: FunctionUID) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = FunctionMessage::Delete {
            function_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
