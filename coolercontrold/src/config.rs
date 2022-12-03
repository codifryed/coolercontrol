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
use anyhow::{Context, Result};
use log::{debug};
use tokio::sync::RwLock;
use toml_edit::{Document, Formatted, Item, Value};

const DEFAULT_CONFIG_FILE_PATH: &str = "/etc/coolercontrol/config.toml";

pub struct Config {
    path: PathBuf,
    document: RwLock<Document>,
}

impl Config {
    /// loads the configuration file data into memory
    pub async fn load() -> Result<Self> {
        // todo: load alternate config file if none found... (AppImage)
        let path = Path::new(DEFAULT_CONFIG_FILE_PATH).to_path_buf();
        let document = tokio::fs::read_to_string(&path).await
            .with_context(|| format!("Reading configuration file {:?}", path))
            .and_then(|config|
                config.parse::<Document>().with_context(|| "Parsing configuration file")
            )?;
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
    pub async fn save(&self) -> Result<()> {
        tokio::fs::write(
            &self.path, self.document.read().await.to_string(),
        ).await.with_context(|| format!("Saving configuration file: {:?}", &self.path))
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

    pub async fn set_legacy690_id(&self, device_id: &String, is_legacy690: &bool) {
        self.document.write().await["legacy690"][device_id.as_str()] = Item::Value(
            Value::Boolean(Formatted::new(
                *is_legacy690
            ))
        );
    }
}