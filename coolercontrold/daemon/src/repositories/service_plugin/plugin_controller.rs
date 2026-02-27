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
use crate::repositories::service_plugin::service_plugin_repo::{ServicePluginRepo, CC_PLUGIN_USER};
use crate::repositories::utils::{ShellCommand, ShellCommandResult};
use anyhow::{anyhow, Context, Result};
use log::{debug, error, warn};
use std::collections::HashMap;
use std::fs::Permissions;
use std::ops::Not;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const PLUGIN_CONFIG_FILE_NAME: &str = "config.json";
const PLUGIN_UI_DIR_NAME: &str = "ui";
const PLUGIN_CONFIG_FILE_PERMISSIONS: u32 = 0o600;

#[derive(Default)]
pub struct PluginController {
    pub plugins: HashMap<ServiceId, ServiceManifest>,
    is_systemd: bool,
}

impl PluginController {
    pub fn new(service_plugin_repo: &ServicePluginRepo, is_systemd: bool) -> Self {
        Self {
            plugins: service_plugin_repo.get_plugins(),
            is_systemd,
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
            })?;
        if manifest.is_managed().not() {
            return Ok(());
        }
        let owner = self.is_systemd.then_some({
            if manifest.privileged {
                "root"
            } else {
                CC_PLUGIN_USER
            }
        });
        if let Err(err) = secure_config_file(&config_path, owner).await {
            warn!(
                "Failed to secure plugin config file {}: {err}",
                config_path.display()
            );
        }
        Ok(())
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

pub async fn secure_config_file(path: &Path, owner: Option<&str>) -> Result<()> {
    cc_fs::set_permissions(path, Permissions::from_mode(PLUGIN_CONFIG_FILE_PERMISSIONS)).await?;
    if let Some(owner) = owner {
        let command = format!("chown {owner}:{owner} {}", path.display());
        match ShellCommand::new(&command, Duration::from_secs(5))
            .run()
            .await
        {
            ShellCommandResult::Success { .. } => {}
            ShellCommandResult::Error(stderr) => return Err(anyhow!("chown failed: {stderr}")),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_root() -> bool {
        nix::unistd::geteuid().is_root()
    }

    #[tokio::test]
    async fn test_secure_config_file_sets_600_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, "{}").unwrap();
        std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

        // secure_config_file will set permissions and attempt chown.
        // chown may fail if not root, but permissions should still be set.
        let _ = secure_config_file(&config_path, Some("root")).await;

        let perms = std::fs::metadata(&config_path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            PLUGIN_CONFIG_FILE_PERMISSIONS,
            "Config file should have 600 permissions"
        );
    }

    #[tokio::test]
    async fn test_secure_config_file_nonexistent_file_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("nonexistent.json");

        let result = secure_config_file(&config_path, Some("root")).await;
        assert!(result.is_err(), "Should fail for nonexistent file");
    }

    #[tokio::test]
    async fn test_secure_config_file_chown_fails_for_non_root() {
        if is_root() {
            // Skip: chown won't fail when running as root
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, "{}").unwrap();

        let result = secure_config_file(&config_path, Some("root")).await;
        assert!(
            result.is_err(),
            "chown to root should fail when not running as root"
        );
    }

    #[tokio::test]
    async fn test_secure_config_file_chown_succeeds_as_root() {
        if !is_root() {
            // Skip: requires root privileges
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, "{}").unwrap();

        let result = secure_config_file(&config_path, Some("root")).await;
        assert!(result.is_ok(), "chown to root should succeed as root");

        let perms = std::fs::metadata(&config_path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            PLUGIN_CONFIG_FILE_PERMISSIONS,
            "Config file should have 600 permissions"
        );
    }

    #[tokio::test]
    async fn test_secure_config_file_permissions_maintained_after_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, "{}").unwrap();

        let _ = secure_config_file(&config_path, Some("root")).await;

        // Simulate a rewrite that resets permissions
        std::fs::write(&config_path, "{\"updated\": true}").unwrap();
        std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

        let _ = secure_config_file(&config_path, Some("root")).await;

        let perms = std::fs::metadata(&config_path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            PLUGIN_CONFIG_FILE_PERMISSIONS,
            "Permissions should be restored to 600 after re-securing"
        );
    }

    #[tokio::test]
    async fn test_secure_config_file_no_owner_skips_chown() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("config.json");
        std::fs::write(&config_path, "{}").unwrap();
        std::fs::set_permissions(&config_path, Permissions::from_mode(0o644)).unwrap();

        let result = secure_config_file(&config_path, None).await;
        assert!(result.is_ok(), "Should succeed without chown");

        let perms = std::fs::metadata(&config_path).unwrap().permissions();
        assert_eq!(
            perms.mode() & 0o777,
            PLUGIN_CONFIG_FILE_PERMISSIONS,
            "Config file should have 600 permissions even without chown"
        );
    }
}
