// SPDX-FileCopyrightText: 2024 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::alerts::AlertController;
use crate::api::actor::{run_api_actor, ApiActor};
use crate::api::CCError;
use crate::config::Config;
use crate::device::ChannelName;
use crate::engine::main::Engine;
use crate::modes::ModeController;
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
    mode_controller: Rc<ModeController>,
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
        mode_controller: Rc<ModeController>,
    ) -> Self {
        Self {
            receiver,
            custom_sensors_repo,
            engine,
            config,
            overrides,
            alert_controller,
            mode_controller,
        }
    }

    /// Deletes the sensor unless something still reads it, then clears what other
    /// controllers stored about it.
    async fn delete(&self, custom_sensor_id: &ChannelName) -> Result<()> {
        let cs_device_uid = self.custom_sensors_repo.get_device_uid();
        let mut users = self
            .engine
            .custom_sensor_users(&cs_device_uid, custom_sensor_id)
            .await?;
        users.extend(
            self.alert_controller
                .alerts_watching(&cs_device_uid, custom_sensor_id)
                .into_iter()
                .map(|name| format!("Alert \"{name}\"")),
        );
        let sensor_label =
            self.overrides
                .resolve_channel_label(&cs_device_uid, custom_sensor_id, None);
        verify_not_in_use(&sensor_label, &users)?;
        self.custom_sensors_repo
            .delete_custom_sensor(custom_sensor_id)?;
        let save_result = self.config.save_config_file().await;
        // Cascade regardless of the save outcome: the sensor is
        // already gone from the repo and IDs are recycled, so a
        // future sensor reusing this ID must not inherit its name.
        if let Err(err) = self
            .overrides
            .remove_channel(&cs_device_uid, custom_sensor_id)
            .await
        {
            warn!(
                "Failed to remove name override for deleted sensor \
                {custom_sensor_id}: {err}"
            );
        }
        if let Err(err) = self
            .mode_controller
            .custom_sensor_deleted(&cs_device_uid, custom_sensor_id)
            .await
        {
            warn!(
                "Failed to save the Modes without deleted sensor \
                {custom_sensor_id}: {err}"
            );
        }
        save_result
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
                let result = self.delete(&custom_sensor_id).await;
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
        mode_controller: Rc<ModeController>,
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
            mode_controller,
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
    use crate::calibration::{CalibrationStore, FanStateMap};
    use crate::cc_fs;
    use crate::repositories::repository::{Repositories, Repository};
    use crate::setting::{
        CustomSensorKind, CustomSensorMetric, CustomSensorMixFunctionType, SensorSource,
    };
    use serial_test::serial;
    use std::collections::HashMap;
    use std::ops::Not;

    fn refusal(result: Result<()>) -> String {
        match result.unwrap_err().downcast::<CCError>() {
            Ok(CCError::UserError { msg }) => msg,
            other => panic!("expected a user error, got {other:?}"),
        }
    }

    const SENSOR_ID: &str = "sensor1";

    struct Harness {
        actor: CustomSensorActor,
        repo: Rc<CustomSensorsRepo>,
        modes: Rc<ModeController>,
        _overrides_dir: tempfile::TempDir,
    }

    impl Harness {
        fn has_sensor(&self) -> bool {
            self.repo.get_custom_sensor(SENSOR_ID).is_ok()
        }

        fn mode_has_lcd_setting(&self) -> bool {
            let modes = self.modes.get_modes();
            assert_eq!(modes.len(), 1);
            modes[0].all_device_settings.is_empty().not()
        }
    }

    fn mix_sensor(id: &str, source_device_uid: &str, source_name: &str) -> CustomSensor {
        CustomSensor {
            id: id.to_string(),
            metric: CustomSensorMetric::Temp,
            kind: CustomSensorKind::Mix {
                mix_function: CustomSensorMixFunctionType::Max,
                sources: vec![SensorSource {
                    device_uid: source_device_uid.to_string(),
                    name: source_name.to_string(),
                    weight: 1,
                }],
            },
            children: Vec::new(),
            parents: Vec::new(),
        }
    }

    /// An actor over one Custom Sensor that a Mode shows on an LCD. `alert_watches` adds an
    /// Alert on the sensor, `has_parent` a second sensor that reads nothing else.
    async fn harness(alert_watches: bool, has_parent: bool) -> Harness {
        let config = Rc::new(Config::init_default_config().unwrap());
        // The delete writes the overrides file, so it gets a directory of its own.
        let overrides_dir = tempfile::tempdir().unwrap();
        let overrides_file = overrides_dir.path().join("overrides.toml");
        let overrides = Rc::new(OverridesController::init_from(overrides_file).await);
        let mut repo =
            CustomSensorsRepo::new(Rc::clone(&config), vec![], Rc::clone(&overrides)).unwrap();
        repo.initialize_devices().await.unwrap();
        let cs_device_uid = repo.get_device_uid();
        // A source on a device that is not there still makes a valid sensor.
        repo.set_custom_sensor(mix_sensor(SENSOR_ID, "missing-device", "temp1"))
            .await
            .unwrap();
        if has_parent {
            repo.set_custom_sensor(mix_sensor("parent", &cs_device_uid, SENSOR_ID))
                .await
                .unwrap();
        }
        let repo = Rc::new(repo);
        let engine = Rc::new(Engine::new(
            Rc::new(HashMap::new()),
            &Rc::new(Repositories::default()),
            Rc::clone(&config),
            Rc::new(CalibrationStore::empty()),
            Rc::new(FanStateMap::new()),
            Rc::clone(&overrides),
        ));
        let watched_channel = if alert_watches { SENSOR_ID } else { "other" };
        let alerts = Rc::new(AlertController::for_test_watching(
            &cs_device_uid,
            watched_channel,
        ));
        let modes = Rc::new(ModeController::for_test_showing_on_lcd(
            &config,
            &cs_device_uid,
            SENSOR_ID,
        ));
        let (_sender, receiver) = mpsc::channel(1);
        let actor = CustomSensorActor::new(
            receiver,
            Rc::clone(&repo),
            engine,
            config,
            overrides,
            alerts,
            Rc::clone(&modes),
        );
        Harness {
            actor,
            repo,
            modes,
            _overrides_dir: overrides_dir,
        }
    }

    #[test]
    #[serial(modes_file)]
    fn a_watching_alert_refuses_the_delete() {
        // Goal: the delete asks the Alerts too, and a refusal leaves the sensor and the
        // Modes alone. Method: one Alert on the sensor, delete, read both back.
        cc_fs::test_runtime(async {
            let h = harness(true, false).await;

            let message = refusal(h.actor.delete(&SENSOR_ID.to_string()).await);

            assert!(message.contains("Alert \"Alert-watching\""), "{message}");
            assert!(h.has_sensor());
            assert!(h.mode_has_lcd_setting());
        });
    }

    #[test]
    #[serial(modes_file)]
    fn a_delete_strips_the_sensor_from_mode_lcd_settings() {
        // Goal: a Mode must not keep an LCD setting showing a sensor that is gone.
        // Method: nothing uses the sensor but a Mode's LCD, delete, read the Mode back.
        cc_fs::test_runtime(async {
            let h = harness(false, false).await;

            h.actor.delete(&SENSOR_ID.to_string()).await.unwrap();

            assert!(h.has_sensor().not());
            assert!(h.mode_has_lcd_setting().not());
        });
    }

    #[test]
    #[serial(modes_file)]
    fn a_delete_the_repo_refuses_strips_no_mode() {
        // Goal: the Modes are only stripped once the sensor is really deleted. Method: the
        // guard passes, but the repo refuses for the parent that reads this sensor alone.
        cc_fs::test_runtime(async {
            let h = harness(false, true).await;

            let message = refusal(h.actor.delete(&SENSOR_ID.to_string()).await);

            assert!(message.contains("only has this one child"), "{message}");
            assert!(h.has_sensor());
            assert!(h.mode_has_lcd_setting());
        });
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
