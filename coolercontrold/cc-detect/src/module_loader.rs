// SPDX-FileCopyrightText: 2026 Guy Boldon, megadjc and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
    /// The driver found no chip of its own and unloaded.
    NoDevice,
    /// The kernel refused the module's signature.
    SignatureRejected,
    Failed(String),
}

impl LoadResult {
    /// The `module_status` string the API and the report carry.
    #[must_use]
    pub fn status(&self) -> String {
        match self {
            Self::Loaded => "loaded".to_string(),
            Self::AlreadyLoaded => "already_loaded".to_string(),
            Self::Blacklisted => "blacklisted".to_string(),
            Self::NoDevice => "no_device".to_string(),
            Self::SignatureRejected => "signature_rejected".to_string(),
            Self::Failed(err) => format!("failed: {err}"),
        }
    }
}

/// Load a kernel module via modprobe.
/// Checks blacklist before loading.
///
/// `lockdown_active` lets a plain permission error be read as the signature
/// refusal it is under lockdown without `module.sig_enforce`.
#[must_use]
pub fn load_module(driver: &str, lockdown_active: bool) -> LoadResult {
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
            let result = classify_failure(err, lockdown_active);
            match &result {
                LoadResult::NoDevice => debug!("Module {driver} found no device"),
                LoadResult::SignatureRejected => {
                    info!("The kernel rejected the signature of module {driver}");
                }
                _ => warn!("Failed to load module {driver}: {result:?}"),
            }
            result
        }
    }
}

/// Reads a modprobe failure from its stderr, which is in the C locale.
///
/// With `module.sig_enforce` the kernel answers a bad signature with
/// `EKEYREJECTED`; under lockdown alone it answers `EPERM`.
fn classify_failure(stderr: String, lockdown_active: bool) -> LoadResult {
    if stderr.contains("Key was rejected by service") {
        return LoadResult::SignatureRejected;
    }
    if stderr.contains("Required key not available") {
        return LoadResult::SignatureRejected;
    }
    if lockdown_active {
        if stderr.contains("Operation not permitted") {
            return LoadResult::SignatureRejected;
        }
    }
    // Also matches ENXIO, "No such device or address".
    if stderr.contains("No such device") {
        return LoadResult::NoDevice;
    }
    LoadResult::Failed(stderr)
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

/// Whether `driver` appears in `content`, the text of `/proc/modules`.
#[must_use]
pub fn is_module_in_proc_modules(driver: &str, content: &str) -> bool {
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

    /// Goal: each modprobe failure maps to its kind, and `EPERM` counts as a
    /// signature refusal only under lockdown. Method: real modprobe stderr.
    #[test]
    fn modprobe_failures_are_classified() {
        let rejected = "modprobe: ERROR: could not insert 'nct6687': Key was rejected by service\n";
        let no_key = "modprobe: ERROR: could not insert 'nct6687': Required key not available\n";
        let eperm = "modprobe: ERROR: could not insert 'nct6687': Operation not permitted\n";
        let enodev = "modprobe: ERROR: could not insert 'it87': No such device\n";
        let enxio = "modprobe: ERROR: could not insert 'it87': No such device or address\n";
        let busy = "modprobe: ERROR: could not insert 'it87': Device or resource busy\n";
        for lockdown_active in [false, true] {
            let classify = |text: &str| classify_failure(text.to_string(), lockdown_active);
            assert_eq!(classify(rejected), LoadResult::SignatureRejected);
            assert_eq!(classify(no_key), LoadResult::SignatureRejected);
            assert_eq!(classify(enodev), LoadResult::NoDevice);
            assert_eq!(classify(enxio), LoadResult::NoDevice);
            assert_eq!(classify(busy), LoadResult::Failed(busy.to_string()));
        }
        assert_eq!(
            classify_failure(eperm.to_string(), true),
            LoadResult::SignatureRejected
        );
        assert_eq!(
            classify_failure(eperm.to_string(), false),
            LoadResult::Failed(eperm.to_string())
        );
    }

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
