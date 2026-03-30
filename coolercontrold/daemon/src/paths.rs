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

//! Centralized path definitions for the daemon.
//!
//! All filesystem paths derived from the config directory are defined
//! here. The base directory defaults to `/etc/coolercontrol` but can
//! be overridden at startup via the `CC_CONFIG_DIR` environment
//! variable.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use crate::{ENV_CONFIG_DIR, ENV_DATA_DIR, ENV_PLUGINS_DIR};

const DEFAULT_CONFIG_DIR: &str = "/etc/coolercontrol";

static CONFIG_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(std::env::var(ENV_CONFIG_DIR).unwrap_or_else(|_| DEFAULT_CONFIG_DIR.to_string()))
});

// -- config --
static CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("config.toml"));
static CONFIG_BACKUP: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("config-bak.toml"));
static UI_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("config-ui.json"));
static UI_CONFIG_BACKUP: LazyLock<PathBuf> =
    LazyLock::new(|| config_dir().join("config-ui-bak.json"));

// -- auth (credentials stay in /etc) --
static PASSWD_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".passwd"));
static TOKENS_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".tokens"));

// -- auth (runtime session state in /var/lib) --
static SESSION_KEY_FILE: LazyLock<PathBuf> = LazyLock::new(|| data_dir().join(".session_key"));
static SESSIONS_DIR: LazyLock<PathBuf> = LazyLock::new(|| data_dir().join("sessions"));

// -- alert logs (runtime state in /var/lib; separate from alert config in /etc) --
static ALERT_LOGS_FILE: LazyLock<PathBuf> = LazyLock::new(|| data_dir().join("alert-logs.json"));

// -- features --
static ALERT_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("alerts.json"));
static MODE_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("modes.json"));
static DETECT_OVERRIDE_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("detect.toml"));

// -- data dir (runtime state, independent of config_dir) --
const DEFAULT_DATA_DIR: &str = "/var/lib/coolercontrol";

static DATA_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    if let Ok(dir) = std::env::var(ENV_DATA_DIR) {
        return PathBuf::from(dir);
    }
    plugins_dir()
        .parent()
        .map_or_else(|| PathBuf::from(DEFAULT_DATA_DIR), Path::to_path_buf)
});

// -- plugins --
const DEFAULT_PLUGINS_DIR: &str = "/var/lib/coolercontrol/plugins";

static PLUGINS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(
        std::env::var(ENV_PLUGINS_DIR).unwrap_or_else(|_| DEFAULT_PLUGINS_DIR.to_string()),
    )
});

static LEGACY_PLUGINS_DIR: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("plugins"));

/// Base configuration directory.
pub fn config_dir() -> &'static Path {
    &CONFIG_DIR
}

/// Runtime state directory (`/var/lib/coolercontrol`).
pub fn data_dir() -> &'static Path {
    &DATA_DIR
}

pub fn config_file() -> &'static Path {
    &CONFIG_FILE
}

pub fn config_backup() -> &'static Path {
    &CONFIG_BACKUP
}

pub fn ui_config_file() -> &'static Path {
    &UI_CONFIG_FILE
}

pub fn ui_config_backup() -> &'static Path {
    &UI_CONFIG_BACKUP
}

pub fn passwd_file() -> &'static Path {
    &PASSWD_FILE
}

pub fn session_key_file() -> &'static Path {
    &SESSION_KEY_FILE
}

pub fn sessions_dir() -> &'static Path {
    &SESSIONS_DIR
}

pub fn alert_logs_file() -> &'static Path {
    &ALERT_LOGS_FILE
}

pub fn tokens_file() -> &'static Path {
    &TOKENS_FILE
}

pub fn alert_config_file() -> &'static Path {
    &ALERT_CONFIG_FILE
}

pub fn mode_config_file() -> &'static Path {
    &MODE_CONFIG_FILE
}

pub fn plugins_dir() -> &'static Path {
    &PLUGINS_DIR
}

/// Legacy plugin directory path (`/etc/coolercontrol/plugins`), used only
/// for backward-compatibility symlink creation.
pub fn legacy_plugins_dir() -> &'static Path {
    &LEGACY_PLUGINS_DIR
}

pub fn detect_override_file() -> &'static Path {
    &DETECT_OVERRIDE_FILE
}

/// Ensures the plugins directory exists at its canonical location and
/// creates a backward-compatibility symlink from the legacy path
/// (`/etc/coolercontrol/plugins`) if needed. Migrates existing plugin
/// data from the legacy location on first run after upgrade.
pub async fn ensure_plugins_dir() -> anyhow::Result<()> {
    let canonical = plugins_dir();
    let legacy = legacy_plugins_dir();
    migrate_plugins_dir(canonical, legacy).await
}

/// Ensures the data directory exists and migrates session data from the
/// old config directory location.
pub async fn ensure_data_dir() -> anyhow::Result<()> {
    use crate::cc_fs;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    let data = data_dir();
    cc_fs::create_dir_all(data).await?;
    cc_fs::set_permissions(data, Permissions::from_mode(0o711)).await?;

    migrate_session_data(config_dir(), data, session_key_file(), sessions_dir());
    Ok(())
}

/// Core session-data migration logic, extracted for testability.
pub fn migrate_session_data(config: &Path, data: &Path, new_key: &Path, new_sessions: &Path) {
    use log::{info, warn};

    if config == data {
        return;
    }

    // Migrate .session_key
    let old_key = config.join(".session_key");
    if old_key.exists() && !new_key.exists() {
        info!("Migrating session key to {}", new_key.display());
        if let Err(err) = move_file(&old_key, new_key) {
            warn!("Failed to migrate session key: {err}");
        }
    }

    // Migrate sessions/ directory
    let old_sessions = config.join("sessions");
    if old_sessions.is_dir() && !old_sessions.is_symlink() && !new_sessions.exists() {
        info!("Migrating sessions to {}", new_sessions.display());
        if let Err(err) = move_file(&old_sessions, new_sessions) {
            warn!("Failed to migrate sessions directory: {err}");
        }
    }
}

/// Move a file or directory, falling back to `mv` for cross-filesystem moves.
fn move_file(src: &Path, dst: &Path) -> anyhow::Result<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    let status = std::process::Command::new("mv")
        .arg(src.as_os_str())
        .arg(dst.as_os_str())
        .status()?;
    if !status.success() {
        anyhow::bail!("mv {} -> {} failed", src.display(), dst.display());
    }
    Ok(())
}

/// Core migration logic, extracted for testability.
pub async fn migrate_plugins_dir(canonical: &Path, legacy: &Path) -> anyhow::Result<()> {
    use crate::cc_fs;
    use log::{info, warn};
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    // Step 1: Create the canonical directory hierarchy.
    cc_fs::create_dir_all(canonical).await?;

    // Set 0o711 on the parent (/var/lib/coolercontrol) so root has full access
    // and cc-plugin-user can traverse into plugin subdirectories.
    if let Some(parent) = canonical.parent() {
        cc_fs::set_permissions(parent, Permissions::from_mode(0o711)).await?;
    }

    // Step 2: If canonical == legacy (env override), no symlink needed.
    if canonical == legacy {
        return Ok(());
    }

    // Step 3: Handle the legacy path.
    let legacy_symlink = legacy.is_symlink();
    let legacy_exists = legacy.exists() || legacy_symlink;

    if legacy_exists {
        if legacy_symlink {
            let target = std::fs::read_link(legacy)?;
            if target == canonical {
                return Ok(()); // already correct
            }
            // Symlink points elsewhere, recreate it.
            info!(
                "Replacing stale plugin symlink {} -> {}",
                legacy.display(),
                target.display()
            );
            std::fs::remove_file(legacy)?;
        } else if legacy.is_dir() {
            // Real directory from old installation, migrate contents.
            info!(
                "Migrating plugins from {} to {}",
                legacy.display(),
                canonical.display()
            );
            if let Ok(entries) = std::fs::read_dir(legacy) {
                for entry in entries.flatten() {
                    let src = entry.path();
                    let dst = canonical.join(entry.file_name());
                    if dst.exists() {
                        warn!(
                            "Plugin '{}' exists in both old and new locations, keeping {}",
                            entry.file_name().to_string_lossy(),
                            dst.display()
                        );
                        continue;
                    }
                    // Try atomic rename first, fall back to mv for cross-fs.
                    if std::fs::rename(&src, &dst).is_err() {
                        let status = std::process::Command::new("mv")
                            .arg(src.as_os_str())
                            .arg(dst.as_os_str())
                            .status();
                        if status.is_err() || !status?.success() {
                            warn!(
                                "Failed to move plugin '{}' to new location",
                                entry.file_name().to_string_lossy()
                            );
                        }
                    }
                }
            }
            // Remove the now-empty legacy directory.
            if let Err(err) = std::fs::remove_dir(legacy) {
                warn!(
                    "Could not remove old plugins directory {}: {err}",
                    legacy.display()
                );
                return Ok(()); // can't create symlink if dir still exists
            }
        }
    }

    // Step 4: Create the backward-compatibility symlink.
    if !legacy.exists() && !legacy.is_symlink() {
        if let Err(err) = std::os::unix::fs::symlink(canonical, legacy) {
            warn!(
                "Could not create compatibility symlink {} -> {}: {err}",
                legacy.display(),
                canonical.display()
            );
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_dir_is_etc_coolercontrol() {
        if std::env::var(ENV_CONFIG_DIR).is_err() {
            assert_eq!(config_dir(), Path::new("/etc/coolercontrol"));
        }
    }

    #[test]
    fn all_config_derived_paths_start_with_config_dir() {
        let dir = config_dir();
        assert!(config_file().starts_with(dir));
        assert!(config_backup().starts_with(dir));
        assert!(ui_config_file().starts_with(dir));
        assert!(ui_config_backup().starts_with(dir));
        assert!(passwd_file().starts_with(dir));
        assert!(tokens_file().starts_with(dir));
        assert!(alert_config_file().starts_with(dir));
        assert!(mode_config_file().starts_with(dir));
        assert!(detect_override_file().starts_with(dir));
    }

    #[test]
    fn session_paths_start_with_data_dir() {
        let dir = data_dir();
        assert!(session_key_file().starts_with(dir));
        assert!(sessions_dir().starts_with(dir));
        assert!(alert_logs_file().starts_with(dir));
    }

    #[test]
    fn data_dir_defaults_to_var_lib() {
        if std::env::var(ENV_PLUGINS_DIR).is_err() {
            assert_eq!(data_dir(), Path::new("/var/lib/coolercontrol"));
        }
    }

    #[test]
    fn derived_paths_have_expected_file_names() {
        assert_eq!(config_file().file_name().unwrap(), "config.toml");
        assert_eq!(config_backup().file_name().unwrap(), "config-bak.toml");
        assert_eq!(ui_config_file().file_name().unwrap(), "config-ui.json");
        assert_eq!(
            ui_config_backup().file_name().unwrap(),
            "config-ui-bak.json"
        );
        assert_eq!(passwd_file().file_name().unwrap(), ".passwd");
        assert_eq!(session_key_file().file_name().unwrap(), ".session_key");
        assert_eq!(sessions_dir().file_name().unwrap(), "sessions");
        assert_eq!(tokens_file().file_name().unwrap(), ".tokens");
        assert_eq!(alert_config_file().file_name().unwrap(), "alerts.json");
        assert_eq!(alert_logs_file().file_name().unwrap(), "alert-logs.json");
        assert_eq!(mode_config_file().file_name().unwrap(), "modes.json");
        assert_eq!(plugins_dir().file_name().unwrap(), "plugins");
        assert_eq!(detect_override_file().file_name().unwrap(), "detect.toml");
    }

    #[test]
    fn plugins_dir_defaults_to_var_lib() {
        if std::env::var(ENV_PLUGINS_DIR).is_err() {
            assert_eq!(plugins_dir(), Path::new("/var/lib/coolercontrol/plugins"));
        }
    }

    #[test]
    fn legacy_plugins_dir_is_under_config_dir() {
        assert!(legacy_plugins_dir().starts_with(config_dir()));
        assert_eq!(legacy_plugins_dir().file_name().unwrap(), "plugins");
    }

    #[tokio::test]
    async fn migrate_fresh_install() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = tmp.path().join("var/lib/coolercontrol/plugins");
        let legacy = tmp.path().join("etc/coolercontrol/plugins");
        // Ensure parent of legacy exists.
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();

        migrate_plugins_dir(&canonical, &legacy).await.unwrap();

        assert!(canonical.is_dir());
        assert!(legacy.is_symlink());
        assert_eq!(std::fs::read_link(&legacy).unwrap(), canonical);
    }

    #[tokio::test]
    async fn migrate_old_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = tmp.path().join("var/lib/coolercontrol/plugins");
        let legacy = tmp.path().join("etc/coolercontrol/plugins");
        // Create legacy dir with a plugin inside.
        std::fs::create_dir_all(legacy.join("test-plugin")).unwrap();
        std::fs::write(legacy.join("test-plugin/manifest.toml"), "id = \"test\"").unwrap();

        migrate_plugins_dir(&canonical, &legacy).await.unwrap();

        assert!(canonical.join("test-plugin/manifest.toml").exists());
        assert!(legacy.is_symlink());
        assert_eq!(std::fs::read_link(&legacy).unwrap(), canonical);
    }

    #[tokio::test]
    async fn migrate_already_done() {
        let tmp = tempfile::tempdir().unwrap();
        let canonical = tmp.path().join("var/lib/coolercontrol/plugins");
        let legacy = tmp.path().join("etc/coolercontrol/plugins");
        std::fs::create_dir_all(&canonical).unwrap();
        std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        std::os::unix::fs::symlink(&canonical, &legacy).unwrap();

        // Should be a no-op.
        migrate_plugins_dir(&canonical, &legacy).await.unwrap();

        assert!(canonical.is_dir());
        assert!(legacy.is_symlink());
        assert_eq!(std::fs::read_link(&legacy).unwrap(), canonical);
    }

    #[tokio::test]
    async fn migrate_same_path_skips_symlink() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("plugins");

        migrate_plugins_dir(&dir, &dir).await.unwrap();

        assert!(dir.is_dir());
        assert!(!dir.is_symlink());
    }

    #[test]
    fn migrate_session_key_from_config_to_data() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("etc/coolercontrol");
        let data = tmp.path().join("var/lib/coolercontrol");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&data).unwrap();

        let old_key = config.join(".session_key");
        let new_key = data.join(".session_key");
        std::fs::write(&old_key, "secret").unwrap();

        migrate_session_data(&config, &data, &new_key, &data.join("sessions"));

        assert!(!old_key.exists());
        assert_eq!(std::fs::read_to_string(&new_key).unwrap(), "secret");
    }

    #[test]
    fn migrate_sessions_dir_from_config_to_data() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("etc/coolercontrol");
        let data = tmp.path().join("var/lib/coolercontrol");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&data).unwrap();

        let old_sessions = config.join("sessions");
        let new_sessions = data.join("sessions");
        std::fs::create_dir_all(old_sessions.join("abc")).unwrap();
        std::fs::write(old_sessions.join("abc/token"), "tok123").unwrap();

        migrate_session_data(&config, &data, &data.join(".session_key"), &new_sessions);

        assert!(!old_sessions.exists());
        assert_eq!(
            std::fs::read_to_string(new_sessions.join("abc/token")).unwrap(),
            "tok123"
        );
    }

    #[test]
    fn migrate_session_skips_when_dest_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let config = tmp.path().join("etc/coolercontrol");
        let data = tmp.path().join("var/lib/coolercontrol");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(&data).unwrap();

        // Both old and new key exist - should keep new.
        let old_key = config.join(".session_key");
        let new_key = data.join(".session_key");
        std::fs::write(&old_key, "old").unwrap();
        std::fs::write(&new_key, "new").unwrap();

        migrate_session_data(&config, &data, &new_key, &data.join("sessions"));

        assert!(old_key.exists()); // not moved
        assert_eq!(std::fs::read_to_string(&new_key).unwrap(), "new");
    }

    #[test]
    fn migrate_session_same_dir_is_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("coolercontrol");
        std::fs::create_dir_all(&dir).unwrap();

        let key = dir.join(".session_key");
        std::fs::write(&key, "val").unwrap();

        migrate_session_data(&dir, &dir, &key, &dir.join("sessions"));

        // Key should be untouched.
        assert_eq!(std::fs::read_to_string(&key).unwrap(), "val");
    }
}
