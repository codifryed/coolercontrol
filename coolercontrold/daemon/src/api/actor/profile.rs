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
use crate::api::CCError;
use crate::config::Config;
use crate::engine::main::Engine;
use crate::modes::ModeController;
use crate::setting::{Profile, ProfileType, ProfileUID};
use crate::AllDevices;
use anyhow::Result;
use moro_local::Scope;
use std::ops::Not;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct ProfileActor {
    all_devices: AllDevices,
    receiver: mpsc::Receiver<ProfileMessage>,
    engine: Rc<Engine>,
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
        all_devices: AllDevices,
        receiver: mpsc::Receiver<ProfileMessage>,
        engine: Rc<Engine>,
        config: Rc<Config>,
        mode_controller: Rc<ModeController>,
    ) -> Self {
        Self {
            all_devices,
            receiver,
            engine,
            config,
            mode_controller,
        }
    }

    async fn verify_profile_internals(&self, profile: &Profile) -> Result<()> {
        if profile.p_type == ProfileType::Graph {
            // verify function exists
            let _ = self.config.get_function(&profile.function_uid)?;
            // verify temp_source exists
            let Some(temp_source) = profile.temp_source.as_ref() else {
                return Err(CCError::UserError {
                    msg: "Temp Source not present in Profile".to_string(),
                }
                .into());
            };
            let Some(temp_source_device_lock) = self.all_devices.get(&temp_source.device_uid)
            else {
                return Err(CCError::UserError {
                    msg: format!("No Device found with given UID: {}", temp_source.device_uid),
                }
                .into());
            };
            let temp_exists = temp_source_device_lock
                .borrow()
                .info
                .temps
                .contains_key(&temp_source.temp_name);
            if temp_exists.not() {
                return Err(CCError::UserError {
                    msg: format!(
                        "Device with given UID: {} doesn't have a temp name: {}",
                        temp_source.device_uid, temp_source.temp_name
                    ),
                }
                .into());
            }
        } else if profile.p_type == ProfileType::Mix {
            let all_profiles = self.config.get_profiles().await?;
            for member_uid in &profile.member_profile_uids {
                // Verify member profile exists
                let Some(member) = all_profiles.iter().find(|p| &p.uid == member_uid) else {
                    return Err(CCError::UserError {
                        msg: format!("Member Profile with UID: {member_uid} not found"),
                    }
                    .into());
                };
                // Verify members are Graph or Mix only
                if member.p_type != ProfileType::Graph && member.p_type != ProfileType::Mix {
                    return Err(CCError::UserError {
                        msg: format!(
                            "Mix member '{}' must be a Graph or Mix profile",
                            member.name
                        ),
                    }
                    .into());
                }
                // For Mix members: verify single-level (no Mix sub-members)
                if member.p_type == ProfileType::Mix {
                    let has_mix_sub_members = member.member_profile_uids.iter().any(|sub_uid| {
                        all_profiles
                            .iter()
                            .find(|p| &p.uid == sub_uid)
                            .is_some_and(|p| p.p_type == ProfileType::Mix)
                    });
                    if has_mix_sub_members {
                        return Err(CCError::UserError {
                            msg: format!(
                                "Mix member '{}' already contains Mix sub-members \
                                 (only single-level nesting allowed)",
                                member.name
                            ),
                        }
                        .into());
                    }
                    // Check circular reference: member doesn't contain this profile's UID
                    if member.member_profile_uids.contains(&profile.uid) {
                        return Err(CCError::UserError {
                            msg: format!(
                                "Circular reference: Mix member '{}' contains this profile",
                                member.name
                            ),
                        }
                        .into());
                    }
                }
            }
            // Check: if this profile is already a child of another Mix, it cannot have Mix members
            let has_mix_members = profile.member_profile_uids.iter().any(|uid| {
                all_profiles
                    .iter()
                    .find(|p| &p.uid == uid)
                    .is_some_and(|p| p.p_type == ProfileType::Mix)
            });
            if has_mix_members {
                let is_child_of_another_mix = all_profiles.iter().any(|p| {
                    p.p_type == ProfileType::Mix
                        && p.uid != profile.uid
                        && p.member_profile_uids.contains(&profile.uid)
                });
                if is_child_of_another_mix {
                    return Err(CCError::UserError {
                        msg: "This Mix profile is already a member of another Mix profile \
                              and cannot contain Mix members itself"
                            .to_string(),
                    }
                    .into());
                }
            }
        }
        Ok(())
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
                    self.verify_profile_internals(&profile).await?;
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
                    self.verify_profile_internals(&profile).await?;
                    self.config.update_profile(profile)?;
                    self.engine.profile_updated(&profile_uid).await;
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
                    self.engine.profile_deleted(&profile_uid).await?;
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
        all_devices: AllDevices,
        engine: Rc<Engine>,
        config: Rc<Config>,
        mode_controller: Rc<ModeController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = ProfileActor::new(all_devices, receiver, engine, config, mode_controller);
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
