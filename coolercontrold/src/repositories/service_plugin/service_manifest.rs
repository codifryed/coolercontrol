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
    pub version: Option<String>,
    pub url: Option<String>,
    pub executable: Option<PathBuf>, // required IF user wants to have the service managed
    pub args: Vec<String>,           // if needed (set log level, etc.) "--arg1 --arg2"
    pub envs: Vec<(String, String)>, // if needed (set log level, etc.) "ENV1=value1 ENV2=value2"
    pub address: ConnectionType,     // required for all device service plugins
    pub privileged: bool,            // for device service plugins (false by default)
    pub path: PathBuf,               // This plugin's folder path
}

impl ServiceManifest {
    pub fn from_document(document: &DocumentMut, path: PathBuf) -> Result<Self> {
        let id = Self::get_optional_string(document, "id")
            .with_context(|| "Service manifest id should be present")?;
        Self::validate_id(&id)?;
        let service_type_str = Self::get_optional_string(document, "type")
            .with_context(|| "Service manifest service type should be present")?
            .to_lowercase();
        let service_type = match service_type_str.as_str() {
            "device" => ServiceType::Device,
            "integration" => ServiceType::Integration,
            _ => return Err(anyhow!("Invalid service type")),
        };
        let description =
            Self::get_optional_string(document, "description").map(|d| sanitize_for_unit_field(&d));
        let version =
            Self::get_optional_string(document, "version").map(|v| sanitize_for_unit_field(&v));
        let url = Self::get_optional_string(document, "url");
        let executable = Self::get_optional_string(document, "executable")
            .map(|exe| {
                if exe.contains('\0') || exe.contains('\n') || exe.contains('\r') {
                    return Err(anyhow!(
                        "Service manifest executable contains invalid control characters"
                    ));
                }
                let mut exe_path = PathBuf::from(exe);
                if exe_path.is_relative() {
                    exe_path = Path::new(DEFAULT_PLUGINS_PATH).join(&id).join(exe_path);
                }
                Ok(exe_path)
            })
            .transpose()?;
        let args_str = Self::get_optional_string(document, "args").unwrap_or_default();
        let args = args_str
            .split_whitespace()
            .map(sanitize_for_unit_field)
            .collect();
        let envs_str = Self::get_optional_string(document, "envs").unwrap_or_default();
        let envs = envs_str
            .split_whitespace()
            .filter_map(|env_str| {
                env_str.split_once('=').map(|(key, value)| {
                    (
                        sanitize_for_unit_field(key.trim()),
                        sanitize_for_unit_field(value.trim()),
                    )
                })
            })
            .collect();
        let address_opt = Self::get_optional_string(document, "address")
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
            version,
            url,
            executable,
            args,
            envs,
            address,
            privileged,
            path,
        })
    }

    fn validate_id(id: &str) -> Result<()> {
        if id.is_empty() || id.len() > 64 {
            return Err(anyhow!("Service manifest id must be 1-64 characters"));
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(anyhow!(
                "Service manifest id contains invalid characters \
                 (only alphanumeric, hyphens, underscores allowed)"
            ));
        }
        Ok(())
    }

    fn get_optional_string(document: &DocumentMut, field_name: &str) -> Option<String> {
        document
            .get(field_name)
            .and_then(|item| item.as_str())
            .map(|d| d.trim().to_string())
            .filter(|d| d.is_empty().not())
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

/// Sanitize a string for safe use in `systemd` unit files and `OpenRC` service files.
/// Uses an allowlist approach to strip control characters, quotes, and shell metacharacters
/// that could be used for directive injection or command injection.
fn sanitize_for_unit_field(input: &str) -> String {
    input
        .chars()
        .filter(|c| {
            c.is_alphanumeric()
                || matches!(c, ' ' | '.' | ',' | ':' | '-' | '_' | '/' | '=' | '+' | '@')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest_toml(overrides: &[(&str, &str)]) -> String {
        let mut fields: Vec<(String, String)> = vec![
            ("id".into(), "\"test-plugin\"".into()),
            ("type".into(), "\"device\"".into()),
        ];
        for (key, value) in overrides {
            if let Some(pos) = fields.iter().position(|(k, _)| k == *key) {
                fields[pos].1 = value.to_string();
            } else {
                fields.push((key.to_string(), value.to_string()));
            }
        }
        fields
            .iter()
            .map(|(k, v)| format!("{k} = {v}"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn parse_manifest(toml_str: &str) -> Result<ServiceManifest> {
        let doc: DocumentMut = toml_str.parse()?;
        ServiceManifest::from_document(&doc, PathBuf::from("/tmp/test"))
    }

    #[test]
    fn test_valid_manifest_parses() {
        let toml = make_manifest_toml(&[
            ("description", "\"A test plugin\""),
            ("version", "\"1.0.0\""),
            ("executable", "\"/usr/bin/test\""),
            ("args", "\"--verbose --port=8080\""),
            ("envs", "\"LOG_LEVEL=debug PORT=3000\""),
        ]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(manifest.id, "test-plugin");
        assert_eq!(manifest.description, Some("A test plugin".into()));
        assert_eq!(manifest.version, Some("1.0.0".into()));
        assert_eq!(manifest.args, vec!["--verbose", "--port=8080"]);
        assert_eq!(
            manifest.envs,
            vec![
                ("LOG_LEVEL".into(), "debug".into()),
                ("PORT".into(), "3000".into()),
            ]
        );
    }

    #[test]
    fn test_id_with_newlines_rejected() {
        let toml = make_manifest_toml(&[("id", "\"test\\nplugin\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("invalid characters"));
    }

    #[test]
    fn test_id_with_shell_metacharacters_rejected() {
        let toml = make_manifest_toml(&[("id", "\"test;rm -rf /\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_id_with_path_traversal_rejected() {
        let toml = make_manifest_toml(&[("id", "\"../etc/passwd\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
    }

    #[test]
    fn test_id_exceeding_64_chars_rejected() {
        let long_id = "a".repeat(65);
        let toml = make_manifest_toml(&[("id", &format!("\"{long_id}\""))]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("1-64 characters"));
    }

    #[test]
    fn test_id_exactly_64_chars_accepted() {
        let id = "a".repeat(64);
        let toml = make_manifest_toml(&[("id", &format!("\"{id}\""))]);
        let manifest = parse_manifest(&toml).unwrap();
        assert_eq!(manifest.id, id);
    }

    #[test]
    fn test_args_injection_sanitized() {
        let toml = make_manifest_toml(&[("args", "\"--verbose\\nExecStart=/bin/malicious\"")]);
        let manifest = parse_manifest(&toml).unwrap();
        // Newline is stripped, so args are joined without injection
        for arg in &manifest.args {
            assert!(!arg.contains('\n'));
            assert!(!arg.contains(';'));
        }
    }

    #[test]
    fn test_envs_quotes_and_newlines_sanitized() {
        let toml = make_manifest_toml(&[("envs", "\"KEY=value\\\"injected KEY2=val\\nue\"")]);
        let manifest = parse_manifest(&toml).unwrap();
        for (key, value) in &manifest.envs {
            assert!(!key.contains('"'));
            assert!(!key.contains('\n'));
            assert!(!value.contains('"'));
            assert!(!value.contains('\n'));
        }
    }

    #[test]
    fn test_description_newline_injection_sanitized() {
        let toml = make_manifest_toml(&[("description", "\"Normal desc\\nExecStart=/bin/evil\"")]);
        let manifest = parse_manifest(&toml).unwrap();
        let desc = manifest.description.unwrap();
        // The newline is stripped, collapsing the injected directive into the description
        // text on a single line, which prevents systemd from interpreting it as a directive.
        assert!(!desc.contains('\n'));
        assert_eq!(desc, "Normal descExecStart=/bin/evil");
    }

    #[test]
    fn test_executable_with_control_chars_rejected() {
        let toml = make_manifest_toml(&[("executable", "\"/usr/bin/test\\n--malicious\"")]);
        let result = parse_manifest(&toml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("control characters"));
    }

    #[test]
    fn test_sanitize_for_unit_field_strips_dangerous_chars() {
        assert_eq!(
            sanitize_for_unit_field("normal-value_1.0"),
            "normal-value_1.0"
        );
        assert_eq!(sanitize_for_unit_field("value\nnewline"), "valuenewline");
        assert_eq!(sanitize_for_unit_field("value\"quoted\""), "valuequoted");
        assert_eq!(sanitize_for_unit_field("val;rm -rf /"), "valrm -rf /");
        assert_eq!(sanitize_for_unit_field("$(evil)"), "evil");
        assert_eq!(sanitize_for_unit_field("key=value"), "key=value");
        assert_eq!(sanitize_for_unit_field(""), "");
    }
}
