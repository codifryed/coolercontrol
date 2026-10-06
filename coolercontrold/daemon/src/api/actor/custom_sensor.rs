// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::alerts::AlertController;
use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::CCError;
use crate::config::Config;
use crate::engine::main::Engine;
use crate::overrides::OverridesController;
use crate::repositories::custom_sensors_repo::CustomSensorsRepo;
use crate::setting::CustomSensor;
use anyhow::Result;
use log::warn;
use moro_local::Scope;
use std::rc::Rc;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

/// A refused delete names at most this many users, so the message stays readable.
const USERS_NAMED_MAX: usize = 5;

struct CustomSensorActor {
    receiver: mpsc::Receiver<CustomSensorMessage>,
    custom_sensors_repo: Rc<CustomSensorsRepo>,
    engine: Rc<Engine>,
    config: Rc<Config>,
    overrides: Rc<OverridesController>,
    alert_controller: Rc<AlertController>,
}

enum CustomSensorMessage {
    Get {
        custom_sensor_id: String,
        respond_to: oneshot::Sender<Result<CustomSensor>>,
    },
    GetAll {
        respond_to: oneshot::Sender<Result<Vec<CustomSensor>>>,
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
        overrides: Rc<OverridesController>,
        alert_controller: Rc<AlertController>,
    ) -> Self {
        Self {
            receiver,
            custom_sensors_repo,
            engine,
            config,
            overrides,
            alert_controller,
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
                    let mut users = self
                        .engine
                        .custom_sensor_users(&cs_device_uid, &custom_sensor_id)
                        .await?;
                    users.extend(
                        self.alert_controller
                            .alerts_watching(&cs_device_uid, &custom_sensor_id)
                            .into_iter()
                            .map(|name| format!("Alert \"{name}\"")),
                    );
                    let sensor_label = self.overrides.resolve_channel_label(
                        &cs_device_uid,
                        &custom_sensor_id,
                        None,
                    );
                    verify_not_in_use(&sensor_label, &users)?;
                    self.custom_sensors_repo
                        .delete_custom_sensor(&custom_sensor_id)?;
                    let save_result = self.config.save_config_file().await;
                    // Cascade regardless of the save outcome: the sensor is
                    // already gone from the repo and IDs are recycled, so a
                    // future sensor reusing this ID must not inherit its name.
                    if let Err(err) = self
                        .overrides
                        .remove_channel(&cs_device_uid, &custom_sensor_id)
                        .await
                    {
                        warn!(
                            "Failed to remove name override for deleted sensor \
                            {custom_sensor_id}: {err}"
                        );
                    }
                    save_result
                }
                .await;
                let _ = respond_to.send(result);
            }
        }
    }
}

/// Refuses the delete while anything still reads the sensor. Naming the users tells the
/// user where to remove it first.
fn verify_not_in_use(sensor_label: &str, users: &[String]) -> Result<()> {
    if users.is_empty() {
        return Ok(());
    }
    let named = users
        .iter()
        .take(USERS_NAMED_MAX)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join(", ");
    let unnamed_count = users.len().saturating_sub(USERS_NAMED_MAX);
    let rest = if unnamed_count > 0 {
        format!(" and {unnamed_count} more")
    } else {
        String::new()
    };
    Err(CCError::UserError {
        msg: format!(
            "Custom Sensor \"{sensor_label}\" is in use by: {named}{rest}. \
            Remove it from them before deleting."
        ),
    }
    .into())
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
        overrides: Rc<OverridesController>,
        alert_controller: Rc<AlertController>,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = CustomSensorActor::new(
            receiver,
            custom_sensors_repo,
            engine,
            config,
            overrides,
            alert_controller,
        );
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ops::Not;

    fn refusal(result: Result<()>) -> String {
        match result.unwrap_err().downcast::<CCError>() {
            Ok(CCError::UserError { msg }) => msg,
            other => panic!("expected a user error, got {other:?}"),
        }
    }

    #[test]
    fn an_unused_sensor_may_be_deleted() {
        // Goal: with nothing reading the sensor the guard lets the delete through.
        assert!(verify_not_in_use("Liquid Delta", &[]).is_ok());
    }

    #[test]
    fn a_refused_delete_names_the_sensor_and_its_users() {
        // Goal: the refusal says which sensor it is about and where it is still used, so
        // the user knows what to change. Method: two users, read the message back.
        let users = [
            "Profile \"Radiator\"".to_string(),
            "LCD of Kraken".to_string(),
        ];

        let message = refusal(verify_not_in_use("Liquid Delta", &users));

        assert_eq!(
            message,
            "Custom Sensor \"Liquid Delta\" is in use by: Profile \"Radiator\", LCD of Kraken. \
            Remove it from them before deleting."
        );
    }

    #[test]
    fn a_refused_delete_names_a_limited_number_of_users() {
        // Goal: a sensor many Profiles read still gets a readable message. Method: two users
        // over the limit, then exactly the limit.
        let users = |count: usize| -> Vec<String> {
            (1..=count).map(|n| format!("Profile \"P{n}\"")).collect()
        };

        let over = refusal(verify_not_in_use("S", &users(USERS_NAMED_MAX + 2)));
        let at = refusal(verify_not_in_use("S", &users(USERS_NAMED_MAX)));

        assert!(over.contains(&format!("Profile \"P{USERS_NAMED_MAX}\" and 2 more.")));
        assert!(over.contains(&format!("P{}", USERS_NAMED_MAX + 1)).not());
        assert!(at.contains(&format!("Profile \"P{USERS_NAMED_MAX}\". Remove")));
        assert!(at.contains("more").not());
    }
}
