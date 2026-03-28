/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2026  Guy Boldon, megadjc and contributors
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

//! Kernel module loading and blacklist checking.

use std::path::Path;
use std::time::Duration;

use log::{debug, info, warn};

use crate::shell_command::{ShellCommand, ShellCommandResult};

const MODPROBE_TIMEOUT: Duration = Duration::from_secs(10);
const UDEVADM_TIMEOUT: Duration = Duration::from_secs(30);

/// Result of a module load attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadResult {
    Loaded,
    AlreadyLoaded,
    Blacklisted,
    Failed(String),
}

/// Load a kernel module via modprobe.
/// Checks blacklist before loading.
#[must_use]
pub fn load_module(driver: &str) -> LoadResult {
    debug!("Attempting to load module: {driver}");

    // Check if already loaded
    if is_module_loaded(driver) {
        debug!("Module {driver} is already loaded");
        return LoadResult::AlreadyLoaded;
    }

    // Check blacklist
    if is_module_blacklisted(driver) {
        info!("Module {driver} is blacklisted, skipping");
        return LoadResult::Blacklisted;
    }

    // Run modprobe
    let cmd = ShellCommand::new(&format!("modprobe {driver}"), MODPROBE_TIMEOUT);
    match cmd.run() {
        ShellCommandResult::Success { .. } => {
            info!("Successfully loaded module: {driver}");
            LoadResult::Loaded
        }
        ShellCommandResult::Error(err) => {
            warn!("Failed to load module {driver}: {err}");
            LoadResult::Failed(err)
        }
    }
}

/// Check if a module is already loaded via /proc/modules.
#[must_use]
pub fn is_module_loaded(driver: &str) -> bool {
    is_module_loaded_from_content(driver, "/proc/modules")
}

fn is_module_loaded_from_content(driver: &str, path: &str) -> bool {
    match std::fs::read_to_string(path) {
        Ok(content) => is_module_in_proc_modules(driver, &content),
        Err(_) => false,
    }
}

fn is_module_in_proc_modules(driver: &str, content: &str) -> bool {
    // Module names in /proc/modules use underscores, not hyphens
    let normalized = driver.replace('-', "_");
    content.lines().any(|line| {
        line.split_whitespace()
            .next()
            .is_some_and(|name| name == normalized)
    })
}

/// Check if a module is blacklisted via /etc/modprobe.d/ or kernel cmdline.
#[must_use]
pub fn is_module_blacklisted(driver: &str) -> bool {
    if is_blacklisted_in_modprobe_d(driver) {
        return true;
    }
    is_blacklisted_in_cmdline(driver)
}

fn is_blacklisted_in_modprobe_d(driver: &str) -> bool {
    let modprobe_d = Path::new("/etc/modprobe.d");
    if !modprobe_d.exists() {
        return false;
    }
    let Ok(entries) = std::fs::read_dir(modprobe_d) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "conf") {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if is_blacklisted_in_content(driver, &content) {
                    debug!("Module {driver} blacklisted in {}", path.display());
                    return true;
                }
            }
        }
    }
    false
}

fn is_blacklisted_in_content(driver: &str, content: &str) -> bool {
    for line in content.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        // Parse "blacklist <module>" directives
        if let Some(module) = line.strip_prefix("blacklist ") {
            let module = module.trim();
            if module == driver || module == driver.replace('-', "_") {
                return true;
            }
        }
    }
    false
}

fn is_blacklisted_in_cmdline(driver: &str) -> bool {
    let Ok(cmdline) = std::fs::read_to_string("/proc/cmdline") else {
        return false;
    };
    is_blacklisted_in_cmdline_content(driver, &cmdline)
}

fn is_blacklisted_in_cmdline_content(driver: &str, cmdline: &str) -> bool {
    for token in cmdline.split_whitespace() {
        if let Some(modules) = token.strip_prefix("modprobe.blacklist=") {
            for module in modules.split(',') {
                if module.trim() == driver || module.trim() == driver.replace('-', "_") {
                    debug!("Module {driver} blacklisted via kernel cmdline");
                    return true;
                }
            }
        }
    }
    false
}

/// Run `udevadm settle` to wait for udev events to be processed.
/// This gives hwmon devices time to appear in sysfs after module loading.
pub fn udevadm_settle() {
    info!("Running udevadm settle");
    // silently fail. We try quickly, but don't want to wait forever
    let cmd = ShellCommand::new("udevadm settle --timeout 2", UDEVADM_TIMEOUT);
    match cmd.run() {
        ShellCommandResult::Success { .. } => {
            info!("udevadm settle completed successfully");
        }
        ShellCommandResult::Error(err) => {
            info!("udevadm settle: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_module_in_proc_modules() {
        let content = "nct6775 16384 0 - Live 0xffffffff\nit87 20480 0 - Live 0xffffffff\n";
        assert!(is_module_in_proc_modules("nct6775", content));
        assert!(is_module_in_proc_modules("it87", content));
        assert!(!is_module_in_proc_modules("nct6687", content));
    }

    #[test]
    fn test_is_module_in_proc_modules_hyphen_normalization() {
        let content = "some_module 16384 0 - Live 0xffffffff\n";
        // Module names with hyphens in input should match underscored names in /proc/modules
        assert!(is_module_in_proc_modules("some-module", content));
        assert!(is_module_in_proc_modules("some_module", content));
    }

    #[test]
    fn test_blacklisted_in_content() {
        let content = "# comment\nblacklist nct6687\nblacklist nouveau\n";
        assert!(is_blacklisted_in_content("nct6687", content));
        assert!(is_blacklisted_in_content("nouveau", content));
        assert!(!is_blacklisted_in_content("nct6775", content));
    }

    #[test]
    fn test_blacklisted_in_content_underscore() {
        let content = "blacklist nct6687\n";
        assert!(is_blacklisted_in_content("nct6687", content));
    }

    #[test]
    fn test_blacklisted_in_content_comments_ignored() {
        let content = "# blacklist nct6687\n";
        assert!(!is_blacklisted_in_content("nct6687", content));
    }

    #[test]
    fn test_blacklisted_in_cmdline_content() {
        let cmdline = "BOOT_IMAGE=/vmlinuz root=/dev/sda1 modprobe.blacklist=nct6687,nouveau";
        assert!(is_blacklisted_in_cmdline_content("nct6687", cmdline));
        assert!(is_blacklisted_in_cmdline_content("nouveau", cmdline));
        assert!(!is_blacklisted_in_cmdline_content("it87", cmdline));
    }
}
