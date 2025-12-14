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

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::plugins::{PluginDto, PluginsDto};
use crate::repositories::service_plugin::plugin_controller::PluginController;
use anyhow::Result;
use moro_local::Scope;
use std::path::PathBuf;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct PluginActor {
    receiver: mpsc::Receiver<PluginMessage>,
    plugin_controller: Rc<PluginController>,
}

enum PluginMessage {
    GetAll {
        respond_to: oneshot::Sender<PluginsDto>,
    },
    GetConfig {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<String>>,
    },
    UpdateConfig {
        plugin_id: String,
        config: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetUiDir {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<PathBuf>>,
    },
}
impl PluginActor {
    pub fn new(
        receiver: mpsc::Receiver<PluginMessage>,
        plugin_controller: Rc<PluginController>,
    ) -> Self {
        Self {
            receiver,
            plugin_controller,
        }
    }
}

impl ApiActor<PluginMessage> for PluginActor {
    fn name(&self) -> &'static str {
        "PluginActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<PluginMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, message: PluginMessage) {
        match message {
            PluginMessage::GetAll { respond_to } => {
                let mut plugins = Vec::new();
                for manifest in self.plugin_controller.plugins.values() {
                    plugins.push(PluginDto {
                        id: manifest.id.clone(),
                        service_type: manifest.service_type.to_string(),
                        address: manifest.address.to_string(),
                        privileged: manifest.privileged,
                        path: manifest.path.display().to_string(),
                    });
                }
                let _ = respond_to.send(PluginsDto { plugins });
            }
            PluginMessage::GetConfig {
                plugin_id,
                respond_to,
            } => {
                let config = self
                    .plugin_controller
                    .load_plugin_config_file(&plugin_id)
                    .await;
                let _ = respond_to.send(config);
            }
            PluginMessage::UpdateConfig {
                plugin_id,
                config,
                respond_to,
            } => {
                let result = self
                    .plugin_controller
                    .save_plugin_config_file(&plugin_id, config)
                    .await;
                let _ = respond_to.send(result);
            }
            PluginMessage::GetUiDir {
                plugin_id,
                respond_to,
            } => {
                let ui_dir = self.plugin_controller.get_plugin_ui_dir(&plugin_id);
                let _ = respond_to.send(ui_dir);
            }
        }
    }
}

#[derive(Clone)]
pub struct PluginHandle {
    sender: mpsc::Sender<PluginMessage>,
}

impl PluginHandle {
    pub fn new<'s>(
        plugin_controller: Rc<PluginController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = PluginActor::new(receiver, plugin_controller);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get_all(&self) -> Result<PluginsDto> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::GetAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        Ok(rx.await?)
    }

    pub async fn get_config(&self, plugin_id: String) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::GetConfig {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_config(&self, plugin_id: String, config: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::UpdateConfig {
            plugin_id,
            config,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_ui_dir(&self, plugin_id: String) -> Result<PathBuf> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::GetUiDir {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
