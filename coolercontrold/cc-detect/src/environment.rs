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

//! Capability checks.
//!
//! Detects runtime environment capabilities to gracefully skip hardware probing
//! when resources are unavailable.

use std::path::Path;

use log::debug;

/// Runtime environment capabilities.
#[derive(Debug, Clone)]
pub struct Environment {
    pub is_container: bool,
    pub has_dev_port: bool,
    pub has_modprobe: bool,
}

impl Environment {
    /// Detect runtime environment capabilities.
    #[must_use]
    pub fn detect() -> Self {
        let is_container = Self::check_container();
        let has_dev_port = Path::new("/dev/port").exists();
        let has_modprobe = Self::command_exists("modprobe");
        let has_proc_modules = Path::new("/proc/modules").exists();

        debug!(
            "Environment: container={is_container}, /dev/port={has_dev_port}, modprobe={has_modprobe}, /proc/modules={has_proc_modules}"
        );

        Self {
            is_container,
            has_dev_port,
            has_modprobe,
        }
    }

    /// Check whether we can perform I/O port probing.
    #[must_use]
    pub fn can_probe(&self) -> bool {
        self.has_dev_port
    }

    /// Check whether we can load kernel modules.
    #[must_use]
    pub fn can_load_modules(&self) -> bool {
        self.has_modprobe && !self.is_container
    }

    fn check_container() -> bool {
        // Check common container indicator files
        if Path::new("/.dockerenv").exists() {
            debug!("Detected Docker container (/.dockerenv)");
            return true;
        }
        if Path::new("/run/.containerenv").exists() {
            debug!("Detected container (/run/.containerenv)");
            return true;
        }
        // Check cgroup for container hints
        if let Ok(cgroup) = std::fs::read_to_string("/proc/1/cgroup") {
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

    fn command_exists(cmd: &str) -> bool {
        std::process::Command::new("which")
            .arg(cmd)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_detect() {
        let env = Environment::detect();
        // These tests are environment-dependent, but should not panic
        let _ = env.is_container;
        let _ = env.has_dev_port;
        let _ = env.has_modprobe;
    }

    #[test]
    fn test_can_probe() {
        let env = Environment {
            is_container: false,
            has_dev_port: true,
            has_modprobe: true,
        };
        assert!(env.can_probe());

        let env_no_port = Environment {
            has_dev_port: false,
            ..env.clone()
        };
        assert!(!env_no_port.can_probe());
    }

    #[test]
    fn test_can_load_modules() {
        let env = Environment {
            is_container: false,
            has_dev_port: true,
            has_modprobe: true,
        };
        assert!(env.can_load_modules());

        let env_container = Environment {
            is_container: true,
            ..env.clone()
        };
        assert!(!env_container.can_load_modules());

        let env_no_modprobe = Environment {
            has_modprobe: false,
            ..env.clone()
        };
        assert!(!env_no_modprobe.can_load_modules());
    }
}
