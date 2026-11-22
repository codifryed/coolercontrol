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
use std::hash::BuildHasher;
use std::io::Read;
use std::ops::Not;
use std::path::Path;

use log::{debug, info};

use crate::module_loader::{self, LoadResult};
use crate::{DetectionResults, KernelProbedDriver};

/// A distro `modules.dep` is around 1 MiB. Anything far larger is not one.
const MODULES_DEP_MAX_BYTES: u64 = 16 * 1024 * 1024;
const _: () = assert!(MODULES_DEP_MAX_BYTES < usize::MAX as u64);
const PLATFORM_DRIVERS_PATH: &str = "/sys/bus/platform/drivers";

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
    let superio_drivers = crate::chip_db::compiled_drivers();
    let bound = bound_platform_drivers(Path::new(PLATFORM_DRIVERS_PATH), superio_drivers);
    let candidates = select_candidates(superio_drivers, &modules_dep, &bound);
    if candidates.is_empty() {
        debug!("No installed out-of-tree Super-I/O driver to load");
        return;
    }
    info!(
        "Super-I/O probe unavailable, loading installed out-of-tree drivers: {}",
        candidates.join(", ")
    );
    let attempts = candidates.len();
    let mut any_loaded = false;
    for driver in candidates {
        let result = module_loader::load_module(&driver, lockdown_active);
        any_loaded |= matches!(result, LoadResult::Loaded);
        record_attempt(results, driver, &result);
    }
    assert_eq!(results.kernel_probed.len(), attempts);
    if any_loaded {
        module_loader::udevadm_settle();
    }
}

/// The out-of-tree Super-I/O drivers worth handing to the kernel, sorted.
///
/// Empty while any Super-I/O driver is bound to a device: that driver already
/// holds the chip. A driver that is only loaded does not count, as in-tree
/// `nct6683` stays loaded on boards whose chip its probe refuses.
///
/// # Panics
///
/// If more candidates come out than Super-I/O drivers went in.
#[must_use]
pub fn select_candidates<S: BuildHasher>(
    superio_drivers: &[String],
    modules_dep: &str,
    bound_drivers: &HashSet<String, S>,
) -> Vec<String> {
    if bound_drivers.is_empty().not() {
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
    assert!(candidates.windows(2).all(|pair| pair[0] < pair[1]));
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
        if path.starts_with("kernel/") {
            continue;
        }
        if path.contains("/kernel/") {
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
    assert!(names.len() <= modules_dep.lines().count());
    for name in &names {
        assert!(name.is_empty().not());
        assert!(name.contains('-').not());
    }
    names
}

/// The given drivers with a device bound under `platform_drivers`, the
/// `/sys/bus/platform/drivers` tree. A bound device shows as a symlink in the
/// driver's directory; `module` is the one symlink that is not a device.
fn bound_platform_drivers(platform_drivers: &Path, drivers: &[String]) -> HashSet<String> {
    let mut bound = HashSet::new();
    for driver in drivers {
        let Ok(entries) = std::fs::read_dir(platform_drivers.join(driver)) else {
            continue;
        };
        let has_device = entries.flatten().any(|entry| {
            if entry.file_name() == "module" {
                return false;
            }
            entry.file_type().is_ok_and(|kind| kind.is_symlink())
        });
        if has_device {
            bound.insert(driver.clone());
        }
    }
    assert!(bound.len() <= drivers.len());
    bound
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
    let path = crate::environment::module_tree(Path::new("/"))?.join("modules.dep");
    let mut content = String::new();
    let read = File::open(&path).and_then(|file| {
        file.take(MODULES_DEP_MAX_BYTES + 1)
            .read_to_string(&mut content)
    });
    if let Err(err) = read {
        debug!("Could not read {}: {err}", path.display());
        return None;
    }
    // `take` caps the read one byte past the limit, so oversize is detectable.
    assert!(content.len() as u64 <= MODULES_DEP_MAX_BYTES + 1);
    if content.len() as u64 > MODULES_DEP_MAX_BYTES {
        debug!("{} is implausibly large, skipping", path.display());
        return None;
    }
    Some(content)
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
        let candidates = select_candidates(&superio_drivers(), MODULES_DEP, &HashSet::new());
        assert_eq!(candidates, vec!["it87".to_string(), "nct6687".to_string()]);
    }

    /// Goal: a bound Super-I/O driver, in-tree or not, stops every attempt,
    /// because it already holds the chip. Method: the fixture with one bound.
    #[test]
    fn a_bound_superio_driver_stops_the_fallback() {
        let drivers = superio_drivers();
        let in_tree = HashSet::from(["nct6683".to_string()]);
        let out_of_tree = HashSet::from(["nct6687".to_string()]);
        assert!(select_candidates(&drivers, MODULES_DEP, &in_tree).is_empty());
        assert!(select_candidates(&drivers, MODULES_DEP, &out_of_tree).is_empty());
    }

    /// Goal: only a driver directory holding a device symlink counts as bound.
    /// A loaded driver whose probe refused the chip, as in-tree `nct6683` does
    /// on MSI boards, keeps the fallback going. Method: a fake
    /// `/sys/bus/platform/drivers` with one bound and one unbound driver.
    #[test]
    fn only_drivers_with_a_device_count_as_bound() {
        let sysfs = tempfile::tempdir().unwrap();
        let device = sysfs.path().join("devices/nct6775.656");
        let module = sysfs.path().join("modules/nct6683");
        std::fs::create_dir_all(&device).unwrap();
        std::fs::create_dir_all(&module).unwrap();
        let unbound = sysfs.path().join("nct6683");
        std::fs::create_dir(&unbound).unwrap();
        std::fs::write(unbound.join("bind"), "").unwrap();
        std::os::unix::fs::symlink(&module, unbound.join("module")).unwrap();
        let bound_dir = sysfs.path().join("nct6775");
        std::fs::create_dir(&bound_dir).unwrap();
        std::os::unix::fs::symlink(&device, bound_dir.join("nct6775.656")).unwrap();

        let bound = bound_platform_drivers(sysfs.path(), &superio_drivers());
        assert_eq!(bound, HashSet::from(["nct6775".to_string()]));

        let unbound_only = bound_platform_drivers(sysfs.path(), &["nct6683".to_string()]);
        assert!(unbound_only.is_empty());
        let candidates = select_candidates(&superio_drivers(), MODULES_DEP, &unbound_only);
        assert_eq!(candidates, vec!["it87".to_string(), "nct6687".to_string()]);
    }

    /// Goal: with no out-of-tree driver installed nothing is attempted, which
    /// is the common case and must stay silent. Method: in-tree lines only.
    #[test]
    fn no_out_of_tree_install_yields_no_candidates() {
        let in_tree_only = "kernel/drivers/hwmon/nct6683.ko.zst:\n";
        let none_bound = HashSet::new();
        assert!(select_candidates(&superio_drivers(), in_tree_only, &none_bound).is_empty());
        assert!(select_candidates(&superio_drivers(), "", &none_bound).is_empty());
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
