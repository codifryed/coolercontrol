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

use std::hash::{Hash, Hasher};
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::device::{Duty, Temp};
use crate::setting::{Function, ProfileUID, TempSource};

mod commanders;
pub mod processors;
mod utils;
pub mod settings;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedGraphProfile {
    profile_uid: ProfileUID,
    speed_profile: Vec<(Temp, Duty)>,
    temp_source: TempSource,
    function: Function,
}

impl Default for NormalizedGraphProfile {
    fn default() -> Self {
        Self {
            profile_uid: String::default(),
            speed_profile: Vec::new(),
            temp_source: TempSource {
                temp_name: String::default(),
                device_uid: String::default(),
            },
            function: Default::default(),
        }
    }
}

impl PartialEq for NormalizedGraphProfile {
    /// Only compare ProfileUID
    /// This allows us to update the Profile settings easily, and the UID is what matters anyway.
    fn eq(&self, other: &Self) -> bool {
        self.profile_uid == other.profile_uid
    }
}

impl Eq for NormalizedGraphProfile {}

impl Hash for NormalizedGraphProfile {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.profile_uid.hash(state)
    }
}

#[async_trait]
trait Processor: Send + Sync {
    async fn is_applicable(&self, data: &SpeedProfileData) -> bool;
    async fn init_state(&self, profile_uid: &ProfileUID);
    async fn clear_state(&self, profile_uid: &ProfileUID);
    async fn process<'a>(&'a self, data: &'a mut SpeedProfileData) -> &'a mut SpeedProfileData;
}

#[derive(Debug, Clone)]
struct SpeedProfileData {
    temp: Option<Temp>,
    duty: Option<Duty>,
    profile: Arc<NormalizedGraphProfile>,
    processing_started: bool,
    /// When this is triggered by the SafetyLatchProcessor, all subsequent processors
    /// MUST return a temp or duty value
    safety_latch_triggered: bool,
}

impl SpeedProfileData {
    async fn apply<'a>(&'a mut self, processor: &'a Arc<dyn Processor>) -> &'a mut Self {
        if processor.is_applicable(self).await {
            processor.process(self).await
        } else {
            self
        }
    }

    fn return_processed_duty(&self) -> Option<u8> {
        self.duty
    }

    // could use in future for special cases:
    // async fn apply_if(&mut self, processor: Arc<dyn Processor>, predicate: impl Fn(&Self) -> bool) -> Self {
    //     if predicate() {
    //         processor.process(self).await
    //     } else {
    //         self
    //     }
    // }
}
