/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

use crate::api::CCError;
use crate::cc_fs;
use crate::repositories::service_plugin::service_management::ServiceId;
use crate::repositories::service_plugin::service_manifest::ServiceManifest;
use crate::repositories::service_plugin::service_plugin_repo::ServicePluginRepo;
use anyhow::{Context, Result};
use log::{debug, error};
use std::collections::HashMap;
use std::path::PathBuf;

const PLUGIN_CONFIG_FILE_NAME: &str = "config.json";
const PLUGIN_UI_DIR_NAME: &str = "ui";

#[derive(Default)]
pub struct PluginController {
    pub plugins: HashMap<ServiceId, ServiceManifest>,
}

impl PluginController {
    pub fn new(service_plugin_repo: &ServicePluginRepo) -> Self {
        Self {
            plugins: service_plugin_repo.get_plugins(),
        }
    }

    pub async fn load_plugin_config_file(&self, plugin_id: &str) -> Result<String> {
        let manifest = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| CCError::NotFound {
                msg: "Plugin not found".to_string(),
            })?;
        let config_path = manifest.path.join(PLUGIN_CONFIG_FILE_NAME);
        let config_result = cc_fs::read_txt(&config_path).await.with_context(|| {
            format!(
                "Loading Plugin configuration file {}",
                config_path.display()
            )
        });
        match config_result {
            Ok(config) => Ok(config),
            Err(err) => {
                for cause in err.chain() {
                    if let Some(io_err) = cause.downcast_ref::<std::io::Error>() {
                        if io_err.kind() == std::io::ErrorKind::NotFound {
                            debug!(
                                "Plugin Config file for {plugin_id} not found. Using empty config file."
                            );
                            return Ok(String::new());
                        }
                    }
                }
                error!(
                    "Error reading Plugin configuration file: {} - {err}",
                    config_path.display()
                );
                Err(err)
            }
        }
    }

    pub async fn save_plugin_config_file(&self, plugin_id: &str, config: String) -> Result<()> {
        let manifest = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| CCError::NotFound {
                msg: "Plugin not found".to_string(),
            })?;
        let config_path = manifest.path.join(PLUGIN_CONFIG_FILE_NAME);
        cc_fs::write_string(&config_path, config)
            .await
            .with_context(|| {
                format!(
                    "Saving Plugin configuration file: {}",
                    config_path.display()
                )
            })
    }

    pub fn get_plugin_ui_dir(&self, plugin_id: &str) -> Result<PathBuf> {
        let dir = self
            .plugins
            .get(plugin_id)
            .ok_or_else(|| CCError::NotFound {
                msg: "Plugin not found".to_string(),
            })
            .and_then(|manifest| {
                let ui_dir = manifest.path.join(PLUGIN_UI_DIR_NAME);
                if ui_dir.exists() {
                    Ok(ui_dir)
                } else {
                    Err(CCError::NotFound {
                        msg: "Plugin doesn't contain a UI directory".to_string(),
                    })
                }
            })?;
        Ok(dir)
    }
}
