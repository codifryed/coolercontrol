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

use crate::repositories::service_plugin::service_plugin_repo::DEFAULT_PLUGINS_PATH;
use anyhow::{anyhow, Context, Result};
use std::ops::Not;
use std::path::{Path, PathBuf};
use strum::{Display, EnumString};
use toml_edit::DocumentMut;

#[derive(Debug, Clone)]
pub struct ServiceManifest {
    pub id: String,                // required for all service plugins
    pub service_type: ServiceType, // required for all service plugins
    pub description: Option<String>,
    pub executable: Option<PathBuf>, // required IF user wants to have the service managed
    pub args: Vec<String>,           // if needed (set log level, etc.) "--arg1 --arg2"
    pub envs: Vec<(String, String)>, // if needed (set log level, etc.) "ENV1=value1 ENV2=value2"
    pub address: ConnectionType,     // required for all device service plugins
    pub privileged: bool,            // for device service plugins (false by default)
    pub path: PathBuf,               // This plugin's folder path
}

impl ServiceManifest {
    pub fn from_document(document: &DocumentMut, path: PathBuf) -> Result<Self> {
        let id = document
            .get("id")
            .and_then(|item| item.as_str())
            .with_context(|| "Service manifest id should be present")?
            .to_string();
        let service_type_str = document
            .get("type")
            .and_then(|item| item.as_str())
            .with_context(|| "Service manifest service type should be present")?
            .to_lowercase();
        let service_type = match service_type_str.as_str() {
            "device" => ServiceType::Device,
            "integration" => ServiceType::Integration,
            _ => return Err(anyhow!("Invalid service type")),
        };
        let description = document
            .get("description")
            .and_then(|item| item.as_str())
            .filter(|d| d.is_empty().not())
            .map(|d| d.trim().to_string());
        let executable = document
            .get("executable")
            .and_then(|item| item.as_str())
            .filter(|exe| exe.trim().is_empty().not())
            .map(|exe| {
                let mut exe_path = PathBuf::from(exe);
                if exe_path.is_relative() {
                    exe_path = Path::new(DEFAULT_PLUGINS_PATH).join(&id).join(exe_path);
                }
                exe_path
            });
        let args_str = document
            .get("args")
            .and_then(|item| item.as_str())
            .unwrap_or_default()
            .trim();
        let args = args_str
            .split_whitespace()
            .map(ToString::to_string)
            .collect();
        let envs_str = document
            .get("envs")
            .and_then(|item| item.as_str())
            .unwrap_or_default()
            .trim();
        let envs = envs_str
            .split_whitespace()
            .filter_map(|env_str| {
                env_str
                    .split_once('=')
                    .map(|(key, value)| (key.trim().to_string(), value.trim().to_string()))
            })
            .collect();
        let address_opt = document
            .get("address")
            .and_then(|item| item.as_str().map(str::trim).map(str::to_string))
            .or_else(|| Some(format!("/tmp/{id}.sock")))
            .filter(|_| service_type == ServiceType::Device);
        let address = match address_opt {
            None => ConnectionType::None,
            Some(address) => {
                if address.is_empty() {
                    ConnectionType::Uds(PathBuf::from(format!(
                        "/run/coolercontrol-plugin-{id}.sock"
                    )))
                } else {
                    let check_path = PathBuf::from(&address);
                    if check_path.is_absolute() {
                        ConnectionType::Uds(check_path)
                    } else {
                        ConnectionType::Tcp(address)
                    }
                }
            }
        };
        let privileged = document
            .get("privileged")
            .and_then(toml_edit::Item::as_bool)
            .unwrap_or(false);
        Ok(Self {
            id,
            service_type,
            description,
            executable,
            args,
            envs,
            address,
            privileged,
            path,
        })
    }

    pub fn is_managed(&self) -> bool {
        self.executable.is_some()
    }
}

#[derive(Debug, PartialEq, Clone, EnumString, Display)]
pub enum ServiceType {
    Device,
    Integration,
}

#[derive(Debug, PartialEq, Clone, EnumString, Display)]
pub enum ConnectionType {
    None,
    Uds(PathBuf),
    Tcp(String),
}
