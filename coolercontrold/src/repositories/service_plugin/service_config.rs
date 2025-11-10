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
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

#[derive(Debug)]
pub struct ServiceConfig {
    pub id: String,
    pub service_type: ServiceType,
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub uds: PathBuf,
    pub privileged: bool,
}

impl ServiceConfig {
    pub fn from_document(document: &DocumentMut) -> Result<Self> {
        let id = document
            .get("id")
            .and_then(|item| item.as_str())
            .with_context(|| "Service Config id should be present")?
            .to_string();
        let service_type_str = document
            .get("type")
            .and_then(|item| item.as_str())
            .with_context(|| "Service Config service type should be present")?
            .to_lowercase();
        let service_type = match service_type_str.as_str() {
            "device" => ServiceType::Device,
            "integration" => ServiceType::Integration,
            _ => return Err(anyhow!("Invalid service type")),
        };
        let executable_str = document
            .get("executable")
            .and_then(|item| item.as_str())
            .with_context(|| "Service Config executable should be present")?;
        let mut executable = PathBuf::from(executable_str);
        if executable.is_relative() {
            executable = Path::new(DEFAULT_PLUGINS_PATH).join(&id).join(executable);
        }
        let args_str = document
            .get("args")
            .and_then(|item| item.as_str())
            .unwrap_or_default()
            .trim();
        let args = args_str
            .split_whitespace()
            .map(std::string::ToString::to_string)
            .collect();
        let uds_str = document
            .get("uds")
            .and_then(|item| item.as_str())
            .unwrap_or_default()
            .trim();
        let uds = if uds_str.is_empty() {
            PathBuf::from(format!("/run/coolercontrol-plugin-{id}.sock"))
        } else {
            PathBuf::from(uds_str)
        };
        let privileged = document
            .get("privileged")
            .and_then(toml_edit::Item::as_bool)
            .unwrap_or(false);
        Ok(Self {
            id,
            service_type,
            executable,
            args,
            uds,
            privileged,
        })
    }
}

#[derive(Debug, PartialEq)]
pub enum ServiceType {
    Device,
    Integration,
}
