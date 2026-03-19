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

use crate::ENV_CONFIG_DIR;

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

// -- auth --
static PASSWD_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".passwd"));
static SESSION_KEY_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".session_key"));
static SESSIONS_DIR: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("sessions"));
static TOKENS_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".tokens"));

// -- features --
static ALERT_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("alerts.json"));
static MODE_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("modes.json"));
static PLUGINS_DIR: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("plugins"));
static DETECT_OVERRIDE_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("detect.toml"));

/// Base configuration directory.
pub fn config_dir() -> &'static Path {
    &CONFIG_DIR
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

pub fn detect_override_file() -> &'static Path {
    &DETECT_OVERRIDE_FILE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_dir_is_etc_coolercontrol() {
        // Goal: verify the default value when no env var is set.
        // Note: this test relies on CC_CONFIG_DIR not being set in
        // the test environment.
        if std::env::var(ENV_CONFIG_DIR).is_err() {
            assert_eq!(config_dir(), Path::new("/etc/coolercontrol"));
        }
    }

    #[test]
    fn all_derived_paths_start_with_config_dir() {
        // Goal: verify every derived path is a child of config_dir.
        // This is the single invariant test that replaces the
        // per-module path_constants tests.
        let dir = config_dir();
        assert!(config_file().starts_with(dir));
        assert!(config_backup().starts_with(dir));
        assert!(ui_config_file().starts_with(dir));
        assert!(ui_config_backup().starts_with(dir));
        assert!(passwd_file().starts_with(dir));
        assert!(session_key_file().starts_with(dir));
        assert!(sessions_dir().starts_with(dir));
        assert!(tokens_file().starts_with(dir));
        assert!(alert_config_file().starts_with(dir));
        assert!(mode_config_file().starts_with(dir));
        assert!(plugins_dir().starts_with(dir));
        assert!(detect_override_file().starts_with(dir));
    }

    #[test]
    fn derived_paths_have_expected_file_names() {
        // Goal: verify each path ends with the expected filename,
        // independent of the base directory.
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
        assert_eq!(mode_config_file().file_name().unwrap(), "modes.json");
        assert_eq!(plugins_dir().file_name().unwrap(), "plugins");
        assert_eq!(detect_override_file().file_name().unwrap(), "detect.toml");
    }
}
