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
use crate::config::Config;
use crate::device::DeviceType;
use crate::repositories::repository::Repository;
use crate::setting::Setting;
use crate::speed_scheduler::SpeedScheduler;

pub type ReposByType = HashMap<DeviceType, Arc<dyn Repository>>;

pub struct DeviceCommander {
    all_devices: AllDevices,
    repos: ReposByType,
    pub speed_scheduler: Arc<SpeedScheduler>,
}

impl DeviceCommander {
    pub fn new(all_devices: AllDevices, repos: Repos, config: Arc<Config>) -> Self {
        let mut repos_by_type = HashMap::new();
        for repo in repos.iter() {
            match repo.device_type() {
                DeviceType::CPU => repos_by_type.insert(DeviceType::CPU, Arc::clone(repo)),
                DeviceType::GPU => repos_by_type.insert(DeviceType::GPU, Arc::clone(repo)),
                DeviceType::Liquidctl => repos_by_type.insert(DeviceType::Liquidctl, Arc::clone(repo)),
                DeviceType::Hwmon => repos_by_type.insert(DeviceType::Hwmon, Arc::clone(repo)),
                DeviceType::Composite => repos_by_type.insert(DeviceType::Composite, Arc::clone(repo)),
            };
        }
        let speed_scheduler = Arc::new(SpeedScheduler::new(
            all_devices.clone(),
            repos_by_type.clone(),
            config,
        ));
        DeviceCommander { all_devices, repos: repos_by_type, speed_scheduler }
    }

    pub async fn set_setting(&self, device_uid: &String, setting: &Setting) -> Result<()> {
        if let Some(device_lock) = self.all_devices.get(device_uid) {
            let device_type = device_lock.read().await.d_type.clone();
            return if let Some(repo) = self.repos.get(&device_type) {
                if let Some(true) = setting.reset_to_default {
                    self.speed_scheduler.clear_channel_setting(device_uid, &setting.channel_name).await;
                    if device_type == DeviceType::Hwmon || device_type == DeviceType::GPU {
                        repo.apply_setting(device_uid, setting).await
                    } else {
                        Ok(()) // nothing to actually set in this case, just clear settings.
                    }
                } else if setting.speed_fixed.is_some() {
                    self.speed_scheduler.clear_channel_setting(device_uid, &setting.channel_name).await;
                    repo.apply_setting(device_uid, setting).await
                } else if setting.lighting.is_some() {
                    repo.apply_setting(device_uid, setting).await
                } else if setting.speed_profile.is_some() {
                    let speed_options = device_lock.read().await
                        .info.as_ref().with_context(|| "Looking for Device Info")?
                        .channels.get(&setting.channel_name).with_context(|| "Looking for Channel Info")?
                        .speed_options.clone().with_context(|| "Looking for Channel Speed Options")?;
                    if speed_options.profiles_enabled {
                        repo.apply_setting(device_uid, setting).await
                    } else if let None = setting.temp_source {
                        Err(anyhow!("A Temp Source must be set when scheduling a Speed Profile for this device: {}", device_uid))
                    } else if (
                        &setting.temp_source.as_ref().unwrap().device_uid == device_uid
                            && speed_options.manual_profiles_enabled)
                        || &setting.temp_source.as_ref().unwrap().device_uid != device_uid {
                        self.speed_scheduler.schedule_setting(device_uid, setting).await
                    } else {
                        Err(anyhow!("Speed Profiles not enabled for this device: {}", device_uid))
                    }
                } else if setting.lcd.is_some() {
                    let has_lcd_modes = !device_lock.read().await
                        .info.as_ref().with_context(|| "Looking for Device Info")?
                        .channels.get(&setting.channel_name).with_context(|| "Looking for Channel Info")?
                        .lcd_modes.is_empty();
                    if has_lcd_modes {
                        repo.apply_setting(device_uid, setting).await
                    } else {
                        Err(anyhow!("LCD Screen modes not enabled for this device: {}", device_uid))
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

    /// This is used to reinitialize liquidctl devices after waking from sleep
    pub async fn reinitialize_devices(&self) {
        if let Some(liquidctl_repo) = self.repos.get(&DeviceType::Liquidctl) {
            liquidctl_repo.reinitialize_devices().await;
        }
    }
}