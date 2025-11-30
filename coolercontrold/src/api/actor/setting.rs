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
use crate::api::settings::{CoolerControlDeviceSettingsDto, CoolerControlSettingsDto};
use crate::api::CCError;
use crate::config::Config;
use crate::device::{DeviceType, DeviceUID};
use crate::setting::{CCDeviceSettings, CoolerControlSettings, DeviceExtensions, Setting};
use crate::AllDevices;
use anyhow::Result;
use moro_local::Scope;
use std::collections::HashMap;
use std::default::Default;
use std::ops::Not;
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
        update: CCDeviceSettings,
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
    fn name(&self) -> &'static str {
        "SettingActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<SettingMessage> {
        &mut self.receiver
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_message(&mut self, msg: SettingMessage) {
        match msg {
            SettingMessage::GetCC { respond_to } => {
                let result = self.config.get_settings();
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateCC { update, respond_to } => {
                let result = async {
                    let current_settings = self.config.get_settings()?;
                    let settings_to_set = update.merge(current_settings);
                    self.config.set_settings(&settings_to_set);
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::GetAllCCDevices { respond_to } => {
                let result = async {
                    let mut devices_settings = HashMap::new();
                    let mut saved_settings_map = self.config.get_all_cc_devices_settings()?;
                    for (device_uid, device_lock) in self.all_devices.iter() {
                        // use saved settings if present, otherwise use default
                        if let Some(settings) = saved_settings_map.remove(device_uid) {
                            devices_settings.insert(
                                device_uid.clone(),
                                CoolerControlDeviceSettingsDto {
                                    uid: device_uid.clone(),
                                    name: settings.name, // keeps user-defined name from UI
                                    disable: settings.disable,
                                    extensions: settings.extensions,
                                    channel_settings: settings.channel_settings,
                                },
                            );
                        } else {
                            let device_name = {
                                let lock = device_lock.borrow();
                                if lock.d_type == DeviceType::CustomSensors {
                                    // custom sensors is handled differently than hardware devices
                                    continue;
                                }
                                lock.info.model.clone().unwrap_or_else(|| lock.name.clone())
                            };
                            devices_settings.insert(
                                device_uid.clone(),
                                CoolerControlDeviceSettingsDto {
                                    uid: device_uid.clone(),
                                    name: device_name,
                                    disable: false,
                                    extensions: DeviceExtensions::default(),
                                    channel_settings: HashMap::with_capacity(0),
                                },
                            );
                        }
                    }
                    // This adds the remaining devices which are currently not present, (i.e. blacklisted devices)
                    for (device_uid, settings) in saved_settings_map {
                        devices_settings.insert(
                            device_uid.clone(),
                            CoolerControlDeviceSettingsDto {
                                uid: device_uid,
                                name: settings.name,
                                disable: settings.disable,
                                extensions: settings.extensions,
                                channel_settings: settings.channel_settings,
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
                    let settings_option = self.config.get_cc_settings_for_device(&device_uid)?;
                    let dto = if let Some(settings) = settings_option {
                        CoolerControlDeviceSettingsDto {
                            uid: device_uid,
                            name: settings.name, // keeps user-defined name from UI
                            disable: settings.disable,
                            extensions: settings.extensions,
                            channel_settings: settings.channel_settings,
                        }
                    } else {
                        // Default settings
                        let current_device_name = {
                            if let Some(device_lock) = self.all_devices.get(&device_uid) {
                                let lock = device_lock.borrow();
                                let device_name =
                                    lock.info.model.clone().unwrap_or_else(|| lock.name.clone());
                                Some(device_name)
                            } else {
                                None
                            }
                        }
                        .ok_or_else(|| CCError::NotFound {
                            msg: "Device not found".to_string(),
                        })?;
                        CoolerControlDeviceSettingsDto {
                            uid: device_uid,
                            name: current_device_name,
                            disable: false,
                            extensions: DeviceExtensions::default(),
                            channel_settings: HashMap::with_capacity(0),
                        }
                    };
                    Ok(dto)
                }
                .await;
                let _ = respond_to.send(result);
            }
            SettingMessage::UpdateCCDevice {
                device_uid,
                mut update,
                respond_to,
            } => {
                let result = async {
                    // update any missing channel labels before saving
                    for (channel_name, settings) in &mut update.channel_settings {
                        if settings
                            .label
                            .as_ref()
                            .is_some_and(|label| label.is_empty().not())
                        {
                            // label may be already set by UI - allowing user-defined labels to persist
                            continue;
                        }
                        if let Some(device_lock) = self.all_devices.get(&device_uid) {
                            let lock = device_lock.borrow();
                            if let Some(temp_info) = lock.info.temps.get(channel_name) {
                                settings.label = Some(temp_info.label.clone());
                            } else if let Some(channel_info) = lock.info.channels.get(channel_name)
                            {
                                settings.label.clone_from(&channel_info.label);
                            }
                        }
                    }
                    self.config.set_cc_settings_for_device(&device_uid, &update);
                    // check for disabled devices and channels and remove their settings:
                    if update.channel_settings.is_empty().not() {
                        for setting in self.config.get_device_settings(&device_uid)? {
                            if update
                                .channel_settings
                                .get(&setting.channel_name)
                                .is_some_and(|s| s.disabled)
                            {
                                let reset_setting = Setting {
                                    channel_name: setting.channel_name,
                                    reset_to_default: Some(true),
                                    ..Default::default()
                                };
                                self.config.set_device_setting(&device_uid, &reset_setting);
                            }
                        }
                    }
                    if update.disable {
                        self.config.clear_device_settings(&device_uid);
                    }
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
        update: CCDeviceSettings,
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
