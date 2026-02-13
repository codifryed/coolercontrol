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

use std::collections::HashMap;
use std::ops::Add;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::cc_fs;
use anyhow::{anyhow, Result};
use log::{debug, error, warn};
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::sleep;

const THINKPAD_ACPI_CONF_PATH: &str = "/etc/modprobe.d";
const THINKPAD_ACPI_CONF_FILE: &str = "thinkpad_acpi.conf";
const RELOAD_THINKPAD_ACPI_MODULE_COMMAND: &str =
    "modprobe -r thinkpad_acpi && modprobe thinkpad_acpi";
const MAX_OUTPUT_LENGTH_BYTES: usize = 2_000; // This is the maximum length of the output we want to log

/// This struct is essentially a wrapper around [`Command`] which adds some
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
    /// Creates a new `ShellCommand` instance with the specified command and timeout.
    /// Make sure to sanitize the command input before passing it to this function.
    pub fn new(command: &str, timeout: Duration) -> Self {
        let default_env = HashMap::from([("LC_ALL".to_string(), "C".to_string())]);
        Self {
            command: command.to_owned(),
            timeout,
            env: default_env,
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
        for (key, value) in &self.env {
            shell_command.env(key, value);
        }
        let spawned_process = shell_command.spawn();
        let timeout_time = Instant::now().add(self.timeout);
        match spawned_process {
            Ok(mut child) => {
                while Instant::now() < timeout_time {
                    sleep(Duration::from_millis(50)).await;
                    match child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) => {} // loop
                        Err(err) => {
                            error!(
                                "Error checking process status for command: {}, {}",
                                &self.command, err
                            );
                            break;
                        }
                    }
                }
                successful = match child.try_wait() {
                    Ok(None) => {
                        warn!(
                            "Shell command did not complete within the specified timeout: {:?} \
                                Killing process for: {}",
                            self.timeout, self.command
                        );
                        child.kill().await.ok();
                        match child.wait().await {
                            Ok(status) => status.success(),
                            Err(err) => {
                                error!(
                                    "Error waiting for killed process: {}, {}",
                                    &self.command, err
                                );
                                false
                            }
                        }
                    }
                    Ok(Some(status)) => status.success(),
                    Err(err) => {
                        error!(
                            "Error checking process status for command: {}, {}",
                            &self.command, err
                        );
                        false
                    }
                };
                if let Some(mut child_err) = child.stderr.take() {
                    if let Err(err) = child_err.read_to_string(&mut stderr).await {
                        error!(
                            "Error reading stderr for command: {}, {}",
                            &self.command, err
                        );
                    }
                    limit_output_length(&mut stderr);
                    stderr = stderr.trim().to_string();
                }
                if let Some(mut child_out) = child.stdout.take() {
                    if let Err(err) = child_out.read_to_string(&mut stdout).await {
                        error!(
                            "Error reading stdout for command: {}, {}",
                            &self.command, err
                        );
                    }
                    limit_output_length(&mut stdout);
                    stdout = stdout.trim().to_string();
                }
            }
            Err(err) => {
                error!(
                    "Unexpected Error spawning process for command: {}, {}",
                    &self.command, err
                );
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

/// This enables or disables the `thinkpad_acpi` kernel module `fan_control` option.
/// It also reloads the module so as to have immediate effect if possible.
pub async fn thinkpad_fan_control(enable: &bool) -> Result<()> {
    let fan_control_option = u8::from(*enable);
    let thinkpad_acpi_conf_file_path =
        PathBuf::from(THINKPAD_ACPI_CONF_PATH).join(THINKPAD_ACPI_CONF_FILE);
    let content = format!("options thinkpad_acpi fan_control={fan_control_option} ");
    cc_fs::create_dir_all(THINKPAD_ACPI_CONF_PATH)?;
    cc_fs::write_string(thinkpad_acpi_conf_file_path, content).await?;
    let command_result =
        ShellCommand::new(RELOAD_THINKPAD_ACPI_MODULE_COMMAND, Duration::from_secs(1))
            .run()
            .await;
    match command_result {
        ShellCommandResult::Error(stderr) => Err(anyhow!(
            "Error trying to reload the thinkpad_acpi kernel module: {stderr}"
        )),
        ShellCommandResult::Success { stdout, stderr } => {
            debug!("ThinkPad ACPI Modprobe output: {stdout} - {stderr}");
            if stderr.is_empty() {
                Ok(())
            } else {
                Err(anyhow!("Error output received when trying to reload the thinkpad_acpi kernel module: {stderr}"))
            }
        }
    }
}

fn limit_output_length(output: &mut String) {
    if output.len() > MAX_OUTPUT_LENGTH_BYTES && output.is_ascii() {
        // In the future when floor_char_boundary is stable, we can use that instead
        output.truncate(MAX_OUTPUT_LENGTH_BYTES);
        *output = format!("{output}... (truncated)");
    }
}

/// Sanitizes a string for safe use in shell commands and notification displays.
/// Removes characters that could enable command injection or XSS attacks.
pub fn sanitize_for_shell(input: &str) -> String {
    input
        .chars()
        .filter(|c| {
            // Allowlist: alphanumeric, spaces, and safe punctuation
            c.is_alphanumeric()
                || matches!(c, ' ' | '.' | ',' | ':' | '-' | '_' | '/' | '%' | '°' | '+')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::repositories::utils::{limit_output_length, sanitize_for_shell};

    #[test]
    fn sanitize_allows_alphanumeric() {
        assert_eq!(sanitize_for_shell("Hello123"), "Hello123");
        assert_eq!(sanitize_for_shell("ABCxyz"), "ABCxyz");
    }

    #[test]
    fn sanitize_allows_safe_punctuation() {
        assert_eq!(sanitize_for_shell("temp: 45.5°C"), "temp: 45.5°C");
        assert_eq!(sanitize_for_shell("fan_1, fan-2"), "fan_1, fan-2");
        assert_eq!(sanitize_for_shell("path/to/file"), "path/to/file");
        assert_eq!(sanitize_for_shell("100% +5"), "100% +5");
    }

    #[test]
    fn sanitize_removes_shell_injection_chars() {
        assert_eq!(sanitize_for_shell("test; rm -rf /"), "test rm -rf /");
        assert_eq!(sanitize_for_shell("$(whoami)"), "whoami");
        assert_eq!(sanitize_for_shell("`id`"), "id");
        assert_eq!(sanitize_for_shell("a && b || c"), "a  b  c");
        assert_eq!(
            sanitize_for_shell("cat /etc/passwd | grep root"),
            "cat /etc/passwd  grep root"
        );
    }

    #[test]
    fn sanitize_removes_quotes() {
        assert_eq!(sanitize_for_shell("it's a \"test\""), "its a test");
        assert_eq!(sanitize_for_shell("name='value'"), "namevalue");
    }

    #[test]
    fn sanitize_removes_xss_chars() {
        assert_eq!(
            sanitize_for_shell("<script>alert(1)</script>"),
            "scriptalert1/script"
        );
        assert_eq!(sanitize_for_shell("a > b < c"), "a  b  c");
    }

    #[test]
    fn sanitize_removes_newlines_and_special_whitespace() {
        assert_eq!(sanitize_for_shell("line1\nline2"), "line1line2");
        assert_eq!(sanitize_for_shell("tab\there"), "tabhere");
        assert_eq!(sanitize_for_shell("cr\rhere"), "crhere");
    }

    #[test]
    fn sanitize_handles_empty_input() {
        assert_eq!(sanitize_for_shell(""), "");
    }

    #[test]
    fn sanitize_handles_only_dangerous_chars() {
        assert_eq!(sanitize_for_shell("$;|&`'\"<>()[]{}!#*?\\~^"), "");
    }

    #[test]
    fn sanitize_realistic_alert_message() {
        let input = "CPU Temp: 85.5°C is greater than allowed maximum: 80";
        assert_eq!(sanitize_for_shell(input), input);

        let malicious = "Alert: $(reboot); test & echo pwned";
        assert_eq!(
            sanitize_for_shell(malicious),
            "Alert: reboot test  echo pwned"
        );
    }

    // Should truncate the output string if it exceeds the maximum length and is ASCII-encoded
    #[test]
    fn should_truncate_output_if_exceeds_max_length_and_is_ascii() {
        let mut output =
            String::from("nMy\n\r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");
        limit_output_length(&mut output);
        assert_eq!(
            output,
            "nMy\n\rhNKPeX3FaVY5B4Z2yrZQhwRuPl1tLad2BWatY8946P0mWyYHDI6b2yPSb3h1wOeuCARUm2NqHk2srdXTBHUfLVgDXtSWttfWAoG8SI9ov2RKrDf9kcqkrCRrjfNuSQfQ4hsqbyfJb5CMMwoGVk7BtmbRkE9iH0qsfqd7NQGfUWv0Og2Mh9b1oZ4JsjF74hjlh7hmoqjgXxT35z4L6W7hTebrAXa8cVOWo7j0ZSJpOnXh9UBXRfsv0uBWykwo1uqiRzbeI0vp4Wwdnm40eWXA1H9J80pOQ5ooqGI9YUoeTCBLfFuu7Lwy6JkeqgSVQbKBHagZ6HXv4en5CAFN4mQGCSOlevkwHAcQIlCRcFNARRdtuIHGClovIczbSc17kckcvXnaPyRO2yScK0SAqdxtyvuW3YXZ1bTXAuHe5oT42hloGGfGycoT693L2HMPZAsnN4hLcc5fLKLW6R0UQWLDNnrLeiFyrV7MtDwdGoQVsH5Rhkwv3lIgCkShPqggrSMIV6joDg87SjQZhVBNcQAe1ZcCNzaGmqYrIg1mt2h3cyXZdMD8iMz6cyx3jUViMscgniegtgr1EmmmmMxgEGivwFTgoxFNdAC1a6ZoJbv9e6uqRNJ19OOpVxZeLrRfKVrwBTmUvmHO9040FLgPq1x1lZxXTC4NLLbdiNUxM5h0Z5fVn9xrta9hSv6B3NvgNMKbKZWVbOpD3C7NjlnS3e72IlLI2KIfv73DERk1jIf625TOSEAzUFG8uJfOr3nrNZGVKa8SlEoVPsghm1Yfsfn6oufQyyF9aKL9Apcw5jsmNJOcLelf7fd9xS8KyoZtyePCLvJbpS9kTzolSLdZmEVSlRs6xum4a7gnzSrBHmRQNtSB7oJfFMDJyWlb1ppOcWPKxn0IhRLQfLXwsUbsi5Q4MDOz2du0ScW45kAQejtNAzT8XljLo6lVQCFwti6vPZIPqOeThfPGaH678EgVNos27x7JSzI2SfsNjoaIgas7CWViXkDgh5aH7eJdFzJvn2OdNDQvDge6oCgWuRxj3oIhZ7ADH2vAdKM6v2EV6wgmD7Ihie2bQ2nI6EtwGAr6Hi2sv27xJYq45zsV9FvoeRNHQotJpYXJrgFZrpffvPiVCMbUw5XEsNgH4VtaaIHofL6ol0THeSxefmEBFeggfL2GR4H6JJ4YOfaIttLVbbgspsNWJyiBzCPGnxTw86Y26PW51vuwAuY68waUy9xsCqia7xxQizk1625NqC09mXD6BmvhTct4lUwzon8WTNnmB4SNmwHzOsRJj5UkQcJUl0emjAxEkObAKqU5woaPCcsZrucyu81C1yiVT6n3TUSN6ecx9M1exdw1bylfOXrs5tV9CsNM0GWqh2fEbEctEzcBWFivB9oPOXOGKYQ6CYLg3fWQNnyUGv73lvuipD84pxtloZM25KqSPaYg6EFgtTeCbV7Ozm4MFfifN7RWVkgGS2NXrANuMDc9cr6OtFTCTPnpMchMegOTrbhAabyrwpFsmrYsoW8YDxDAx2hvEfvyiXp64iNLKx5hVubriSDW4UTLdf1DvNbl5jIJLCq8eWsXGijWHLEljNl9xy8F9tmuMcsEgGvB8t30JmDsRt7FJESomJ8lVNeO7Y7Tv3PM5ajhLnSpiNRx4uJcZ6XLRsFkiIEHrC2JubSUkVFoptX6NNEbPzsiGwDZbwMk7KBimQM2yA0JFfQEb8LxyOQLpQpM4bD70dMfRJ4Y5rLN9HzSbwC1pFpY4w9pUS1P0dlZy77lq357wkz62I49dl8z1CKcZIkuXfZkyVn4qg26fAeRccz0QYAxnxIvPsruSt0i0EAMKg6cN7ay5JE60XMwGwNDc2KgYAys0y1xQt9xx4XaaF5aVhFVf1oG9nRUVH2bn9JIDwjFxgca1qBCZs5mzZH1TeXNFIbpJzPBAQ9iNr9P4l19jVI5v8l5jLpDyJfY4yCyjmMKsu3gpli1OC6M3ve3V8tDEs41ZTKHg3J... (truncated)"
        );
    }

    // Should not modify the output string if it is shorter than or equal to the maximum length
    #[test]
    fn should_not_modify_output_if_shorter_or_equal_to_max_length() {
        let mut output = String::from("Short output");
        limit_output_length(&mut output);
        assert_eq!(output, "Short output");
    }
}
