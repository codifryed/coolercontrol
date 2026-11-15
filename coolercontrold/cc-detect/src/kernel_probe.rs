// SPDX-FileCopyrightText: 2026 Guy Boldon, megadjc and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Driver loading for when the I/O port probe cannot run.
//!
//! Kernel lockdown blocks our probe, but not a driver probing for its own chip
//! from inside the kernel. An out-of-tree Super-I/O driver the user installed
//! is a clear sign they want it, so it is handed to the kernel, which decides
//! whether its chip is present. Such drivers return `-ENODEV` and unload when
//! it is not. In-tree drivers are never loaded blindly.

use std::collections::HashSet;
use std::fs::File;
use std::io::Read;
use std::ops::Not;
use std::path::PathBuf;

use log::{debug, info};

use crate::module_loader::{self, LoadResult};
use crate::{DetectionResults, KernelProbedDriver};

/// A distro `modules.dep` is around 1 MiB. Anything far larger is not one.
const MODULES_DEP_MAX_BYTES: u64 = 16 * 1024 * 1024;
const OSRELEASE_PATH: &str = "/proc/sys/kernel/osrelease";
const PROC_MODULES_PATH: &str = "/proc/modules";

/// Loads every installed out-of-tree Super-I/O driver, recording each attempt
/// in `results`. Blocking: reads module files and runs modprobe.
///
/// # Panics
///
/// If `results` already records attempts: this runs once per detection.
pub fn load_installed_out_of_tree(lockdown_active: bool, results: &mut DetectionResults) {
    assert!(results.kernel_probed.is_empty());
    let Some(modules_dep) = read_modules_dep() else {
        return;
    };
    let proc_modules = std::fs::read_to_string(PROC_MODULES_PATH).unwrap_or_default();
    let candidates = select_candidates(
        crate::chip_db::compiled_drivers(),
        &modules_dep,
        &proc_modules,
    );
    if candidates.is_empty() {
        debug!("No installed out-of-tree Super-I/O driver to load");
        return;
    }
    info!(
        "Super-I/O probe unavailable, loading installed out-of-tree drivers: {}",
        candidates.join(", ")
    );
    for driver in candidates {
        let result = module_loader::load_module(&driver, lockdown_active);
        record_attempt(results, driver, &result);
    }
    let any_loaded = results
        .kernel_probed
        .iter()
        .any(|probed| probed.module_status == "loaded");
    if any_loaded {
        module_loader::udevadm_settle();
    }
}

/// The out-of-tree Super-I/O drivers worth handing to the kernel, sorted.
///
/// Empty while any Super-I/O driver is loaded: that driver already holds the
/// chip, so an out-of-tree one could not bind to it anyway.
///
/// # Panics
///
/// If more candidates come out than Super-I/O drivers went in.
#[must_use]
pub fn select_candidates(
    superio_drivers: &[String],
    modules_dep: &str,
    proc_modules: &str,
) -> Vec<String> {
    let any_loaded = superio_drivers
        .iter()
        .any(|driver| module_loader::is_module_in_proc_modules(driver, proc_modules));
    if any_loaded {
        return Vec::new();
    }
    let installed = out_of_tree_module_names(modules_dep);
    let mut candidates = superio_drivers
        .iter()
        .filter(|driver| installed.contains(driver.replace('-', "_").as_str()))
        .cloned()
        .collect::<Vec<_>>();
    candidates.sort_unstable();
    candidates.dedup();
    assert!(candidates.len() <= superio_drivers.len());
    candidates
}

/// Module names whose `modules.dep` path lies outside the kernel's own tree:
/// `updates/` for DKMS, `extra/` for akmods. Hyphens are normalized to
/// underscores, as modprobe does.
fn out_of_tree_module_names(modules_dep: &str) -> HashSet<String> {
    let mut names = HashSet::new();
    for line in modules_dep.lines() {
        let Some((path, _)) = line.split_once(':') else {
            continue;
        };
        if path.starts_with("kernel/") || path.contains("/kernel/") {
            continue;
        }
        let Some(file_name) = path.rsplit('/').next() else {
            continue;
        };
        let Some((name, _)) = file_name.split_once(".ko") else {
            continue;
        };
        if name.is_empty().not() {
            names.insert(name.replace('-', "_"));
        }
    }
    names
}

fn record_attempt(results: &mut DetectionResults, driver: String, result: &LoadResult) {
    match result {
        LoadResult::Blacklisted => results.blacklisted.push(driver.clone()),
        LoadResult::SignatureRejected => results.signature_rejected.push(driver.clone()),
        _ => {}
    }
    results.kernel_probed.push(KernelProbedDriver {
        driver,
        module_status: result.status(),
    });
}

/// `None` when the running kernel's `modules.dep` cannot be read, as on a
/// system that keeps its modules elsewhere.
fn read_modules_dep() -> Option<String> {
    let release = std::fs::read_to_string(OSRELEASE_PATH).ok()?;
    let release = release.trim();
    if release.is_empty() || release.contains('/') {
        return None;
    }
    let path = PathBuf::from(format!("/lib/modules/{release}/modules.dep"));
    let mut content = String::new();
    let read = File::open(&path).and_then(|file| {
        file.take(MODULES_DEP_MAX_BYTES + 1)
            .read_to_string(&mut content)
    });
    match read {
        Ok(length_bytes)
            if u64::try_from(length_bytes).is_ok_and(|length| length <= MODULES_DEP_MAX_BYTES) =>
        {
            Some(content)
        }
        Ok(_) => {
            debug!("{} is implausibly large, skipping", path.display());
            None
        }
        Err(err) => {
            debug!("Could not read {}: {err}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULES_DEP: &str = "\
kernel/drivers/hwmon/nct6775-core.ko.zst: kernel/drivers/hwmon/hwmon-vid.ko.zst
kernel/drivers/hwmon/nct6683.ko.zst:
kernel/drivers/hwmon/it87.ko.zst: kernel/drivers/hwmon/hwmon-vid.ko.zst
updates/dkms/nct6687.ko.zst:
extra/it87.ko.xz:
updates/dkms/some-other.ko:
";

    fn superio_drivers() -> Vec<String> {
        ["f71882fg", "it87", "nct6683", "nct6687", "nct6775"]
            .map(String::from)
            .to_vec()
    }

    /// Goal: only modules installed outside `kernel/` count, by module name.
    /// Method: a `modules.dep` mixing in-tree, DKMS and akmods paths, plus an
    /// absolute in-tree path and malformed lines.
    #[test]
    fn out_of_tree_names_come_from_updates_and_extra() {
        let content = format!(
            "{MODULES_DEP}/lib/modules/6.17.0/kernel/drivers/hwmon/f71882fg.ko:\nno colon\n.ko:\n"
        );
        let names = out_of_tree_module_names(&content);
        assert_eq!(
            names,
            HashSet::from(["nct6687", "it87", "some_other"].map(String::from))
        );
        assert!(names.contains("nct6683").not());
        assert!(names.contains("f71882fg").not());
    }

    /// Goal: candidates are the installed out-of-tree Super-I/O drivers, never
    /// an in-tree one and never an unrelated out-of-tree module. Method: the
    /// fixture with nothing loaded.
    #[test]
    fn candidates_are_installed_out_of_tree_superio_drivers() {
        let candidates = select_candidates(&superio_drivers(), MODULES_DEP, "");
        assert_eq!(candidates, vec!["it87".to_string(), "nct6687".to_string()]);
    }

    /// Goal: a loaded Super-I/O driver, in-tree or not, stops every attempt,
    /// because it already holds the chip. An unrelated loaded module does not.
    /// Method: the same fixture under three `/proc/modules` contents.
    #[test]
    fn a_loaded_superio_driver_stops_the_fallback() {
        let drivers = superio_drivers();
        let in_tree = "nct6683 24576 0 - Live 0x0000000000000000\n";
        let out_of_tree = "nct6687 24576 0 - Live 0x0000000000000000 (OE)\n";
        let unrelated = "k10temp 12288 0 - Live 0x0000000000000000\n";
        assert!(select_candidates(&drivers, MODULES_DEP, in_tree).is_empty());
        assert!(select_candidates(&drivers, MODULES_DEP, out_of_tree).is_empty());
        assert_eq!(select_candidates(&drivers, MODULES_DEP, unrelated).len(), 2);
    }

    /// Goal: with no out-of-tree driver installed nothing is attempted, which
    /// is the common case and must stay silent. Method: in-tree lines only.
    #[test]
    fn no_out_of_tree_install_yields_no_candidates() {
        let in_tree_only = "kernel/drivers/hwmon/nct6683.ko.zst:\n";
        assert!(select_candidates(&superio_drivers(), in_tree_only, "").is_empty());
        assert!(select_candidates(&superio_drivers(), "", "").is_empty());
    }

    /// Goal: each attempt is recorded, and rejected or blacklisted drivers also
    /// land in their own lists, mirroring the probe path. Method: one of each.
    #[test]
    fn attempts_are_recorded_with_their_lists() {
        let mut results = DetectionResults::empty(crate::EnvironmentInfo {
            is_container: false,
            is_secure_boot: true,
            has_dev_port: true,
        });
        record_attempt(
            &mut results,
            "nct6687".into(),
            &LoadResult::SignatureRejected,
        );
        record_attempt(&mut results, "it87".into(), &LoadResult::NoDevice);
        record_attempt(&mut results, "f71882fg".into(), &LoadResult::Blacklisted);
        assert_eq!(results.signature_rejected, vec!["nct6687".to_string()]);
        assert_eq!(results.blacklisted, vec!["f71882fg".to_string()]);
        let statuses = results
            .kernel_probed
            .iter()
            .map(|probed| probed.module_status.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            statuses,
            vec!["signature_rejected", "no_device", "blacklisted"]
        );
    }
}
