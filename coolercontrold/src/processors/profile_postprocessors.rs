/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
 * |
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 * |
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 * |
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */


use std::collections::{HashMap, VecDeque};
use async_trait::async_trait;

use log::trace;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

use crate::AllDevices;
use crate::device::UID;
use crate::processors::{Processor, SpeedProfileData};
use crate::setting::ProfileType;

const MAX_SAMPLE_SIZE: usize = 20;
const APPLY_DUTY_THRESHOLD: u8 = 2;
const MAX_UNDER_THRESHOLD_COUNTER: usize = 5;
const MAX_UNDER_THRESHOLD_CURRENT_DUTY_COUNTER: usize = 2;

/// This processor keeps a set of last-applied-duties so that we don't apply the same duty
/// that's already set. It also handles improvements for edge cases.
pub struct DutyThresholdPostProcessor {
    all_devices: AllDevices,
    scheduled_settings_metadata: RwLock<HashMap<UID, HashMap<String, SettingMetadata>>>,
}

impl DutyThresholdPostProcessor {
    pub fn new(all_devices: AllDevices) -> Self {
        Self {
            all_devices,
            scheduled_settings_metadata: RwLock::new(HashMap::new()),
        }
    }

    async fn duty_is_above_threshold(&self, device_uid: &UID, channel_name: &str, duty_to_set: u8) -> bool {
        if self.scheduled_settings_metadata.read().await[device_uid][channel_name]
            .last_manual_speeds_set.is_empty() {
            return true;
        }
        let last_duty = self.get_appropriate_last_duty(device_uid, channel_name).await;
        let diff_to_last_duty = duty_to_set.abs_diff(last_duty);
        let under_threshold_counter = self.scheduled_settings_metadata.read()
            .await[device_uid][channel_name]
            .under_threshold_counter;
        let threshold = if under_threshold_counter < MAX_UNDER_THRESHOLD_COUNTER {
            APPLY_DUTY_THRESHOLD
        } else { 0 };
        diff_to_last_duty > threshold
    }

    /// This either uses the last applied duty as a comparison or the actual current duty.
    /// This handles situations where the last applied duty is not what the actual duty is
    /// in some circumstances, such as some when external programs are also trying to manipulate the duty.
    /// There needs to be a delay here (currently 2 seconds), as the device's duty often doesn't change instantaneously.
    async fn get_appropriate_last_duty(&self, device_uid: &UID, channel_name: &str) -> u8 {
        let metadata = &self.scheduled_settings_metadata.read()
            .await[device_uid][channel_name];
        if metadata.under_threshold_counter < MAX_UNDER_THRESHOLD_CURRENT_DUTY_COUNTER {
            metadata.last_manual_speeds_set.back().unwrap().clone()  // already checked to exist
        } else {
            let current_duty = self.all_devices[device_uid].read().await
                .status_history.iter().last()
                .and_then(|status| status.channels.iter()
                    .filter(|channel_status| channel_status.name == channel_name)
                    .find_map(|channel_status| channel_status.duty)
                );
            if let Some(duty) = current_duty {
                duty.round() as u8
            } else {
                metadata.last_manual_speeds_set.back().unwrap().clone()
            }
        }
    }
}

#[async_trait]
impl Processor for DutyThresholdPostProcessor {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool {
        data.profile.p_type == ProfileType::Graph
            && data.duty.is_some()
    }

    async fn init_state(&self, device_uid: &UID, channel_name: &str) {
        self.scheduled_settings_metadata.write().await
            .entry(device_uid.clone())
            .or_insert_with(HashMap::new)
            .insert(channel_name.to_string(), SettingMetadata::new());
    }

    async fn clear_state(&self, device_uid: &UID, channel_name: &str) {
        if let Some(device_channel_settings) =
            self.scheduled_settings_metadata.write().await
                .get_mut(device_uid) {
            device_channel_settings.remove(channel_name);
        }
    }

    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData {
        if self.duty_is_above_threshold(&data.device_uid, &data.channel_name, data.duty.unwrap()).await {
            {
                let mut metadata_lock = self.scheduled_settings_metadata.write().await;
                let metadata = metadata_lock
                    .get_mut(&data.device_uid).unwrap()
                    .get_mut(&data.channel_name).unwrap();
                metadata.last_manual_speeds_set.push_back(data.duty.unwrap());
                metadata.under_threshold_counter = 0;
                if metadata.last_manual_speeds_set.len() > MAX_SAMPLE_SIZE {
                    metadata.last_manual_speeds_set.pop_front();
                }
            }
            data
        } else {
            data.duty = None;
            self.scheduled_settings_metadata.write().await
                .get_mut(&data.device_uid).unwrap()
                .get_mut(&data.channel_name).unwrap()
                .under_threshold_counter += 1;
            trace!("Duty not above threshold to be applied to device. Skipping");
            trace!(
                "Last applied duties: {:?}",self.scheduled_settings_metadata.read().await
                .get(&data.device_uid).unwrap().get(&data.channel_name).unwrap()
                .last_manual_speeds_set
            );
            data
        }
    }
}

/// This is used to help in deciding exactly when to apply a setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SettingMetadata {
    /// (internal use) the last duty speeds that we set manually. This keeps track of applied settings
    /// to not re-apply the same setting over and over again needlessly. eg: [20, 25, 30]
    #[serde(skip_serializing, skip_deserializing)]
    pub last_manual_speeds_set: VecDeque<u8>,

    /// (internal use) a counter to be able to know how many times the to-be-applied duty was under
    /// the apply-threshold. This helps mitigate issues where the duty is 1% off target for a long time.
    #[serde(skip_serializing, skip_deserializing)]
    pub under_threshold_counter: usize,
}

impl SettingMetadata {
    pub fn new() -> Self {
        Self {
            last_manual_speeds_set: VecDeque::with_capacity(MAX_SAMPLE_SIZE + 1),
            under_threshold_counter: 0,
        }
    }
}
