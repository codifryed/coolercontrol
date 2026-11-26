// SPDX-FileCopyrightText: 2026 Guy Boldon, megadjc and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Capability checks.
//!
//! Detects runtime environment capabilities to gracefully skip hardware probing
//! when resources are unavailable.

use std::ops::Not;
use std::path::{Path, PathBuf};

use log::debug;

const LOCKDOWN_PATH: &str = "/sys/kernel/security/lockdown";
/// Bit of `CAP_SYS_MODULE` in a capability mask, from `linux/capability.h`.
const CAP_SYS_MODULE_BIT: u32 = 16;
const _: () = assert!(CAP_SYS_MODULE_BIT < u64::BITS);
/// Bit of `CAP_SYS_RAWIO` in a capability mask, from `linux/capability.h`.
const CAP_SYS_RAWIO_BIT: u32 = 17;
const _: () = assert!(CAP_SYS_RAWIO_BIT < u64::BITS);
/// `/proc/self/uid_map` of the initial user namespace: every ID maps to itself.
const INITIAL_USER_NAMESPACE_UID_MAP: [&str; 3] = ["0", "0", "4294967295"];

/// Runtime environment capabilities.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Environment {
    pub is_container: bool,
    pub is_secure_boot: bool,
    pub has_dev_port: bool,
    pub has_modprobe: bool,
    /// Whether a container was given what loading a host module needs. Only
    /// consulted inside a container.
    pub has_module_access: bool,
}

impl Environment {
    /// Detect runtime environment capabilities.
    #[must_use]
    pub fn detect() -> Self {
        let is_container = Self::check_container();
        let is_secure_boot = Self::check_secure_boot();
        let has_dev_port = Path::new("/dev/port").exists();
        let has_modprobe = Self::command_exists("modprobe");
        let has_proc_modules = Path::new("/proc/modules").exists();
        let has_module_access = module_access_detected(Path::new("/"));

        debug!(
            "Environment: container={is_container}, secure_boot={is_secure_boot}, \
             /dev/port={has_dev_port}, modprobe={has_modprobe}, \
             /proc/modules={has_proc_modules}, module_access={has_module_access}"
        );

        Self {
            is_container,
            is_secure_boot,
            has_dev_port,
            has_modprobe,
            has_module_access,
        }
    }

    /// Check whether we can perform I/O port probing.
    #[must_use]
    pub fn can_probe(&self) -> bool {
        self.has_dev_port && !self.is_secure_boot
    }

    /// Check whether we can load kernel modules. A container can only when it
    /// was given access to the host's, see [`module_access_detected`].
    #[must_use]
    pub fn can_load_modules(&self) -> bool {
        if self.has_modprobe.not() {
            return false;
        }
        if self.is_container {
            return self.has_module_access;
        }
        true
    }

    fn check_secure_boot() -> bool {
        let Ok(content) = std::fs::read_to_string(LOCKDOWN_PATH) else {
            debug!("Lockdown file not available ({LOCKDOWN_PATH})");
            return false;
        };
        parse_lockdown_active(&content)
    }

    fn check_container() -> bool {
        container_detected(Path::new("/"))
    }

    fn command_exists(cmd: &str) -> bool {
        std::env::var_os("PATH")
            .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(cmd).is_file()))
    }
}

/// Whether the tree under `root` (`/` outside tests) is a container. Blocking.
///
/// # Panics
///
/// If `root` is relative.
#[must_use]
pub fn container_detected(root: &Path) -> bool {
    assert!(root.is_absolute());
    // Check common container indicator files
    if root.join(".dockerenv").exists() {
        debug!("Detected Docker container (/.dockerenv)");
        return true;
    }
    if root.join("run/.containerenv").exists() {
        debug!("Detected container (/run/.containerenv)");
        return true;
    }
    // Check cgroup for container hints
    if let Ok(cgroup) = std::fs::read_to_string(root.join("proc/1/cgroup")) {
        if cgroup.contains("docker")
            || cgroup.contains("containerd")
            || cgroup.contains("lxc")
            || cgroup.contains("kubepods")
        {
            debug!("Detected container from cgroup");
            return true;
        }
    }
    false
}

/// Whether a container under `root` (`/` outside tests) can load modules into
/// the host kernel: it holds `CAP_SYS_MODULE` in the initial user namespace,
/// and the running kernel's module tree is mounted in. Blocking.
///
/// A rootless container holds the capability only in its own user namespace,
/// where the kernel refuses the load.
///
/// # Panics
///
/// If `root` is relative.
#[must_use]
pub fn module_access_detected(root: &Path) -> bool {
    assert!(root.is_absolute());
    let Ok(status) = std::fs::read_to_string(root.join("proc/self/status")) else {
        return false;
    };
    if has_effective_capability(&status, CAP_SYS_MODULE_BIT).not() {
        debug!("CAP_SYS_MODULE is not held");
        return false;
    }
    let Ok(uid_map) = std::fs::read_to_string(root.join("proc/self/uid_map")) else {
        return false;
    };
    if is_initial_user_namespace(&uid_map).not() {
        debug!("Not in the initial user namespace");
        return false;
    }
    module_tree(root).is_some()
}

/// Whether the process under `root` (`/` outside tests) holds what opening
/// `/dev/port` needs: `CAP_SYS_RAWIO` in the initial user namespace. Without
/// it the kernel refuses the open whether or not it is locked down. Blocking.
///
/// # Panics
///
/// If `root` is relative.
#[must_use]
pub fn raw_io_access_detected(root: &Path) -> bool {
    assert!(root.is_absolute());
    let Ok(status) = std::fs::read_to_string(root.join("proc/self/status")) else {
        return false;
    };
    if has_effective_capability(&status, CAP_SYS_RAWIO_BIT).not() {
        return false;
    }
    let Ok(uid_map) = std::fs::read_to_string(root.join("proc/self/uid_map")) else {
        return false;
    };
    is_initial_user_namespace(&uid_map)
}

/// The running kernel's module directory under `root` (`/` outside tests).
/// `None` when it is not there, as in a container without the host's mounted,
/// or on a system that keeps its modules elsewhere. Blocking.
///
/// # Panics
///
/// If `root` is relative.
#[must_use]
pub fn module_tree(root: &Path) -> Option<PathBuf> {
    assert!(root.is_absolute());
    let release = std::fs::read_to_string(root.join("proc/sys/kernel/osrelease")).ok()?;
    let release = release.trim();
    if release.is_empty() {
        return None;
    }
    // A separator would lead out of `lib/modules`.
    if release.contains('/') {
        return None;
    }
    let tree = root.join("lib/modules").join(release);
    if tree.is_dir() {
        return Some(tree);
    }
    debug!("No kernel module tree at {}", tree.display());
    None
}

/// Whether `status`, the text of `/proc/<pid>/status`, has `capability_bit` set
/// in its effective set (`CapEff`, a hex mask).
fn has_effective_capability(status: &str, capability_bit: u32) -> bool {
    assert!(capability_bit < u64::BITS);
    let Some(mask_hex) = status.lines().find_map(|line| line.strip_prefix("CapEff:")) else {
        return false;
    };
    let Ok(mask) = u64::from_str_radix(mask_hex.trim(), 16) else {
        return false;
    };
    mask & (1 << capability_bit) != 0
}

/// Whether `uid_map`, the text of `/proc/<pid>/uid_map`, is the initial user
/// namespace's. A nested namespace maps a narrower range, or more than one.
fn is_initial_user_namespace(uid_map: &str) -> bool {
    let mut fields = uid_map.split_whitespace();
    let is_identity = fields
        .by_ref()
        .take(INITIAL_USER_NAMESPACE_UID_MAP.len())
        .eq(INITIAL_USER_NAMESPACE_UID_MAP);
    if is_identity {
        return fields.next().is_none();
    }
    false
}

/// Parse the content of `/sys/kernel/security/lockdown` and return whether
/// the kernel is locked down (integrity or confidentiality mode active).
fn parse_lockdown_active(content: &str) -> bool {
    // Format: "[none] integrity confidentiality"
    // The active mode is enclosed in brackets.
    if let Some(start) = content.find('[') {
        if let Some(end) = content.find(']') {
            let active = content[start + 1..end].trim();
            return matches!(active, "integrity" | "confidentiality");
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "gated-tests")]
    fn test_environment_detect() {
        let env = Environment::detect();
        // These tests are environment-dependent, but should not panic
        let _ = env.is_container;
        let _ = env.is_secure_boot;
        let _ = env.has_dev_port;
        let _ = env.has_modprobe;
        let _ = env.has_module_access;
    }

    #[test]
    fn test_can_probe() {
        let env = Environment {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
            has_modprobe: true,
            has_module_access: false,
        };
        assert!(env.can_probe());

        let env_no_port = Environment {
            has_dev_port: false,
            ..env.clone()
        };
        assert!(!env_no_port.can_probe());

        let env_secure_boot = Environment {
            is_secure_boot: true,
            ..env.clone()
        };
        assert!(!env_secure_boot.can_probe());
    }

    /// Goal: a host needs only modprobe, a container also needs module access.
    /// Method: each combination of the three flags that decides the answer.
    #[test]
    fn test_can_load_modules() {
        let env = Environment {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
            has_modprobe: true,
            has_module_access: false,
        };
        assert!(env.can_load_modules());

        let env_container = Environment {
            is_container: true,
            ..env.clone()
        };
        assert!(!env_container.can_load_modules());

        let env_container_with_access = Environment {
            has_module_access: true,
            ..env_container.clone()
        };
        assert!(env_container_with_access.can_load_modules());

        let env_no_modprobe = Environment {
            has_modprobe: false,
            ..env.clone()
        };
        assert!(!env_no_modprobe.can_load_modules());

        let env_container_no_modprobe = Environment {
            has_modprobe: false,
            ..env_container_with_access.clone()
        };
        assert!(env_container_no_modprobe.can_load_modules().not());
    }

    const RELEASE: &str = "6.17.0-test";
    /// All capabilities, as root on a host or in a privileged container.
    const STATUS_ALL_CAPABILITIES: &str = "Name:\tcoolercontrold\nCapEff:\t000001ffffffffff\n";
    /// Podman's default set, which leaves out `CAP_SYS_MODULE`.
    const STATUS_DEFAULT_CAPABILITIES: &str = "Name:\tcoolercontrold\nCapEff:\t00000000800405fb\n";
    const UID_MAP_INITIAL: &str = "         0          0 4294967295\n";
    const UID_MAP_ROOTLESS: &str =
        "         0       1000          1\n         1     100000      65536\n";

    /// A root tree holding the given `/proc` files and, when `with_module_tree`,
    /// the module directory of `RELEASE`.
    fn root_tree(status: &str, uid_map: &str, with_module_tree: bool) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        let proc_self = root.path().join("proc/self");
        let proc_kernel = root.path().join("proc/sys/kernel");
        std::fs::create_dir_all(&proc_self).unwrap();
        std::fs::create_dir_all(&proc_kernel).unwrap();
        std::fs::write(proc_self.join("status"), status).unwrap();
        std::fs::write(proc_self.join("uid_map"), uid_map).unwrap();
        std::fs::write(proc_kernel.join("osrelease"), format!("{RELEASE}\n")).unwrap();
        if with_module_tree {
            std::fs::create_dir_all(root.path().join("lib/modules").join(RELEASE)).unwrap();
        }
        root
    }

    /// Goal: module access needs the capability, the initial user namespace and
    /// the module tree, all three. Method: a tree with all of them, then one
    /// with each taken away in turn.
    #[test]
    fn module_access_needs_capability_namespace_and_tree() {
        let granted = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_INITIAL, true);
        assert!(module_access_detected(granted.path()));

        let no_capability = root_tree(STATUS_DEFAULT_CAPABILITIES, UID_MAP_INITIAL, true);
        assert!(module_access_detected(no_capability.path()).not());

        let rootless = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_ROOTLESS, true);
        assert!(module_access_detected(rootless.path()).not());

        let no_tree = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_INITIAL, false);
        assert!(module_access_detected(no_tree.path()).not());
    }

    /// Goal: an unreadable `/proc` denies access instead of assuming it.
    /// Method: an empty root.
    #[test]
    fn module_access_is_denied_without_proc() {
        let root = tempfile::tempdir().unwrap();
        assert!(module_access_detected(root.path()).not());
    }

    /// Goal: raw I/O access needs `CAP_SYS_RAWIO` and the initial user
    /// namespace, both. Method: a tree with both, one with each taken away, and
    /// an empty root.
    #[test]
    fn raw_io_access_needs_capability_and_namespace() {
        let granted = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_INITIAL, false);
        assert!(raw_io_access_detected(granted.path()));

        // Podman's default set leaves out `CAP_SYS_RAWIO` too.
        let no_capability = root_tree(STATUS_DEFAULT_CAPABILITIES, UID_MAP_INITIAL, false);
        assert!(raw_io_access_detected(no_capability.path()).not());

        let rootless = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_ROOTLESS, false);
        assert!(raw_io_access_detected(rootless.path()).not());

        let no_proc = tempfile::tempdir().unwrap();
        assert!(raw_io_access_detected(no_proc.path()).not());
    }

    /// Goal: the module tree is found only for a usable release with its
    /// directory present. Method: a present tree, a missing one, and releases
    /// that are empty or hold a path separator.
    #[test]
    fn module_tree_follows_the_running_release() {
        let present = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_INITIAL, true);
        let expected = present.path().join("lib/modules").join(RELEASE);
        assert_eq!(module_tree(present.path()), Some(expected));

        let missing = root_tree(STATUS_ALL_CAPABILITIES, UID_MAP_INITIAL, false);
        assert_eq!(module_tree(missing.path()), None);

        let osrelease = present.path().join("proc/sys/kernel/osrelease");
        std::fs::write(&osrelease, "\n").unwrap();
        assert_eq!(module_tree(present.path()), None);
        // The parent of the module tree exists, so only the check rejects it.
        std::fs::write(&osrelease, format!("{RELEASE}/..\n")).unwrap();
        assert_eq!(module_tree(present.path()), None);
    }

    /// Goal: a capability counts only when its bit is set in the effective set.
    /// Method: full and default masks, a permitted-only mask, and the lone bit.
    #[test]
    fn test_effective_capability_bit() {
        assert!(has_effective_capability(
            STATUS_ALL_CAPABILITIES,
            CAP_SYS_MODULE_BIT
        ));
        assert!(has_effective_capability(STATUS_DEFAULT_CAPABILITIES, CAP_SYS_MODULE_BIT).not());
        // Only the effective set counts, not the permitted one.
        let permitted_only = "CapPrm:\t000001ffffffffff\nCapEff:\t0000000000000000\n";
        assert!(has_effective_capability(permitted_only, CAP_SYS_MODULE_BIT).not());
        assert!(has_effective_capability(
            "CapEff:\t0000000000010000\n",
            CAP_SYS_MODULE_BIT
        ));
    }

    /// Goal: a status without a readable `CapEff` grants nothing.
    /// Method: empty text, a missing line, a non-hex mask and an empty one.
    #[test]
    fn test_effective_capability_malformed() {
        assert!(has_effective_capability("", CAP_SYS_MODULE_BIT).not());
        assert!(has_effective_capability("Name:\tcoolercontrold\n", CAP_SYS_MODULE_BIT).not());
        assert!(has_effective_capability("CapEff:\tgarbage\n", CAP_SYS_MODULE_BIT).not());
        assert!(has_effective_capability("CapEff:\n", CAP_SYS_MODULE_BIT).not());
    }

    /// Goal: only the full identity map, alone, is the initial user namespace.
    /// Method: the initial map, then rootless, remapped, extended, short and
    /// empty ones.
    #[test]
    fn test_initial_user_namespace() {
        assert!(is_initial_user_namespace(UID_MAP_INITIAL));
        assert!(is_initial_user_namespace(UID_MAP_ROOTLESS).not());
        // A rootful container with a remapped range.
        assert!(is_initial_user_namespace("0 100000 65536\n").not());
        // The identity line followed by another is not the initial namespace's.
        assert!(is_initial_user_namespace("0 0 4294967295\n1 1 1\n").not());
        assert!(is_initial_user_namespace("0 0\n").not());
        assert!(is_initial_user_namespace("").not());
    }

    #[test]
    fn test_parse_lockdown_none() {
        assert!(!parse_lockdown_active("[none] integrity confidentiality"));
    }

    #[test]
    fn test_parse_lockdown_integrity() {
        assert!(parse_lockdown_active("none [integrity] confidentiality"));
    }

    #[test]
    fn test_parse_lockdown_confidentiality() {
        assert!(parse_lockdown_active("none integrity [confidentiality]"));
    }

    #[test]
    fn test_parse_lockdown_empty() {
        assert!(!parse_lockdown_active(""));
    }

    #[test]
    fn test_parse_lockdown_malformed() {
        assert!(!parse_lockdown_active("garbage"));
    }
}
