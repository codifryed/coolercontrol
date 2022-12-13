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
use std::sync::Arc;

use anyhow::{Context, Result};
use log::{debug, warn};
use tokio::sync::RwLock;
use toml_edit::{Document, Formatted, Item, Value};

use crate::device::UID;
use crate::repositories::repository::DeviceLock;
use crate::setting::{LcdSettings, LightingSettings, Setting};

const DEFAULT_CONFIG_FILE_PATH: &str = "/etc/coolercontrol/config.toml";

pub struct Config {
    path: PathBuf,
    document: RwLock<Document>,
}

impl Config {
    /// loads the configuration file data into memory
    pub async fn load_config_file() -> Result<Self> {
        // todo: load alternate config file if none found... (AppImage)
        let path = Path::new(DEFAULT_CONFIG_FILE_PATH).to_path_buf();
        let config_contents = match tokio::fs::read_to_string(&path).await {
            Ok(contents) => contents,
            Err(err) => {
                warn!("Error trying to read configuration file: {}", err);
                warn!("Attempting to write a new configuration file");
                tokio::fs::write(&path, DEFAULT_CONFIG_FILE.as_bytes()).await
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
            document: RwLock::new(document),
        };
        // test parsing of config data to make sure everything is readable
        let _ = config.legacy690_ids().await?;
        Ok(config)
    }

    /// saves any changes to the configuration file - preserving formatting and comments
    pub async fn save_config_file(&self) -> Result<()> {
        tokio::fs::write(
            &self.path, self.document.read().await.to_string(),
        ).await.with_context(|| format!("Saving configuration file: {:?}", &self.path))
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

    pub async fn set_legacy690_id(&self, device_uid: &String, is_legacy690: &bool) {
        self.document.write().await["legacy690"][device_uid.as_str()] = Item::Value(
            Value::Boolean(Formatted::new(
                *is_legacy690
            ))
        );
    }

    pub async fn set_setting(&self, device_uid: &String, setting: &Setting) {
        {
            let mut doc = self.document.write().await;
            let mut device_settings = &mut doc["device-settings"][device_uid.as_str()];
            let mut channel_setting = &mut device_settings[setting.channel_name.clone()];
            if let Some(pwm_mode) = setting.pwm_mode {
                channel_setting["pwm_mode"] = Item::Value(
                    Value::Integer(Formatted::new(pwm_mode as i64))
                );
            }
            if setting.reset_to_default.unwrap_or(false) {
                *channel_setting = Item::None;
            } else if let Some(speed_fixed) = setting.speed_fixed {
                Self::set_setting_fixed_speed(channel_setting, speed_fixed);
            } else if let Some(profile) = &setting.speed_profile {
                Self::set_setting_speed_profile(channel_setting, setting, profile)
            } else if let Some(lighting) = &setting.lighting {
                Self::set_setting_lighting(channel_setting, lighting);
            } else if let Some(lcd) = &setting.lcd {
                Self::set_setting_lcd(channel_setting, lcd);
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
            channel_setting["temp_source"]["frontend_temp_name"] = Item::Value(
                Value::String(Formatted::new(temp_source.frontend_temp_name.clone()))
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

    fn set_setting_lcd(channel_setting: &mut Item, lcd: &LcdSettings) {
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
        if let Some(image_file) = &lcd.image_file {
            channel_setting["lcd"]["image_file"] = Item::Value(
                Value::String(Formatted::new(image_file.clone()))
            );
        }
        if let Some(tmp_image_file) = &lcd.tmp_image_file {
            channel_setting["lcd"]["tmp_image_file"] = Item::Value(
                Value::String(Formatted::new(tmp_image_file.clone()))
            );
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
    }
}

const DEFAULT_CONFIG_FILE: &str = r###"
# This is the CoolerControl configuration file.
# Comments and most formatting is preserved.
# Most of this file you can edit by hand, but it is recommended to stop the daemon when doing so.
# -------------------------------


# Unique ID Device List
# -------------------------------
# This is a simple UID and device name key-value pair, that is automatically generated at startup
#  to help humans distinguish which UID belongs to which device in this config file.
#  Only the device name is given here, complete Device information can be requested from the API.
#  UIDs are generated sha256 hashes based on specific criteria to help determine device uniqueness.
# ANY CHANGES WILL BE OVERWRITTEN.
# Example:
# 21091c4fb341ceab6236e8c9e905ccc263a4ac08134b036ed415925ba4c1645d = "Nvidia GPU"
[devices]


# Legacy690 Option for devices
# -------------------------------
# There are 2 Asetek 690LC liquid coolers that have the same device ID.
#  To tell them apart we need user input to know which cooler we're actually dealing with.
#  This is an assignment of liquidctl AseTek690LC device UIDs to true/false:
#   true = Legacy690 Cooler aka NZXT Kraken X40, X60, X31, X41, X51 and X61
#   false = Modern690 Cooler aka EVGA CLC 120 (CLC12), 240, 280 and 360
# Example:
# 21091c4fb341ceab6236e8c9e905ccc263a4ac08134b036ed415925ba4c1645d = true
[legacy690]


# Device Settings
# -------------------------------
# This is where CoolerControl will save device settings per device.
# Settings can be set here also specifically by hand. (restart required for applying)
# These settings are applied on startup and each is overwritten once a new setting
# has been applied.
# Example:
# [device-settings.4b9cd1bc5fb2921253e6b7dd5b1b011086ea529d915a86b3560c236084452807]
# pump = { speed_fixed = 30 }
# logo = { lighting = { mode = "fixed", colors = [[0, 255, 255]] } }
# ring = { lighting = { mode = "spectrum-wave", backward = true, colors = [] } }
[device-settings]


# Cooler Control Settings per device
# -------------------------------
# This is where CoolerControl specifc settings and settings per device are set,
# such as disabling/enabling a particular device.
[settings]


"###;
