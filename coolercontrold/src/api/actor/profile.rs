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
use crate::modes::ModeController;
use crate::processing::settings::SettingsController;
use crate::setting::{Profile, ProfileUID};
use anyhow::Result;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct ProfileActor {
    receiver: mpsc::Receiver<ProfileMessage>,
    settings_controller: Rc<SettingsController>,
    config: Rc<Config>,
    mode_controller: Rc<ModeController>,
}

enum ProfileMessage {
    GetAll {
        respond_to: oneshot::Sender<Result<Vec<Profile>>>,
    },
    SaveOrder {
        order: Vec<Profile>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Create {
        profile: Profile,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Update {
        profile: Profile,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Delete {
        profile_uid: ProfileUID,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl ProfileActor {
    pub fn new(
        receiver: mpsc::Receiver<ProfileMessage>,
        settings_controller: Rc<SettingsController>,
        config: Rc<Config>,
        mode_controller: Rc<ModeController>,
    ) -> Self {
        Self {
            receiver,
            settings_controller,
            config,
            mode_controller,
        }
    }
}

impl ApiActor<ProfileMessage> for ProfileActor {
    fn name(&self) -> &'static str {
        "ProfileActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<ProfileMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: ProfileMessage) {
        match msg {
            ProfileMessage::GetAll { respond_to } => {
                let result = self.config.get_profiles().await;
                let _ = respond_to.send(result);
            }
            ProfileMessage::SaveOrder { order, respond_to } => {
                let result = async {
                    self.config.set_profiles_order(&order)?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            ProfileMessage::Create {
                profile,
                respond_to,
            } => {
                let result = async {
                    self.config.set_profile(profile)?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            ProfileMessage::Update {
                profile,
                respond_to,
            } => {
                let result = async {
                    let profile_uid = profile.uid.clone();
                    self.config.update_profile(profile)?;
                    self.settings_controller.profile_updated(&profile_uid).await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            ProfileMessage::Delete {
                profile_uid,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .profile_deleted(&profile_uid)
                        .await?;
                    self.config.delete_profile(&profile_uid)?;
                    self.config.save_config_file().await?;
                    self.mode_controller.profile_deleted(&profile_uid).await
                }
                .await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct ProfileHandle {
    sender: mpsc::Sender<ProfileMessage>,
}

impl ProfileHandle {
    pub fn new<'s>(
        settings_controller: Rc<SettingsController>,
        config: Rc<Config>,
        mode_controller: Rc<ModeController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = ProfileActor::new(receiver, settings_controller, config, mode_controller);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get_all(&self) -> Result<Vec<Profile>> {
        let (tx, rx) = oneshot::channel();
        let msg = ProfileMessage::GetAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn save_order(&self, order: Vec<Profile>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ProfileMessage::SaveOrder {
            order,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn create(&self, profile: Profile) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ProfileMessage::Create {
            profile,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update(&self, profile: Profile) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ProfileMessage::Update {
            profile,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn delete(&self, profile_uid: ProfileUID) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = ProfileMessage::Delete {
            profile_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
