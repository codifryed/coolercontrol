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

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace};
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::RwLock;
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, SpeedOptions, Status, TempInfo,
    TempStatus, UID,
};
use crate::repositories::hwmon::{devices, fans, temps};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{LcdSettings, LightingSettings, TempSource};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum HwmonChannelType {
    Fan,
    Temp,
    Load,
    Freq,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonChannelInfo {
    pub hwmon_type: HwmonChannelType,
    pub number: u8,
    pub pwm_enable_default: Option<u8>,
    pub name: String,
    pub label: Option<String>,
    pub pwm_mode_supported: bool,
}

impl Default for HwmonChannelInfo {
    fn default() -> Self {
        Self {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            pwm_enable_default: None,
            name: String::new(),
            label: None,
            pwm_mode_supported: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonDriverInfo {
    pub name: String,
    pub path: PathBuf,
    pub model: Option<String>,
    pub u_id: UID,
    pub channels: Vec<HwmonChannelInfo>,
}

/// A Repository for Hwmon Devices
pub struct HwmonRepo {
    config: Arc<Config>,
    devices: HashMap<UID, (DeviceLock, Arc<HwmonDriverInfo>)>,
    preloaded_statuses: RwLock<HashMap<u8, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
}

impl HwmonRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        Ok(Self {
            config,
            devices: HashMap::new(),
            preloaded_statuses: RwLock::new(HashMap::new()),
        })
    }

    /// Maps driver infos to our Devices
    /// `ThinkPads` need special handling, see:
    /// https://www.kernel.org/doc/html/latest/admin-guide/laptops/thinkpad-acpi.html#fan-control-and-monitoring-fan-speed-fan-enable-disable
    async fn map_into_our_device_model(&mut self, hwmon_drivers: Vec<HwmonDriverInfo>) {
        for (index, driver) in hwmon_drivers.into_iter().enumerate() {
            let temps = driver
                .channels
                .iter()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: channel
                                .label
                                .as_ref()
                                .map(|l| l.to_title_case())
                                .unwrap_or_else(|| channel.name.to_title_case()),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut channels = HashMap::new();
            let mut thinkpad_fan_control = (
                driver.name == devices::THINKPAD_DEVICE_NAME
                // first check if this is a ThinkPad
            )
                .then_some(false);
            for channel in &driver.channels {
                if channel.hwmon_type != HwmonChannelType::Fan {
                    continue; // only Fan channels currently have controls
                }
                if thinkpad_fan_control.is_some() && channel.number == 1 {
                    thinkpad_fan_control = Some(
                        // verify if fan control for this ThinkPad is enabled or not:
                        fans::set_pwm_enable(&2, &driver.path, channel)
                            .await
                            .is_ok(),
                    );
                }
                let channel_info = ChannelInfo {
                    label: channel.label.clone(),
                    speed_options: Some(SpeedOptions {
                        profiles_enabled: false,
                        fixed_enabled: true,
                        manual_profiles_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                channels.insert(channel.name.clone(), channel_info);
            }
            let device_info = DeviceInfo {
                temps,
                channels,
                temp_min: 0,
                temp_max: 100,
                profile_max_length: 21,
                model: driver.model.clone(),
                thinkpad_fan_control,
                ..Default::default()
            };
            let type_index = (index + 1) as u8;
            let channel_statuses = fans::extract_fan_statuses(&driver).await;
            let temp_statuses = temps::extract_temp_statuses(&driver).await;
            self.preloaded_statuses.write().await.insert(
                type_index,
                (channel_statuses.clone(), temp_statuses.clone()),
            );
            let mut device = Device::new(
                driver.name.clone(),
                DeviceType::Hwmon,
                type_index,
                None,
                device_info,
                Some(driver.u_id.clone()),
            );
            let status = Status {
                channels: channel_statuses,
                temps: temp_statuses,
                ..Default::default()
            };
            device.initialize_status_history_with(status);
            let cc_device_setting = self
                .config
                .get_cc_settings_for_device(&device.uid)
                .await
                .ok()
                .flatten();
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!(
                    "Skipping disabled device: {} with UID: {}",
                    device.name, device.uid
                );
                continue; // skip loading this device into the device list
            }
            self.devices.insert(
                device.uid.clone(),
                (Arc::new(RwLock::new(device)), Arc::new(driver)),
            );
        }
    }

    /// Gets the info necessary to apply setting to the device channel
    fn get_hwmon_info(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<(&Arc<HwmonDriverInfo>, &HwmonChannelInfo)> {
        let (_, hwmon_driver) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        let channel_info = hwmon_driver
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| format!("Searching for channel name: {channel_name}"))?;
        Ok((hwmon_driver, channel_info))
    }
}

#[async_trait]
impl Repository for HwmonRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Hwmon
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();

        let base_paths = devices::find_all_hwmon_device_paths();
        if base_paths.is_empty() {
            return Err(anyhow!(
                "No HWMon devices were found, try running sensors-detect"
            ));
        }
        let mut hwmon_drivers: Vec<HwmonDriverInfo> = Vec::new();
        for path in base_paths {
            let device_name = devices::get_device_name(&path).await;
            if devices::is_already_used_by_other_repo(&device_name) {
                continue;
            }
            let mut channels = vec![];
            match fans::init_fans(&path, &device_name).await {
                Ok(fans) => channels.extend(fans),
                Err(err) => error!("Error initializing Hwmon Fans: {}", err),
            };
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(temps),
                Err(err) => error!("Error initializing Hwmon Temps: {}", err),
            };
            if channels.is_empty() {
                // we only add hwmon drivers that have usable data
                continue;
            }
            let model = devices::get_device_model_name(&path).await;
            let u_id = devices::get_device_unique_id(&path).await;
            let hwmon_driver_info = HwmonDriverInfo {
                name: device_name,
                path,
                model,
                u_id,
                channels,
            };
            hwmon_drivers.push(hwmon_driver_info);
        }
        devices::handle_duplicate_device_names(&mut hwmon_drivers).await;
        // re-sorted by name to help keep some semblance of order after reboots & device changes.
        hwmon_drivers.sort_by(|d1, d2| d1.name.cmp(&d2.name));

        self.map_into_our_device_model(hwmon_drivers).await;

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(
                uid.clone(),
                (device.read().await.clone(), hwmon_info.clone()),
            );
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Hwmon Devices: {:?}", init_devices);
        } else {
            info!(
                "Initialized Hwmon Devices: {:?}",
                init_devices
                    .iter()
                    .map(|d| d.1 .0.name.clone())
                    .collect::<Vec<String>>()
            );
        }
        trace!(
            "Time taken to initialize all Hwmon devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("HWMON Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices
            .values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();

        let mut tasks = Vec::new();
        for (device_lock, driver) in self.devices.values() {
            let self = Arc::clone(&self);
            let device_lock = Arc::clone(device_lock);
            let driver = Arc::clone(driver);
            let join_handle = tokio::task::spawn(async move {
                let device_id = device_lock.read().await.type_index;
                self.preloaded_statuses.write().await.insert(
                    device_id,
                    (
                        fans::extract_fan_statuses(&driver).await,
                        temps::extract_temp_statuses(&driver).await,
                    ),
                );
            });
            tasks.push(join_handle);
        }
        for task in tasks {
            if let Err(err) = task.await {
                error!("{}", err);
            }
        }
        trace!(
            "STATUS PRELOAD Time taken for all HWMON devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (device, _) in self.devices.values() {
            let preloaded_statuses_map = self.preloaded_statuses.read().await;
            let preloaded_statuses = preloaded_statuses_map.get(&device.read().await.type_index);
            if preloaded_statuses.is_none() {
                error!(
                    "There is no status preloaded for this device: {}",
                    device.read().await.type_index
                );
                continue;
            }
            let (channels, temps) = preloaded_statuses.unwrap().clone();
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            trace!(
                "Hwmon device: {} status was updated with: {:?}",
                device.read().await.name,
                status
            );
            device.write().await.set_status(status);
        }
        trace!(
            "STATUS SNAPSHOT Time taken for all HWMON devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (_, hwmon_driver) in self.devices.values() {
            for channel_info in &hwmon_driver.channels {
                if channel_info.hwmon_type != HwmonChannelType::Fan {
                    continue;
                }
                fans::set_pwm_enable_to_default(&hwmon_driver.path, channel_info).await?;
            }
        }
        info!("HWMON Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let (hwmon_driver, channel_info) = self.get_hwmon_info(device_uid, channel_name)?;
        debug!(
            "Applying HWMON device: {} channel: {}; Resetting to Original fan control mode",
            device_uid, channel_name
        );
        fans::set_pwm_enable_to_default(&hwmon_driver.path, channel_info).await
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        let (hwmon_driver, channel_info) = self.get_hwmon_info(device_uid, channel_name)?;
        debug!(
            "Applying HWMON device: {} channel: {}; Fixed Speed: {}",
            device_uid, channel_name, speed_fixed
        );
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {}", speed_fixed));
        }
        if speed_fixed == 100
            && hwmon_driver.name == devices::THINKPAD_DEVICE_NAME
            && self.config.get_settings().await?.thinkpad_full_speed
        {
            fans::set_thinkpad_to_full_speed(&hwmon_driver.path, channel_info).await
        } else {
            fans::set_pwm_duty(&hwmon_driver.path, channel_info, speed_fixed).await
        }
    }

    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Speed Profiles are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Lighting settings are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying LCD settings are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        device_uid: &UID,
        channel_name: &str,
        pwm_mode: u8,
    ) -> Result<()> {
        let (hwmon_driver, channel_info) = self.get_hwmon_info(device_uid, channel_name)?;
        info!(
            "Applying HWMON device: {} channel: {}; PWM Mode: {}",
            device_uid, channel_name, pwm_mode
        );
        fans::set_pwm_mode(&hwmon_driver.path, channel_info, Some(pwm_mode)).await
    }
}
