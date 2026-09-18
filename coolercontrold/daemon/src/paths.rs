// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Centralized path definitions for the daemon.
//!
//! All filesystem paths derived from the config directory are defined
//! here. The base directory defaults to `/etc/coolercontrol` but can
//! be overridden at startup via the `CC_CONFIG_DIR` environment
//! variable.

use std::ops::Not;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

#[cfg(not(test))]
use crate::{ENV_CONFIG_DIR, ENV_DATA_DIR};
use crate::{ENV_PLUGINS_DIR, ENV_SERVICE_DIR};

// -- config dir (independent of data_dir) --
const DEFAULT_CONFIG_DIR: &str = "/etc/coolercontrol";
#[cfg(not(test))]
static CONFIG_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(std::env::var(ENV_CONFIG_DIR).unwrap_or_else(|_| DEFAULT_CONFIG_DIR.to_string()))
});
// Test builds must never resolve to /etc: `cargo test` runs every test in one process,
// any test may initialize this static (freezing it process-wide), and package builds may
// run the suite as root. The env override is ignored so tests stay hermetic.
#[cfg(test)]
static CONFIG_DIR: LazyLock<PathBuf> = LazyLock::new(|| test_sandbox_dir("config"));

// -- data dir (runtime state, independent of config_dir) --
const DEFAULT_DATA_DIR: &str = "/var/lib/coolercontrol";
#[cfg(not(test))]
static DATA_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    PathBuf::from(std::env::var(ENV_DATA_DIR).unwrap_or_else(|_| DEFAULT_DATA_DIR.to_string()))
});
// Same sandbox rule as CONFIG_DIR: never /var/lib in test builds.
#[cfg(test)]
static DATA_DIR: LazyLock<PathBuf> = LazyLock::new(|| test_sandbox_dir("data"));

/// One writable per-process sandbox directory under the system temp dir. The pid keeps
/// parallel test processes (workspace crates, reruns) from sharing state.
#[cfg(test)]
fn test_sandbox_dir(kind: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("coolercontrol-test-{kind}-{}", std::process::id()));
    // A recycled pid must not hand this process an old run's files (or a planted link).
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_file(&dir);
    std::fs::create_dir_all(&dir).expect("test sandbox dir must be creatable");
    dir
}

// -- plugins (defaults under data_dir; overridable via CC_PLUGINS_DIR) --
static PLUGINS_DIR: LazyLock<PathBuf> = LazyLock::new(|| {
    std::env::var(ENV_PLUGINS_DIR).map_or_else(|_| data_dir().join("plugins"), PathBuf::from)
});

// -- service manager unit/script dir (overridable via CC_SERVICE_DIR) --
// Unset leaves each service manager on its own default, so only distros that need it pay
// any attention to this.
#[cfg(not(test))]
static SERVICE_DIR: LazyLock<Option<PathBuf>> =
    LazyLock::new(|| parse_service_dir_override(std::env::var(ENV_SERVICE_DIR).ok()));
// Same sandbox rule as CONFIG_DIR: the env override is ignored so a developer or packager
// with CC_SERVICE_DIR exported cannot change what the suite sees. The parsing itself is
// tested directly through `parse_service_dir_override`.
#[cfg(test)]
static SERVICE_DIR: LazyLock<Option<PathBuf>> = LazyLock::new(|| None);

// -- config --
static CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("config.toml"));
static UI_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("config-ui.json"));
// Rotated, timestamped configuration backups live under this directory.
static BACKUPS_DIR: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("backups"));

// -- auth (credentials stay in /etc) --
static PASSWD_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".passwd"));
static TOKENS_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join(".tokens"));

// -- features --
static ALERT_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("alerts.json"));
static MODE_CONFIG_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("modes.json"));
static CALIBRATION_CONFIG_FILE: LazyLock<PathBuf> =
    LazyLock::new(|| config_dir().join("calibrations.json"));
static DETECT_OVERRIDE_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("detect.toml"));
static OVERRIDES_FILE: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("overrides.toml"));

// -- LCD images --
// One file per device channel. Earlier daemons wrote a single shared file per content type,
// so two screens overwrote each other's image; those names are kept only for the migration.
static LCD_IMAGE_DIR: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("lcd_images"));
static LEGACY_LCD_IMAGE_PNG: LazyLock<PathBuf> =
    LazyLock::new(|| config_dir().join("lcd_image.png"));
static LEGACY_LCD_IMAGE_GIF: LazyLock<PathBuf> =
    LazyLock::new(|| config_dir().join("lcd_image.gif"));

// -- auth (runtime session state in /var/lib) --
static SESSION_KEY_FILE: LazyLock<PathBuf> = LazyLock::new(|| data_dir().join(".session_key"));
static SESSIONS_DIR: LazyLock<PathBuf> = LazyLock::new(|| data_dir().join("sessions"));

// -- alert logs (runtime state in /var/lib; separate from alert config in /etc) --
static ALERT_LOGS_FILE: LazyLock<PathBuf> = LazyLock::new(|| data_dir().join("alert-logs.json"));

static LEGACY_PLUGINS_DIR: LazyLock<PathBuf> = LazyLock::new(|| config_dir().join("plugins"));

/// Base configuration directory.
pub fn config_dir() -> &'static Path {
    &CONFIG_DIR
}

/// Runtime state directory (`/var/lib/coolercontrol`).
pub fn data_dir() -> &'static Path {
    &DATA_DIR
}

/// The directory a service manager writes its plugin unit/script files to: the
/// `CC_SERVICE_DIR` override when one is set, otherwise `manager_default`.
pub fn service_dir(manager_default: &Path) -> PathBuf {
    debug_assert!(manager_default.is_absolute());
    SERVICE_DIR
        .as_deref()
        .map_or_else(|| manager_default.to_path_buf(), Path::to_path_buf)
}

/// Validates the raw `CC_SERVICE_DIR` value, extracted for testability.
///
/// Only an absolute path is accepted. The daemon runs as root and writes service
/// definitions here, so a relative path would resolve against whatever working directory
/// the service manager happened to start it in. A rejected value falls back to the
/// manager default rather than failing startup, since plugins are optional.
fn parse_service_dir_override(raw: Option<String>) -> Option<PathBuf> {
    use log::warn;
    let raw = raw?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        warn!("{ENV_SERVICE_DIR} is empty, using the service manager default");
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.is_absolute().not() {
        warn!(
            "{ENV_SERVICE_DIR} must be an absolute path, ignoring '{trimmed}' \
             and using the service manager default"
        );
        return None;
    }
    Some(path)
}

pub fn config_file() -> &'static Path {
    &CONFIG_FILE
}

pub fn ui_config_file() -> &'static Path {
    &UI_CONFIG_FILE
}

/// Directory holding rotated, timestamped configuration backups.
pub fn backups_dir() -> &'static Path {
    &BACKUPS_DIR
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

pub fn calibration_config_file() -> &'static Path {
    &CALIBRATION_CONFIG_FILE
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

pub fn overrides_file() -> &'static Path {
    &OVERRIDES_FILE
}

/// Per-channel LCD image storage, so two screens cannot share and overwrite one image.
pub fn lcd_image_dir() -> &'static Path {
    &LCD_IMAGE_DIR
}

/// The single shared LCD image files written by earlier daemons, one per content type.
/// Used only to migrate old configs onto per-channel paths.
pub fn legacy_lcd_image_files() -> [&'static Path; 2] {
    [&LEGACY_LCD_IMAGE_PNG, &LEGACY_LCD_IMAGE_GIF]
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
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;

    // Step 1: Create canonical directory with traversal permissions.
    cc_fs::create_dir_all(canonical).await?;
    if let Some(parent) = canonical.parent() {
        // 0o711: root has full access; cc-plugin-user can traverse into subdirectories.
        cc_fs::set_permissions(parent, Permissions::from_mode(0o711)).await?;
    }

    // Step 2: No symlink needed when both paths resolve to the same location.
    if canonical == legacy {
        return Ok(());
    }

    // Step 3: Clear the legacy path so the compatibility symlink can be placed.
    if !prepare_legacy_path_for_symlink(canonical, legacy)? {
        return Ok(());
    }

    // Step 4: Create the backward-compatibility symlink.
    if !legacy.exists() && !legacy.is_symlink() {
        if let Err(err) = std::os::unix::fs::symlink(canonical, legacy) {
            use log::warn;
            warn!(
                "Could not create compatibility symlink {} -> {}: {err}",
                legacy.display(),
                canonical.display()
            );
        }
    }

    Ok(())
}

/// Prepares the legacy path for compatibility symlink creation.
///
/// Returns `true` when the caller should proceed to create a symlink,
/// `false` when either the symlink already points to `canonical` (done)
/// or the legacy directory could not be removed (skip symlink).
fn prepare_legacy_path_for_symlink(canonical: &Path, legacy: &Path) -> anyhow::Result<bool> {
    use log::{info, warn};

    let is_symlink = legacy.is_symlink();
    if !legacy.exists() && !is_symlink {
        return Ok(true); // nothing to clear; ready to create symlink
    }

    if is_symlink {
        let target = std::fs::read_link(legacy)?;
        if target == canonical {
            return Ok(false); // already correct; nothing to do
        }
        // Stale symlink pointing elsewhere; remove and recreate.
        info!(
            "Replacing stale plugin symlink {} -> {}",
            legacy.display(),
            target.display()
        );
        std::fs::remove_file(legacy)?;
        return Ok(true);
    }

    if legacy.is_dir() {
        info!(
            "Migrating plugins from {} to {}",
            legacy.display(),
            canonical.display()
        );
        migrate_legacy_plugin_entries(canonical, legacy);
        if let Err(err) = std::fs::remove_dir(legacy) {
            warn!(
                "Could not remove old plugins directory {}: {err}",
                legacy.display()
            );
            return Ok(false); // leave legacy dir in place; skip symlink creation
        }
    }

    Ok(true)
}

/// Moves each entry from the legacy plugins directory into the canonical one.
/// Logs and skips entries that already exist at the destination or fail to move.
fn migrate_legacy_plugin_entries(canonical: &Path, legacy: &Path) {
    use log::warn;
    let Ok(entries) = std::fs::read_dir(legacy) else {
        return;
    };
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
        // Try atomic rename first; fall back to `mv` for cross-filesystem moves.
        if std::fs::rename(&src, &dst).is_err() {
            let moved = std::process::Command::new("mv")
                .arg(src.as_os_str())
                .arg(dst.as_os_str())
                .status()
                .is_ok_and(|s| s.success());
            if !moved {
                warn!(
                    "Failed to move plugin '{}' to new location",
                    entry.file_name().to_string_lossy()
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn service_dir_override_accepts_an_absolute_path() {
        // Goal: the value NixOS sets is taken as-is. Method: feed the writable runtime
        // directory systemd also reads and pin the parsed result.
        assert_eq!(
            parse_service_dir_override(Some("/run/systemd/system".to_string())),
            Some(PathBuf::from("/run/systemd/system"))
        );
    }

    #[test]
    fn service_dir_override_trims_surrounding_whitespace() {
        // Goal: a value that picked up padding from a unit file still resolves. Method:
        // pad both ends and assert the path is unpadded.
        assert_eq!(
            parse_service_dir_override(Some("  /run/systemd/system \n".to_string())),
            Some(PathBuf::from("/run/systemd/system"))
        );
    }

    #[test]
    fn service_dir_override_rejects_unusable_values() {
        // Goal: nothing that would put unit files somewhere unintended gets through. The
        // daemon runs as root, so a relative path would resolve against whatever working
        // directory the service manager started it in. Method: the negative space, unset
        // and every malformed shape, all of which must fall back to the manager default.
        assert_eq!(parse_service_dir_override(None), None);
        assert_eq!(parse_service_dir_override(Some(String::new())), None);
        assert_eq!(parse_service_dir_override(Some("   ".to_string())), None);
        assert_eq!(
            parse_service_dir_override(Some("run/systemd".to_string())),
            None
        );
        assert_eq!(
            parse_service_dir_override(Some("./units".to_string())),
            None
        );
        assert_eq!(
            parse_service_dir_override(Some("../units".to_string())),
            None
        );
    }

    #[test]
    fn service_dir_falls_back_to_the_manager_default() {
        // Goal: a daemon with no override left exactly where it was before. Method: the
        // override static is sandboxed to None in test builds, so the accessor must hand
        // back each manager's own directory unchanged.
        assert_eq!(
            service_dir(Path::new("/etc/systemd/system")),
            PathBuf::from("/etc/systemd/system")
        );
        assert_eq!(
            service_dir(Path::new("/etc/init.d")),
            PathBuf::from("/etc/init.d")
        );
    }

    #[test]
    fn production_defaults_are_unchanged() {
        // Guards the production constants; the runtime statics are sandboxed in test
        // builds, so the defaults can only be asserted as constants here.
        assert_eq!(DEFAULT_CONFIG_DIR, "/etc/coolercontrol");
        assert_eq!(DEFAULT_DATA_DIR, "/var/lib/coolercontrol");
    }

    #[test]
    fn test_build_dirs_are_sandboxed_and_writable() {
        // Test builds must never point at system directories: any test may freeze these
        // statics process-wide, and a package build may run the suite as root. Both dirs
        // must live under the system temp dir and accept writes.
        let temp_base = std::env::temp_dir();
        for dir in [config_dir(), data_dir()] {
            assert!(
                dir.starts_with(&temp_base),
                "{} escapes the test sandbox",
                dir.display()
            );
            let probe = dir.join(".write_probe");
            std::fs::write(&probe, b"ok").unwrap();
            std::fs::remove_file(&probe).unwrap();
        }
    }

    #[test]
    fn all_config_derived_paths_start_with_config_dir() {
        let dir = config_dir();
        assert!(config_file().starts_with(dir));
        assert!(ui_config_file().starts_with(dir));
        assert!(backups_dir().starts_with(dir));
        assert!(passwd_file().starts_with(dir));
        assert!(tokens_file().starts_with(dir));
        assert!(alert_config_file().starts_with(dir));
        assert!(mode_config_file().starts_with(dir));
        assert!(calibration_config_file().starts_with(dir));
        assert!(detect_override_file().starts_with(dir));
        assert!(overrides_file().starts_with(dir));
    }

    #[test]
    fn session_paths_start_with_data_dir() {
        let dir = data_dir();
        assert!(session_key_file().starts_with(dir));
        assert!(sessions_dir().starts_with(dir));
        assert!(alert_logs_file().starts_with(dir));
    }

    #[test]
    fn derived_paths_have_expected_file_names() {
        assert_eq!(config_file().file_name().unwrap(), "config.toml");
        assert_eq!(ui_config_file().file_name().unwrap(), "config-ui.json");
        assert_eq!(backups_dir().file_name().unwrap(), "backups");
        assert_eq!(passwd_file().file_name().unwrap(), ".passwd");
        assert_eq!(session_key_file().file_name().unwrap(), ".session_key");
        assert_eq!(sessions_dir().file_name().unwrap(), "sessions");
        assert_eq!(tokens_file().file_name().unwrap(), ".tokens");
        assert_eq!(alert_config_file().file_name().unwrap(), "alerts.json");
        assert_eq!(alert_logs_file().file_name().unwrap(), "alert-logs.json");
        assert_eq!(mode_config_file().file_name().unwrap(), "modes.json");
        assert_eq!(
            calibration_config_file().file_name().unwrap(),
            "calibrations.json"
        );
        assert_eq!(plugins_dir().file_name().unwrap(), "plugins");
        assert_eq!(detect_override_file().file_name().unwrap(), "detect.toml");
        assert_eq!(overrides_file().file_name().unwrap(), "overrides.toml");
    }

    #[test]
    fn plugins_dir_starts_with_data_dir_by_default() {
        if std::env::var(ENV_PLUGINS_DIR).is_err() {
            assert!(plugins_dir().starts_with(data_dir()));
            assert_eq!(plugins_dir().file_name().unwrap(), "plugins");
        }
    }

    #[test]
    fn legacy_plugins_dir_is_under_config_dir() {
        assert!(legacy_plugins_dir().starts_with(config_dir()));
        assert_eq!(legacy_plugins_dir().file_name().unwrap(), "plugins");
    }

    #[test]
    fn migrate_fresh_install() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let canonical = tmp.path().join("var/lib/coolercontrol/plugins");
            let legacy = tmp.path().join("etc/coolercontrol/plugins");
            // Ensure parent of legacy exists.
            std::fs::create_dir_all(legacy.parent().unwrap()).unwrap();

            migrate_plugins_dir(&canonical, &legacy).await.unwrap();

            assert!(canonical.is_dir());
            assert!(legacy.is_symlink());
            assert_eq!(std::fs::read_link(&legacy).unwrap(), canonical);
        });
    }

    #[test]
    fn migrate_old_directory() {
        crate::rt::test_runtime(async {
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
        });
    }

    #[test]
    fn migrate_already_done() {
        crate::rt::test_runtime(async {
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
        });
    }

    #[test]
    fn migrate_same_path_skips_symlink() {
        crate::rt::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let dir = tmp.path().join("plugins");

            migrate_plugins_dir(&dir, &dir).await.unwrap();

            assert!(dir.is_dir());
            assert!(!dir.is_symlink());
        });
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
