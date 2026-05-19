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
use crate::api::stats::{DeviceStatsDto, StatsResponse};
use crate::AllDevices;
use anyhow::Result;
use moro_local::Scope;
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::CancellationToken;

struct StatsActor {
    receiver: mpsc::Receiver<StatsMessage>,
    all_devices: AllDevices,
}

enum StatsMessage {
    GetAll {
        respond_to: oneshot::Sender<StatsResponse>,
    },
    ResetAll {
        respond_to: oneshot::Sender<StatsResponse>,
    },
}

impl StatsActor {
    pub fn new(receiver: mpsc::Receiver<StatsMessage>, all_devices: AllDevices) -> Self {
        Self {
            receiver,
            all_devices,
        }
    }

    fn snapshot(&self) -> StatsResponse {
        let mut devices = Vec::with_capacity(self.all_devices.len());
        for device_lock in self.all_devices.values() {
            let device = device_lock.borrow();
            devices.push(DeviceStatsDto {
                uid: device.uid.clone(),
                temps: device.stats().temps.clone(),
                channels: device.stats().channels.clone(),
            });
        }
        StatsResponse { devices }
    }
}

impl ApiActor<StatsMessage> for StatsActor {
    fn name(&self) -> &'static str {
        "StatsActor"
    }

    fn receiver(&mut self) -> &mut mpsc::Receiver<StatsMessage> {
        &mut self.receiver
    }

    async fn handle_message(&mut self, msg: StatsMessage) {
        match msg {
            StatsMessage::GetAll { respond_to } => {
                let _ = respond_to.send(self.snapshot());
            }
            StatsMessage::ResetAll { respond_to } => {
                for device_lock in self.all_devices.values() {
                    device_lock.borrow_mut().reset_stats();
                }
                let _ = respond_to.send(self.snapshot());
            }
        }
    }
}

#[derive(Clone)]
pub struct StatsHandle {
    sender: mpsc::Sender<StatsMessage>,
}

impl StatsHandle {
    pub fn new<'s>(
        all_devices: AllDevices,
        cancel_token: CancellationToken,
        main_scope: &'s Scope<'s, 's, Result<()>>,
    ) -> Self {
        let (sender, receiver) = mpsc::channel(10);
        let actor = StatsActor::new(receiver, all_devices);
        main_scope.spawn(run_api_actor(actor, cancel_token));
        Self { sender }
    }

    pub async fn all(&self) -> StatsResponse {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(StatsMessage::GetAll { respond_to: tx })
            .await;
        rx.await.unwrap_or_default()
    }

    pub async fn reset_all(&self) -> StatsResponse {
        let (tx, rx) = oneshot::channel();
        let _ = self
            .sender
            .send(StatsMessage::ResetAll { respond_to: tx })
            .await;
        rx.await.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device::{
        ChannelDataType, ChannelStatus, Device, DeviceInfo, DeviceType, Status, TempStatus,
    };
    use chrono::Local;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    fn make_status(temp: f64, duty: f64, rpm: u32) -> Status {
        Status {
            timestamp: Local::now(),
            temps: vec![TempStatus {
                name: "cpu".to_string(),
                temp,
            }],
            channels: vec![ChannelStatus {
                name: "fan1".to_string(),
                duty: Some(duty),
                rpm: Some(rpm),
                freq: None,
                watts: None,
                pwm_mode: None,
            }],
        }
    }

    fn make_all_devices() -> AllDevices {
        let mut device = Device::new(
            "test-device".to_string(),
            DeviceType::Hwmon,
            0,
            None,
            DeviceInfo::default(),
            Some("test-id-0".to_string()),
            1.0,
        );
        device.initialize_status_history_with(make_status(50.0, 60.0, 1500), 1.0);
        let uid = device.uid.clone();
        let mut map = HashMap::with_capacity(1);
        map.insert(uid, Rc::new(RefCell::new(device)));
        Rc::new(map)
    }

    /// snapshot() must return one DTO per device with the channel and
    /// temp maps populated from each device's stats(), without mutating
    /// any device state.
    #[test]
    fn snapshot_returns_one_dto_per_device() {
        let all_devices = make_all_devices();
        let (_tx, rx) = mpsc::channel(1);
        let actor = StatsActor::new(rx, all_devices.clone());
        let snap = actor.snapshot();
        assert_eq!(snap.devices.len(), 1);
        let dto = &snap.devices[0];
        assert_eq!(dto.temps.get("cpu").unwrap().count, 1);
        let fan1 = dto.channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().avg, 60.0);
        assert_eq!(fan1.get(&ChannelDataType::Rpm).unwrap().avg, 1500.0);
        // Source device untouched.
        assert_eq!(
            all_devices
                .values()
                .next()
                .unwrap()
                .borrow()
                .stats()
                .temps
                .get("cpu")
                .unwrap()
                .count,
            1
        );
    }

    /// Driving the actor's ResetAll handler must reset each device's
    /// stats and the returned snapshot reflects the post-reset state
    /// (count=1, min=max=avg=most-recent-value).
    #[tokio::test(flavor = "current_thread")]
    async fn reset_all_resets_each_device_and_returns_new_snapshot() {
        let all_devices = make_all_devices();
        // Push a second status so count = 2 before reset.
        for dev in all_devices.values() {
            dev.borrow_mut().set_status(make_status(80.0, 90.0, 2400));
        }
        let (_tx, rx) = mpsc::channel(1);
        let mut actor = StatsActor::new(rx, all_devices.clone());
        let (respond_to, response) = oneshot::channel();
        actor
            .handle_message(StatsMessage::ResetAll { respond_to })
            .await;
        let snap = response.await.expect("respond_to should fire");
        let dto = &snap.devices[0];
        assert_eq!(dto.temps.get("cpu").unwrap().count, 1);
        assert_eq!(dto.temps.get("cpu").unwrap().min, 80.0);
        let fan1 = dto.channels.get("fan1").unwrap();
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().count, 1);
        assert_eq!(fan1.get(&ChannelDataType::Duty).unwrap().avg, 90.0);
    }
}
