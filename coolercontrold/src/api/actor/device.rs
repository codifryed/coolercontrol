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
use crate::api::devices::DeviceDto;
use crate::config::Config;
use crate::device::{ChannelName, DeviceUID, Duty};
use crate::modes::ModeController;
use crate::processing::settings::SettingsController;
use crate::setting::{LcdSettings, LightingSettings, ProfileUID, Setting};
use crate::AllDevices;
use anyhow::Result;
use mime::Mime;
use moro_local::Scope;
use std::ops::Deref;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct DeviceActor {
    receiver: mpsc::Receiver<DeviceMessage>,
    all_devices: AllDevices,
    settings_controller: Rc<SettingsController>,
    modes_controller: Rc<ModeController>,
    config: Rc<Config>,
}

enum DeviceMessage {
    ThinkPadFanControl {
        enable: bool,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DevicesGet {
        respond_to: oneshot::Sender<Result<Vec<DeviceDto>>>,
    },
    DeviceImageGet {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Result<(Mime, Vec<u8>)>>,
    },
    DeviceImageProcess {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        files: Vec<(Mime, Vec<u8>)>,
        respond_to: oneshot::Sender<Result<(Mime, Vec<u8>)>>,
    },
    DeviceImageUpdate {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        mode: String,
        brightness: Option<u8>,
        orientation: Option<u16>,
        files: Vec<(Mime, Vec<u8>)>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceSettingsGet {
        device_uid: DeviceUID,
        respond_to: oneshot::Sender<Result<Vec<Setting>>>,
    },
    DeviceSettingManual {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        duty: Duty,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceSettingProfile {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        profile_uid: ProfileUID,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceSettingLCD {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        lcd_settings: LcdSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceSettingLighting {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        lighting_settings: LightingSettings,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceSettingPWMMode {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        pwm_mode: u8,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceSettingReset {
        device_uid: DeviceUID,
        channel_name: ChannelName,
        respond_to: oneshot::Sender<Result<()>>,
    },
    DeviceAseTekType {
        device_uid: DeviceUID,
        is_legacy690: bool,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl DeviceActor {
    pub fn new(
        receiver: mpsc::Receiver<DeviceMessage>,
        all_devices: AllDevices,
        settings_controller: Rc<SettingsController>,
        modes_controller: Rc<ModeController>,
        config: Rc<Config>,
    ) -> Self {
        Self {
            receiver,
            all_devices,
            settings_controller,
            modes_controller,
            config,
        }
    }
}

impl ApiActor<DeviceMessage> for DeviceActor {
    fn name(&self) -> &str {
        "DeviceActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<DeviceMessage> {
        &mut self.receiver
    }

    #[allow(clippy::too_many_lines)]
    async fn handle_message(&mut self, msg: DeviceMessage) {
        match msg {
            DeviceMessage::ThinkPadFanControl { enable, respond_to } => {
                let response = self.settings_controller.thinkpad_fan_control(&enable).await;
                let _ = respond_to.send(response);
            }
            DeviceMessage::DevicesGet { respond_to } => {
                let mut all_devices = Vec::new();
                for device in self.all_devices.values() {
                    all_devices.push(device.borrow().deref().into());
                }
                let _ = respond_to.send(Ok(all_devices));
            }
            DeviceMessage::DeviceImageGet {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let response = self
                    .settings_controller
                    .get_lcd_image(&device_uid, &channel_name)
                    .await;
                let _ = respond_to.send(response);
            }
            DeviceMessage::DeviceImageProcess {
                device_uid,
                channel_name,
                mut files,
                respond_to,
            } => {
                let response = self
                    .settings_controller
                    .process_lcd_image(&device_uid, &channel_name, &mut files)
                    .await;
                let _ = respond_to.send(response);
            }
            DeviceMessage::DeviceImageUpdate {
                device_uid,
                channel_name,
                mode,
                brightness,
                orientation,
                mut files,
                respond_to,
            } => {
                let result = async {
                    let processed_image_data = self
                        .settings_controller
                        .process_lcd_image(&device_uid, &channel_name, &mut files)
                        .await?;
                    let image_path = self
                        .settings_controller
                        .save_lcd_image(&processed_image_data.0, processed_image_data.1)
                        .await?;
                    let lcd_settings = LcdSettings {
                        mode,
                        brightness,
                        orientation,
                        image_file_processed: Some(image_path),
                        carousel: None,
                        temp_source: None,
                        colors: Vec::with_capacity(0),
                    };
                    self.settings_controller
                        .set_lcd(&device_uid, channel_name.as_str(), &lcd_settings)
                        .await?;
                    let config_setting = Setting {
                        channel_name,
                        lcd: Some(lcd_settings),
                        ..Default::default()
                    };
                    self.config.set_device_setting(&device_uid, &config_setting);
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceSettingsGet {
                device_uid,
                respond_to,
            } => {
                let response = self.config.get_device_settings(&device_uid);
                let _ = respond_to.send(response);
            }
            DeviceMessage::DeviceSettingManual {
                device_uid,
                channel_name,
                duty,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .set_fixed_speed(&device_uid, &channel_name, duty)
                        .await?;
                    let config_settings = Setting {
                        channel_name,
                        speed_fixed: Some(duty),
                        ..Default::default()
                    };
                    self.config
                        .set_device_setting(&device_uid, &config_settings);
                    self.modes_controller.clear_active_modes().await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceSettingProfile {
                device_uid,
                channel_name,
                profile_uid,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .set_profile(&device_uid, &channel_name, &profile_uid)
                        .await?;
                    let config_setting = Setting {
                        channel_name,
                        profile_uid: Some(profile_uid),
                        ..Default::default()
                    };
                    self.config.set_device_setting(&device_uid, &config_setting);
                    self.modes_controller.clear_active_modes().await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceSettingLCD {
                device_uid,
                channel_name,
                lcd_settings,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .set_lcd(&device_uid, &channel_name, &lcd_settings)
                        .await?;
                    let config_setting = Setting {
                        channel_name,
                        lcd: Some(lcd_settings),
                        ..Default::default()
                    };
                    self.config.set_device_setting(&device_uid, &config_setting);
                    self.modes_controller.clear_active_modes().await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceSettingLighting {
                device_uid,
                channel_name,
                lighting_settings,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .set_lighting(&device_uid, &channel_name, &lighting_settings)
                        .await?;
                    let config_setting = Setting {
                        channel_name,
                        lighting: Some(lighting_settings),
                        ..Default::default()
                    };
                    self.config.set_device_setting(&device_uid, &config_setting);
                    self.modes_controller.clear_active_modes().await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceSettingPWMMode {
                device_uid,
                channel_name,
                pwm_mode,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .set_pwm_mode(&device_uid, &channel_name, pwm_mode)
                        .await?;
                    let config_setting = Setting {
                        channel_name,
                        pwm_mode: Some(pwm_mode),
                        ..Default::default()
                    };
                    self.config.set_device_setting(&device_uid, &config_setting);
                    self.modes_controller.clear_active_modes().await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceSettingReset {
                device_uid,
                channel_name,
                respond_to,
            } => {
                let result = async {
                    self.settings_controller
                        .set_reset(&device_uid, &channel_name)
                        .await?;
                    let config_setting = Setting {
                        channel_name,
                        reset_to_default: Some(true),
                        ..Default::default()
                    };
                    self.config.set_device_setting(&device_uid, &config_setting);
                    self.modes_controller.clear_active_modes().await;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            DeviceMessage::DeviceAseTekType {
                device_uid,
                is_legacy690,
                respond_to,
            } => {
                let result = async {
                    self.config.set_legacy690_id(&device_uid, is_legacy690);
                    self.config.save_config_file().await?;
                    // Device is now known. Legacy690Lc devices still require a restart of the daemon.
                    if let Some(device) = self.all_devices.get(&device_uid) {
                        if device.borrow().lc_info.is_some() {
                            device.borrow_mut().lc_info.as_mut().unwrap().unknown_asetek = false;
                        }
                    }
                    Ok(())
                }
                .await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct DeviceHandle {
    sender: mpsc::Sender<DeviceMessage>,
}

impl DeviceHandle {
    pub fn new<'s>(
        all_devices: AllDevices,
        settings_controller: Rc<SettingsController>,
        modes_controller: Rc<ModeController>,
        config: Rc<Config>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = DeviceActor::new(
            receiver,
            all_devices,
            settings_controller,
            modes_controller,
            config,
        );
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn thinkpad_fan_control(&self, enable: bool) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::ThinkPadFanControl {
            enable,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn devices_get(&self) -> Result<Vec<DeviceDto>> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DevicesGet { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_image_get(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
    ) -> Result<(Mime, Vec<u8>)> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceImageGet {
            device_uid,
            channel_name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_image_process(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        files: Vec<(Mime, Vec<u8>)>,
    ) -> Result<(Mime, Vec<u8>)> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceImageProcess {
            device_uid,
            channel_name,
            files,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_image_update(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        mode: String,
        brightness: Option<u8>,
        orientation: Option<u16>,
        files: Vec<(Mime, Vec<u8>)>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceImageUpdate {
            device_uid,
            channel_name,
            mode,
            brightness,
            orientation,
            files,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_settings_get(&self, device_uid: DeviceUID) -> Result<Vec<Setting>> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingsGet {
            device_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_setting_manual(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        duty: Duty,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingManual {
            device_uid,
            channel_name,
            duty,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_setting_profile(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        profile_uid: ProfileUID,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingProfile {
            device_uid,
            channel_name,
            profile_uid,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_setting_lcd(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        lcd_settings: LcdSettings,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingLCD {
            device_uid,
            channel_name,
            lcd_settings,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_setting_lighting(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        lighting_settings: LightingSettings,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingLighting {
            device_uid,
            channel_name,
            lighting_settings,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_setting_pwm_mode(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
        pwm_mode: u8,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingPWMMode {
            device_uid,
            channel_name,
            pwm_mode,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_setting_reset(
        &self,
        device_uid: DeviceUID,
        channel_name: ChannelName,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceSettingReset {
            device_uid,
            channel_name,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn device_asetek_type(
        &self,
        device_uid: DeviceUID,
        is_legacy690: bool,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = DeviceMessage::DeviceAseTekType {
            device_uid,
            is_legacy690,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
