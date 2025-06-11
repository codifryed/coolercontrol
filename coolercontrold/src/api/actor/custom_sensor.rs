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
use crate::engine::main::Engine;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::setting::CustomSensor;
use anyhow::Result;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct CustomSensorActor {
    receiver: mpsc::Receiver<CustomSensorMessage>,
    custom_sensors_repo: Rc<CustomSensorsRepo>,
    engine: Rc<Engine>,
    config: Rc<Config>,
}

enum CustomSensorMessage {
    Get {
        custom_sensor_id: String,
        respond_to: oneshot::Sender<Result<CustomSensor>>,
    },
    GetAll {
        respond_to: oneshot::Sender<Result<Vec<CustomSensor>>>,
    },
    SaveOrder {
        order: Vec<CustomSensor>,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Create {
        custom_sensor: CustomSensor,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Update {
        custom_sensor: CustomSensor,
        respond_to: oneshot::Sender<Result<()>>,
    },
    Delete {
        custom_sensor_id: String,
        respond_to: oneshot::Sender<Result<()>>,
    },
}

impl CustomSensorActor {
    pub fn new(
        receiver: mpsc::Receiver<CustomSensorMessage>,
        custom_sensors_repo: Rc<CustomSensorsRepo>,
        engine: Rc<Engine>,
        config: Rc<Config>,
    ) -> Self {
        Self {
            receiver,
            custom_sensors_repo,
            engine,
            config,
        }
    }
}

impl ApiActor<CustomSensorMessage> for CustomSensorActor {
    fn name(&self) -> &'static str {
        "CustomSensorActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<CustomSensorMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: CustomSensorMessage) {
        match msg {
            CustomSensorMessage::Get {
                custom_sensor_id,
                respond_to,
            } => {
                let result = self
                    .custom_sensors_repo
                    .get_custom_sensor(&custom_sensor_id);
                let _ = respond_to.send(result);
            }
            CustomSensorMessage::GetAll { respond_to } => {
                let result = self.custom_sensors_repo.get_custom_sensors();
                let _ = respond_to.send(Ok(result));
            }
            CustomSensorMessage::SaveOrder { order, respond_to } => {
                let result = async {
                    self.custom_sensors_repo.set_custom_sensors_order(&order)?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            CustomSensorMessage::Create {
                custom_sensor,
                respond_to,
            } => {
                let result = async {
                    self.custom_sensors_repo
                        .set_custom_sensor(custom_sensor)
                        .await?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            CustomSensorMessage::Update {
                custom_sensor,
                respond_to,
            } => {
                let result = async {
                    self.custom_sensors_repo
                        .update_custom_sensor(custom_sensor)
                        .await?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
            CustomSensorMessage::Delete {
                custom_sensor_id,
                respond_to,
            } => {
                let result = async {
                    let cs_device_uid = self.custom_sensors_repo.get_device_uid();
                    self.engine
                        .custom_sensor_deleted(&cs_device_uid, &custom_sensor_id)
                        .await?;
                    self.custom_sensors_repo
                        .delete_custom_sensor(&custom_sensor_id)?;
                    self.config.save_config_file().await
                }
                .await;
                let _ = respond_to.send(result);
            }
        }
    }
}

#[derive(Clone)]
pub struct CustomSensorHandle {
    sender: mpsc::Sender<CustomSensorMessage>,
}

impl CustomSensorHandle {
    pub fn new<'s>(
        custom_sensors_repo: Rc<CustomSensorsRepo>,
        engine: Rc<Engine>,
        config: Rc<Config>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor =
            CustomSensorActor::new(receiver, custom_sensors_repo, engine, config);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn get(&self, custom_sensor_id: String) -> Result<CustomSensor> {
        let (tx, rx) = oneshot::channel();
        let msg = CustomSensorMessage::Get {
            custom_sensor_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn get_all(&self) -> Result<Vec<CustomSensor>> {
        let (tx, rx) = oneshot::channel();
        let msg = CustomSensorMessage::GetAll { respond_to: tx };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn save_order(&self, order: Vec<CustomSensor>) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = CustomSensorMessage::SaveOrder {
            order,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn create(&self, custom_sensor: CustomSensor) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = CustomSensorMessage::Create {
            custom_sensor,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn update(&self, custom_sensor: CustomSensor) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = CustomSensorMessage::Update {
            custom_sensor,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }

    pub async fn delete(&self, custom_sensor_id: String) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        let msg = CustomSensorMessage::Delete {
            custom_sensor_id,
            respond_to: tx,
        };
        let _ = self.sender.send(msg).await;
        rx.await?
    }
}
