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
 ******************************************************************************/

use std::path::PathBuf;

use anyhow::{anyhow, Result};
use log::debug;
use tokio::process::Command;

const THINKPAD_ACPI_CONF_PATH: &str = "/etc/modprobe.d/thinkpad_acpi.conf";
const RELOAD_THINKPAD_ACPI_MODULE_COMMAND: &str = "modprobe -r thinkpad_acpi && modprobe thinkpad_acpi";
macro_rules! format_thinkpad_conf_content { ($($arg:tt)*) => {{ format!("options thinkpad_acpi fan_control={}", $($arg)*) }}; }

/// This enables or disables the thinkpad_acpi kernel module fan_control option.
/// It also reloads the module so as to have immediate effect if possible.
pub async fn thinkpad_fan_control(enable: &bool) -> Result<()> {
    let fan_control_option = *enable as u8;
    let content = format_thinkpad_conf_content!(fan_control_option);
    tokio::fs::write(
        PathBuf::from(THINKPAD_ACPI_CONF_PATH),
        content.as_bytes(),
    ).await?;
    let output = Command::new("sh")
        .arg("-c")
        .arg(RELOAD_THINKPAD_ACPI_MODULE_COMMAND)
        .output().await?;
    if output.status.success() {
        let out_std = String::from_utf8(output.stdout).unwrap().trim().to_owned();
        let out_err = String::from_utf8(output.stderr).unwrap().trim().to_owned();
        debug!("Thinkpad acpi Modprobe output: \n{}\n{}", out_std, out_err);
        if out_err.is_empty() {
            Ok(())
        } else {
            Err(anyhow!(
                "Error output received when trying to reload the thinkpad_acpi kernel module: \n{}",
                out_err
            ))
        }
    } else {
        let out_err = String::from_utf8(output.stderr).unwrap().trim().to_owned();
        Err(anyhow!("Error trying to reload the thinkpad_acpi kernel module: {}", out_err))
    }
}