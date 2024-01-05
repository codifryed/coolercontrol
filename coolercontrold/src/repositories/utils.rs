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

use std::collections::HashMap;
use std::ops::Add;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use log::debug;
use log::error;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::sleep;

const THINKPAD_ACPI_CONF_PATH: &str = "/etc/modprobe.d";
const THINKPAD_ACPI_CONF_FILE: &str = "thinkpad_acpi.conf";
const RELOAD_THINKPAD_ACPI_MODULE_COMMAND: &str = "modprobe -r thinkpad_acpi && modprobe thinkpad_acpi";

/// This struct is essentially a wrapper around [`tokio::process::Command`] which adds some
/// additional safety measures and handling for our use cases.
pub struct ShellCommand {
    command: String,
    timeout: Duration,
    env: HashMap<String, String>,
}

pub enum ShellCommandResult {
    Success { stdout: String, stderr: String },
    Error(String),
}

impl ShellCommand {
    pub fn new(command: &str, timeout: Duration) -> Self {
        Self {
            command: command.to_owned(),
            timeout,
            env: HashMap::new(),
        }
    }

    pub fn env(&mut self, key: &str, value: &str) -> &mut Self {
        self.env.insert(key.to_owned(), value.to_owned());
        self
    }

    pub async fn run(&self) -> ShellCommandResult {
        let mut successful = false;
        let mut stdout = String::new();
        let mut stderr = String::new();
        let mut shell_command = Command::new("sh");
        shell_command
            .arg("-c")
            .arg(&self.command)
            .kill_on_drop(true)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in self.env.iter() {
            shell_command.env(key, value);
        }
        let spawned_process = shell_command.spawn();
        let timeout_time = Instant::now().add(self.timeout);
        match spawned_process {
            Ok(mut child) => {
                while Instant::now() < timeout_time {
                    sleep(Duration::from_millis(50)).await;
                    if let Some(_) = child.try_wait().unwrap() {
                        break;
                    }
                }
                successful = match child.try_wait().unwrap() {
                    None => {
                        error!(
                                "Shell command did not complete within the specified timeout: {:?} \
                                Killing process for: {}",
                                self.timeout,
                                self.command
                            );
                        child.kill().await.ok();
                        child.wait().await.ok().unwrap().success()
                    }
                    Some(status) => status.success()
                };
                if let Some(mut child_err) = child.stderr.take() {
                    child_err.read_to_string(&mut stderr).await.unwrap();
                    stderr = stderr.trim().to_owned();
                };
                if let Some(mut child_out) = child.stdout.take() {
                    child_out.read_to_string(&mut stdout).await.unwrap();
                    stdout = stdout.trim().to_owned();
                }
            }
            Err(err) => {
                error!("Unexpected Error spawning process for command: {}, {}", &self.command, err);
                stderr = err.to_string();
            }
        }
        if successful {
            ShellCommandResult::Success { stdout, stderr }
        } else {
            ShellCommandResult::Error(stderr)
        }
    }
}

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
        ShellCommandResult::Error(stderr) =>
            Err(anyhow!("Error trying to reload the thinkpad_acpi kernel module: {}", stderr)),
        ShellCommandResult::Success { stdout, stderr } => {
            debug!("ThinkPad ACPI Modprobe output: {} - {}", stdout, stderr);
            if stderr.is_empty() {
                Ok(())
            } else {
                Err(anyhow!(
                "Error output received when trying to reload the thinkpad_acpi kernel module: {}",
                stderr))
            }
        }
    }
}
