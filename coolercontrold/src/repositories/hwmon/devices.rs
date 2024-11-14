/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use crate::cc_fs;
use crate::device::UID;
use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;
use cached::proc_macro::cached;
use log::{debug, warn};
use nu_glob::{glob, GlobResult};
use pciid_parser::Database;
use regex::Regex;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const GLOB_PWM_PATH: &str = "/sys/class/hwmon/hwmon*/pwm*";
const GLOB_TEMP_PATH: &str = "/sys/class/hwmon/hwmon*/temp*_input";
// CentOS has an intermediate /device directory:
const GLOB_PWM_PATH_CENTOS: &str = "/sys/class/hwmon/hwmon*/device/pwm*";
const GLOB_TEMP_PATH_CENTOS: &str = "/sys/class/hwmon/hwmon*/device/temp*_input";
const PATTERN_PWN_PATH_NUMBER: &str = r".*/pwm\d+$";
const PATTERN_HWMON_PATH_NUMBER: &str = r"/(?P<hwmon>hwmon)(?P<number>\d+)";
// const NODE_PATH: &str = "/sys/devices/system/node"; // NOT USED until hwmon driver fixed
// these are devices that are handled by other repos (liqiuidctl/gpu) and need not be duplicated
pub const HWMON_DEVICE_NAME_BLACKLIST: [&str; 1] = [
    "amdgpu", // GPU Repo handles this
];
const LAPTOP_DEVICE_NAMES: [&str; 3] = ["thinkpad", "asus-nb-wmi", "asus_fan"];
pub const THINKPAD_DEVICE_NAME: &str = "thinkpad";

struct GlobPaths {
    pwm: String,
    pwm_centos: String,
    temp: String,
    temp_centos: String,
}

impl Default for GlobPaths {
    fn default() -> Self {
        Self {
            pwm: GLOB_PWM_PATH.to_string(),
            pwm_centos: GLOB_PWM_PATH_CENTOS.to_string(),
            temp: GLOB_TEMP_PATH.to_string(),
            temp_centos: GLOB_TEMP_PATH_CENTOS.to_string(),
        }
    }
}

/// Get distinct sorted hwmon paths that have either fan controls or temps.
/// We additionally need to check for `CentOS` style paths.
pub fn find_all_hwmon_device_paths() -> Vec<PathBuf> {
    find_all_hwmon_device_paths_inner(&GlobPaths::default())
}

/// Note: checking for both path types works because we are specifically looking for pwm and
/// temp files. Just checking base paths would not work due to the same "device" directory.
fn find_all_hwmon_device_paths_inner(glob_paths: &GlobPaths) -> Vec<PathBuf> {
    let pwm_glob_results = glob(&glob_paths.pwm)
        .unwrap()
        .chain(glob(&glob_paths.pwm_centos).unwrap())
        .collect::<Vec<GlobResult>>();
    let regex_pwm_path = Regex::new(PATTERN_PWN_PATH_NUMBER).unwrap();
    let mut base_paths = pwm_glob_results
        .into_iter()
        .filter_map(Result::ok)
        .filter(|path| path.is_absolute())
        // search for only pwm\d+ files (no _mode, _enable, etc):
        .filter(|path| regex_pwm_path.is_match(path.to_str().expect("Path should be UTF-8")))
        .map(|path| path.parent().unwrap().to_path_buf())
        .collect::<Vec<PathBuf>>();
    let temp_glob_results = glob(&glob_paths.temp)
        .unwrap()
        .chain(glob(&glob_paths.temp_centos).unwrap())
        .collect::<Vec<GlobResult>>();
    base_paths.append(
        &mut temp_glob_results
            .into_iter()
            .filter_map(Result::ok)
            .filter(|path| path.is_absolute())
            .map(|path| path.parent().unwrap().to_path_buf())
            .collect::<Vec<PathBuf>>(),
    );
    deduplicate_and_sort_paths(base_paths)
}

fn deduplicate_and_sort_paths(base_paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut sorted_path_list = base_paths
        .into_iter()
        .collect::<HashSet<PathBuf>>()
        .into_iter()
        .collect::<Vec<PathBuf>>();
    sorted_path_list.sort();
    sorted_path_list
}

/// Returns the found device "name" or if not found, the hwmon number
pub async fn get_device_name(base_path: &PathBuf) -> String {
    if let Ok(contents) = cc_fs::read_sysfs(base_path.join("name")).await {
        contents.trim().to_string()
    } else {
        // hwmon\d+ should always exist in the path (from previous search)
        let captures = Regex::new(PATTERN_HWMON_PATH_NUMBER)
            .unwrap()
            .captures(base_path.to_str().unwrap())
            .unwrap();
        let hwmon_number = captures.name("number").unwrap().as_str().to_string();
        let hwmon_name = format!("Hwmon#{hwmon_number}");
        warn!(
            "Hwmon driver at location: {:?} has no name set, using default: {}",
            base_path, &hwmon_name
        );
        hwmon_name
    }
}

/// Some drivers like thinkpad should have an automatic fallback for safety reasons.
pub fn device_needs_pwm_fallback(device_name: &str) -> bool {
    LAPTOP_DEVICE_NAMES.contains(&device_name)
}

/// Returns the device model name if it exists.
/// This is common for some hardware, like hard drives, and helps differentiate similar devices.
pub async fn get_device_model_name(base_path: &Path) -> Option<String> {
    cc_fs::read_sysfs(device_path(base_path).join("model"))
        .await
        .map(|model| model.trim().to_string())
        .ok()
}

/// Gets the real device path under /sys. This path doesn't change between boots
/// and contains additional sysfs files outside of hardware monitoring.
/// All `HWMon` devices should have this path.
pub fn get_static_device_path_str(base_path: &Path) -> Option<String> {
    get_canonical_path_str(&device_path(base_path))
}

/// Returns the sysfs device path for a given `base_path`.
///
/// If the `base_path` already ends with "device", it is assumed to be a `CentOS` style path
/// and is returned as is. Otherwise, the "device" component is appended to the `base_path`.
///
/// # Examples
///
/// * For a `CentOS` style path, `device_path("/sys/class/hwmon/hwmon0/device")` would return
///   `"/sys/class/hwmon/hwmon0/device"`.
/// * For a standard Linux style path, `device_path("/sys/class/hwmon/hwmon0")` would return
///   `"/sys/class/hwmon/hwmon0/device"`.
fn device_path(base_path: &Path) -> PathBuf {
    // CentOS style path:
    if base_path.ends_with("device") {
        base_path.to_path_buf()
    } else {
        base_path.join("device")
    }
}

fn get_canonical_path_str(path: &Path) -> Option<String> {
    cc_fs::canonicalize(path)
        .inspect_err(|err| warn!("Error getting device path from {path:?}, {err}"))
        .ok()
        .and_then(|path| path.to_str().map(std::borrow::ToOwned::to_owned))
}

/// Creates a unique identifier for a device.
/// The preferred order of identifiers is:
///
/// 1. device serial number
/// 2. realpath under /sys
/// 3. PCI ID
/// 4. device name
///
/// The purpose of this is to ensure that we have unique IDs for device settings that persist
/// across boots and hardware changes if possible.
pub async fn get_device_unique_id(base_path: &Path, device_name: &str) -> UID {
    if let Some(serial) = get_device_serial_number(base_path).await {
        serial
    } else if let Some(device_path) = get_static_device_path_str(base_path) {
        device_path
    } else if let Some(vendor_and_model_id) =
        get_device_uevent_details(base_path).await.get("PCI_ID")
    {
        vendor_and_model_id.to_owned()
    } else {
        device_name.to_owned()
    }
}

/// Returns the device serial number if found.
pub async fn get_device_serial_number(base_path: &Path) -> Option<String> {
    match cc_fs::read_sysfs(device_path(base_path).join("serial")).await {
        Ok(serial) => Some(serial.trim().to_string()),
        Err(_) => {
            // usb hid serial numbers are here:
            let device_details = get_device_uevent_details(base_path).await;
            device_details.get("HID_UNIQ").map(ToString::to_string)
        }
    }
}

/// Checks if there are duplicate device names but different device paths,
/// and adjust them as necessary. i.e. nvme drivers.
pub async fn handle_duplicate_device_names(hwmon_drivers: &mut [HwmonDriverInfo]) {
    let mut duplicate_name_count_map = HashMap::new();
    for (sd_index, starting_driver) in hwmon_drivers.iter().enumerate() {
        let mut count = 0;
        for (other_index, other_driver) in hwmon_drivers.iter().enumerate() {
            if sd_index == other_index || starting_driver.name == other_driver.name {
                count += 1;
            }
        }
        duplicate_name_count_map.insert(sd_index, count);
    }
    for (driver_index, count) in duplicate_name_count_map {
        if count > 1 {
            if let Some(driver) = hwmon_drivers.get_mut(driver_index) {
                let alternate_name = get_alternative_device_name(driver).await;
                driver.name = alternate_name;
            }
        }
    }
}

/// Searches for the best alternative name to use in case of a duplicate device name
async fn get_alternative_device_name(driver: &HwmonDriverInfo) -> String {
    let device_details = get_device_uevent_details(&driver.path).await;
    if let Some(dev_name) = device_details.get("DEVNAME") {
        dev_name.to_string()
    } else if let Some(minor_num) = device_details.get("MINOR") {
        format!("{}{}", driver.name, minor_num)
    } else if let Some(model) = driver.model.clone() {
        model
    } else {
        driver.name.clone()
    }
}

/// Gets the device's **PCI and SUBSYSTEM PCI** vendor and model names
pub async fn get_device_pci_names(base_path: &Path) -> Option<PciDeviceNames> {
    let uevents = get_device_uevent_details(base_path).await;
    let (vendor_id, model_id) = uevents.get("PCI_ID")?.split_once(':')?;
    let (subsys_vendor_id, subsys_model_id) = uevents.get("PCI_SUBSYS_ID")?.split_once(':')?;
    let db = Database::read()
        .inspect_err(|err| {
            warn!("Could not read PCI ID database: {err}, device name information will be limited");
        })
        .ok()?;
    let info = db.get_device_info(vendor_id, model_id, subsys_vendor_id, subsys_model_id);
    let pci_device_names = PciDeviceNames {
        vendor_name: info.vendor_name.map(str::to_owned),
        device_name: info.device_name.map(str::to_owned),
        subvendor_name: info.subvendor_name.map(str::to_owned),
        subdevice_name: info.subdevice_name.map(str::to_owned),
    };
    debug!("Found PCI Device Names: {pci_device_names:?}");
    Some(pci_device_names)
}

pub async fn get_pci_slot_name(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("PCI_SLOT_NAME")
        .map(std::borrow::ToOwned::to_owned)
}

pub async fn get_device_driver_name(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("DRIVER")
        .map(std::borrow::ToOwned::to_owned)
}

pub async fn get_device_mod_alias(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("MODALIAS")
        .map(std::borrow::ToOwned::to_owned)
}

pub async fn get_device_hid_phys(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("HID_PHYS")
        .map(std::borrow::ToOwned::to_owned)
}

#[cached(
    key = "String",
    convert = r#"{ format!("{:?}", base_path) }"#,
    sync_writes = true
)]
async fn get_device_uevent_details(base_path: &Path) -> HashMap<String, String> {
    let mut device_details = HashMap::new();
    if let Ok(content) = cc_fs::read_txt(device_path(base_path).join("uevent")).await {
        for line in content.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_string();
                let value = v.trim().to_string();
                device_details.insert(key, value);
            }
        }
    }
    device_details
}

// NOT USED UNTIL ABOVE BUG IS FIXED IN HWMON DRIVER:
// Returns the associated processor IDs.
// NOTE: This is only for AMD CPUs.
//
// The standard location of base_path/device/local_cpulist does
// not actually give the correct cpulist for multiple cpus. Seems like a kernel driver bug.
// Due to that issue, we use the "node" device, which is the only place found as of yet that
// actually gives the separate cpulist. There is currently no way to be 100% sure that the hwmon
// device lines up with which cpulist. (best guess for now, index == node)
// pub async fn get_processor_ids_from_node_cpulist(index: &usize) -> Result<Vec<u16>> {
//     let mut processor_ids = Vec::new();
//     let content = tokio::fs::read_to_string(
//         PathBuf::from(NODE_PATH).join(format!("node{}", index)).join("cpulist")
//     ).await?;
//     for line in content.lines() {
//         for id_range_raw in line.split(",") {
//             let id_range = id_range_raw.trim();
//             if id_range.contains("-") {
//                 if let Some((start_str, end_incl_str)) = id_range.split_once("-") {
//                     let start = start_str.parse()?;
//                     let end_incl = end_incl_str.parse()?;
//                     for id in start..=end_incl {
//                         processor_ids.push(id);
//                     }
//                 }
//             } else {
//                 processor_ids.push(id_range.parse()?);
//             }
//         }
//     }
//     processor_ids.sort_unstable();
//     Ok(processor_ids)
// }

#[derive(Debug, Clone)]
pub struct PciDeviceNames {
    #[allow(dead_code)]
    pub vendor_name: Option<String>,
    pub device_name: Option<String>,
    #[allow(dead_code)]
    pub subvendor_name: Option<String>,
    #[allow(dead_code)]
    pub subdevice_name: Option<String>,
}

/// Tests
#[cfg(test)]
mod tests {
    use std::path::Path;

    use test_context::{test_context, AsyncTestContext};
    use uuid::Uuid;

    use super::*;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    struct HwmonDeviceContext {
        base_path: PathBuf,
        base_path_centos: PathBuf,
        glob_paths: GlobPaths,
    }

    impl AsyncTestContext for HwmonDeviceContext {
        async fn setup() -> HwmonDeviceContext {
            let base_path = Path::new(
                &(TEST_BASE_PATH_STR.to_string() + &Uuid::new_v4().to_string() + "/hwmon/hwmon1/"),
            )
            .to_path_buf();
            tokio::fs::create_dir_all(&base_path).await.unwrap();
            let base_path_centos = Path::new(
                &(TEST_BASE_PATH_STR.to_string()
                    + &Uuid::new_v4().to_string()
                    + "/hwmon/hwmon2/device/"),
            )
            .to_path_buf();
            tokio::fs::create_dir_all(&base_path_centos).await.unwrap();
            let glob_pwm = base_path
                .to_str()
                .unwrap()
                .to_owned()
                .replace("hwmon1", "hwmon*")
                + "pwm*";
            let glob_temp = base_path
                .to_str()
                .unwrap()
                .to_owned()
                .replace("hwmon1", "hwmon*")
                + "temp*";
            let glob_pwm_centos = base_path_centos
                .to_str()
                .unwrap()
                .to_owned()
                .replace("hwmon2", "hwmon*")
                + "pwm*";
            let glob_temp_centos = base_path_centos
                .to_str()
                .unwrap()
                .to_owned()
                .replace("hwmon2", "hwmon*")
                + "temp*";
            HwmonDeviceContext {
                base_path,
                base_path_centos,
                glob_paths: GlobPaths {
                    pwm: glob_pwm,
                    pwm_centos: glob_pwm_centos,
                    temp: glob_temp,
                    temp_centos: glob_temp_centos,
                },
            }
        }

        async fn teardown(self) {
            tokio::fs::remove_dir_all(&self.base_path).await.unwrap();
        }
    }
    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_device_empty(ctx: &mut HwmonDeviceContext) {
        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(hwmon_paths.is_empty());
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_pwm_device(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            ctx.base_path.join("pwm1"),
            b"127", // duty
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 1);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_pwm_device_centos(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            ctx.base_path_centos.join("pwm1"),
            b"127", // duty
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 1);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_temp_device(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            &ctx.base_path.join("temp1"),
            b"70000", // temp
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 1);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_temp_device_centos(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            ctx.base_path_centos.join("temp1"),
            b"70000", // temp
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 1);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_pwm_centos_and_temp_device(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            ctx.base_path_centos.join("pwm1"),
            b"127", // duty
        )
        .await
        .unwrap();
        tokio::fs::write(
            ctx.base_path.join("temp1"),
            b"70000", // temp
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 2);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_pwm_and_temp_centos_device(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            ctx.base_path.join("pwm1"),
            b"127", // duty
        )
        .await
        .unwrap();
        tokio::fs::write(
            ctx.base_path_centos.join("temp1"),
            b"70000", // temp
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 2);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_pwm_device_norm_and_centos(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(
            ctx.base_path.join("pwm1"),
            b"127", // duty
        )
        .await
        .unwrap();

        tokio::fs::write(
            ctx.base_path_centos.join("pwm1"),
            b"127", // duty
        )
        .await
        .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 2);
    }

    #[test_context(HwmonDeviceContext)]
    #[tokio::test]
    async fn find_temp_device_norm_and_centos(ctx: &mut HwmonDeviceContext) {
        // given:
        tokio::fs::write(ctx.base_path.join("temp1"), b"70000")
            .await
            .unwrap();

        tokio::fs::write(ctx.base_path_centos.join("temp1"), b"70000")
            .await
            .unwrap();

        // when:
        let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

        // then:
        assert!(!hwmon_paths.is_empty());
        assert_eq!(hwmon_paths.len(), 2);
    }
}
