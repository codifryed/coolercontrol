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
use crate::api::settings::{CoolerControlDeviceSettingsDto, CoolerControlSettingsDto};
use crate::api::CCError;
use crate::config::Config;
use crate::device::DeviceUID;
use crate::setting::{CoolerControlDeviceSettings, CoolerControlSettings};
use crate::AllDevices;
use anyhow::Result;
use moro_local::Scope;
use std::collections::HashMap;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct SettingActor {
    receiver: mpsc::Receiver<SettingMessage>,
    all_devices: AllDevices,
    config: Rc<Config>,
}

enum SettingMessage {
    GetCC {
        respond_to: oneshot::Sender<Result<CoolerControlSettings>>,
    },
    UpdateCC {
        update: CoolerControlSettingsDto,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetAllCCDevices {
        respond_to: oneshot::Sender<Result<Vec<CoolerControlDeviceSettingsDto>>>,
    },
    GetCCDevice {
        device_uid: DeviceUID,
        respond_to: oneshot::Sender<Result<CoolerControlDeviceSettingsDto>>,
    },
    UpdateCCDevice {
        device_uid: DeviceUID,
        update: CoolerControlDeviceSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
    GetUI {
        respond_to: oneshot::Sender<Result<String>>,
    },
    UpdateUI {
        settings: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl SettingActor {
    pub fn new(
        receiver: mpsc::Receiver<SettingMessage>,
        all_devices: AllDevices,
        config: Rc<Config>,
    ) -> Self {
        Self {
            receiver,
            all_devices,
            config,
        }
    }
}

impl ApiActor<SettingMessage> for SettingActor {
    fn name(&self) -> &str {
        "SettingActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<SettingMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: SettingMessage) {
        match msg {
            SettingMessage::GetCC { respond_to } => {
                let result = self.config.get_settings().await;
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateCC { update, respond_to } => {
                let result = async {
                    let current_settings = self.config.get_settings().await?;
                    let settings_to_set = update.merge(current_settings);
                    self.config.set_settings(&settings_to_set).await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetAllCCDevices { respond_to } => {
                let result = async {
                    let settings_map = self.config.get_all_cc_devices_settings().await?;
                    let mut devices_settings = HashMap::new();
                    for (device_uid, device_lock) in self.all_devices.iter() {
                        let name = device_lock.borrow().name.clone();
                        // first fill with the default
                        devices_settings.insert(
                            device_uid.clone(),
                            CoolerControlDeviceSettingsDto {
                                uid: device_uid.to_string(),
                                name,
                                disable: false,
                            },
                        );
                    }
                    for (device_uid, setting_option) in settings_map {
                        let setting = setting_option.ok_or_else(|| CCError::InternalError {
                            msg: "CC Settings option should always be present in this situation"
                                .to_string(),
                        })?;
                        // override and fill with blacklisted devices:
                        devices_settings.insert(
                            device_uid.clone(),
                            CoolerControlDeviceSettingsDto {
                                uid: device_uid,
                                name: setting.name,
                                disable: setting.disable,
                            },
                        );
                    }
                    let cc_devices_settings = devices_settings
                        .into_values()
                        .collect::<Vec<CoolerControlDeviceSettingsDto>>();
                    Ok(cc_devices_settings)
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetCCDevice {
                device_uid,
                respond_to,
            } => {
                let result = async {
                    let settings_option =
                        self.config.get_cc_settings_for_device(&device_uid).await?;
                    let dto = match settings_option {
                        Some(settings) => CoolerControlDeviceSettingsDto {
                            uid: device_uid,
                            name: settings.name,
                            disable: settings.disable,
                        },
                        None => {
                            // Default settings for a device: (Same as None)
                            let device_name = self
                                .all_devices
                                .get(&device_uid)
                                .ok_or_else(|| CCError::NotFound {
                                    msg: "Device not found".to_string(),
                                })?
                                .borrow()
                                .name
                                .clone();
                            CoolerControlDeviceSettingsDto {
                                uid: device_uid,
                                name: device_name,
                                disable: false,
                            }
                        }
                    };
                    Ok(dto)
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateCCDevice {
                device_uid,
                update,
                respond_to,
            } => {
                let result = async {
                    self.config
                        .set_cc_settings_for_device(&device_uid, &update)
                        .await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetUI { respond_to } => {
                let result = self.config.load_ui_config_file().await;
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateUI {
                settings,
                respond_to,
            } => {
                let result = self.config.save_ui_config_file(settings).await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct SettingHandle {
    sender: mpsc::Sender<SettingMessage>,
}

impl SettingHandle {
    pub fn new<'s>(
        all_devices: AllDevices,
        config: Rc<Config>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = SettingActor::new(receiver, all_devices, config);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get_cc(&self) -> Result<CoolerControlSettings> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetCC { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_cc(&self, update: CoolerControlSettingsDto) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::UpdateCC {
            update,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_all_cc_devices(&self) -> Result<Vec<CoolerControlDeviceSettingsDto>> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetAllCCDevices { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_cc_device(
        &self,
        device_uid: DeviceUID,
    ) -> Result<CoolerControlDeviceSettingsDto> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetCCDevice {
            device_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_cc_device(
        &self,
        device_uid: DeviceUID,
        update: CoolerControlDeviceSettings,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::UpdateCCDevice {
            device_uid,
            update,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_ui(&self) -> Result<String> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::GetUI { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update_ui(&self, settings: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = SettingMessage::UpdateUI {
            settings,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
