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

//! Detects Super-I/O chips via I/O port probing and loads the appropriate
//! kernel modules. Based on the
//! [TsunamiMommy/lm-sensors](https://github.com/TsunamiMommy/lm-sensors) fork with a two-path
//! detection algorithm (fast non-invasive + fallback with config mode entry).
//!
//! This crate is x86_64-only - Super-I/O I/O port probing is architecture-specific.

pub mod chip_db;
pub mod chips_custom;
pub mod environment;
pub mod module_loader;
pub mod port_io;
pub mod shell_command;
pub mod superio;

use std::path::Path;

use log::{debug, info, warn};
use serde::Serialize;

use chip_db::ChipDatabase;
use environment::Environment;
use module_loader::LoadResult;
use superio::DetectedChip;

/// Override file path for user/distro chip additions.
pub const OVERRIDE_FILE_PATH: &str = "/etc/coolercontrol/detect.toml";

/// Complete detection results.
#[derive(Debug, Clone, Serialize)]
pub struct DetectionResults {
    pub detected_chips: Vec<DetectedChipInfo>,
    pub skipped: Vec<SkippedDriver>,
    pub blacklisted: Vec<String>,
    pub environment: EnvironmentInfo,
}

/// Information about a detected chip and its module load status.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedChipInfo {
    pub name: String,
    pub driver: String,
    pub address: String,
    pub base_address: String,
    pub features: Vec<String>,
    pub module_status: String,
}

/// A driver that was skipped due to conflict.
#[derive(Debug, Clone, Serialize)]
pub struct SkippedDriver {
    pub driver: String,
    pub reason: String,
    pub preferred: String,
}

/// Environment information included in results.
#[derive(Debug, Clone, Serialize)]
pub struct EnvironmentInfo {
    pub is_container: bool,
    pub has_dev_port: bool,
}

/// Run the full Super-I/O detection pipeline.
///
/// # Arguments
/// * `load_modules` - If `true`, also load detected kernel modules via modprobe.
///
/// # Returns
/// Detection results including detected chips, skipped drivers, and environment info.
#[cfg(target_arch = "x86_64")]
#[must_use]
pub fn run_detection(load_modules: bool) -> DetectionResults {
    info!("Starting Super-I/O hardware detection");

    let env = Environment::detect();
    let env_info = EnvironmentInfo {
        is_container: env.is_container,
        has_dev_port: env.has_dev_port,
    };

    if !env.can_probe() {
        info!("/dev/port unavailable, skipping hardware detection");
        return DetectionResults {
            detected_chips: Vec::new(),
            skipped: Vec::new(),
            blacklisted: Vec::new(),
            environment: env_info,
        };
    }

    // Load chip database
    let mut db = ChipDatabase::load_compiled();
    let override_path = Path::new(OVERRIDE_FILE_PATH);
    if override_path.exists() {
        db.load_override(override_path);
    }

    // Open /dev/port
    let mut port = match port_io::DevPort::open() {
        Ok(port) => port,
        Err(err) => {
            warn!("Failed to open /dev/port: {err}");
            return DetectionResults {
                detected_chips: Vec::new(),
                skipped: Vec::new(),
                blacklisted: Vec::new(),
                environment: env_info,
            };
        }
    };

    // Run detection
    let detected = superio::detect_superio(&mut port, &db);

    // Process results and optionally load modules
    process_results(&detected, load_modules, &env, env_info)
}

/// Non-x86_64 stub: detection is not supported.
#[cfg(not(target_arch = "x86_64"))]
pub fn run_detection(_load_modules: bool) -> DetectionResults {
    info!("Super-I/O detection is only supported on x86_64");
    DetectionResults {
        detected_chips: Vec::new(),
        skipped: Vec::new(),
        blacklisted: Vec::new(),
        environment: EnvironmentInfo {
            is_container: false,
            has_dev_port: false,
        },
    }
}

fn process_results(
    detected: &Vec<DetectedChip>,
    load_modules: bool,
    env: &Environment,
    env_info: EnvironmentInfo,
) -> DetectionResults {
    let mut chip_infos = Vec::new();
    let mut skipped = Vec::new();
    let mut blacklisted = Vec::new();

    // Collect all detected driver names for conflict resolution
    let all_drivers: Vec<String> = detected.iter().map(|c| c.driver.clone()).collect();

    for chip in detected {
        let module_status = if !load_modules {
            "detection_only".to_string()
        } else if !env.can_load_modules() {
            "skipped_no_modprobe".to_string()
        } else {
            match module_loader::load_module(&chip.driver, &all_drivers) {
                LoadResult::Loaded => "loaded".to_string(),
                LoadResult::AlreadyLoaded => "already_loaded".to_string(),
                LoadResult::Blacklisted => {
                    blacklisted.push(chip.driver.clone());
                    "blacklisted".to_string()
                }
                LoadResult::ConflictSkipped { preferred } => {
                    skipped.push(SkippedDriver {
                        driver: chip.driver.clone(),
                        reason: "conflict_with_preferred".to_string(),
                        preferred: preferred.clone(),
                    });
                    format!("skipped_conflict_{preferred}")
                }
                LoadResult::Failed(err) => {
                    format!("failed: {err}")
                }
            }
        };

        chip_infos.push(DetectedChipInfo {
            name: chip.name.clone(),
            driver: chip.driver.clone(),
            address: format!("0x{:02X}", chip.address),
            base_address: format!("0x{:04X}", chip.base_address),
            features: chip.features.clone(),
            module_status,
        });
    }

    // Run udevadm settle if we loaded any modules
    if load_modules
        && env.can_load_modules()
        && chip_infos.iter().any(|c| c.module_status == "loaded")
    {
        module_loader::udevadm_settle();
    }

    debug!(
        "Detection complete: {} chips detected, {} skipped, {} blacklisted",
        chip_infos.len(),
        skipped.len(),
        blacklisted.len()
    );

    DetectionResults {
        detected_chips: chip_infos,
        skipped,
        blacklisted,
        environment: env_info,
    }
}

/// Attempt to print detection results in a human-readable table format.
pub fn output_results(results: &DetectionResults) {
    if results.detected_chips.is_empty() {
        info!("No Super-I/O chips detected.");
        if results.environment.is_container {
            info!("  (Running inside a container - hardware probing may be limited)");
        }
        if !results.environment.has_dev_port {
            info!("  (/dev/port is not available)");
        }
        return;
    }

    info!("Detected Super-I/O chips:");
    info!(
        "  {:<40} {:<14} {:<8} {:<10} Status",
        "Chip", "Driver", "Address", "Base Addr"
    );
    info!("  {}", "-".repeat(90));
    for chip in &results.detected_chips {
        info!(
            "  {:<40} {:<14} {:<8} {:<10} {}",
            chip.name, chip.driver, chip.address, chip.base_address, chip.module_status
        );
    }

    if !results.skipped.is_empty() {
        info!("\nSkipped drivers:");
        for skip in &results.skipped {
            info!(
                "  {} - {} (preferred: {})",
                skip.driver, skip.reason, skip.preferred
            );
        }
    }

    if !results.blacklisted.is_empty() {
        info!("\nBlacklisted drivers: {}", results.blacklisted.join(", "));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_results_detection_only() {
        let detected = vec![DetectedChip {
            name: "ITE IT8686E".into(),
            driver: "it87".into(),
            address: 0x2E,
            base_address: 0x0290,
            features: vec!["fan".into(), "temp".into()],
            active: true,
        }];
        let env = Environment {
            is_container: false,
            has_dev_port: true,
            has_modprobe: true,
        };
        let env_info = EnvironmentInfo {
            is_container: false,
            has_dev_port: true,
        };

        let results = process_results(&detected, false, &env, env_info);
        assert_eq!(results.detected_chips.len(), 1);
        assert_eq!(results.detected_chips[0].module_status, "detection_only");
    }

    #[test]
    fn test_process_results_empty() {
        let env = Environment {
            is_container: false,
            has_dev_port: true,
            has_modprobe: true,
        };
        let env_info = EnvironmentInfo {
            is_container: false,
            has_dev_port: true,
        };

        let results = process_results(&Vec::new(), false, &env, env_info);
        assert!(results.detected_chips.is_empty());
        assert!(results.skipped.is_empty());
        assert!(results.blacklisted.is_empty());
    }

    #[test]
    fn test_print_results_empty() {
        let results = DetectionResults {
            detected_chips: Vec::new(),
            skipped: Vec::new(),
            blacklisted: Vec::new(),
            environment: EnvironmentInfo {
                is_container: false,
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
                features: vec!["fan".into()],
                module_status: "loaded".into(),
            }],
            skipped: vec![SkippedDriver {
                driver: "nct6687".into(),
                reason: "conflict".into(),
                preferred: "nct6775".into(),
            }],
            blacklisted: vec!["nouveau".into()],
            environment: EnvironmentInfo {
                is_container: false,
                has_dev_port: true,
            },
        };
        // Should not panic
        output_results(&results);
    }
}
