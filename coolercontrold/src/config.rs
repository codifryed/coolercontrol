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
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use const_format::concatcp;
use log::{debug, error, info, warn};
use tokio::sync::RwLock;
use toml_edit::{ArrayOfTables, Document, Formatted, Item, Table, Value};

use crate::device::UID;
use crate::repositories::repository::DeviceLock;
use crate::setting::{CoolerControlDeviceSettings, CoolerControlSettings, Function, FunctionType, LcdSettings, LightingSettings, Profile, ProfileType, Setting, TempSource};

pub const DEFAULT_CONFIG_DIR: &str = "/etc/coolercontrol";
const DEFAULT_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/config.toml");
const DEFAULT_UI_CONFIG_FILE_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/config-ui.json");
const DEFAULT_CONFIG_FILE_BYTES: &[u8] = include_bytes!("../resources/config-default.toml");

pub struct Config {
    path: PathBuf,
    path_ui: PathBuf,
    document: RwLock<Document>,
}

impl Config {
    /// loads the configuration file data into memory
    pub async fn load_config_file() -> Result<Self> {
        let config_dir = Path::new(DEFAULT_CONFIG_DIR);
        if !config_dir.exists() {
            warn!("config directory doesn't exist. Attempting to create it: {}", DEFAULT_CONFIG_DIR);
            tokio::fs::create_dir_all(&config_dir).await?;
        }
        let path = Path::new(DEFAULT_CONFIG_FILE_PATH).to_path_buf();
        let path_ui = Path::new(DEFAULT_UI_CONFIG_FILE_PATH).to_path_buf();
        let config_contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(err) => {
                warn!("Error trying to read configuration file: {}", err);
                warn!("Attempting to write a new configuration file");
                tokio::fs::write(&path, DEFAULT_CONFIG_FILE_BYTES).await
                    .with_context(|| format!("Writing new configuration file: {:?}", path))?;
                tokio::fs::read_to_string(&path).await
                    .with_context(|| format!("Reading configuration file {:?}", path))?
            }
        };
        let document = config_contents.parse::<Document>()
            .with_context(|| "Parsing configuration file")?;
        debug!("Loaded configuration file:\n{}", document);
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
        info!("Configuration file check successful");
        Ok(config)
    }

    /// saves any changes to the configuration file - preserving formatting and comments
    pub async fn save_config_file(&self) -> Result<()> {
        tokio::fs::write(
            &self.path, self.document.read().await.to_string(),
        ).await.with_context(|| format!("Saving configuration file: {:?}", &self.path))
    }

    pub async fn save_ui_config_file(&self, ui_settings: &String) -> Result<()> {
        tokio::fs::write(
            &self.path_ui, ui_settings,
        ).await.with_context(|| format!("Saving UI configuration file: {:?}", &self.path_ui))
    }

    pub async fn load_ui_config_file(&self) -> Result<String> {
        tokio::fs::read_to_string(&self.path_ui)
            .await.with_context(|| format!("Loading UI configuration file {:?}", &self.path_ui))
    }

    /// This adds a human readable device list with UIDs to the config file
    pub async fn create_device_list(&self, devices: Arc<HashMap<UID, DeviceLock>>) -> Result<()> {
        for (uid, device) in devices.iter() {
            self.document.write().await["devices"][uid.as_str()] = Item::Value(
                Value::String(Formatted::new(device.read().await.name.clone()))
            )
        }
        Ok(())
    }

    pub async fn legacy690_ids(&self) -> Result<HashMap<String, bool>> {
        let mut legacy690_ids = HashMap::new();
        if let Some(table) = self.document.read().await["legacy690"].as_table() {
            for (key, value) in table.iter() {
                legacy690_ids.insert(
                    key.to_string(),
                    value.as_bool().with_context(|| "Parsing boolean value for legacy690")?,
                );
            }
        }
        Ok(legacy690_ids)
    }

    pub async fn set_legacy690_id(&self, device_uid: &str, is_legacy690: &bool) {
        self.document.write().await["legacy690"][device_uid] = Item::Value(
            Value::Boolean(Formatted::new(
                *is_legacy690
            ))
        );
    }

    pub async fn set_device_setting(&self, device_uid: &str, setting: &Setting) {
        {
            let mut doc = self.document.write().await;
            let device_settings = doc["device-settings"][device_uid]
                .or_insert(Item::Table(Table::new()));
            let channel_setting = &mut device_settings[setting.channel_name.as_str()];
            if let Some(pwm_mode) = setting.pwm_mode {
                channel_setting["pwm_mode"] = Item::Value(
                    Value::Integer(Formatted::new(pwm_mode as i64))
                );
            }
            if setting.reset_to_default.unwrap_or(false) {
                *channel_setting = Item::None;  // removes channel from settings
            } else if let Some(speed_fixed) = setting.speed_fixed {
                Self::set_setting_fixed_speed(channel_setting, speed_fixed);
            } else if let Some(profile) = &setting.speed_profile {
                Self::set_setting_speed_profile(channel_setting, setting, profile)
            } else if let Some(lighting) = &setting.lighting {
                Self::set_setting_lighting(channel_setting, lighting);
                if &setting.channel_name != "sync" {
                    device_settings["sync"] = Item::None;
                }
            } else if let Some(lcd) = &setting.lcd {
                Self::set_setting_lcd(channel_setting, setting, lcd);
            }
        }
    }

    fn set_setting_fixed_speed(channel_setting: &mut Item, speed_fixed: u8) {
        channel_setting["speed_profile"] = Item::None;  // clear profile setting
        channel_setting["temp_source"] = Item::None; // clear fixed setting
        channel_setting["speed_fixed"] = Item::Value(
            Value::Integer(Formatted::new(speed_fixed as i64))
        );
    }

    fn set_setting_speed_profile(channel_setting: &mut Item, setting: &Setting, profile: &Vec<(u8, u8)>) {
        let mut profile_array = toml_edit::Array::new();
        for (temp, duty) in profile.clone() {
            let mut pair_array = toml_edit::Array::new();
            pair_array.push(Value::Integer(Formatted::new(temp as i64)));
            pair_array.push(Value::Integer(Formatted::new(duty as i64)));
            profile_array.push(pair_array);
        }
        channel_setting["speed_fixed"] = Item::None; // clear fixed setting
        channel_setting["speed_profile"] = Item::Value(
            Value::Array(profile_array)
        );
        if let Some(temp_source) = &setting.temp_source {
            channel_setting["temp_source"]["temp_name"] = Item::Value(
                Value::String(Formatted::new(temp_source.temp_name.clone()))
            );
            channel_setting["temp_source"]["device_uid"] = Item::Value(
                Value::String(Formatted::new(temp_source.device_uid.clone()))
            );
        }
    }

    fn set_setting_lighting(channel_setting: &mut Item, lighting: &LightingSettings) {
        channel_setting["lighting"] = Item::None;
        channel_setting["lighting"]["mode"] = Item::Value(
            Value::String(Formatted::new(lighting.mode.clone()))
        );
        if let Some(speed) = &lighting.speed {
            channel_setting["lighting"]["speed"] = Item::Value(
                Value::String(Formatted::new(speed.clone()))
            );
        }
        if lighting.backward.unwrap_or(false) {
            channel_setting["lighting"]["backward"] = Item::Value(
                Value::Boolean(Formatted::new(true))
            );
        }
        let mut color_array = toml_edit::Array::new();
        for (r, g, b) in lighting.colors.clone() {
            let mut rgb_array = toml_edit::Array::new();
            rgb_array.push(Value::Integer(Formatted::new(r as i64)));
            rgb_array.push(Value::Integer(Formatted::new(g as i64)));
            rgb_array.push(Value::Integer(Formatted::new(b as i64)));
            color_array.push(rgb_array);
        }
        channel_setting["lighting"]["colors"] = Item::Value(
            Value::Array(color_array)
        );
    }

    fn set_setting_lcd(channel_setting: &mut Item, setting: &Setting, lcd: &LcdSettings) {
        channel_setting["lcd"] = Item::None;
        channel_setting["lcd"]["mode"] = Item::Value(
            Value::String(Formatted::new(lcd.mode.clone()))
        );
        if let Some(brightness) = lcd.brightness {
            channel_setting["lcd"]["brightness"] = Item::Value(
                Value::Integer(Formatted::new(brightness as i64))
            );
        }
        if let Some(orientation) = lcd.orientation {
            channel_setting["lcd"]["orientation"] = Item::Value(
                Value::Integer(Formatted::new(orientation as i64))
            );
        }
        if let Some(image_file_src) = &lcd.image_file_src {
            channel_setting["lcd"]["image_file_src"] = Item::Value(
                Value::String(Formatted::new(image_file_src.clone()))
            );
        }
        if let Some(image_file_processed) = &lcd.image_file_processed {
            // We copy the processed image file from /tmp to our config directory and use that at startup
            let tmp_path = Path::new(image_file_processed);
            if let Some(image_file_name) = tmp_path.file_name() {
                let daemon_config_image_path = Path::new(DEFAULT_CONFIG_DIR)
                    .join(image_file_name);
                let daemon_config_image_path_str = daemon_config_image_path.to_str().unwrap().to_string();
                match std::fs::copy(tmp_path, daemon_config_image_path) {
                    Ok(_) => channel_setting["lcd"]["image_file_processed"] = Item::Value(
                        Value::String(Formatted::new(daemon_config_image_path_str))
                    ),
                    Err(err) => error!("Error copying processed image for for daemon: {}", err)
                }
            }
        }
        let mut color_array = toml_edit::Array::new();
        for (r, g, b) in lcd.colors.clone() {
            let mut rgb_array = toml_edit::Array::new();
            rgb_array.push(Value::Integer(Formatted::new(r as i64)));
            rgb_array.push(Value::Integer(Formatted::new(g as i64)));
            rgb_array.push(Value::Integer(Formatted::new(b as i64)));
            color_array.push(rgb_array);
        }
        channel_setting["lcd"]["colors"] = Item::Value(
            Value::Array(color_array)
        );
        if let Some(temp_source) = &setting.temp_source {
            channel_setting["temp_source"]["temp_name"] = Item::Value(
                Value::String(Formatted::new(temp_source.temp_name.clone()))
            );
            channel_setting["temp_source"]["device_uid"] = Item::Value(
                Value::String(Formatted::new(temp_source.device_uid.clone()))
            );
        }
    }

    /// Retrieves the device settings from the config file to our Setting model.
    /// This has to be done defensively, as the user may change the config file.
    pub async fn get_device_settings(&self, device_uid: &str) -> Result<Vec<Setting>> {
        let mut settings = Vec::new();
        if let Some(table_item) = self.document.read().await["device-settings"].get(device_uid) {
            let table = table_item.as_table().with_context(|| "device setting should be a table")?;
            for (channel_name, base_item) in table.iter() {
                let setting_table = base_item.as_inline_table()
                    .with_context(|| "Channel Setting should be an inline table")?
                    .clone().into_table();
                let speed_fixed = Self::get_speed_fixed(&setting_table)?;
                let speed_profile = Self::get_speed_profile(&setting_table)?;
                let temp_source = Self::get_temp_source(&setting_table)?;
                let lighting = Self::get_lighting(&setting_table)?;
                let lcd = Self::get_lcd(&setting_table)?;
                let pwm_mode = Self::get_pwm_mode(&setting_table)?;
                settings.push(Setting {
                    channel_name: channel_name.to_string(),
                    speed_fixed,
                    speed_profile,
                    temp_source,
                    lighting,
                    lcd,
                    pwm_mode,
                    reset_to_default: None,
                });
            }
        }
        Ok(settings)
    }

    async fn get_all_devices_settings(&self) -> Result<HashMap<UID, Vec<Setting>>> {
        let mut devices_settings = HashMap::new();
        if let Some(device_table) = self.document.read().await["device-settings"].as_table() {
            for (device_uid, _value) in device_table {
                let settings = self.get_device_settings(device_uid).await?;
                devices_settings.insert(device_uid.to_string(), settings);
            }
        }
        Ok(devices_settings)
    }

    async fn get_all_cc_devices_settings(&self) -> Result<HashMap<UID, Option<CoolerControlDeviceSettings>>> {
        let mut devices_settings = HashMap::new();
        if let Some(device_table) = self.document.read().await["settings"].as_table() {
            for (device_uid, _value) in device_table {
                if device_uid.len() == 64 { // there are other settings here, we want only the ones with proper UIDs
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
                .as_integer().with_context(|| "speed_fixed should be an integer")?
                .try_into().ok().with_context(|| "speed_fixed must be a value between 0-100")?;
            Some(speed)
        } else { None };
        Ok(speed_fixed)
    }

    fn get_speed_profile(setting_table: &Table) -> Result<Option<Vec<(u8, u8)>>> {
        let speed_profile = if let Some(value) = setting_table.get("speed_profile") {
            let mut profiles = Vec::new();
            let speeds = value.as_array().with_context(|| "profile should be an array")?;
            for profile_pair_value in speeds.iter() {
                let profile_pair_array = profile_pair_value.as_array()
                    .with_context(|| "profile pairs should be an array")?;
                let temp: u8 = profile_pair_array.get(0)
                    .with_context(|| "Speed Profiles must be pairs")?
                    .as_integer().with_context(|| "Speed Profiles must be integers")?
                    .try_into().ok().with_context(|| "speed profiles must be values between 0-100")?;
                let speed: u8 = profile_pair_array.get(1)
                    .with_context(|| "Speed Profiles must be pairs")?
                    .as_integer().with_context(|| "Speed Profiles must be integers")?
                    .try_into().ok().with_context(|| "speed profiles must be values between 0-100")?;
                profiles.push((temp, speed));
            }
            Some(profiles)
        } else { None };
        Ok(speed_profile)
    }

    fn get_temp_source(setting_table: &Table) -> Result<Option<TempSource>> {
        let temp_source = if let Some(value) = setting_table.get("temp_source") {
            let temp_source_table = value.as_inline_table()
                .with_context(|| "temp_source should be an inline table")?;
            let temp_name = temp_source_table.get("temp_name")
                .with_context(|| "temp_source must have temp_name and device_uid set")?
                .as_str().with_context(|| "temp_name should be a String")?
                .to_string();
            let device_uid = temp_source_table.get("device_uid")
                .with_context(|| "temp_source must have frontend_temp_name and device_uid set")?
                .as_str().with_context(|| "device_uid should be a String")?
                .to_string();
            Some(TempSource {
                temp_name,
                device_uid,
            })
        } else { None };
        Ok(temp_source)
    }

    fn get_lighting(setting_table: &Table) -> Result<Option<LightingSettings>> {
        let lighting = if let Some(value) = setting_table.get("lighting") {
            let lighting_table = value.as_inline_table()
                .with_context(|| "lighting should be an inline table")?;
            let mode = lighting_table.get("mode")
                .with_context(|| "lighting.mode should be present")?
                .as_str().with_context(|| "lighting.mode should be a String")?
                .to_string();
            let speed = if let Some(value) = lighting_table.get("speed") {
                Some(value
                    .as_str().with_context(|| "lighting.speed should be a String")?
                    .to_string()
                )
            } else { None };
            let backward = if let Some(value) = setting_table.get("backward") {
                Some(value.as_bool().with_context(|| "lighting.backward should be a boolean")?)
            } else { None };
            let mut colors = Vec::new();
            let colors_array = lighting_table.get("colors")
                .with_context(|| "lighting.colors should always be present")?
                .as_array().with_context(|| "lighting.colors should be an array")?;
            for rgb_value in colors_array {
                let rgb_array = rgb_value.as_array()
                    .with_context(|| "RGB values should be an array")?;
                let r: u8 = rgb_array.get(0)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer().with_context(|| "RGB values must be integers")?
                    .try_into().ok().with_context(|| "RGB values must be between 0-255")?;
                let g: u8 = rgb_array.get(1)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer().with_context(|| "RGB values must be integers")?
                    .try_into().ok().with_context(|| "RGB values must be between 0-255")?;
                let b: u8 = rgb_array.get(2)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer().with_context(|| "RGB values must be integers")?
                    .try_into().ok().with_context(|| "RGB values must be between 0-255")?;
                colors.push((r, g, b))
            }
            Some(LightingSettings {
                mode,
                speed,
                backward,
                colors,
            })
        } else { None };
        Ok(lighting)
    }


    fn get_lcd(setting_table: &Table) -> Result<Option<LcdSettings>> {
        let lcd = if let Some(value) = setting_table.get("lcd") {
            let lcd_table = value.as_inline_table()
                .with_context(|| "lcd should be an inline table")?;
            let mode = lcd_table.get("mode")
                .with_context(|| "lcd.mode should be present")?
                .as_str().with_context(|| "lcd.mode should be a String")?
                .to_string();
            let brightness = if let Some(brightness_value) = lcd_table.get("brightness") {
                let brightness_u8: u8 = brightness_value.as_integer()
                    .with_context(|| "brightness should be an integer")?
                    .try_into().ok().with_context(|| "brightness should be a value between 0-100")?;
                Some(brightness_u8)
            } else { None };
            let orientation = if let Some(orientation_value) = lcd_table.get("orientation") {
                let orientation_u16: u16 = orientation_value.as_integer()
                    .with_context(|| "orientation should be an integer")?
                    .try_into().ok().with_context(|| "orientation should be a value between 0-270")?;
                Some(orientation_u16)
            } else { None };
            let image_file_src = if let Some(image_file_src_value) = lcd_table.get("image_file_src") {
                Some(image_file_src_value
                    .as_str().with_context(|| "image_file_src should be a String")?
                    .to_string()
                )
            } else { None };
            let image_file_processed = if let Some(image_file_processed_value) = lcd_table.get("image_file_processed") {
                Some(image_file_processed_value
                    .as_str().with_context(|| "image_file_processed should be a String")?
                    .to_string()
                )
            } else { None };
            let mut colors = Vec::new();
            let colors_array = lcd_table.get("colors")
                .with_context(|| "lcd.colors should always be present")?
                .as_array().with_context(|| "lcd.colors should be an array")?;
            for rgb_value in colors_array {
                let rgb_array = rgb_value.as_array()
                    .with_context(|| "RGB values should be an array")?;
                let r: u8 = rgb_array.get(0)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer().with_context(|| "RGB values must be integers")?
                    .try_into().ok().with_context(|| "RGB values must be between 0-255")?;
                let g: u8 = rgb_array.get(1)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer().with_context(|| "RGB values must be integers")?
                    .try_into().ok().with_context(|| "RGB values must be between 0-255")?;
                let b: u8 = rgb_array.get(2)
                    .with_context(|| "RGB values must be in arrays of 3")?
                    .as_integer().with_context(|| "RGB values must be integers")?
                    .try_into().ok().with_context(|| "RGB values must be between 0-255")?;
                colors.push((r, g, b))
            }
            Some(LcdSettings {
                mode,
                brightness,
                orientation,
                image_file_src,
                image_file_processed,
                colors,
            })
        } else { None };
        Ok(lcd)
    }

    fn get_pwm_mode(setting_table: &Table) -> Result<Option<u8>> {
        let pwm_mode = if let Some(value) = setting_table.get("pwm_mode") {
            let p_mode: u8 = value
                .as_integer().with_context(|| "pwm_mode should be an integer")?
                .try_into().ok().with_context(|| "pwm_mode should be a value between 0-2")?;
            Some(p_mode)
        } else { None };
        Ok(pwm_mode)
    }

    /// Returns CoolerControl general settings
    pub async fn get_settings(&self) -> Result<CoolerControlSettings> {
        if let Some(settings_item) = self.document.read().await.get("settings") {
            let settings = settings_item.as_table().with_context(|| "Settings should be a table")?;
            let apply_on_boot = settings.get("apply_on_boot")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(true))))
                .as_bool().with_context(|| "apply_on_boot should be a boolean value")?;
            let no_init = settings.get("no_init")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool().with_context(|| "no_init should be a boolean value")?;
            let handle_dynamic_temps = settings.get("handle_dynamic_temps")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool().with_context(|| "handle_dynamic_temps should be a boolean value")?;
            let startup_delay = Duration::from_secs(
                settings.get("startup_delay")
                    .unwrap_or(&Item::Value(Value::Integer(Formatted::new(2))))
                    .as_integer().with_context(|| "startup_delay should be an integer value")?
                    .max(0)
                    .min(10) as u64
            );
            let smoothing_level = settings.get("smoothing_level")
                .unwrap_or(&Item::Value(Value::Integer(Formatted::new(0))))
                .as_integer().with_context(|| "smoothing_level should be an integer value")?
                .max(0)
                .min(5) as u8;
            let thinkpad_full_speed = settings.get("thinkpad_full_speed")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool().with_context(|| "thinkpad_full_speed should be a boolean value")?;
            Ok(CoolerControlSettings {
                apply_on_boot,
                no_init,
                handle_dynamic_temps,
                startup_delay,
                smoothing_level,
                thinkpad_full_speed,
            })
        } else {
            Err(anyhow!("Setting table not found in configuration file"))
        }
    }

    /// Sets CoolerControl settings
    pub async fn set_settings(&self, cc_settings: &CoolerControlSettings) {
        let mut doc = self.document.write().await;
        let base_settings = doc["settings"].or_insert(Item::Table(Table::new()));
        base_settings["apply_on_boot"] = Item::Value(
            Value::Boolean(Formatted::new(cc_settings.apply_on_boot))
        );
        base_settings["no_init"] = Item::Value(
            Value::Boolean(Formatted::new(cc_settings.no_init))
        );
        base_settings["handle_dynamic_temps"] = Item::Value(
            Value::Boolean(Formatted::new(cc_settings.handle_dynamic_temps))
        );
        base_settings["startup_delay"] = Item::Value(
            Value::Integer(Formatted::new(cc_settings.startup_delay.as_secs() as i64))
        );
        base_settings["smoothing_level"] = Item::Value(
            Value::Integer(Formatted::new(cc_settings.smoothing_level as i64))
        );
        base_settings["thinkpad_full_speed"] = Item::Value(
            Value::Boolean(Formatted::new(cc_settings.thinkpad_full_speed))
        );
    }

    /// This gets the CoolerControl settings for specific devices
    /// This differs from Device Settings, in that these settings are applied in CoolerControl,
    /// and not on the devices themselves.
    pub async fn get_cc_settings_for_device(
        &self, device_uid: &str,
    ) -> Result<Option<CoolerControlDeviceSettings>> {
        if let Some(table_item) = self.document.read().await["settings"].get(device_uid) {
            let device_settings_table = table_item.as_table()
                .with_context(|| "CoolerControl device settings should be a table")?;
            let disable = device_settings_table.get("disable")
                .unwrap_or(&Item::Value(Value::Boolean(Formatted::new(false))))
                .as_bool().with_context(|| "disable should be a boolean value")?;
            Ok(Some(CoolerControlDeviceSettings {
                disable
            }))
        } else {
            Ok(None)
        }
    }

    #[allow(dead_code)] // todo: remove once we implement ignoring a device from the frontend
    /// Sets CoolerControl device settings
    pub async fn set_cc_settings_for_device(
        &self,
        device_uid: &str,
        cc_device_settings: &CoolerControlDeviceSettings,
    ) {
        let mut doc = self.document.write().await;
        let device_settings_table = doc["settings"][device_uid]
            .or_insert(Item::Table(Table::new()));
        device_settings_table["disable"] = Item::Value(
            Value::Boolean(Formatted::new(cc_device_settings.disable))
        );
    }

    /// Loads the current Profile array from the config file.
    /// If there are none setup yet, it returns the initial default Profile,
    /// which should always be present.
    pub async fn get_profiles(&self) -> Result<Vec<Profile>> {
        let mut profiles = Vec::new();
        if let Some(profiles_item) = self.document.read().await.get("profiles") {
            let profiles_array = profiles_item.as_array_of_tables()
                .with_context(|| "Profiles should be an array of tables")?;
            for profile_table in profiles_array.iter() {
                let uid = profile_table.get("uid")
                    .with_context(|| "Profile UID should be present")?
                    .as_str().with_context(|| "UID should be a string")?
                    .to_owned();
                let name = profile_table.get("name")
                    .with_context(|| "Profile Name should be present")?
                    .as_str().with_context(|| "Name should be a string")?
                    .to_owned();
                let p_type_str = profile_table.get("p_type")
                    .with_context(|| "Profile type should be present")?
                    .as_str().with_context(|| "Profile type should be a string")?;
                let p_type = ProfileType::from_str(p_type_str)
                    .with_context(|| "Profile type should be a valid member")?;
                let speed_fixed = Self::get_speed_fixed(profile_table)?;
                let speed_profile = Self::get_speed_profile(profile_table)?;
                let temp_source = Self::get_temp_source(profile_table)?;
                let temp_function_default_uid_value = Item::Value(Value::String(Formatted::new("0".to_string())));
                let function_uid = profile_table.get("function_uid")
                    .unwrap_or(&temp_function_default_uid_value)
                    .as_str().with_context(|| "function UID in Profile should be a string")?
                    .to_string();
                let profile = Profile {
                    uid,
                    p_type,
                    name,
                    speed_fixed,
                    speed_profile,
                    temp_source,
                    function_uid,
                };
                profiles.push(profile);
            }
        } else {
            profiles.push(Profile::default());
        }
        Ok(profiles)
    }

    /// Sets the array of profiles, replacing the existing array
    pub async fn set_profiles(&self, profiles_param: &Vec<Profile>) {
        let mut profiles = profiles_param.clone();
        let mut doc = self.document.write().await;
        let profiles_array = doc["profiles"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        profiles_array.clear();
        let default_profile_is_not_present = profiles.iter().find(|p| p.uid == "0").is_none();
        if default_profile_is_not_present {
            profiles.push(Profile::default())
        }
        for profile in profiles.into_iter() {
            let mut new_profile = Table::new();
            new_profile["uid"] = Item::Value(Value::String(Formatted::new(profile.uid)));
            new_profile["name"] = Item::Value(Value::String(Formatted::new(profile.name)));
            new_profile["p_type"] = Item::Value(Value::String(Formatted::new(profile.p_type.to_string())));
            if let Some(speed_fixed) = profile.speed_fixed {
                new_profile["speed_fixed"] = Item::Value(
                    Value::Integer(Formatted::new(speed_fixed as i64))
                );
            }
            if let Some(speed_profile) = profile.speed_profile {
                let mut profile_array = toml_edit::Array::new();
                for (temp, duty) in speed_profile {
                    let mut pair_array = toml_edit::Array::new();
                    pair_array.push(Value::Integer(Formatted::new(temp as i64)));
                    pair_array.push(Value::Integer(Formatted::new(duty as i64)));
                    profile_array.push(pair_array);
                }
                new_profile["speed_profile"] = Item::Value(Value::Array(profile_array));
            }
            if let Some(temp_source) = profile.temp_source {
                new_profile["temp_source"]["temp_name"] = Item::Value(
                    Value::String(Formatted::new(temp_source.temp_name))
                );
                new_profile["temp_source"]["device_uid"] = Item::Value(
                    Value::String(Formatted::new(temp_source.device_uid))
                );
            }
            new_profile["function_uid"] = Item::Value(Value::String(Formatted::new(profile.function_uid)));
            profiles_array.push(new_profile);
        }
    }

    /// Loads the current Function array from the config file.
    /// If none are set it returns the initial default Function,
    /// which should be always present.
    pub async fn get_functions(&self) -> Result<Vec<Function>> {
        let mut functions = Vec::new();
        if let Some(functions_item) = self.document.read().await.get("functions") {
            let functions_array = functions_item.as_array_of_tables()
                .with_context(|| "Functions should be an array of tables")?;
            for function_table in functions_array.iter() {
                let uid = function_table.get("uid")
                    .with_context(|| "Function UID should be present")?
                    .as_str().with_context(|| "UID should be a string")?
                    .to_owned();
                let name = function_table.get("name")
                    .with_context(|| "Function Name should be present")?
                    .as_str().with_context(|| "Name should be a string")?
                    .to_owned();
                let f_type_str = function_table.get("f_type")
                    .with_context(|| "Function type should be present")?
                    .as_str().with_context(|| "Function type should be a string")?;
                let f_type = FunctionType::from_str(f_type_str)
                    .with_context(|| "Function type should be a valid member")?;
                let response_delay = if let Some(delay_value) = function_table
                    .get("response_delay") {
                    let delay: u8 = delay_value
                        .as_integer().with_context(|| "response_delay should be an integer")?
                        .try_into().ok().with_context(|| "response_delay must be a value between 0-255")?;
                    Some(delay)
                } else { None };
                let deviance = if let Some(deviance_value) = function_table.get("deviance") {
                    let dev: f64 = deviance_value
                        .as_float().with_context(|| "deviance should be a float")?
                        .try_into().ok().with_context(|| "deviance should be a valid float64 value")?;
                    Some(dev)
                } else { None };
                let sample_window = if let Some(sample_window_value) = function_table.get("sample_window") {
                    let s_window: u16 = sample_window_value
                        .as_integer().with_context(|| "sample_window should be an integer")?
                        .try_into().ok().with_context(|| "sample_window should be a value between 0-65_535")?;
                    Some(s_window)
                } else { None };
                let function = Function {
                    uid,
                    name,
                    f_type,
                    response_delay,
                    deviance,
                    sample_window,
                };
                functions.push(function);
            }
        } else {
            functions.push(Function::default())
        }
        Ok(functions)
    }

    /// Sets the array of functions, replacing the current array
    pub async fn set_functions(&self, functions_param: &Vec<Function>) {
        let mut functions = functions_param.clone();
        let mut doc = self.document.write().await;
        let functions_array = doc["functions"]
            .or_insert(Item::ArrayOfTables(ArrayOfTables::new()))
            .as_array_of_tables_mut()
            .unwrap();
        functions_array.clear();
        let default_function_is_not_present = functions.iter().find(|p| p.uid == "0").is_none();
        if default_function_is_not_present {
            functions.push(Function::default())
        }
        for function in functions.into_iter() {
            let mut new_function = Table::new();
            new_function["uid"] = Item::Value(Value::String(Formatted::new(function.uid)));
            new_function["name"] = Item::Value(Value::String(Formatted::new(function.name)));
            new_function["f_type"] = Item::Value(Value::String(Formatted::new(function.f_type.to_string())));
            if let Some(response_delay) = function.response_delay {
                new_function["response_delay"] = Item::Value(
                    Value::Integer(Formatted::new(response_delay as i64))
                );
            }
            if let Some(deviance) = function.deviance {
                new_function["deviance"] = Item::Value(
                    Value::Float(Formatted::new(deviance))
                );
            }
            if let Some(sample_window) = function.sample_window {
                new_function["sample_window"] = Item::Value(
                    Value::Integer(Formatted::new(sample_window as i64))
                );
            }
            functions_array.push(new_function);
        }
    }
}
