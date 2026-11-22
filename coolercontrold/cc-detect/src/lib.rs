// SPDX-FileCopyrightText: 2026 Guy Boldon, megadjc and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Detects Super-I/O chips via I/O port probing and loads the appropriate
//! kernel modules.
//!
//! # Attribution
//!
//! This crate is a Rust reimplementation derived from the `sensors-detect`
//! script in [lm-sensors](https://github.com/lm-sensors/lm-sensors)
//! (GPL-2.0-or-later, Copyright (C) the lm-sensors contributors) and the
//! [TsunamiMommy/lm-sensors](https://github.com/TsunamiMommy/lm-sensors) fork.
//!
//! The chip databases in `data/` are likewise derived from lm-sensors chip
//! definitions and are distributed under the same GPL-2.0-or-later terms.
//!
//! # Architecture
//!
//! This crate is x86_64-only - Super-I/O I/O port probing is
//! architecture-specific. On all other targets `run_detection` is a no-op.

pub mod chip_db;
pub mod chips_custom;
pub mod environment;
pub mod kernel_probe;
pub mod module_loader;
pub mod port_io;
pub mod shell_command;
pub mod superio;

use std::path::Path;

use log::info;
use serde::Serialize;

#[cfg(target_arch = "x86_64")]
use log::{debug, warn};
#[cfg(target_arch = "x86_64")]
use std::ops::Not;

#[cfg(target_arch = "x86_64")]
use chip_db::ChipDatabase;
#[cfg(target_arch = "x86_64")]
use environment::Environment;
#[cfg(target_arch = "x86_64")]
use module_loader::LoadResult;
#[cfg(target_arch = "x86_64")]
use superio::DetectedChip;

/// Whether this build can actually probe for Super-I/O chips.
///
/// Callers must consult this before interpreting `EnvironmentInfo`: the
/// non-x86_64 stub reports `has_dev_port: false` and `is_secure_boot: false`,
/// and those are defaults, not observations. Treating them as observations
/// would report a blocked environment on every machine that simply has no
/// Super-I/O bus to probe.
pub const DETECTION_SUPPORTED: bool = cfg!(target_arch = "x86_64");

/// Complete detection results.
#[derive(Debug, Clone, Serialize)]
pub struct DetectionResults {
    pub detected_chips: Vec<DetectedChipInfo>,
    pub blacklisted: Vec<String>,
    /// Drivers the kernel refused to load because of their signature.
    pub signature_rejected: Vec<String>,
    /// Drivers handed to the kernel without a probe, because the probe could
    /// not run. See `kernel_probe`.
    pub kernel_probed: Vec<KernelProbedDriver>,
    pub environment: EnvironmentInfo,
}

/// A driver loaded without a chip id, and what came of it.
#[derive(Debug, Clone, Serialize)]
pub struct KernelProbedDriver {
    pub driver: String,
    pub module_status: String,
}

/// Information about a detected chip and its module load status.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedChipInfo {
    pub name: String,
    pub driver: String,
    pub address: String,
    pub base_address: String,
    pub device_id: String,
    pub features: Vec<String>,
    pub module_status: String,
}

/// Environment information included in results.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentInfo {
    pub is_container: bool,
    pub is_secure_boot: bool,
    pub has_dev_port: bool,
}

/// Run the full Super-I/O detection pipeline.
///
/// # Arguments
/// * `load_modules` - If `true`, also load detected kernel modules via modprobe.
/// * `override_path` - Optional path to a TOML file with additional chip definitions to merge
///   into the compiled-in database at runtime. Pass `None` to skip.
///
/// # Returns
/// Detection results including detected chips, skipped drivers, and environment info.
#[cfg(target_arch = "x86_64")]
#[must_use]
pub fn run_detection(load_modules: bool, override_path: Option<&Path>) -> DetectionResults {
    info!("Starting Super-I/O hardware detection");

    let env = Environment::detect();
    let env_info = EnvironmentInfo {
        is_container: env.is_container,
        is_secure_boot: env.is_secure_boot,
        has_dev_port: env.has_dev_port,
    };

    if !env.can_probe() {
        if env.is_secure_boot {
            info!(
                "Kernel lockdown is active (usually from Secure Boot), so Super-I/O \
                 chips cannot be probed. Drivers loaded by other means are not affected."
            );
        } else {
            info!("/dev/port unavailable, skipping hardware detection");
        }
        return detect_without_probe(&env, env_info, load_modules);
    }

    // Load chip database
    let mut db = ChipDatabase::load_compiled();
    if let Some(path) = override_path {
        if path.exists() {
            db.load_override(path);
        }
    }

    // Open /dev/port
    let mut port = match port_io::DevPort::open() {
        Ok(port) => port,
        Err(err) => {
            // A container cannot see the lockdown state, so lockdown first shows here.
            warn!("Failed to open /dev/port: {err}");
            return detect_without_probe(&env, env_info, load_modules);
        }
    };

    // Run detection
    let detected = superio::detect_superio(&mut port, &db);

    // Process results and optionally load modules
    process_results(&detected, load_modules, &env, env_info)
}

/// The results of a detection whose probe could not run: no chips, and the
/// installed out-of-tree drivers handed to the kernel when that is allowed.
#[cfg(target_arch = "x86_64")]
fn detect_without_probe(
    env: &Environment,
    env_info: EnvironmentInfo,
    load_modules: bool,
) -> DetectionResults {
    let mut results = DetectionResults::empty(env_info);
    assert!(results.detected_chips.is_empty());
    if should_load_without_probe(env, load_modules) {
        kernel_probe::load_installed_out_of_tree(env.is_secure_boot, &mut results);
    }
    results
}

/// Whether installed out-of-tree drivers stand in for a probe that could not
/// run: only when module loading was asked for, and modprobe can run here.
///
/// The environment's own verdict on the probe is not consulted: `/dev/port` can
/// be present and still refuse to open.
#[cfg(target_arch = "x86_64")]
fn should_load_without_probe(env: &Environment, load_modules: bool) -> bool {
    if load_modules.not() {
        return false;
    }
    env.can_load_modules()
}

/// Non-x86_64 stub: detection is not supported.
#[cfg(not(target_arch = "x86_64"))]
pub fn run_detection(_load_modules: bool, _override_path: Option<&Path>) -> DetectionResults {
    info!("Super-I/O detection is only supported on x86_64");
    DetectionResults::empty(EnvironmentInfo {
        is_container: false,
        is_secure_boot: false,
        has_dev_port: false,
    })
}

impl DetectionResults {
    /// Nothing detected and nothing loaded yet.
    #[must_use]
    pub fn empty(environment: EnvironmentInfo) -> Self {
        Self {
            detected_chips: Vec::new(),
            blacklisted: Vec::new(),
            signature_rejected: Vec::new(),
            kernel_probed: Vec::new(),
            environment,
        }
    }
}

#[cfg(target_arch = "x86_64")]
fn process_results(
    detected: &Vec<DetectedChip>,
    load_modules: bool,
    env: &Environment,
    env_info: EnvironmentInfo,
) -> DetectionResults {
    let mut chip_infos = Vec::new();
    let mut blacklisted = Vec::new();
    let mut signature_rejected = Vec::new();

    for chip in detected {
        let module_status = if !load_modules {
            "detection_only".to_string()
        } else if !env.can_load_modules() {
            "skipped_no_modprobe".to_string()
        } else {
            let result = module_loader::load_module(&chip.driver, env.is_secure_boot);
            match result {
                LoadResult::Blacklisted => blacklisted.push(chip.driver.clone()),
                LoadResult::SignatureRejected => signature_rejected.push(chip.driver.clone()),
                _ => {}
            }
            result.status()
        };

        chip_infos.push(DetectedChipInfo {
            name: chip.name.clone(),
            driver: chip.driver.clone(),
            address: format!("0x{:02X}", chip.address),
            base_address: format!("0x{:04X}", chip.base_address),
            device_id: format!("0x{:04X}", chip.device_id),
            features: chip.features.clone(),
            module_status,
        });
    }

    debug!(
        "Detection complete: {} chips detected, {} blacklisted",
        chip_infos.len(),
        blacklisted.len()
    );

    DetectionResults {
        detected_chips: chip_infos,
        blacklisted,
        signature_rejected,
        kernel_probed: Vec::new(),
        environment: env_info,
    }
}

/// Attempt to print detection results in a human-readable table format.
pub fn output_results(results: &DetectionResults) {
    for probed in &results.kernel_probed {
        info!(
            "Loaded without a probe: {} ({})",
            probed.driver, probed.module_status
        );
    }
    if results.detected_chips.is_empty() {
        info!("No Super-I/O chips detected.");
        if results.environment.is_container {
            info!("  (Running inside a container - hardware probing may be limited)");
        }
        if results.environment.is_secure_boot {
            info!(
                "  (Kernel lockdown is active, usually from Secure Boot, so Super-I/O \
                 chips could not be probed)"
            );
        }
        if !results.environment.has_dev_port {
            info!("  (/dev/port is not available)");
        }
        return;
    }

    info!("Detected Super-I/O chips:");
    info!(
        "  {:<40} {:<14} {:<8} {:<10} {:<8} Status",
        "Chip", "Driver", "Address", "Base Addr", "Dev ID"
    );
    info!("  {}", "-".repeat(98));
    for chip in &results.detected_chips {
        info!(
            "  {:<40} {:<14} {:<8} {:<10} {:<8} {}",
            chip.name,
            chip.driver,
            chip.address,
            chip.base_address,
            chip.device_id,
            chip.module_status
        );
    }

    if !results.blacklisted.is_empty() {
        info!("\nBlacklisted drivers: {}", results.blacklisted.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn test_process_results_detection_only() {
        let detected = vec![DetectedChip {
            name: "ITE IT8686E".into(),
            driver: "it87".into(),
            address: 0x2E,
            base_address: 0x0290,
            device_id: 0x0290,
            features: vec!["fan".into(), "temp".into()],
            active: true,
        }];
        let env = Environment {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
            has_modprobe: true,
            has_module_access: false,
        };
        let env_info = EnvironmentInfo {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
        };

        let results = process_results(&detected, false, &env, env_info);
        assert_eq!(results.detected_chips.len(), 1);
        assert_eq!(results.detected_chips[0].module_status, "detection_only");
    }

    #[cfg(target_arch = "x86_64")]
    #[test]
    fn test_process_results_empty() {
        let env = Environment {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
            has_modprobe: true,
            has_module_access: false,
        };
        let env_info = EnvironmentInfo {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
        };

        let results = process_results(&Vec::new(), false, &env, env_info);
        assert!(results.detected_chips.is_empty());
        assert!(results.blacklisted.is_empty());
    }

    /// Goal: the no-probe fallback runs under lockdown, without `/dev/port`,
    /// and when a present `/dev/port` refused to open, and never when loading
    /// is off or modprobe cannot run. A container counts only with module
    /// access. Method: each gate flipped from a locked-down base.
    #[cfg(target_arch = "x86_64")]
    #[test]
    fn fallback_runs_whenever_modules_can_be_loaded() {
        let locked_down = Environment {
            is_container: false,
            is_secure_boot: true,
            has_dev_port: true,
            has_modprobe: true,
            has_module_access: false,
        };
        let no_dev_port = Environment {
            is_secure_boot: false,
            has_dev_port: false,
            ..locked_down.clone()
        };
        // What a container sees under lockdown: the probe looks possible.
        let open_refused = Environment {
            is_secure_boot: false,
            ..locked_down.clone()
        };
        assert!(open_refused.can_probe());
        let container = Environment {
            is_container: true,
            ..locked_down.clone()
        };
        let container_with_access = Environment {
            has_module_access: true,
            ..container.clone()
        };
        let no_modprobe = Environment {
            has_modprobe: false,
            ..locked_down.clone()
        };
        assert!(should_load_without_probe(&locked_down, true));
        assert!(should_load_without_probe(&container_with_access, true));
        assert!(should_load_without_probe(&no_dev_port, true));
        assert!(should_load_without_probe(&locked_down, false).not());
        assert!(should_load_without_probe(&open_refused, true));
        assert!(should_load_without_probe(&open_refused, false).not());
        assert!(should_load_without_probe(&container, true).not());
        assert!(should_load_without_probe(&no_modprobe, true).not());
    }

    #[test]
    fn test_print_results_empty() {
        let results = DetectionResults {
            detected_chips: Vec::new(),
            blacklisted: Vec::new(),
            signature_rejected: Vec::new(),
            kernel_probed: Vec::new(),
            environment: EnvironmentInfo {
                is_container: false,
                is_secure_boot: false,
                has_dev_port: true,
            },
        };
        // Should not panic
        output_results(&results);
    }

    #[test]
    fn test_print_results_with_chips() {
        let results = DetectionResults {
            detected_chips: vec![DetectedChipInfo {
                name: "ITE IT8686E".into(),
                driver: "it87".into(),
                address: "0x2E".into(),
                base_address: "0x0290".into(),
                device_id: "0x0290".into(),
                features: vec!["fan".into()],
                module_status: "loaded".into(),
            }],
            blacklisted: vec!["nouveau".into()],
            signature_rejected: Vec::new(),
            kernel_probed: vec![KernelProbedDriver {
                driver: "nct6687".into(),
                module_status: "signature_rejected".into(),
            }],
            environment: EnvironmentInfo {
                is_container: false,
                is_secure_boot: false,
                has_dev_port: true,
            },
        };
        // Should not panic
        output_results(&results);
    }
}
