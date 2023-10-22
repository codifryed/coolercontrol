/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2023  Guy Boldon
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
 */

use std::path::PathBuf;
use std::time::Duration;

use anyhow::{anyhow, Result};
use log::debug;

use ShellCommandResult::{Error, Success};

use crate::utils::{ShellCommand, ShellCommandResult};

const THINKPAD_ACPI_CONF_PATH: &str = "/etc/modprobe.d";
const THINKPAD_ACPI_CONF_FILE: &str = "thinkpad_acpi.conf";
const RELOAD_THINKPAD_ACPI_MODULE_COMMAND: &str = "modprobe -r thinkpad_acpi && modprobe thinkpad_acpi";

/// This enables or disables the thinkpad_acpi kernel module fan_control option.
/// It also reloads the module so as to have immediate effect if possible.
pub async fn thinkpad_fan_control(enable: &bool) -> Result<()> {
    let fan_control_option = *enable as u8;
    let thinkpad_acpi_conf_file_path = PathBuf::from(THINKPAD_ACPI_CONF_PATH)
        .join(THINKPAD_ACPI_CONF_FILE);
    let content = format!("options thinkpad_acpi fan_control={} ", fan_control_option);
    tokio::fs::create_dir_all(THINKPAD_ACPI_CONF_PATH).await?;
    tokio::fs::write(thinkpad_acpi_conf_file_path, content.as_bytes()).await?;
    let command_result = ShellCommand::new(
        RELOAD_THINKPAD_ACPI_MODULE_COMMAND,
        Duration::from_secs(1),
    ).run().await;
    match command_result {
        Error(stderr) => Err(anyhow!("Error trying to reload the thinkpad_acpi kernel module: {}", stderr)),
        Success { stdout, stderr } => {
            debug!("ThinkPad ACPI Modprobe output: \n{}\n{}", stdout, stderr);
            if stderr.is_empty() {
                Ok(())
            } else {
                Err(anyhow!(
                "Error output received when trying to reload the thinkpad_acpi kernel module: \n{}",
                stderr))
            }
        }
    }
}