/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2022  Guy Boldon
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
 ******************************************************************************/

use std::collections::HashMap;
use std::sync::Arc;
use anyhow::{anyhow, Context, Result};
use crate::{AllDevices, Repos};
use crate::device::DeviceType;
use crate::repositories::repository::Repository;
use crate::setting::Setting;

pub struct DeviceCommander {
    all_devices: AllDevices,
    repos: HashMap<DeviceType, Arc<dyn Repository>>,
    // speed_scheduler: SpeedScheduler,
}

impl DeviceCommander {
    pub fn new(all_devices: AllDevices, repos: Repos) -> Self {
        let mut repos_map = HashMap::new();
        for repo in repos.iter() {
            match repo.device_type() {
                DeviceType::CPU => repos_map.insert(DeviceType::CPU, repo.clone()),
                DeviceType::GPU => repos_map.insert(DeviceType::GPU, repo.clone()),
                DeviceType::Liquidctl => repos_map.insert(DeviceType::Liquidctl, repo.clone()),
                DeviceType::Hwmon => repos_map.insert(DeviceType::Hwmon, repo.clone()),
                DeviceType::Composite => repos_map.insert(DeviceType::Composite, repo.clone()),
            };
        }
        DeviceCommander { all_devices, repos: repos_map }
    }

    pub async fn set_setting(&self, device_uid: &String, setting: &Setting) -> Result<()> {
        if let Some(device_lock) = self.all_devices.get(device_uid) {
            let device_type = device_lock.read().await.d_type.clone();
            return if let Some(repo) = self.repos.get(&device_type) {
                if let Some(true) = setting.reset_to_default {
                    repo.apply_setting(device_uid, setting).await
                } else if setting.speed_fixed.is_some() || setting.lighting.is_some() {
                    repo.apply_setting(device_uid, setting).await
                } else if setting.speed_profile.is_some() {
                    let speed_options = device_lock.read().await
                        .info.as_ref().with_context(|| "Looking for Device Info")?
                        .channels.get(&setting.channel_name).with_context(|| "Looking for Channel")?
                        .speed_options.clone().with_context(|| "Looking for Speed Options")?;
                    if speed_options.profiles_enabled {
                        repo.apply_setting(device_uid, setting).await
                    } else if speed_options.manual_profiles_enabled {
                        todo!("Speed Scheduler")
                    } else {
                        Err(anyhow!("Speed Profiles not enabled for this device: {}", device_uid))
                    }
                } else {
                    Err(anyhow!("Invalid Setting combination: {:?}", setting))
                }
            } else {
                Err(anyhow!("Repository: {:?} for device is currently not running!", device_type))
            };
        }
        {
            Err(anyhow!("Device Not Found: {}", device_uid))
        }
    }
}