// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::plugins::{PluginDto, PluginStatusDto, PluginsDto};
use crate::repositories::service_plugin::plugin_controller::PluginController;
use crate::repositories::service_plugin::service_manifest::ConnectionType;
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
    StartPlugin {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    StopPlugin {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    RestartPlugin {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    ReloadPlugin {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetStatus {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<PluginStatusDto>>,
    },
    DisablePlugin {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    EnablePlugin {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetProxyPort {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<Option<u16>>>,
    },
    IsPluginDisabled {
        plugin_id: String,
        respond_to: oneshot::Sender<Result<bool>>,
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

    fn collect_all_plugins(&self) -> PluginsDto {
        let manifests = self.plugin_controller.manifests();
        let mut plugins = Vec::with_capacity(manifests.len());
        for manifest in manifests {
            let address = match manifest.address {
                ConnectionType::None => String::new(),
                ConnectionType::Uds(uds_path) => uds_path.display().to_string(),
                ConnectionType::Tcp(addr) => addr,
            };
            plugins.push(PluginDto {
                disabled: self.plugin_controller.is_plugin_disabled(&manifest.id),
                id: manifest.id,
                service_type: manifest.service_type.to_string(),
                description: manifest.description,
                version: manifest.version,
                url: manifest.url,
                address,
                privileged: manifest.privileged,
                path: manifest.path.display().to_string(),
            });
        }
        PluginsDto { plugins }
    }
}

impl ApiActor<PluginMessage> for PluginActor {
    fn name(&self) -> &'static str {
        "PluginActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<PluginMessage> {
        &mut self.receiver
    }

    // A flat dispatch with one short arm per message: splitting it would only hide the list.
    #[allow(clippy::too_many_lines)]
    async fn handle_message(&mut self, message: PluginMessage) {
        match message {
            PluginMessage::GetAll { respond_to } => {
                self.plugin_controller.discover_plugins().await;
                let _ = respond_to.send(self.collect_all_plugins());
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
            PluginMessage::StartPlugin {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.start_plugin(&plugin_id).await;
                let _ = respond_to.send(result);
            }
            PluginMessage::StopPlugin {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.stop_plugin(&plugin_id).await;
                let _ = respond_to.send(result);
            }
            PluginMessage::RestartPlugin {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.restart_plugin(&plugin_id).await;
                let _ = respond_to.send(result);
            }
            PluginMessage::ReloadPlugin {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.reload_plugin(&plugin_id).await;
                let _ = respond_to.send(result);
            }
            PluginMessage::GetStatus {
                plugin_id,
                respond_to,
            } => {
                let result = self
                    .plugin_controller
                    .get_plugin_status(&plugin_id)
                    .await
                    .map(Into::into);
                let _ = respond_to.send(result);
            }
            PluginMessage::DisablePlugin {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.disable_plugin(&plugin_id).await;
                let _ = respond_to.send(result);
            }
            PluginMessage::EnablePlugin {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.enable_plugin(&plugin_id).await;
                let _ = respond_to.send(result);
            }
            PluginMessage::GetProxyPort {
                plugin_id,
                respond_to,
            } => {
                let result = self.plugin_controller.get_proxy_port(&plugin_id);
                let _ = respond_to.send(result);
            }
            PluginMessage::IsPluginDisabled {
                plugin_id,
                respond_to,
            } => {
                let disabled = self.plugin_controller.is_plugin_disabled(&plugin_id);
                let _ = respond_to.send(Ok(disabled));
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

    pub async fn start_plugin(&self, plugin_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::StartPlugin {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn stop_plugin(&self, plugin_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::StopPlugin {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn restart_plugin(&self, plugin_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::RestartPlugin {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn reload_plugin(&self, plugin_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::ReloadPlugin {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_plugin_status(&self, plugin_id: String) -> Result<PluginStatusDto> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::GetStatus {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn disable_plugin(&self, plugin_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::DisablePlugin {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn enable_plugin(&self, plugin_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::EnablePlugin {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_proxy_port(&self, plugin_id: String) -> Result<Option<u16>> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::GetProxyPort {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn is_plugin_disabled(&self, plugin_id: String) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        let msg = PluginMessage::IsPluginDisabled {
            plugin_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
