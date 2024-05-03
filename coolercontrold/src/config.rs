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
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use const_format::concatcp;
use log::{error, info, trace, warn};
use tokio::sync::RwLock;
use toml_edit::{ArrayOfTables, DocumentMut, Formatted, Item, Table, Value};

use crate::api::CCError;
use crate::device::{ChannelName, UID};
use crate::processing::processors::functions::TMA_DEFAULT_WINDOW_SIZE;
use crate::repositories::repository::DeviceLock;
use crate::setting::{
    CoolerControlDeviceSettings, CoolerControlSettings, CustomSensor, CustomSensorMixFunctionType,
    CustomSensorType, CustomTempSourceData, Function, FunctionType, LcdSettings, LightingSettings,
    Profile, ProfileMixFunctionType, ProfileType, Setting, TempSource, DEFAULT_FUNCTION_UID,
    DEFAULT_PROFILE_UID,
};

pub const DEFAULT_CONFIG_DIR: &str = "/etc/coolercontrol";
const DEFAULT_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/config.toml");
const DEFAULT_BACKUP_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/config-bak.toml");
const DEFAULT_UI_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/config-ui.json");
const DEFAULT_BACKUP_UI_CONFIG_FILE_PATH: &str =
    concatcp!(DEFAULT_CONFIG_DIR, "/config-ui-bak.json");
const DEFAULT_CONFIG_FILE_BYTES: &[u8] = include_bytes!("../resources/config-default.toml");

type ChannelLabel = Option<String>;
type TempName = String;
type TempLabel = String;

pub struct Config {
    path: PathBuf,
    path_ui: PathBuf,
    document: RwLock<DocumentMut>,
}

impl Config {
    /// loads the configuration file data into memory
    pub async fn load_config_file() -> Result<Self> {
        let config_dir = Path::new(DEFAULT_CONFIG_DIR);
        if !config_dir.exists() {
            warn!(
                "config directory doesn't exist. Attempting to create it: {}",
                DEFAULT_CONFIG_DIR
            );
            tokio::fs::create_dir_all(&config_dir).await?;
        }
        let path = Path::new(DEFAULT_CONFIG_FILE_PATH).to_path_buf();
        let path_ui = Path::new(DEFAULT_UI_CONFIG_FILE_PATH).to_path_buf();
        let config_contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(err) => {
                warn!("Error trying to read configuration file: {}", err);
                warn!("Attempting to write a new configuration file");
                tokio::fs::write(&path, DEFAULT_CONFIG_FILE_BYTES)
                    .await
                    .with_context(|| format!("Writing new configuration file: {path:?}"))?;
                tokio::fs::read_to_string(&path)
                    .await
                    .with_context(|| format!("Reading configuration file {path:?}"))?
            }
        };
        let document = config_contents
            .parse::<DocumentMut>()
            .with_context(|| "Parsing configuration file")?;
        trace!("Loaded configuration file: {}", document);
        let config = Self {
            path,
            path_ui,
            document: RwLock::new(document),
        };
        // test parsing of config data to make sure everything is readable
        let _ = config.legacy690_ids().await?;
        let _ = config.get_settings().await?;
        if let Err(err) = config.get_all_devices_settings().await {
            error!("Configuration File contains invalid settings: {}", err);
            return Err(err);
        };
        if let Err(err) = config.get_all_cc_devices_settings().await {
            error!("Configuration File contains invalid settings: {}", err);
            return Err(err);
        };
        if let Err(err) = config.get_profiles().await {
            error!("Configuration File contains invalid settings: {}", err);
            return Err(err);
        };
        if let Err(err) = config.get_functions().await {
            error!("Configuration File contains invalid settings: {}", err);
            return Err(err);
        };
        if let Err(err) = config.get_custom_sensors().await {
            error!("Configuration File contains invalid settings: {}", err);
            return Err(err);
        };
        info!("Configuration file check successful");
        Ok(config)
    }

    /// saves any changes to the configuration file - preserving formatting and comments
    pub async fn save_config_file(&self) -> Result<()> {
        tokio::fs::write(&self.path, self.document.read().await.to_string())
            .await
            .with_context(|| format!("Saving configuration file: {:?}", &self.path))
    }

    /// saves a backup of the daemon config file
    pub async fn save_backup_config_file(&self) -> Result<()> {
        let backup_path = Path::new(DEFAULT_BACKUP_CONFIG_FILE_PATH).to_path_buf();
        tokio::fs::write(&backup_path, self.document.read().await.to_string())
            .await
            .with_context(|| format!("Saving backup configuration file: {:?}", &backup_path))
    }

    pub async fn save_ui_config_file(&self, ui_settings: &String) -> Result<()> {
        tokio::fs::write(&self.path_ui, ui_settings)
            .await
            .with_context(|| format!("Saving UI configuration file: {:?}", &self.path_ui))
    }

    pub async fn save_backup_ui_config_file(&self, ui_settings: &String) -> Result<()> {
        let backup_path = Path::new(DEFAULT_BACKUP_UI_CONFIG_FILE_PATH).to_path_buf();
        tokio::fs::write(&backup_path, ui_settings)
            .await
            .with_context(|| format!("Saving backup UI configuration file: {:?}", &backup_path))
    }

    pub async fn load_ui_config_file(&self) -> Result<String> {
        tokio::fs::read_to_string(&self.path_ui)
            .await
            .with_context(|| format!("Loading UI configuration file {:?}", &self.path_ui))
    }

    /// This adds a human readable device list with UIDs to the config file
    pub async fn create_device_list(&self, devices: Arc<HashMap<UID, DeviceLock>>) -> Result<()> {
        for (uid, device) in devices.iter() {
            self.document.write().await["devices"][uid.as_str()] = Item::Value(Value::String(
                Formatted::new(device.read().await.name.clone()),
            ));
        }
        Ok(())
    }

    /// Updates current deprecated settings to the new format.
    /// In particular the temp and channel names were standardized, breaking previous settings
    /// using the old format.
    ///
    /// Arguments:
    ///
    /// * `devices`: The `devices` parameter is an `Arc` (atomic reference count) of a `HashMap` that
    /// maps `UID` (unique identifier) to `DeviceLock`. The `DeviceLock` is a lock that allows
    /// concurrent access to the device.
    ///
    /// Returns:
    ///
    /// a `Result<()>`.
    pub async fn update_deprecated_settings(
        &self,
        devices: Arc<HashMap<UID, DeviceLock>>,
    ) -> Result<()> {
        // Collect all channel names & labels, and all temp names & labels/frontend_names.
        let mut device_channel_names: HashMap<UID, Vec<(ChannelName, ChannelLabel)>> =
            HashMap::new();
        let mut device_temp_names: HashMap<UID, Vec<(TempName, TempLabel)>> = HashMap::new();
        for device in devices.values() {
            let device = device.read().await;
            for (channel_name, channel_info) in &device.info.channels {
                device_channel_names
                    .entry(device.uid.clone())
                    .or_default()
                    .push((channel_name.clone(), channel_info.label.clone()));
            }
            for (temp_name, temp_info) in &device.info.temps {
                device_temp_names
                    .entry(device.uid.clone())
                    .or_default()
                    .push((temp_name.clone(), temp_info.label.clone()));
            }
        }
        self.update_deprecated_channel_names_in_setting(&device_channel_names)
            .await?;
        self.update_deprecated_lcd_temp_sources_in_setting(&device_temp_names)
            .await?;
        self.update_deprecated_profile_temp_sources(&device_temp_names)
            .await?;
        Ok(())
    }

    async fn update_deprecated_channel_names_in_setting(
        &self,
        device_channel_names: &HashMap<UID, Vec<(ChannelName, ChannelLabel)>>,
    ) -> Result<()> {
        let all_device_settings = self.get_all_devices_settings().await?;
        for (device_uid, device_settings) in &all_device_settings {
            for setting in device_settings {
                if let Some(c_name_label_list) = device_channel_names.get(device_uid) {
                    let channel_name_is_up_to_date = c_name_label_list
                        .iter()
                        .any(|(channel_name, _)| &setting.channel_name == channel_name);
                    if channel_name_is_up_to_date {
                        continue;
                    }
                    let updated_channel_name =
                        c_name_label_list
                            .iter()
                            .find_map(|(channel_name, channel_label)| {
                                if channel_label.is_some()
                                    && normalized(&setting.channel_name)
                                        == normalized(channel_label.as_ref().unwrap())
                                {
                                    Some(channel_name.clone())
                                } else {
                                    None
                                }
                            });
                    if updated_channel_name.is_none() {
                        warn!(
                            "Channel name {} not found for device {} from Device Settings",
                            &setting.channel_name, device_uid
                        );
                        continue;
                    }
                    let new_setting = Setting {
                        channel_name: updated_channel_name.unwrap(),
                        ..setting.clone()
                    };
                    self.set_device_setting(device_uid, &new_setting).await;
                    // remove previous setting:
                    let mut doc = self.document.write().await;
                    let device_settings =
                        doc["device-settings"][device_uid].or_insert(Item::Table(Table::new()));
                    device_settings[setting.channel_name.as_str()] = Item::None;
                }
            }
        }
        Ok(())
    }

    async fn update_deprecated_lcd_temp_sources_in_setting(
        &self,
        device_temp_names: &HashMap<UID, Vec<(TempName, TempLabel)>>,
    ) -> Result<()> {
        let all_device_settings = self.get_all_devices_settings().await?;
        for (device_uid, device_settings) in &all_device_settings {
            for setting in device_settings {
                if setting.lcd.is_none() {
                    continue;
                }
                if setting.lcd.as_ref().unwrap().temp_source.is_none() {
                    continue;
                }
                let temp_source = setting.lcd.as_ref().unwrap().temp_source.as_ref().unwrap();
                if let Some(t_name_label_list) = device_temp_names.get(&temp_source.device_uid) {
                    let temp_name_is_up_to_date = t_name_label_list
                        .iter()
                        .any(|(temp_name, _)| &temp_source.temp_name == temp_name);
                    if temp_name_is_up_to_date {
                        continue;
                    }
                    let updated_temp_name =
                        t_name_label_list
                            .iter()
                            .find_map(|(temp_name, temp_label)| {
                                if normalized(temp_label) == normalized(&temp_source.temp_name) {
                                    Some(temp_name.clone())
                                } else {
                                    None
                                }
                            });
                    if updated_temp_name.is_none() {
                        warn!(
                            "Temp name {} not found for device {} from Legacy Device Settings",
                            &temp_source.temp_name, temp_source.device_uid
                        );
                        continue;
                    }
                    let new_lcd_setting = LcdSettings {
                        temp_source: Some(TempSource {
                            temp_name: updated_temp_name.unwrap(),
                            ..temp_source.clone()
                        }),
                        ..setting.lcd.as_ref().unwrap().clone()
                    };
                    let new_setting = Setting {
                        lcd: Some(new_lcd_setting),
                        ..setting.clone()
                    };
                    self.set_device_setting(device_uid, &new_setting).await;
                }
            }
        }
        Ok(())
    }

    async fn update_deprecated_profile_temp_sources(
        &self,
        device_temp_names: &HashMap<UID, Vec<(TempName, TempLabel)>>,
    ) -> Result<()> {
        if let Ok(profiles) = self.get_profiles().await {
            for profile in &profiles {
                if profile.temp_source.is_none() {
                    continue;
                }
                let temp_source = profile.temp_source.as_ref().unwrap();
                if let Some(t_name_label_list) = device_temp_names.get(&temp_source.device_uid) {
                    let temp_name_is_up_to_date = t_name_label_list
                        .iter()
                        .any(|(temp_name, _)| &temp_source.temp_name == temp_name);
                    if temp_name_is_up_to_date {
                        continue;
                    }
                    let updated_temp_name =
                        t_name_label_list
                            .iter()
                            .find_map(|(temp_name, temp_label)| {
                                if normalized(temp_label) == normalized(&temp_source.temp_name) {
                                    Some(temp_name.clone())
                                } else {
                                    None
                                }
                            });
                    if updated_temp_name.is_none() {
                        warn!(
                            "Temp name {} not found for device {} from Profile {}",
                            temp_source.temp_name, temp_source.device_uid, profile.uid
                        );
                        continue;
                    }
                    let updated_profile = Profile {
                        temp_source: Some(TempSource {
                            temp_name: updated_temp_name.unwrap(),
                            ..temp_source.clone()
                        }),
                        ..profile.clone()
                    };
                    self.update_profile(updated_profile).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn update_deprecated_custom_sensor_temp_sources(
        &self,
        devices: &HashMap<UID, DeviceLock>,
    ) -> Result<()> {
        let mut device_temp_names: HashMap<UID, Vec<(TempName, TempLabel)>> = HashMap::new();
        for device in devices.values() {
            let device = device.read().await;
            for (temp_name, temp_info) in &device.info.temps {
                device_temp_names
                    .entry(device.uid.clone())
                    .or_default()
                    .push((temp_name.clone(), temp_info.label.clone()));
            }
        }
        if let Ok(sensors) = self.get_custom_sensors().await {
            for sensor in &sensors {
                let mut sources_updated = false;
                let mut new_sources = sensor.sources.clone();
                for source in &sensor.sources {
                    let temp_source = &source.temp_source;
                    if let Some(t_name_label_list) = device_temp_names.get(&temp_source.device_uid)
                    {
                        let temp_name_is_up_to_date = t_name_label_list
                            .iter()
                            .any(|(temp_name, _)| &temp_source.temp_name == temp_name);
                        if temp_name_is_up_to_date {
                            continue;
                        }
                        let updated_temp_name =
                            t_name_label_list
                                .iter()
                                .find_map(|(temp_name, temp_label)| {
                                    if normalized(temp_label) == normalized(&temp_source.temp_name)
                                    {
                                        Some(temp_name.clone())
                                    } else {
                                        None
                                    }
                                });
                        if updated_temp_name.is_none() {
                            warn!(
                                "Temp name {} not found for device {} from Custom Sensor {}",
                                temp_source.temp_name, temp_source.device_uid, sensor.id
                            );
                            continue;
                        }
                        for new_source in &mut new_sources {
                            if &new_source.temp_source == temp_source {
                                new_source.temp_source = TempSource {
                                    temp_name: updated_temp_name.unwrap(),
                                    device_uid: temp_source.device_uid.clone(),
                                };
                                sources_updated = true;
                                break;
                            }
                        }
                    }
                }
                if sources_updated {
                    let updated_sensor = CustomSensor {
                        sources: new_sources,
                        ..sensor.clone()
                    };
                    self.update_custom_sensor(updated_sensor).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn legacy690_ids(&self) -> Result<HashMap<String, bool>> {
        let mut legacy690_ids = HashMap::new();
        if let Some(table) = self.document.read().await["legacy690"].as_table() {
            for (key, value) in table {
                legacy690_ids.insert(
                    key.to_string(),
                    value
                        .as_bool()
                        .with_context(|| "Parsing boolean value for legacy690")?,
                );
            }
        }
        Ok(legacy690_ids)
    }

    pub async fn set_legacy690_id(&self, device_uid: &str, is_legacy690: &bool) {
        self.document.write().await["legacy690"][device_uid] =
            Item::Value(Value::Boolean(Formatted::new(*is_legacy690)));
    }

    pub async fn set_device_setting(&self, device_uid: &str, setting: &Setting) {
        let mut doc = self.document.write().await;
        let device_settings =
            doc["device-settings"][device_uid].or_insert(Item::Table(Table::new()));
        let channel_setting = &mut device_settings[setting.channel_name.as_str()];
        if let Some(pwm_mode) = setting.pwm_mode {
            channel_setting["pwm_mode"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(pwm_mode))));
        }
        if setting.reset_to_default.unwrap_or(false) {
            *channel_setting = Item::None; // removes channel from settings
        } else if let Some(speed_fixed) = setting.speed_fixed {
            Self::set_setting_fixed_speed(channel_setting, speed_fixed);
        } else if let Some(lighting) = &setting.lighting {
            Self::set_setting_lighting(channel_setting, lighting);
        } else if let Some(lcd) = &setting.lcd {
            Self::set_setting_lcd(channel_setting, lcd);
        } else if let Some(profile_uid) = &setting.profile_uid {
            Self::set_profile_uid(channel_setting, profile_uid);
        }
    }

    fn set_setting_fixed_speed(channel_setting: &mut Item, speed_fixed: u8) {
        channel_setting["speed_profile"] = Item::None; // clear profile setting
        channel_setting["temp_source"] = Item::None;
        channel_setting["speed_fixed"] =
            Item::Value(Value::Integer(Formatted::new(i64::from(speed_fixed))));
    }

    fn set_setting_lighting(channel_setting: &mut Item, lighting: &LightingSettings) {
        channel_setting["lighting"] = Item::None;
        channel_setting["lighting"]["mode"] =
            Item::Value(Value::String(Formatted::new(lighting.mode.clone())));
        if let Some(speed) = &lighting.speed {
            channel_setting["lighting"]["speed"] =
                Item::Value(Value::String(Formatted::new(speed.clone())));
        }
        if lighting.backward.unwrap_or(false) {
            channel_setting["lighting"]["backward"] =
                Item::Value(Value::Boolean(Formatted::new(true)));
        }
        let mut color_array = toml_edit::Array::new();
        for (r, g, b) in lighting.colors.clone() {
            let mut rgb_array = toml_edit::Array::new();
            rgb_array.push(Value::Integer(Formatted::new(i64::from(r))));
            rgb_array.push(Value::Integer(Formatted::new(i64::from(g))));
            rgb_array.push(Value::Integer(Formatted::new(i64::from(b))));
            color_array.push(rgb_array);
        }
        channel_setting["lighting"]["colors"] = Item::Value(Value::Array(color_array));
    }

    fn set_setting_lcd(channel_setting: &mut Item, lcd: &LcdSettings) {
        channel_setting["lcd"] = Item::None;
        channel_setting["lcd"]["mode"] =
            Item::Value(Value::String(Formatted::new(lcd.mode.clone())));
        if let Some(brightness) = lcd.brightness {
            channel_setting["lcd"]["brightness"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(brightness))));
        }
        if let Some(orientation) = lcd.orientation {
            channel_setting["lcd"]["orientation"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(orientation))));
        }
        if let Some(image_file_processed) = &lcd.image_file_processed {
            channel_setting["lcd"]["image_file_processed"] =
                Item::Value(Value::String(Formatted::new(image_file_processed.clone())));
        }
        let mut color_array = toml_edit::Array::new();
        for (r, g, b) in lcd.colors.clone() {
            let mut rgb_array = toml_edit::Array::new();
            rgb_array.push(Value::Integer(Formatted::new(i64::from(r))));
            rgb_array.push(Value::Integer(Formatted::new(i64::from(g))));
            rgb_array.push(Value::Integer(Formatted::new(i64::from(b))));
            color_array.push(rgb_array);
        }
        channel_setting["lcd"]["colors"] = Item::Value(Value::Array(color_array));
        if let Some(temp_source) = &lcd.temp_source {
            channel_setting["temp_source"] = Item::None;
            channel_setting["lcd"]["temp_source"]["temp_name"] =
                Item::Value(Value::String(Formatted::new(temp_source.temp_name.clone())));
            channel_setting["lcd"]["temp_source"]["device_uid"] = Item::Value(Value::String(
                Formatted::new(temp_source.device_uid.clone()),
            ));
        }
    }

    fn set_profile_uid(channel_setting: &mut Item, profile_uid: &UID) {
        channel_setting["speed_profile"] = Item::None; // clear profile setting
        channel_setting["temp_source"] = Item::None; // clear profile setting
        channel_setting["speed_fixed"] = Item::None; // clear fixed setting
        channel_setting["profile_uid"] =
            Item::Value(Value::String(Formatted::new(profile_uid.clone())));
    }

    pub async fn get_device_channel_settings(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<Setting> {
        let device_settings = self.get_device_settings(device_uid).await?;
        for setting in device_settings {
            if setting.channel_name == channel_name {
                return Ok(setting);
            }
        }
        Err(CCError::NotFound {
            msg: "Device Channel Setting".to_string(),
        }
        .into())
    }

    /// Retrieves the device settings from the config file to our Setting model.
    /// This has to be done defensively, as the user may change the config file.
    pub async fn get_device_settings(&self, device_uid: &str) -> Result<Vec<Setting>> {
        let mut settings = Vec::new();
        if let Some(table_item) = self.document.read().await["device-settings"].get(device_uid) {
            let table = table_item
                .as_table()
                .with_context(|| "device setting should be a table")?;
            for (channel_name, base_item) in table {
                let setting_table = base_item
                    .as_inline_table()
                    .with_context(|| "Channel Setting should be an inline table")?
                    .clone()
                    .into_table();
                let speed_fixed = Self::get_speed_fixed(&setting_table)?;
                let lighting = Self::get_lighting(&setting_table)?;
                let lcd = Self::get_lcd(&setting_table)?;
                let pwm_mode = Self::get_pwm_mode(&setting_table)?;
                let profile_uid = Self::get_profile_uid(&setting_table)?;
                settings.push(Setting {
                    channel_name: channel_name.to_string(),
                    speed_fixed,
                    lighting,
                    lcd,
                    pwm_mode,
                    reset_to_default: None,
                    profile_uid,
                });
            }
        }
        Ok(settings)
    }

    pub async fn get_all_devices_settings(&self) -> Result<HashMap<UID, Vec<Setting>>> {
        let mut devices_settings = HashMap::new();
        if let Some(device_table) = self.document.read().await["device-settings"].as_table() {
            for (device_uid, _value) in device_table {
                let settings = self.get_device_settings(device_uid).await?;
                devices_settings.insert(device_uid.to_string(), settings);
            }
        }
        Ok(devices_settings)
    }

    pub async fn get_all_cc_devices_settings(
        &self,
    ) -> Result<HashMap<UID, Option<CoolerControlDeviceSettings>>> {
        let mut devices_settings = HashMap::new();
        if let Some(device_table) = self.document.read().await["settings"].as_table() {
            for (device_uid, _value) in device_table {
                if device_uid.len() == 64 {
                    // there are other settings here, we want only the ones with proper UIDs
                    let settings = self.get_cc_settings_for_device(device_uid).await?;
                    devices_settings.insert(device_uid.to_string(), settings);
                }
            }
        }
        Ok(devices_settings)
    }

    fn get_speed_fixed(setting_table: &Table) -> Result<Option<u8>> {
        let speed_fixed = if let Some(speed_value) = setting_table.get("speed_fixed") {
            let speed: u8 = speed_value
                .as_integer()
                .with_context(|| "speed_fixed should be an integer")?
                .try_into()
                .ok()
                .with_context(|| "speed_fixed must be a value between 0-100")?;
            Some(speed.clamp(0, 100))
        } else {
            None
        };
        Ok(speed_fixed)
    }

    fn get_profile_uids(setting_table: &Table) -> Result<Vec<UID>> {
        let profile_uids = if let Some(value) = setting_table.get("member_profile_uids") {
            value
                .as_array()
                .with_context(|| "profile_uids should be an array")?
                .into_iter()
                .map(|value| value.as_str().expect("uid must be a string").to_string())
                .collect()
        } else {
            Vec::new()
        };
        Ok(profile_uids)
    }

    fn get_mix_function_type(setting_table: &Table) -> Result<Option<ProfileMixFunctionType>> {
        let mix_function_type = if let Some(value) = setting_table.get("mix_function_type") {
            Some(
                value
                    .as_str()
                    .expect("mix_function_type must be a valid string")
                    .try_into()
                    .ok()
                    .with_context(|| "mix_function_type must be a valid string")?,
            )
        } else {
            None
        };
        Ok(mix_function_type)
    }

    fn get_speed_profile(setting_table: &Table) -> Result<Option<Vec<(f64, u8)>>> {
        let speed_profile = if let Some(value) = setting_table.get("speed_profile") {
            let mut profiles = Vec::new();
            let speeds = value
                .as_array()
                .with_context(|| "profile should be an array")?;
            for profile_pair_value in speeds {
                let profile_pair_array = profile_pair_value
                    .as_array()
                    .with_context(|| "profile pairs should be an array")?;
                let temp_value = profile_pair_array
                    .get(0)
                    .with_context(|| "Speed Profiles must be pairs")?;
                // toml edit can't convert 20 to a float like 20.0. We need to handle integer values:
                let temp: f64 = match temp_value.as_float() {
                    None => {
                        let temp_i64 = temp_value
                            .as_integer()
                            .with_context(|| "Speed Profile Temps must be integers or floats")?;
                        if temp_i64 > f64::MAX as i64 {
                            f64::MAX
                        } else if temp_i64 < f64::MIN as i64 {
                            f64::MIN
                        } else {
                            temp_i64 as f64
                        }
                    }
                    Some(temp_f64) => temp_f64,
                };
                let speed: u8 = profile_pair_array
                    .get(1)
                    .with_context(|| "Speed Profiles must be pairs")?
                    .as_integer()
                    .with_context(|| "Speed Profile Duties must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "speed profiles must be values between 0-100")?;
                profiles.push((temp, speed.clamp(0, 100)));
            }
            Some(profiles)
        } else {
            None
        };
        Ok(speed_profile)
    }

    fn get_temp_source(setting_table: &Table) -> Result<Option<TempSource>> {
        let temp_source = if let Some(value) = setting_table.get("temp_source") {
            let temp_source_table = value
                .as_inline_table()
                .with_context(|| "temp_source should be an inline table")?;
            let temp_name = temp_source_table
                .get("temp_name")
                .with_context(|| "temp_source must have temp_name and device_uid set")?
                .as_str()
                .with_context(|| "temp_name should be a String")?
                .to_string();
            let device_uid = temp_source_table
                .get("device_uid")
                .with_context(|| "temp_source must have frontend_temp_name and device_uid set")?
                .as_str()
                .with_context(|| "device_uid should be a String")?
                .to_string();
            Some(TempSource {
                temp_name,
                device_uid,
            })
        } else {
            None
        };
        Ok(temp_source)
    }

    fn get_lighting(setting_table: &Table) -> Result<Option<LightingSettings>> {
        let lighting = if let Some(value) = setting_table.get("lighting") {
            let lighting_table = value
                .as_inline_table()
                .with_context(|| "lighting should be an inline table")?;
            let mode = lighting_table
                .get("mode")
                .with_context(|| "lighting.mode should be present")?
                .as_str()
                .with_context(|| "lighting.mode should be a String")?
                .to_string();
            let speed = if let Some(value) = lighting_table.get("speed") {
                Some(
                    value
                        .as_str()
                        .with_context(|| "lighting.speed should be a String")?
                        .to_string(),
                )
            } else {
                None
            };
            let backward = if let Some(value) = setting_table.get("backward") {
                Some(
                    value
                        .as_bool()
                        .with_context(|| "lighting.backward should be a boolean")?,
                )
            } else {
                None
            };
            let mut colors = Vec::new();
            let colors_array = lighting_table
                .get("colors")
                .with_context(|| "lighting.colors should always be present")?
                .as_array()
                .with_context(|| "lighting.colors should be an array")?;
            for rgb_value in colors_array {
                let rgb_array = rgb_value
                    .as_array()
                    .with_context(|| "RGB values should be an array")?;
                let r: u8 = rgb_array
                    .get(0)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer()
                    .with_context(|| "RGB values must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "RGB values must be between 0-255")?;
                let g: u8 = rgb_array
                    .get(1)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer()
                    .with_context(|| "RGB values must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "RGB values must be between 0-255")?;
                let b: u8 = rgb_array
                    .get(2)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer()
                    .with_context(|| "RGB values must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "RGB values must be between 0-255")?;
                colors.push((r, g, b));
            }
            Some(LightingSettings {
                mode,
                speed,
                backward,
                colors,
            })
        } else {
            None
        };
        Ok(lighting)
    }

    fn get_lcd(setting_table: &Table) -> Result<Option<LcdSettings>> {
        let lcd = if let Some(value) = setting_table.get("lcd") {
            let lcd_table = value
                .as_inline_table()
                .with_context(|| "lcd should be an inline table")?;
            let mode = lcd_table
                .get("mode")
                .with_context(|| "lcd.mode should be present")?
                .as_str()
                .with_context(|| "lcd.mode should be a String")?
                .to_string();
            let brightness = if let Some(brightness_value) = lcd_table.get("brightness") {
                let brightness_u8: u8 = brightness_value
                    .as_integer()
                    .with_context(|| "brightness should be an integer")?
                    .try_into()
                    .ok()
                    .with_context(|| "brightness should be a value between 0-100")?;
                Some(brightness_u8.clamp(0, 100))
            } else {
                None
            };
            let orientation = if let Some(orientation_value) = lcd_table.get("orientation") {
                let orientation_u16: u16 = orientation_value
                    .as_integer()
                    .with_context(|| "orientation should be an integer")?
                    .try_into()
                    .ok()
                    .with_context(|| "orientation should be a value between 0-270")?;
                Some(orientation_u16.clamp(0, 270))
            } else {
                None
            };
            let image_file_processed =
                if let Some(image_file_processed_value) = lcd_table.get("image_file_processed") {
                    Some(
                        image_file_processed_value
                            .as_str()
                            .with_context(|| "image_file_processed should be a String")?
                            .to_string(),
                    )
                } else {
                    None
                };
            let mut colors = Vec::new();
            let colors_array = lcd_table
                .get("colors")
                .with_context(|| "lcd.colors should always be present")?
                .as_array()
                .with_context(|| "lcd.colors should be an array")?;
            for rgb_value in colors_array {
                let rgb_array = rgb_value
                    .as_array()
                    .with_context(|| "RGB values should be an array")?;
                let r: u8 = rgb_array
                    .get(0)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer()
                    .with_context(|| "RGB values must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "RGB values must be between 0-255")?;
                let g: u8 = rgb_array
                    .get(1)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer()
                    .with_context(|| "RGB values must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "RGB values must be between 0-255")?;
                let b: u8 = rgb_array
                    .get(2)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer()
                    .with_context(|| "RGB values must be integers")?
                    .try_into()
                    .ok()
                    .with_context(|| "RGB values must be between 0-255")?;
                colors.push((r, g, b));
            }
            let temp_source = Self::get_temp_source(&lcd_table.clone().into_table())?;
            Some(LcdSettings {
                mode,
                brightness,
                orientation,
                image_file_processed,
                colors,
                temp_source,
            })
        } else {
            None
        };
        Ok(lcd)
    }

    fn get_pwm_mode(setting_table: &Table) -> Result<Option<u8>> {
        let pwm_mode = if let Some(value) = setting_table.get("pwm_mode") {
            let p_mode: u8 = value
                .as_integer()
                .with_context(|| "pwm_mode should be an integer")?
                .try_into()
                .ok()
                .with_context(|| "pwm_mode should be a value between 0-2")?;
            Some(p_mode.clamp(0, 2))
        } else {
            None
        };
        Ok(pwm_mode)
    }

    fn get_profile_uid(setting_table: &Table) -> Result<Option<String>> {
        let profile_uid = if let Some(value) = setting_table.get("profile_uid") {
            let p_uid = value
                .as_str()
                .with_context(|| "profile_uid should be a String")?
                .to_string();
            Some(p_uid)
        } else {
            None
        };
        Ok(profile_uid)
    }

    /// Returns `CoolerControl` general settings
    pub async fn get_settings(&self) -> Result<CoolerControlSettings> {
        if let Some(settings_item) = self.document.read().await.get("settings") {
            let settings = settings_item
                .as_table()
                .with_context(|| "Settings should be a table")?;
            let apply_on_boot = settings
                .get("apply_on_boot")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(true))))
                .as_bool()
                .with_context(|| "apply_on_boot should be a boolean value")?;
            let no_init = settings
                .get("no_init")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool()
                .with_context(|| "no_init should be a boolean value")?;
            let startup_delay = Duration::from_secs(
                settings
                    .get("startup_delay")
                    .unwrap_or(&Item::Value(Value::Integer(Formatted::new(2))))
                    .as_integer()
                    .with_context(|| "startup_delay should be an integer value")?
                    .clamp(0, 10) as u64,
            );
            let thinkpad_full_speed = settings
                .get("thinkpad_full_speed")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool()
                .with_context(|| "thinkpad_full_speed should be a boolean value")?;
            let port = if let Some(value) = settings.get("port") {
                let clamped_port = value
                    .as_integer()
                    .with_context(|| "port should be an integer value")?
                    .clamp(80, i64::from(u16::MAX)) as u16;
                Some(clamped_port)
            } else {
                None
            };
            let ipv4_address = if let Some(value) = settings.get("ipv4_address") {
                let ipv4_str = value
                    .as_str()
                    .with_context(|| "ipv4_address should be a string")?
                    .trim()
                    .to_string();
                Some(ipv4_str)
            } else {
                None
            };
            let ipv6_address = if let Some(value) = settings.get("ipv6_address") {
                let ipv6_str = value
                    .as_str()
                    .with_context(|| "ipv6_address should be a string")?
                    .trim()
                    .to_string();
                Some(ipv6_str)
            } else {
                None
            };
            Ok(CoolerControlSettings {
                apply_on_boot,
                no_init,
                startup_delay,
                thinkpad_full_speed,
                port,
                ipv4_address,
                ipv6_address,
            })
        } else {
            Err(anyhow!("Setting table not found in configuration file"))
        }
    }

    /// Sets `CoolerControl` settings
    pub async fn set_settings(&self, cc_settings: &CoolerControlSettings) {
        let mut doc = self.document.write().await;
        let base_settings = doc["settings"].or_insert(Item::Table(Table::new()));
        base_settings["apply_on_boot"] =
            Item::Value(Value::Boolean(Formatted::new(cc_settings.apply_on_boot)));
        base_settings["no_init"] = Item::Value(Value::Boolean(Formatted::new(cc_settings.no_init)));
        base_settings["startup_delay"] = Item::Value(Value::Integer(Formatted::new(
            cc_settings.startup_delay.as_secs() as i64,
        )));
        base_settings["thinkpad_full_speed"] = Item::Value(Value::Boolean(Formatted::new(
            cc_settings.thinkpad_full_speed,
        )));
    }

    /// This gets the `CoolerControl` settings for specific devices
    /// This differs from Device Settings, in that these settings are applied in `CoolerControl`,
    /// and not on the devices themselves.
    pub async fn get_cc_settings_for_device(
        &self,
        device_uid: &str,
    ) -> Result<Option<CoolerControlDeviceSettings>> {
        if let Some(table_item) = self.document.read().await["settings"].get(device_uid) {
            let device_settings_table = table_item
                .as_table()
                .with_context(|| "CoolerControl device settings should be a table")?;
            let disable = device_settings_table
                .get("disable")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool()
                .with_context(|| "disable should be a boolean value")?;
            let name = device_settings_table
                .get("name")
                .unwrap_or(&Item::Value(Value::String(Formatted::new(
                    device_uid.to_string(),
                ))))
                .as_str()
                .with_context(|| "name should be a string")?
                .to_string();
            Ok(Some(CoolerControlDeviceSettings { name, disable }))
        } else {
            Ok(None)
        }
    }

    /// Sets `CoolerControl` device settings
    pub async fn set_cc_settings_for_device(
        &self,
        device_uid: &str,
        cc_device_settings: &CoolerControlDeviceSettings,
    ) {
        let mut doc = self.document.write().await;
        let device_settings_table =
            doc["settings"][device_uid].or_insert(Item::Table(Table::new()));
        device_settings_table["name"] = Item::Value(Value::String(Formatted::new(
            cc_device_settings.name.clone(),
        )));
        device_settings_table["disable"] =
            Item::Value(Value::Boolean(Formatted::new(cc_device_settings.disable)));
    }

    /*
     *
     * PROFILES
     *
     */

    /// Loads the current Profile array from the config file.
    /// If there are none setup yet, it returns the initial default Profile,
    /// which should always be present.
    pub async fn get_profiles(&self) -> Result<Vec<Profile>> {
        let mut profiles = self.get_current_profiles().await?;
        if profiles.iter().any(|p| p.uid == *DEFAULT_PROFILE_UID).not() {
            // Default Profile not found, probably the first time loading
            profiles.push(Profile::default());
            self.set_profile(Profile::default()).await?;
            self.save_config_file().await?;
        }
        Ok(profiles)
    }

    async fn get_current_profiles(&self) -> Result<Vec<Profile>> {
        let mut profiles = Vec::new();
        if let Some(profiles_item) = self.document.read().await.get("profiles") {
            let profiles_array = profiles_item
                .as_array_of_tables()
                .with_context(|| "Profiles should be an array of tables")?;
            for profile_table in profiles_array {
                let uid = profile_table
                    .get("uid")
                    .with_context(|| "Profile UID should be present")?
                    .as_str()
                    .with_context(|| "UID should be a string")?
                    .to_owned();
                let name = profile_table
                    .get("name")
                    .with_context(|| "Profile Name should be present")?
                    .as_str()
                    .with_context(|| "Name should be a string")?
                    .to_owned();
                let p_type_str = profile_table
                    .get("p_type")
                    .with_context(|| "Profile type should be present")?
                    .as_str()
                    .with_context(|| "Profile type should be a string")?;
                let p_type = ProfileType::from_str(p_type_str)
                    .with_context(|| "Profile type should be a valid member")?;
                let speed_fixed = Self::get_speed_fixed(profile_table)?;
                let speed_profile = Self::get_speed_profile(profile_table)?;
                let temp_source = Self::get_temp_source(profile_table)?;
                let temp_function_default_uid_value = Item::Value(Value::String(Formatted::new(
                    DEFAULT_FUNCTION_UID.to_string(),
                )));
                let function_uid = profile_table
                    .get("function_uid")
                    .unwrap_or(&temp_function_default_uid_value)
                    .as_str()
                    .with_context(|| "function UID in Profile should be a string")?
                    .to_string();
                let member_profile_uids = Self::get_profile_uids(profile_table)?;
                let mix_function_type = Self::get_mix_function_type(profile_table)?;
                let profile = Profile {
                    uid,
                    p_type,
                    name,
                    speed_fixed,
                    speed_profile,
                    temp_source,
                    function_uid,
                    member_profile_uids,
                    mix_function_type,
                };
                profiles.push(profile);
            }
        }
        Ok(profiles)
    }

    /// Sets the order of stored profiles to that of the order of the give vector of profiles.
    /// It uses the UID to match and reuses the existing stored profiles.
    pub async fn set_profiles_order(&self, profiles_ordered: &[Profile]) -> Result<()> {
        let mut new_profiles_array_item = Item::ArrayOfTables(ArrayOfTables::new());
        if let Some(profiles_item) = self.document.read().await.get("profiles") {
            let profiles_array = profiles_item
                .as_array_of_tables()
                .with_context(|| "Profiles should be an array of tables")?;
            if profiles_ordered.len() != profiles_array.len() {
                return Err(anyhow!(
                    "The number of stored profiles and requested profiles to order \
                are not equal. Make sure all profiles have been created/deleted"
                ));
            }
            let new_profiles_array = new_profiles_array_item.as_array_of_tables_mut().unwrap();
            for profile in profiles_ordered {
                new_profiles_array.push(Self::find_profile_in_array(&profile.uid, profiles_array)?);
            }
        } else {
            return Err(anyhow!(
                "There are no stored profiles in the config to order."
            ));
        }
        self.document.write().await["profiles"] = new_profiles_array_item;
        Ok(())
    }

    /// Sets the given new Profile
    pub async fn set_profile(&self, profile: Profile) -> Result<()> {
        let mut doc = self.document.write().await;
        let profiles_array = doc["profiles"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let profile_already_exists = profiles_array
            .iter()
            .any(|p| p.get("uid").unwrap().as_str().unwrap_or_default() == profile.uid);
        if profile_already_exists {
            return Err(anyhow!(
                "Profile already exists. Use the patch operation to update it."
            ));
        }
        profiles_array.push(Self::create_profile_table_from(profile));
        Ok(())
    }

    pub async fn update_profile(&self, profile: Profile) -> Result<()> {
        let mut doc = self.document.write().await;
        let profiles_array = doc["profiles"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let found_profile = profiles_array
            .iter_mut()
            .find(|p| p.get("uid").unwrap().as_str().unwrap_or_default() == profile.uid);
        match found_profile {
            None => Err(anyhow!("Profile to update not found: {}", profile.uid)),
            Some(profile_table) => {
                Self::add_profile_properties_to_profile_table(profile, profile_table);
                Ok(())
            }
        }
    }

    pub async fn delete_profile(&self, profile_uid: &UID) -> Result<()> {
        let mut doc = self.document.write().await;
        let profiles_array = doc["profiles"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let index_to_delete = profiles_array
            .iter()
            .position(|p| p.get("uid").unwrap().as_str().unwrap_or_default() == profile_uid);
        match index_to_delete {
            None => Err(anyhow!("Profile to delete not found: {}", profile_uid)),
            Some(position) => {
                profiles_array.remove(position);
                Ok(())
            }
        }
    }

    fn find_profile_in_array(profile_uid: &UID, profiles_array: &ArrayOfTables) -> Result<Table> {
        for profile_table in profiles_array {
            if profile_table
                .get("uid")
                .with_context(|| "Profile UID should be present")?
                .as_str()
                .with_context(|| "UID should be a string")?
                == profile_uid
            {
                return Ok(profile_table.clone());
            }
        }
        Err(anyhow!(
            "Could not find Profile UID in existing profiles array."
        ))
    }

    /// Consumes the Profile and returns a new Profile Table
    fn create_profile_table_from(profile: Profile) -> Table {
        let mut new_profile = Table::new();
        Self::add_profile_properties_to_profile_table(profile, &mut new_profile);
        new_profile
    }

    fn add_profile_properties_to_profile_table(profile: Profile, profile_table: &mut Table) {
        profile_table["uid"] = Item::Value(Value::String(Formatted::new(profile.uid)));
        profile_table["name"] = Item::Value(Value::String(Formatted::new(profile.name)));
        profile_table["p_type"] =
            Item::Value(Value::String(Formatted::new(profile.p_type.to_string())));
        if let Some(speed_fixed) = profile.speed_fixed {
            profile_table["speed_fixed"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(speed_fixed))));
        } else {
            profile_table["speed_fixed"] = Item::None;
        }
        if let Some(speed_profile) = profile.speed_profile {
            let mut profile_array = toml_edit::Array::new();
            for (temp, duty) in speed_profile {
                let mut pair_array = toml_edit::Array::new();
                pair_array.push(Value::Float(Formatted::new(temp)));
                pair_array.push(Value::Integer(Formatted::new(i64::from(duty))));
                profile_array.push(pair_array);
            }
            profile_table["speed_profile"] = Item::Value(Value::Array(profile_array));
        } else {
            profile_table["speed_profile"] = Item::None;
        }
        if let Some(temp_source) = profile.temp_source {
            profile_table["temp_source"]["temp_name"] =
                Item::Value(Value::String(Formatted::new(temp_source.temp_name)));
            profile_table["temp_source"]["device_uid"] =
                Item::Value(Value::String(Formatted::new(temp_source.device_uid)));
        } else {
            profile_table["temp_source"] = Item::None;
        }
        profile_table["function_uid"] =
            Item::Value(Value::String(Formatted::new(profile.function_uid)));
        if profile.member_profile_uids.is_empty().not() {
            profile_table["member_profile_uids"] = Item::Value(Value::Array(
                profile
                    .member_profile_uids
                    .iter()
                    .map(|uid| Value::String(Formatted::new(uid.clone())))
                    .collect(),
            ));
        } else {
            profile_table["member_profile_uids"] = Item::None;
        }
        if let Some(mix_function_type) = profile.mix_function_type {
            profile_table["mix_function_type"] =
                Item::Value(Value::String(Formatted::new(mix_function_type.to_string())));
        } else {
            profile_table["mix_function_type"] = Item::None;
        }
    }

    /*
     *
     * FUNCTIONS
     *
     */

    /// Loads the current Function array from the config file.
    /// If none are set it returns the initial default Function,
    /// which should be always present.
    pub async fn get_functions(&self) -> Result<Vec<Function>> {
        let mut functions = self.get_current_functions().await?;
        if functions
            .iter()
            .any(|f| f.uid == *DEFAULT_FUNCTION_UID)
            .not()
        {
            // Default Function not found, probably the first time loading
            functions.push(Function::default());
            self.set_function(Function::default()).await?;
            self.save_config_file().await?;
        } else {
            // update original default function name
            functions
                .iter_mut()
                .filter(|f| f.uid == *DEFAULT_FUNCTION_UID && f.name == *"Identity")
                .for_each(|f| f.name = "Default Function".to_string());
        }
        Ok(functions)
    }

    async fn get_current_functions(&self) -> Result<Vec<Function>> {
        let mut functions = Vec::new();
        if let Some(functions_item) = self.document.read().await.get("functions") {
            let functions_array = functions_item
                .as_array_of_tables()
                .with_context(|| "Functions should be an array of tables")?;
            for function_table in functions_array {
                let uid = function_table
                    .get("uid")
                    .with_context(|| "Function UID should be present")?
                    .as_str()
                    .with_context(|| "UID should be a string")?
                    .to_owned();
                let name = function_table
                    .get("name")
                    .with_context(|| "Function Name should be present")?
                    .as_str()
                    .with_context(|| "Name should be a string")?
                    .to_owned();
                let f_type_str = function_table
                    .get("f_type")
                    .with_context(|| "Function type should be present")?
                    .as_str()
                    .with_context(|| "Function type should be a string")?;
                let f_type = FunctionType::from_str(f_type_str)
                    .with_context(|| "Function type should be a valid member")?;
                let mut duty_minimum: u8 = if let Some(duty_minimum_value) =
                    function_table.get("duty_minimum")
                {
                    let duty_minimum_raw: u8 = duty_minimum_value
                        .as_integer()
                        .with_context(|| "duty_minimum should be an integer")?
                        .try_into()
                        .ok()
                        .with_context(|| "duty_minimum should be an integer between 1 and 99")?;
                    duty_minimum_raw.clamp(2, 99)
                } else {
                    2
                };
                let duty_maximum: u8 = if let Some(duty_maximum_value) =
                    function_table.get("duty_maximum")
                {
                    let duty_maximum_raw: u8 = duty_maximum_value
                        .as_integer()
                        .with_context(|| "duty_maximum should be an integer")?
                        .try_into()
                        .ok()
                        .with_context(|| "duty_maximum should be an integer between 2 and 100")?;
                    duty_maximum_raw.clamp(2, 100)
                } else {
                    100
                };
                // sanity checks for user input values:
                if duty_minimum >= duty_maximum {
                    duty_minimum = duty_maximum - 1;
                }
                let response_delay = if let Some(delay_value) = function_table.get("response_delay")
                {
                    let delay: u8 = delay_value
                        .as_integer()
                        .with_context(|| "response_delay should be an integer")?
                        .try_into()
                        .ok()
                        .with_context(|| "response_delay must be a value between 0-255")?;
                    Some(delay)
                } else {
                    None
                };
                let deviance = if let Some(deviance_value) = function_table.get("deviance") {
                    let dev: f64 = deviance_value
                        .as_float()
                        .with_context(|| "deviance should be a valid float64 value")?;
                    Some(dev)
                } else {
                    None
                };
                let only_downward =
                    if let Some(downward_value) = function_table.get("only_downward") {
                        let downward: bool = downward_value
                            .as_bool()
                            .with_context(|| "only_downward should be a boolean value")?;
                        Some(downward)
                    } else {
                        None
                    };
                let sample_window =
                    if let Some(sample_window_value) = function_table.get("sample_window") {
                        let s_window: u8 = sample_window_value
                            .as_integer()
                            .with_context(|| "sample_window should be an integer")?
                            .try_into()
                            .ok()
                            .with_context(|| "sample_window should be a value between 1-16")?;
                        let validated_sample_window = if !(1..=16).contains(&s_window) {
                            TMA_DEFAULT_WINDOW_SIZE
                        } else {
                            s_window
                        };
                        Some(validated_sample_window)
                    } else {
                        None
                    };
                let function = Function {
                    uid,
                    name,
                    f_type,
                    duty_minimum,
                    duty_maximum,
                    response_delay,
                    deviance,
                    only_downward,
                    sample_window,
                };
                functions.push(function);
            }
        }
        Ok(functions)
    }

    /// Sets the order of stored functions to that of the order of the given vector of functions.
    /// It uses the UID to match and reuses the existing stored functions.
    pub async fn set_functions_order(&self, functions_ordered: &[Function]) -> Result<()> {
        let mut new_functions_array_item = Item::ArrayOfTables(ArrayOfTables::new());
        if let Some(functions_item) = self.document.read().await.get("functions") {
            let functions_array = functions_item
                .as_array_of_tables()
                .with_context(|| "Functions should be an array of tables")?;
            if functions_ordered.len() != functions_array.len() {
                return Err(anyhow!(
                    "The number of stored functions and requested functions to order \
                are not equal. Make sure all functions have been created/deleted"
                ));
            }
            let new_functions_array = new_functions_array_item.as_array_of_tables_mut().unwrap();
            for function in functions_ordered {
                new_functions_array.push(Self::find_function_in_array(
                    &function.uid,
                    functions_array,
                )?);
            }
        } else {
            return Err(anyhow!(
                "There are no stored functions in the config to order."
            ));
        }
        self.document.write().await["functions"] = new_functions_array_item;
        Ok(())
    }

    /// Sets the given new Function
    pub async fn set_function(&self, function: Function) -> Result<()> {
        let mut doc = self.document.write().await;
        let functions_array = doc["functions"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let function_already_exists = functions_array
            .iter()
            .any(|f| f.get("uid").unwrap().as_str().unwrap_or_default() == function.uid);
        if function_already_exists {
            return Err(anyhow!(
                "Function already exists. Use the update operation to update it."
            ));
        }
        functions_array.push(Self::create_function_table_from(function));
        Ok(())
    }

    pub async fn update_function(&self, function: Function) -> Result<()> {
        let mut doc = self.document.write().await;
        let functions_array = doc["functions"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let found_function = functions_array
            .iter_mut()
            .find(|f| f.get("uid").unwrap().as_str().unwrap_or_default() == function.uid);
        match found_function {
            None => Err(anyhow!("Function to update not found: {}", function.uid)),
            Some(function_table) => {
                Self::add_function_properties_to_function_table(function, function_table);
                Ok(())
            }
        }
    }

    pub async fn delete_function(&self, function_uid: &UID) -> Result<()> {
        let mut doc = self.document.write().await;
        let functions_array = doc["functions"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let index_to_delete = functions_array
            .iter()
            .position(|f| f.get("uid").unwrap().as_str().unwrap_or_default() == function_uid);
        match index_to_delete {
            None => Err(anyhow!("Function to delete not found: {}", function_uid)),
            Some(position) => {
                functions_array.remove(position);
                Ok(())
            }
        }
    }

    fn find_function_in_array(
        function_uid: &UID,
        functions_array: &ArrayOfTables,
    ) -> Result<Table> {
        for function_table in functions_array {
            if function_table
                .get("uid")
                .with_context(|| "Function UID should be present")?
                .as_str()
                .with_context(|| "UID should be a string")?
                == function_uid
            {
                return Ok(function_table.clone());
            }
        }
        Err(anyhow!(
            "Could not find function UID in existing functions array."
        ))
    }

    /// Consumes the Function and returns a new Function Table
    fn create_function_table_from(function: Function) -> Table {
        let mut new_function = Table::new();
        Self::add_function_properties_to_function_table(function, &mut new_function);
        new_function
    }

    fn add_function_properties_to_function_table(function: Function, function_table: &mut Table) {
        function_table["uid"] = Item::Value(Value::String(Formatted::new(function.uid)));
        function_table["name"] = Item::Value(Value::String(Formatted::new(function.name)));
        function_table["f_type"] =
            Item::Value(Value::String(Formatted::new(function.f_type.to_string())));
        function_table["duty_minimum"] = Item::Value(Value::Integer(Formatted::new(i64::from(
            function.duty_minimum,
        ))));
        function_table["duty_maximum"] = Item::Value(Value::Integer(Formatted::new(i64::from(
            function.duty_maximum,
        ))));
        if let Some(response_delay) = function.response_delay {
            function_table["response_delay"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(response_delay))));
        } else {
            function_table["response_delay"] = Item::None;
        }
        if let Some(deviance) = function.deviance {
            function_table["deviance"] = Item::Value(Value::Float(Formatted::new(deviance)));
        } else {
            function_table["deviance"] = Item::None;
        }
        if let Some(only_downward) = function.only_downward {
            function_table["only_downward"] =
                Item::Value(Value::Boolean(Formatted::new(only_downward)));
        } else {
            function_table["only_downward"] = Item::None;
        }
        if let Some(sample_window) = function.sample_window {
            let validated_window = if !(1..=16).contains(&sample_window) {
                TMA_DEFAULT_WINDOW_SIZE
            } else {
                sample_window
            };
            function_table["sample_window"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(validated_window))));
        } else {
            function_table["sample_window"] = Item::None;
        }
    }

    /*
     *
     * Custom Sensors
     *
     */

    pub async fn get_custom_sensors(&self) -> Result<Vec<CustomSensor>> {
        let mut custom_sensors = Vec::new();
        if let Some(custom_sensors_item) = self.document.read().await.get("custom_sensors") {
            let c_sensors_array = custom_sensors_item
                .as_array_of_tables()
                .with_context(|| "customer_sensors should be an array of tables")?;
            for c_sensor_table in c_sensors_array {
                let id = c_sensor_table
                    .get("id")
                    .with_context(|| "Sensor ID should be present")?
                    .as_str()
                    .with_context(|| "ID should be a string")?
                    .to_owned();
                let cs_type_str = c_sensor_table
                    .get("cs_type")
                    .with_context(|| "Sensor type should be present")?
                    .as_str()
                    .with_context(|| "Sensor type should be a string")?
                    .to_owned();
                let cs_type = CustomSensorType::from_str(&cs_type_str)
                    .with_context(|| "Sensor type should be a valid member")?;
                let mix_function_str = c_sensor_table
                    .get("mix_function")
                    .with_context(|| "mix_func_type should be present")?
                    .as_str()
                    .with_context(|| "mix_func_type should be a string")?
                    .to_owned();
                let mix_function = CustomSensorMixFunctionType::from_str(&mix_function_str)
                    .with_context(|| "mix_func_type should be a valid member")?;
                let mut sources = Vec::new();
                if let Some(sources_item) = c_sensor_table.get("sources") {
                    let sources_array = sources_item
                        .as_array_of_tables()
                        .with_context(|| "custom_sensors.sources should be an array")?;
                    for source_data_table in sources_array {
                        let temp_source =
                            Self::get_temp_source(source_data_table)?.with_context(|| {
                                "TempSource should always be present for Custom Sensor Sources"
                            })?;
                        let weight_raw: u8 = source_data_table
                            .get("weight")
                            .with_context(|| "weight should be present")?
                            .as_integer()
                            .with_context(|| "weight should be an integer")?
                            .try_into()
                            .ok()
                            .with_context(|| "weight must be a value between 1-254")?;
                        let weight = weight_raw.clamp(1, 254);
                        let custom_temp_source_data = CustomTempSourceData {
                            temp_source,
                            weight,
                        };
                        sources.push(custom_temp_source_data);
                    }
                }
                let file_path = if let Some(file_path_value) = c_sensor_table.get("file_path") {
                    let file_path_str = file_path_value
                        .as_str()
                        .with_context(|| "file_path should be a string")?
                        .to_string();
                    Some(Path::new(&file_path_str).to_path_buf())
                } else {
                    None
                };
                let custom_sensor = CustomSensor {
                    id,
                    cs_type,
                    mix_function,
                    sources,
                    file_path,
                };
                custom_sensors.push(custom_sensor);
            }
        }
        let mut ids = Vec::new();
        for custom_sensor in &custom_sensors {
            if ids.contains(&custom_sensor.id) {
                return Err(CCError::InternalError {
                    msg: "Custom Sensor IDs must be unique".to_string(),
                }
                .into());
            } else {
                ids.push(custom_sensor.id.clone());
            }
        }
        Ok(custom_sensors)
    }

    /// Sets the order of stored custom sensors to that of the order of the given vector of custom sensors.
    /// It uses the ID to match and reuses the existing stored custom sensor.
    pub async fn set_custom_sensor_order(&self, cs_ordered: &[CustomSensor]) -> Result<()> {
        let mut new_custom_sensors_array_item = Item::ArrayOfTables(ArrayOfTables::new());
        if let Some(custom_sensors_item) = self.document.read().await.get("custom_sensors") {
            let cs_array = custom_sensors_item
                .as_array_of_tables()
                .with_context(|| "Custom_Sensors should be an array of tables")?;
            if cs_ordered.len() != cs_array.len() {
                return Err(CCError::UserError {
                    msg:
                        "The number of stored custom_sensors and requested custom sensors to order \
                    are not equal. Make sure all functions have been created/deleted"
                            .to_string(),
                }
                .into());
            }
            let new_cs_array = new_custom_sensors_array_item
                .as_array_of_tables_mut()
                .unwrap();
            for custom_sensor in cs_ordered {
                new_cs_array.push(Self::find_custom_sensor_in_array(
                    &custom_sensor.id,
                    cs_array,
                )?);
            }
        } else {
            return Err(CCError::NotFound {
                msg: "There are no stored custom sensors in the config to order.".to_string(),
            }
            .into());
        }
        self.document.write().await["custom_sensors"] = new_custom_sensors_array_item;
        Ok(())
    }

    /// Sets the given new Custom Sensor
    pub async fn set_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        let mut doc = self.document.write().await;
        let cs_array = doc["custom_sensors"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let cs_already_exists = cs_array
            .iter()
            .any(|cs| cs.get("id").unwrap().as_str().unwrap_or_default() == custom_sensor.id);
        if cs_already_exists {
            return Err(CCError::UserError {
                msg: "Custom Sensor already exists. Use the update operation to update it."
                    .to_string(),
            }
            .into());
        }
        cs_array.push(Self::create_custom_sensor_table_from(custom_sensor));
        Ok(())
    }

    pub async fn update_custom_sensor(&self, custom_sensor: CustomSensor) -> Result<()> {
        let mut doc = self.document.write().await;
        let cs_array = doc["custom_sensors"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let found_custom_sensor = cs_array
            .iter_mut()
            .find(|cs| cs.get("id").unwrap().as_str().unwrap_or_default() == custom_sensor.id);
        match found_custom_sensor {
            None => Err(CCError::NotFound {
                msg: format!("Custom Sensor to update not found: {}", custom_sensor.id),
            }
            .into()),
            Some(cs_table) => {
                Self::add_custom_sensor_properties_to_custom_sensor_table(custom_sensor, cs_table);
                Ok(())
            }
        }
    }

    pub async fn delete_custom_sensor(&self, custom_sensor_id: &str) -> Result<()> {
        let mut doc = self.document.write().await;
        let cs_array = doc["custom_sensors"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        let index_to_delete = cs_array
            .iter()
            .position(|cs| cs.get("id").unwrap().as_str().unwrap_or_default() == custom_sensor_id);
        match index_to_delete {
            None => Err(CCError::NotFound {
                msg: format!("Custom Sensor to delete not found: {custom_sensor_id}"),
            }
            .into()),
            Some(position) => {
                cs_array.remove(position);
                Ok(())
            }
        }
    }

    fn find_custom_sensor_in_array(
        custom_sensor_id: &String,
        cs_array: &ArrayOfTables,
    ) -> Result<Table> {
        for cs_table in cs_array {
            if cs_table
                .get("id")
                .with_context(|| "Custom Sensor ID should be present")?
                .as_str()
                .with_context(|| "Custom Sensor ID should be a string")?
                == custom_sensor_id
            {
                return Ok(cs_table.clone());
            }
        }
        Err(CCError::NotFound {
            msg: "Could not find Custom Sensor ID in existing functions array.".to_string(),
        }
        .into())
    }

    /// Consumes the `CustomSensor` and returns a new `CustomSensor` Table
    fn create_custom_sensor_table_from(custom_sensor: CustomSensor) -> Table {
        let mut new_custom_sensor = Table::new();
        Self::add_custom_sensor_properties_to_custom_sensor_table(
            custom_sensor,
            &mut new_custom_sensor,
        );
        new_custom_sensor
    }

    fn add_custom_sensor_properties_to_custom_sensor_table(
        custom_sensor: CustomSensor,
        cs_table: &mut Table,
    ) {
        cs_table["id"] = Item::Value(Value::String(Formatted::new(custom_sensor.id)));
        cs_table["cs_type"] = Item::Value(Value::String(Formatted::new(
            custom_sensor.cs_type.to_string(),
        )));
        cs_table["mix_function"] = Item::Value(Value::String(Formatted::new(
            custom_sensor.mix_function.to_string(),
        )));
        let sources_array = cs_table["sources"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        sources_array.clear(); // remove any existing temp sources
        for source in &custom_sensor.sources {
            let mut source_table = Table::new();
            source_table["temp_source"]["temp_name"] = Item::Value(Value::String(Formatted::new(
                source.temp_source.temp_name.clone(),
            )));
            source_table["temp_source"]["device_uid"] = Item::Value(Value::String(Formatted::new(
                source.temp_source.device_uid.clone(),
            )));
            source_table["weight"] =
                Item::Value(Value::Integer(Formatted::new(i64::from(source.weight))));
            sources_array.push(source_table);
        }
        if let Some(file_path) = custom_sensor.file_path {
            cs_table["file_path"] = Item::Value(Value::String(Formatted::new(
                file_path.to_string_lossy().to_string(),
            )));
        } else {
            cs_table["file_path"] = Item::None;
        }
    }
}

fn normalized(name: &str) -> String {
    name.to_lowercase().replace('_', " ")
}
