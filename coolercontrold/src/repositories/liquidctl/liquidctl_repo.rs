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


use std::borrow::Borrow;
use std::clone::Clone;
use std::collections::HashMap;
use std::str::FromStr;
use std::string::ToString;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use const_format::concatcp;
use log::{debug, error, info, warn};
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::sleep;
use zbus::export::futures_util::future::join_all;

use crate::config::Config;
use crate::Device;
use crate::device::{DeviceType, LcInfo, Status, UID};
use crate::repositories::liquidctl::base_driver::BaseDriver;
use crate::repositories::liquidctl::device_mapper::DeviceMapper;
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::Setting;

pub const LIQCTLD_ADDRESS: &str = "http://127.0.0.1:11986";
const LIQCTLD_TIMEOUT_SECONDS: u64 = 10;
const LIQCTLD_HANDSHAKE: &str = concatcp!(LIQCTLD_ADDRESS, "/handshake");
const LIQCTLD_DEVICES: &str = concatcp!(LIQCTLD_ADDRESS, "/devices");
const LIQCTLD_LEGACY690: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/legacy690");
const LIQCTLD_INITIALIZE: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/initialize");
const LIQCTLD_STATUS: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/status");
const LIQCTLD_FIXED_SPEED: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/speed/fixed");
const LIQCTLD_SPEED_PROFILE: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/speed/profile");
const LIQCTLD_COLOR: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/color");
const LIQCTLD_SCREEN: &str = concatcp!(LIQCTLD_ADDRESS, "/devices/{}/screen");
const LIQCTLD_QUIT: &str = concatcp!(LIQCTLD_ADDRESS, "/quit");
const PATTERN_TEMP_SOURCE_NUMBER: &str = r"(?P<number>\d+)$";

type LCStatus = Vec<(String, String, String)>;

pub struct LiquidctlRepo {
    config: Arc<Config>,
    client: Client,
    device_mapper: DeviceMapper,
    devices: HashMap<UID, DeviceLock>,
    preloaded_statuses: RwLock<HashMap<u8, LCStatus>>,
}

impl LiquidctlRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(LIQCTLD_TIMEOUT_SECONDS))
            .build()?;
        Self::establish_connection(&client).await?;
        info!("Communication established with Liqctld.");
        Ok(LiquidctlRepo {
            config,
            client,
            device_mapper: DeviceMapper::new(),
            devices: HashMap::new(),
            preloaded_statuses: RwLock::new(HashMap::new()),
        })
    }

    async fn establish_connection(client: &Client) -> Result<()> {
        let mut retry_count: u8 = 0;
        while retry_count < 5 {
            match client.get(LIQCTLD_HANDSHAKE).send().await {
                Ok(response) =>
                    return match response.json::<HandshakeResponse>().await {
                        Ok(handshake_response) => if handshake_response.shake {
                            Ok(())
                        } else {
                            Err(anyhow!(
                                    "Incorrect Handshake confirmation. Shake: {}",
                                    handshake_response.shake)
                            )
                        }
                        Err(err) => Err(anyhow!(err))
                    },
                Err(err) =>
                    error!(
                    "Could not establish communication with coolercontrol-liqctld socket connection, retry #{}. \n{}",
                    retry_count + 1, err
                )
            };
            sleep(Duration::from_secs(1)).await;
            retry_count += 1;
        }
        bail!("Failed to connect to coolercontrol-liqctld after {} tries", retry_count);
    }

    pub async fn get_devices(&mut self) -> Result<()> {
        let devices_response = self.client.get(LIQCTLD_DEVICES)
            .send().await?
            .json::<DevicesResponse>().await?;
        let mut preloaded_status_map = self.preloaded_statuses.write().await;
        for device_response in devices_response.devices {
            let driver_type = match self.map_driver_type(&device_response) {
                None => {
                    warn!("Device is currently not supported: {:?}", device_response.device_type);
                    continue;
                }
                Some(d_type) => d_type
            };
            preloaded_status_map.insert(device_response.id, Vec::new());
            let device_info = self.device_mapper
                .extract_info(&driver_type, &device_response.id, &device_response.properties);
            let mut device = Device::new(
                device_response.description,
                DeviceType::Liquidctl,
                device_response.id,
                Some(LcInfo {
                    driver_type,
                    firmware_version: None,
                    unknown_asetek: false,
                }),
                Some(device_info),
                None,
                device_response.serial_number,
            );
            let cc_device_setting = self.config.get_cc_settings_for_device(&device.uid).await?;
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!("Skipping disabled device: {} with UID: {}", device.name, device.uid);
                continue; // skip loading this device into the device list
            }
            self.check_for_legacy_690(&mut device).await?;
            self.devices.insert(
                device.uid.clone(),
                Arc::new(RwLock::new(device)),
            );
        }
        debug!("List of received Devices: {:?}", self.devices);
        Ok(())
    }

    fn map_driver_type(&self, device: &DeviceResponse) -> Option<BaseDriver> {
        BaseDriver::from_str(device.device_type.as_str())
            .ok()
            .filter(|driver| self.device_mapper.is_device_supported(driver))
    }

    async fn call_status(&self, device_id: &u8) -> Result<LCStatus> {
        let status_response = self.client
            .get(LIQCTLD_STATUS.replace("{}", device_id.to_string().as_str()))
            .send().await
            .with_context(|| format!("Trying to get status for device_id: {}", device_id))?
            .json::<StatusResponse>().await?;
        Ok(status_response.status)
    }

    fn map_status(&self,
                  driver_type: &BaseDriver,
                  lc_statuses: &LCStatus,
                  device_index: &u8,
    ) -> Status {
        let mut status_map: HashMap<String, String> = HashMap::new();
        for lc_status in lc_statuses {
            status_map.insert(lc_status.0.to_lowercase(), lc_status.1.clone());
        }
        self.device_mapper.extract_status(driver_type, &status_map, device_index)
    }

    async fn call_initialize_concurrently(&self) {
        let mut futures = vec![];
        for device in self.devices.values() {
            futures.push(self.call_initialize_per_device(device));
        }
        let results: Vec<Result<()>> = join_all(futures).await;
        for result in results {
            match result {
                Ok(_) => {}
                Err(err) => error!("Error getting initializing device: {}", err)
            }
        }
    }

    async fn call_initialize_per_device(&self, device_lock: &DeviceLock) -> Result<()> {
        let mut device = device_lock.write().await;
        let status_response = self.client.borrow()
            .post(LIQCTLD_INITIALIZE
                .replace("{}", device.type_index.to_string().as_str())
            )
            .json(&InitializeRequest { pump_mode: None })
            .send().await?
            .json::<StatusResponse>().await?;
        let device_index = device.type_index;
        let mut lc_info = device.lc_info.as_mut().expect("This should always be set for LIQUIDCTL devices");
        let init_status = self.map_status(
            &lc_info.driver_type,
            &status_response.status,
            &device_index,
        );
        lc_info.firmware_version = init_status.firmware_version.clone();
        Ok(())
    }

    async fn call_reinitialize_concurrently(&self) {
        let mut futures = vec![];
        for device in self.devices.values() {
            futures.push(self.call_reinitialize_per_device(device));
        }
        let results: Vec<Result<()>> = join_all(futures).await;
        for result in results {
            match result {
                Ok(_) => {}
                Err(err) => error!("Error reinitializing device: {}", err)
            }
        }
    }

    async fn call_reinitialize_per_device(&self, device_lock: &DeviceLock) -> Result<()> {
        let device = device_lock.read().await;
        let _ = self.client.borrow()
            .post(LIQCTLD_INITIALIZE
                .replace("{}", device.type_index.to_string().as_str())
            )
            .json(&InitializeRequest { pump_mode: None })  // pump_modes will be set after reinitializing
            .send().await?
            .json::<StatusResponse>().await?;
        Ok(())
    }

    async fn check_for_legacy_690(&self, device: &mut Device) -> Result<()> {
        let mut lc_info = device.lc_info.as_mut().expect("Should be present");
        if lc_info.driver_type == BaseDriver::Modern690Lc {
            if let Some(is_legacy690) = self.config.legacy690_ids().await?.get(&device.uid) {
                if *is_legacy690 {
                    let device_response = self.client.borrow()
                        .put(LIQCTLD_LEGACY690
                            .replace("{}", device.type_index.to_string().as_str())
                        )
                        .send().await?
                        .json::<DeviceResponse>().await?;
                    device.name = device_response.description.clone();
                    lc_info.driver_type = self.map_driver_type(&device_response)
                        .expect("Should be Legacy690Lc");
                    device.info = Some(
                        self.device_mapper
                            .extract_info(&lc_info.driver_type, &device_response.id, &device_response.properties)
                    );
                }
                // if is_legacy690 is false, then Modern690Lc is correct, nothing to do.
            } else {
                // if there is no setting for this device then we want to signal a request for
                // this info from the user.
                lc_info.unknown_asetek = true;
            }
        }
        Ok(())
    }

    async fn set_fixed_speed(&self, setting: &Setting, device_data: &CachedDeviceData) -> Result<()> {
        let fixed_speed = setting.speed_fixed.with_context(|| "speed_fixed should be present")?;
        if device_data.driver_type == BaseDriver::HydroPlatinum && setting.channel_name == "pump" {
            // limits from tested Hydro H150i Pro XT
            let pump_mode =
                if fixed_speed < 56 {
                    "quiet".to_string()
                } else if fixed_speed > 75 {
                    "extreme".to_string()
                } else {
                    "balanced".to_string()
                };
            self.client.borrow()
                .post(LIQCTLD_INITIALIZE
                    .replace("{}", device_data.type_index.to_string().as_str())
                )
                .json(&InitializeRequest { pump_mode: Some(pump_mode) })
                .send().await?
                .error_for_status()
                .map(|_| ())  // ignore successful result
                .with_context(|| format!("Setting fixed speed through initialization for LIQUIDCTL Device #{}: {}", device_data.type_index, device_data.uid))
        } else if device_data.driver_type == BaseDriver::HydroPro && setting.channel_name == "pump" {
            let pump_mode =
                if fixed_speed < 34 {
                    "quiet".to_string()
                } else if fixed_speed > 66 {
                    "performance".to_string()
                } else {
                    "balanced".to_string()
                };
            self.client.borrow()
                .post(LIQCTLD_INITIALIZE
                    .replace("{}", device_data.type_index.to_string().as_str())
                )
                .json(&InitializeRequest { pump_mode: Some(pump_mode) })
                .send().await?
                .error_for_status()
                .map(|_| ())  // ignore successful result
                .with_context(|| format!("Setting fixed speed through initialization for LIQUIDCTL Device #{}: {}", device_data.type_index, device_data.uid))
        } else {
            self.client.borrow()
                .put(LIQCTLD_FIXED_SPEED
                    .replace("{}", device_data.type_index.to_string().as_str())
                )
                .json(&FixedSpeedRequest {
                    channel: setting.channel_name.clone(),
                    duty: fixed_speed,
                })
                .send().await?
                .error_for_status()
                .map(|_| ())  // ignore successful result
                .with_context(|| format!("Setting fixed speed for LIQUIDCTL Device #{}: {}", device_data.type_index, device_data.uid))
        }
    }

    async fn set_speed_profile(&self, setting: &Setting, device_data: &CachedDeviceData) -> Result<()> {
        let profile = setting.speed_profile.as_ref()
            .with_context(|| "Speed Profile should be present")?
            .clone();
        let temp_source = setting.temp_source.as_ref()
            .with_context(|| "Temp Source should be present when setting speed profiles")?;
        let regex_temp_sensor_number = Regex::new(PATTERN_TEMP_SOURCE_NUMBER)?;
        let temperature_sensor = if regex_temp_sensor_number.is_match(&temp_source.temp_name) {
            let temp_sensor_number: u8 = regex_temp_sensor_number
                .captures(&temp_source.temp_name)
                .context("Temp Sensor Number should exist")?
                .name("number").context("Number Group should exist")?.as_str().parse()?;
            Some(temp_sensor_number)
        } else { None };
        self.client.borrow()
            .put(LIQCTLD_SPEED_PROFILE
                .replace("{}", device_data.type_index.to_string().as_str())
            )
            .json(&SpeedProfileRequest {
                channel: setting.channel_name.clone(),
                profile,
                temperature_sensor,
            })
            .send().await?
            .error_for_status()
            .map(|_| ())  // ignore successful result
            .with_context(|| format!("Setting speed profile for LIQUIDCTL Device #{}: {}", device_data.type_index, device_data.uid))
    }

    async fn set_color(&self, setting: &Setting, device_data: &CachedDeviceData) -> Result<()> {
        let lighting_settings = setting.lighting.as_ref()
            .with_context(|| "LightingSettings should be present")?;
        let mode = lighting_settings.mode.clone();
        let colors = lighting_settings.colors.clone();
        let mut time_per_color: Option<u8> = None;
        let mut speed: Option<String> = None;
        if let Some(speed_setting) = &lighting_settings.speed {
            if device_data.driver_type == BaseDriver::Legacy690Lc
                || device_data.driver_type == BaseDriver::Hydro690Lc {
                time_per_color = Some(speed_setting.parse::<u8>()?);  // time is always an integer
            } else if device_data.driver_type == BaseDriver::Modern690Lc { // EVGA uses both for different modes
                time_per_color = Some(speed_setting.parse::<u8>()?);
                speed = Some(speed_setting.clone());  // liquidctl will handle convert to int here
            } else {
                speed = Some(speed_setting.clone());  // str normally for most all devices
            }
        }
        let direction = if lighting_settings.backward.unwrap_or(false) {
            Some("backward".to_string())
        } else { None };
        self.client.borrow()
            .put(LIQCTLD_COLOR
                .replace("{}", device_data.type_index.to_string().as_str())
            )
            .json(&ColorRequest {
                channel: setting.channel_name.clone(),
                mode,
                colors,
                time_per_color,
                speed,
                direction,
            })
            .send().await?
            .error_for_status()
            .map(|_| ())  // ignore successful result
            .with_context(|| format!("Setting Lighting for LIQUIDCTL Device #{}: {}", device_data.type_index, device_data.uid))
    }

    async fn set_screen(&self, setting: &Setting, device_data: &CachedDeviceData) -> Result<()> {
        let lcd_settings = setting.lcd.as_ref()
            .with_context(|| "LcdSettings should be present")?;
        // We set several settings at once for lcd/screen settings
        if let Some(brightness) = lcd_settings.brightness {
            if let Err(err) = self.send_screen_request(
                &ScreenRequest {
                    channel: setting.channel_name.clone(),
                    mode: "brightness".to_string(),
                    value: Some(brightness.to_string()),  // liquidctl handles conversion to int
                }, &device_data.type_index, &device_data.uid,
            ).await { error!("Error setting lcd/screen brightness {} | {}", brightness, err); }
            // we don't abort if there are brightness or orientation setting errors
        }
        if let Some(orientation) = lcd_settings.orientation {
            if let Err(err) = self.send_screen_request(
                &ScreenRequest {
                    channel: setting.channel_name.clone(),
                    mode: "orientation".to_string(),
                    value: Some(orientation.to_string()),  // liquidctl handles conversion to int
                }, &device_data.type_index, &device_data.uid,
            ).await { error!("Error setting lcd/screen orientation {} | {}", orientation, err); }
            // we don't abort if there are brightness or orientation setting errors
        }
        if lcd_settings.mode == "image" {
            if let Some(image_file) = &lcd_settings.image_file_processed {
                let mode = if image_file.contains(".gif") {  // tmp image is pre-processed
                    "gif".to_string()
                } else {
                    "static".to_string()
                };
                self.send_screen_request(
                    &ScreenRequest {
                        channel: setting.channel_name.clone(),
                        mode,
                        value: Some(image_file.clone()),
                    }, &device_data.type_index, &device_data.uid,
                ).await.with_context(|| "Setting lcd/screen 'image/gif'")?;
            }
        } else if lcd_settings.mode == "liquid" {
            self.send_screen_request(
                &ScreenRequest {
                    channel: setting.channel_name.clone(),
                    mode: lcd_settings.mode.clone(),
                    value: None,
                }, &device_data.type_index, &device_data.uid,
            ).await.with_context(|| "Setting lcd/screen 'liquid' mode")?;
        }
        Ok(())
    }

    async fn send_screen_request(
        &self, screen_request: &ScreenRequest, type_index: &u8, uid: &String,
    ) -> Result<()> {
        self.client.borrow()
            .put(LIQCTLD_SCREEN
                .replace("{}", type_index.to_string().as_str())
            )
            .json(screen_request)
            .send().await?
            .error_for_status()
            .map(|_| ())  // ignore successful result
            .with_context(|| format!("Setting screen for LIQUIDCTL Device #{}: {}", type_index, uid))
    }

    async fn cache_device_data(&self, device_uid: &UID) -> Result<CachedDeviceData> {
        let device = self.devices.get(device_uid)
            .with_context(|| format!("Device UID not found! {}", device_uid))?
            .read().await;
        Ok(CachedDeviceData {
            type_index: device.type_index,
            uid: device.uid.clone(),
            driver_type: device.lc_info.as_ref()
                .expect("lc_info for LC Device should always be present")
                .driver_type.clone(),
        })
    }
}

#[async_trait]
impl Repository for LiquidctlRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Liquidctl
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.call_initialize_concurrently().await;
        let mut init_devices = HashMap::new();
        for (uid, device) in self.devices.iter() {
            init_devices.insert(uid.clone(), device.read().await.clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Devices: {:#?}", init_devices);  // pretty output for easy reading
        } else {
            info!("Initialized Devices: {:?}", init_devices);
        }
        debug!(
            "Time taken to initialize all LIQUIDCTL devices: {:?}", start_initialization.elapsed()
        );
        info!("LIQUIDCTL Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();
        let mut futures = Vec::new();
        for device_lock in self.devices.values() {
            futures.push(
                async {
                    let device_id = device_lock.read().await.type_index;
                    match self.call_status(&device_id).await {
                        Ok(status) => {
                            self.preloaded_statuses.write().await.insert(device_id, status);
                            ()
                        }
                        // this leaves the previous status in the map as backup for temporary issues
                        Err(err) => error!("Error getting status from device #{}: {}", device_id, err)
                    }
                }
            )
        }
        join_all(futures).await;
        debug!(
            "STATUS PRELOAD Time taken for all LIQUIDCTL devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for device_lock in self.devices.values() {
            let status = {
                let device = device_lock.read().await;
                let preloaded_statuses = self.preloaded_statuses.read().await;
                let lc_status = preloaded_statuses.get(&device.type_index);
                if let None = lc_status {
                    error!("There is no status preloaded for this device: {}", device.uid);
                    continue;
                }
                let status = self.map_status(
                    &device.lc_info.as_ref()
                        .expect("Should always be present for LC devices")
                        .driver_type,
                    lc_status.unwrap(),
                    &device.type_index,
                );
                debug!("Device: {} status updated: {:?}", device.name, status);
                status
            };
            device_lock.write().await.set_status(status);
        }
        debug!(
            "STATUS SNAPSHOT Time taken for all LIQUIDCTL devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        let quit_response = self.client
            .post(LIQCTLD_QUIT)
            .send().await?
            .json::<QuitResponse>().await?;
        info!("LIQUIDCTL Repository Shutdown");
        return if quit_response.quit {
            info!("Quit Signal successfully sent to Liqctld");
            Ok(())
        } else {
            Err(anyhow!("Incorrect quit response from coolercontrol-liqctld: {}", quit_response.quit))
        };
    }

    async fn apply_setting(&self, device_uid: &UID, setting: &Setting) -> Result<()> {
        info!("Applying device: {} settings: {:?}", device_uid, setting);
        let cached_device_data = self.cache_device_data(device_uid).await?;
        if setting.speed_fixed.is_some() {
            self.set_fixed_speed(setting, &cached_device_data).await
        } else if setting.speed_profile.is_some() {
            self.set_speed_profile(setting, &cached_device_data).await
        } else if setting.lighting.is_some() {
            self.set_color(setting, &cached_device_data).await
        } else if setting.lcd.is_some() {
            self.set_screen(setting, &cached_device_data).await
        } else {
            Err(anyhow!("Setting not applicable to LIQUIDCTL devices: {:?}", setting))
        }
    }

    async fn reinitialize_devices(&self) {
        let no_init = match self.config.get_settings().await {
            Ok(settings) => settings.no_init,
            Err(err) => {
                error!("Error reading settings: {}", err);
                false
            }
        };
        if !no_init {
            self.call_reinitialize_concurrently().await
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct HandshakeResponse {
    shake: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuitResponse {
    quit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DevicesResponse {
    devices: Vec<DeviceResponse>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DeviceResponse {
    id: u8,
    description: String,
    device_type: String,
    serial_number: Option<String>,
    properties: DeviceProperties,
}


#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceProperties {
    pub speed_channels: Vec<String>,
    pub color_channels: Vec<String>,
    pub supports_cooling: Option<bool>,
    pub supports_cooling_profiles: Option<bool>,
    pub supports_lighting: Option<bool>,
    pub led_count: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct InitializeRequest {
    pump_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub status: LCStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FixedSpeedRequest {
    channel: String,
    duty: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SpeedProfileRequest {
    channel: String,
    profile: Vec<(u8, u8)>,
    temperature_sensor: Option<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ColorRequest {
    channel: String,
    mode: String,
    colors: Vec<(u8, u8, u8)>,
    time_per_color: Option<u8>,
    speed: Option<String>,
    direction: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ScreenRequest {
    channel: String,
    mode: String,
    value: Option<String>,
}

#[derive(Debug)]
struct CachedDeviceData {
    type_index: u8,
    uid: UID,
    driver_type: BaseDriver,
}