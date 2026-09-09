// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::pci_ids;
use crate::cc_fs;
use crate::device::UID;
use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;
use log::{debug, info, warn};
use nu_glob::{glob, GlobResult, Uninterruptible};
use regex::Regex;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::{Path, PathBuf};

thread_local! {
    static UEVENT_CACHE: RefCell<HashMap<PathBuf, HashMap<String, String>>> =
        RefCell::new(HashMap::new());
}

// controllable fans:
const GLOB_PWM_PATH: &str = "/sys/class/hwmon/hwmon*/pwm*";
// temps:
const GLOB_TEMP_PATH: &str = "/sys/class/hwmon/hwmon*/temp*_input";
// rpm-read-only fans:
const GLOB_FAN_PATH: &str = "/sys/class/hwmon/hwmon*/fan*_input";
// CentOS has an intermediate /device directory:
const GLOB_PWM_PATH_CENTOS: &str = "/sys/class/hwmon/hwmon*/device/pwm*";
const GLOB_TEMP_PATH_CENTOS: &str = "/sys/class/hwmon/hwmon*/device/temp*_input";
const GLOB_FAN_PATH_CENTOS: &str = "/sys/class/hwmon/hwmon*/device/fan*_input";
/// PCI slot of AMD node 0's northbridge / data fabric function, per `k10temp.c`.
const AMD_NODE0_PCI_SLOT: u8 = 0x18;
/// A PCI slot is five bits wide, so 0x18 through 0x1f bounds the nodes it can hold.
const AMD_MAX_NODES: u8 = 8;
const PATTERN_PWN_PATH_NUMBER: &str = r".*/pwm\d+$";
const PATTERN_HWMON_PATH_NUMBER: &str = r"/(?P<hwmon>hwmon)(?P<number>\d+)";
// const NODE_PATH: &str = "/sys/devices/system/node"; // NOT USED until hwmon driver fixed
// these are devices that are handled by other repos (liqiuidctl/gpu) and need not be duplicated
pub const HWMON_DEVICE_NAME_BLACKLIST: [&str; 1] = [
    "amdgpu", // GPU Repo handles this
];
// The hardware report names the GPU repository as this list's reason, which is
// only true while the GPU repository is the only thing in it.
const _: () = assert!(HWMON_DEVICE_NAME_BLACKLIST.len() == 1);
const LAPTOP_DEVICE_NAMES: [&str; 3] = ["thinkpad", "asus-nb-wmi", "asus_fan"];
pub const DEVICE_NAME_THINK_PAD: &str = "thinkpad";
pub const DEVICE_NAME_MAC_SMC: &str = "macsmc-hwmon";
pub const DEVICE_NAMES_APPLE: [&str; 2] = ["applesmc", DEVICE_NAME_MAC_SMC];

struct GlobPaths {
    pwm: String,
    pwm_centos: String,
    temp: String,
    temp_centos: String,
    fan: String,
    fan_centos: String,
}

impl Default for GlobPaths {
    fn default() -> Self {
        Self {
            pwm: GLOB_PWM_PATH.to_string(),
            pwm_centos: GLOB_PWM_PATH_CENTOS.to_string(),
            temp: GLOB_TEMP_PATH.to_string(),
            temp_centos: GLOB_TEMP_PATH_CENTOS.to_string(),
            fan: GLOB_FAN_PATH.to_string(),
            fan_centos: GLOB_FAN_PATH_CENTOS.to_string(),
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
    let pwm_glob_results = glob(&glob_paths.pwm, Uninterruptible)
        .unwrap()
        .chain(glob(&glob_paths.pwm_centos, Uninterruptible).unwrap())
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
    let temp_glob_results = glob(&glob_paths.temp, Uninterruptible)
        .unwrap()
        .chain(glob(&glob_paths.temp_centos, Uninterruptible).unwrap())
        .collect::<Vec<GlobResult>>();
    let fan_glob_results = glob(&glob_paths.fan, Uninterruptible)
        .unwrap()
        .chain(glob(&glob_paths.fan_centos, Uninterruptible).unwrap())
        .collect::<Vec<GlobResult>>();
    base_paths.append(&mut convert_glob_results_to_valid_paths(temp_glob_results));
    base_paths.append(&mut convert_glob_results_to_valid_paths(fan_glob_results));
    deduplicate_and_sort_paths(base_paths)
}

fn convert_glob_results_to_valid_paths(glob_results: Vec<GlobResult>) -> Vec<PathBuf> {
    glob_results
        .into_iter()
        .filter_map(Result::ok)
        .filter(|path| path.is_absolute())
        .map(|path| path.parent().unwrap().to_path_buf())
        .collect()
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
pub async fn get_device_name(base_path: &Path) -> String {
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
        info!(
            "Hwmon driver at location: {} has no name set, using default: {}",
            base_path.display(),
            hwmon_name
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

/// Gets the real device path under /sys. It contains additional sysfs files
/// outside of hardware monitoring. All `HWMon` devices should have this path.
///
/// NOT stable across boots: SCSI/SAS/SATA disks embed probe-order `hostN` / `ataN`.
/// Note: Some 'Virtual' `HWMon` drivers do not have `device` paths, but the `base_path`
/// is the same as the `device` path of normal drivers (/hwmon/hwmon* not in it).
pub fn get_static_device_path_str(base_path: &Path) -> Option<String> {
    get_canonical_path_str(&device_path(base_path)).or_else(|| {
        // for Virtual HWMon drivers with no `device` path:
        get_canonical_path_str(base_path)
            .filter(|path| path.contains("devices") && path.contains("hwmon").not())
    })
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
pub fn device_path(base_path: &Path) -> PathBuf {
    // CentOS style path:
    if base_path.ends_with("device") {
        base_path.to_path_buf()
    } else {
        base_path.join("device")
    }
}

fn get_canonical_path_str(path: &Path) -> Option<String> {
    cc_fs::canonicalize(path)
        .inspect_err(|err| warn!("Error getting device path from {}, {err}", path.display()))
        .ok()
        .and_then(|path| path.to_str().map(ToOwned::to_owned))
}

/// Creates a unique identifier for a device.
/// The preferred order of identifiers is:
///
/// 1. device serial number
/// 2. SCSI wwid (`sd` disks only)
/// 3. realpath under /sys
/// 4. PCI ID
/// 5. device name
///
/// The purpose of this is to ensure that we have unique IDs for device settings that persist
/// across boots and hardware changes if possible.
///
/// The wwid sits below the serial so a device that already resolves by serial is not re-keyed
/// for no gain. Uniqueness is handled separately by `Device::assign_unique`.
pub async fn get_device_unique_id(base_path: &Path, device_name: &str) -> UID {
    if let Some(serial) = get_device_serial_number(base_path).await {
        return serial;
    }
    if let Some(wwid) = get_scsi_device_wwid(base_path).await {
        return wwid;
    }
    get_location_based_id(base_path, device_name).await
}

/// What `get_device_unique_id` returned before the `wwid` rung, so the migration can find what
/// an older release wrote. Identical to the current chain for any device with a serial.
pub async fn get_legacy_device_unique_id(base_path: &Path, device_name: &str) -> UID {
    if let Some(serial) = get_device_serial_number(base_path).await {
        return serial;
    }
    get_location_based_id(base_path, device_name).await
}

/// Location-derived tail shared by both chains so they cannot drift.
async fn get_location_based_id(base_path: &Path, device_name: &str) -> UID {
    if let Some(device_path) = get_static_device_path_str(base_path) {
        device_path
    } else if let Some(vendor_and_model_id) =
        get_device_uevent_details(base_path).await.get("PCI_ID")
    {
        vendor_and_model_id.to_owned()
    } else {
        device_name.to_owned()
    }
}

/// Returns the SCSI `wwid`, which only a `scsi_device` exposes, so this doubles as the test
/// for whether the device is an `sd` disk. Behind a USB bridge it identifies the BRIDGE, which
/// is why `assign_unique` stays load bearing.
pub async fn get_scsi_device_wwid(base_path: &Path) -> Option<String> {
    let raw = cc_fs::read_sysfs(device_path(base_path).join("wwid"))
        .await
        .ok()?;
    normalize_sysfs_identifier(&raw)
}

const VPD_PAGE_UNIT_SERIAL: u8 = 0x80;
/// Peripheral type, page code, then a 2 byte length.
const VPD_HEADER_LEN: usize = 4;

/// The drive serial from VPD page 0x80, used only as a collision tiebreaker. Must NOT be folded
/// into `get_device_serial_number`: that feeds the legacy chain, so the migration would see no
/// change and skip these drives.
pub async fn get_scsi_vpd_serial(base_path: &Path) -> Option<String> {
    let page = cc_fs::read_bytes(device_path(base_path).join("vpd_pg80"))
        .await
        .ok()?;
    parse_vpd_unit_serial(&page)
}

/// Split out from the read so the byte handling is testable. The field is padded with spaces
/// on some drives and NULs on others, so both are trimmed.
fn parse_vpd_unit_serial(page: &[u8]) -> Option<String> {
    if page.len() < VPD_HEADER_LEN {
        return None;
    }
    if page[1] != VPD_PAGE_UNIT_SERIAL {
        return None;
    }
    let declared_len = usize::from(u16::from_be_bytes([page[2], page[3]]));
    let end = declared_len
        .checked_add(VPD_HEADER_LEN)
        .map_or(page.len(), |end| end.min(page.len()));
    let serial = std::str::from_utf8(page.get(VPD_HEADER_LEN..end)?).ok()?;
    let serial = serial.trim_matches(|c: char| c.is_whitespace() || c == '\0');
    if serial.is_empty() {
        return None;
    }
    Some(serial.to_owned())
}

/// The kernel escapes non-printables in `wwid`, so a trailing NUL run arrives as the literal
/// text `\0`. Interior padding is kept: it belongs to the T10 designator layout.
fn normalize_sysfs_identifier(raw: &str) -> Option<String> {
    let mut identifier = raw.trim();
    while let Some(stripped) = identifier.strip_suffix("\\0") {
        identifier = stripped.trim_end();
    }
    if identifier.is_empty() {
        return None;
    }
    Some(identifier.to_owned())
}

/// Returns the device serial number if found.
pub async fn get_device_serial_number(base_path: &Path) -> Option<String> {
    if let Ok(serial) = cc_fs::read_sysfs(device_path(base_path).join("serial")).await {
        Some(serial.trim().to_string())
    } else {
        // usb hid serial numbers are here:
        let device_details = get_device_uevent_details(base_path).await;
        device_details.get("HID_UNIQ").map(ToString::to_string)
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
        dev_name.clone()
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
    let info = pci_ids::lookup_device(
        parse_hex_str_to_u16(vendor_id)?,
        parse_hex_str_to_u16(model_id)?,
        parse_hex_str_to_u16(subsys_vendor_id)?,
        parse_hex_str_to_u16(subsys_model_id)?,
    )
    .inspect_err(|err| {
        info!("Could not read PCI ID database: {err}, device name information will be limited");
    })
    .ok()?;
    let pci_device_names = PciDeviceNames {
        vendor_name: info.vendor_name,
        device_name: info.device_name,
        subvendor_name: info.subvendor_name,
        subdevice_name: info.subdevice_name,
    };
    debug!("Found PCI Device Names: {pci_device_names:?}");
    Some(pci_device_names)
}

fn parse_hex_str_to_u16(value: &str) -> Option<u16> {
    u16::from_str_radix(value, 16).ok()
}

/// The instance id of a platform device, i.e. `1` for `/sys/devices/platform/coretemp.1`.
///
/// Drivers that own one platform device per CPU package derive this id from the CPU topology
/// rather than from probe order, so unlike the `hwmonN` index it is stable across boots.
pub fn get_platform_device_id(base_path: &Path) -> Option<u8> {
    parse_platform_device_id(&get_static_device_path_str(base_path)?)
}

/// Only a direct child of the platform bus has an instance id we can read. A PCI address ends in
/// a function number that would otherwise parse as one, so the prefix check is load bearing.
fn parse_platform_device_id(device_path: &str) -> Option<u8> {
    let device_name = device_path.strip_prefix("/sys/devices/platform/")?;
    if device_name.contains('/') {
        return None;
    }
    let (_, instance_id) = device_name.rsplit_once('.')?;
    instance_id.parse().ok()
}

/// The AMD node whose northbridge or data fabric function this PCI device is: node N sits at PCI
/// slot `AMD_NODE0_PCI_SLOT + N`. This is the kernel's own derivation, `amd_pci_dev_to_node_id()`
/// in `k10temp.c`, and the slot is fixed by the hardware rather than by probe order.
pub fn get_amd_node_id(base_path: &Path) -> Option<u8> {
    parse_amd_node_id(&get_static_device_path_str(base_path)?)
}

/// A PCI address is `<domain>:<bus>:<slot>.<function>`, i.e. `0000:00:18.3` for node 0. Requiring
/// all three colon-separated fields keeps a platform device name from parsing as an address.
fn parse_amd_node_id(device_path: &str) -> Option<u8> {
    let device_name = device_path.rsplit('/').next()?;
    let (pci_address, _function) = device_name.rsplit_once('.')?;
    let mut address_fields = pci_address.split(':');
    let (_domain, _bus, slot) = (
        address_fields.next()?,
        address_fields.next()?,
        address_fields.next()?,
    );
    if address_fields.next().is_some() {
        return None;
    }
    let node_id = u8::from_str_radix(slot, 16)
        .ok()?
        .checked_sub(AMD_NODE0_PCI_SLOT)?;
    // A PCI slot is five bits, so 0x18..=0x1f is the whole range a node can occupy.
    (node_id < AMD_MAX_NODES).then_some(node_id)
}

pub async fn get_pci_slot_name(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("PCI_SLOT_NAME")
        .map(ToOwned::to_owned)
}

pub async fn get_device_driver_name(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("DRIVER")
        .map(ToOwned::to_owned)
}

pub async fn get_device_mod_alias(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("MODALIAS")
        .map(ToOwned::to_owned)
}

pub async fn get_device_hid_phys(base_path: &Path) -> Option<String> {
    get_device_uevent_details(base_path)
        .await
        .get("HID_PHYS")
        .map(ToOwned::to_owned)
}

async fn get_device_uevent_details(base_path: &Path) -> HashMap<String, String> {
    let cached = UEVENT_CACHE.with(|cache| cache.borrow().get(base_path).cloned());
    if let Some(details) = cached {
        return details;
    }
    let mut device_details = HashMap::new();
    let mut uevent_content = cc_fs::read_txt(device_path(base_path).join("uevent")).await;
    if uevent_content.is_err() {
        // If the `device_path` doesn't exist, try to read it from the `base_path`
        uevent_content = cc_fs::read_txt(base_path.join("uevent")).await;
    }
    if let Ok(content) = uevent_content {
        for line in content.lines() {
            if let Some((k, v)) = line.split_once('=') {
                let key = k.trim().to_string();
                let value = v.trim().to_string();
                device_details.insert(key, value);
            }
        }
    }
    UEVENT_CACHE.with(|cache| {
        cache
            .borrow_mut()
            .insert(base_path.to_path_buf(), device_details.clone());
    });
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
//     let content = cc_fs::read_txt(
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

#[allow(clippy::struct_field_names)]
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
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    use super::*;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    /// Goal: the k10temp node id must come from the PCI slot, since that is what the kernel
    /// itself derives it from and it does not move with probe order. Method: the real device
    /// paths from a two-socket EPYC report, plus the shapes that must be rejected.
    #[test]
    fn amd_node_id_is_read_from_the_pci_slot() {
        // The two k10temp devices of a dual-socket EPYC, same domain and bus, slots 0x18 / 0x19.
        assert_eq!(
            parse_amd_node_id("/sys/devices/pci0000:00/0000:00:18.3"),
            Some(0)
        );
        assert_eq!(
            parse_amd_node_id("/sys/devices/pci0000:00/0000:00:19.3"),
            Some(1)
        );
        // A node's slot is what counts, not the domain it is reached through.
        assert_eq!(
            parse_amd_node_id("/sys/devices/pci0000:60/0000:60:1a.3"),
            Some(2)
        );
        // A platform device name must not parse as a PCI address.
        assert_eq!(parse_amd_node_id("/sys/devices/platform/coretemp.0"), None);
        assert_eq!(
            parse_amd_node_id("/sys/devices/platform/applesmc.768"),
            None
        );
        // Devices below node 0's slot are not nodes. An NVMe drive from the same report:
        assert_eq!(
            parse_amd_node_id("/sys/devices/pci0000:40/0000:40:01.2/0000:42:00.0"),
            None
        );
        // An address must have exactly the three fields a PCI address has, so that no other
        // colon-separated name can present its tail as a slot.
        assert_eq!(parse_amd_node_id("/sys/devices/weird/a:b:0000:18.3"), None);
        // Nor is anything past the five bits a PCI slot has to spend.
        assert_eq!(
            parse_amd_node_id("/sys/devices/pci0000:00/0000:00:20.3"),
            None
        );
        assert_eq!(
            parse_amd_node_id("/sys/devices/pci0000:00/0000:00:ff.3"),
            None
        );
        assert_eq!(parse_amd_node_id(""), None);
    }

    /// Goal: a coretemp zone number must be read from the platform device path, since that is
    /// the only stable tie between an hwmon device and a CPU package. Method: the real path
    /// shapes seen on multi-socket and single-socket machines, plus the shapes that must be
    /// rejected so a PCI function number is never mistaken for an instance id.
    #[test]
    fn platform_device_id_is_parsed_only_for_platform_devices() {
        assert_eq!(
            parse_platform_device_id("/sys/devices/platform/coretemp.0"),
            Some(0)
        );
        assert_eq!(
            parse_platform_device_id("/sys/devices/platform/coretemp.1"),
            Some(1)
        );
        // A PCI address ends in a function number that would parse as an instance id.
        assert_eq!(
            parse_platform_device_id("/sys/devices/pci0000:00/0000:00:18.3"),
            None
        );
        // Ids wider than a zone number belong to ISA addresses, not package instances.
        assert_eq!(
            parse_platform_device_id("/sys/devices/platform/applesmc.768"),
            None
        );
        assert_eq!(
            parse_platform_device_id("/sys/devices/platform/nct6687.2592"),
            None
        );
        // Nested platform devices are not package instances.
        assert_eq!(
            parse_platform_device_id("/sys/devices/platform/foo.0/bar.1"),
            None
        );
        // No instance suffix at all.
        assert_eq!(
            parse_platform_device_id("/sys/devices/platform/coretemp"),
            None
        );
        assert_eq!(parse_platform_device_id(""), None);
    }

    struct HwmonDeviceContext {
        test_dir: String,
        hwmon_path: PathBuf,
        hwmon_path_centos: PathBuf,
        glob_paths: GlobPaths,
    }

    async fn setup() -> HwmonDeviceContext {
        let test_dir = TEST_BASE_PATH_STR.to_string() + Uuid::new_v4().to_string().as_str();
        let base_path_str = test_dir.clone() + "/hwmon/hwmon1/";
        let base_path_centos_str = test_dir.clone() + "/hwmon/hwmon2/device/";
        let hwmon_path = Path::new(&base_path_str).to_path_buf();
        let hwmon_path_centos = Path::new(&base_path_centos_str).to_path_buf();
        cc_fs::create_dir_all(&hwmon_path).await.unwrap();
        cc_fs::create_dir_all(&hwmon_path_centos).await.unwrap();
        let glob_pwm = hwmon_path
            .to_str()
            .unwrap()
            .to_owned()
            .replace("hwmon1", "hwmon*")
            + "pwm*";
        let glob_temp = hwmon_path
            .to_str()
            .unwrap()
            .to_owned()
            .replace("hwmon1", "hwmon*")
            + "temp*_input";
        let glob_fan = hwmon_path
            .to_str()
            .unwrap()
            .to_owned()
            .replace("hwmon1", "hwmon*")
            + "fan*_input";
        let glob_pwm_centos = hwmon_path_centos
            .to_str()
            .unwrap()
            .to_owned()
            .replace("hwmon2", "hwmon*")
            + "pwm*";
        let glob_temp_centos = hwmon_path_centos
            .to_str()
            .unwrap()
            .to_owned()
            .replace("hwmon2", "hwmon*")
            + "temp*_input";
        let glob_fan_centos = hwmon_path_centos
            .to_str()
            .unwrap()
            .to_owned()
            .replace("hwmon2", "hwmon*")
            + "fan*_input";
        HwmonDeviceContext {
            test_dir,
            hwmon_path,
            hwmon_path_centos,
            glob_paths: GlobPaths {
                pwm: glob_pwm,
                pwm_centos: glob_pwm_centos,
                temp: glob_temp,
                temp_centos: glob_temp_centos,
                fan: glob_fan,
                fan_centos: glob_fan_centos,
            },
        }
    }

    async fn teardown(ctx: &HwmonDeviceContext) {
        cc_fs::remove_dir_all(&ctx.test_dir).await.unwrap();
    }

    #[test]
    #[serial]
    fn find_device_empty() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(hwmon_paths.is_empty());
        });
    }

    #[test]
    #[serial]
    fn find_pwm_device() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn find_pwm_device_centos() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path_centos.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn find_temp_device() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                &ctx.hwmon_path.join("temp1_input"),
                b"70000".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn find_temp_device_centos() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path_centos.join("temp1_input"),
                b"70000".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn find_fan_device() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                &ctx.hwmon_path.join("fan1_input"),
                b"1200".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn find_fan_device_centos() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path_centos.join("fan1_input"),
                b"1200".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn find_pwm_centos_and_temp_device() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path_centos.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();
            cc_fs::write(
                ctx.hwmon_path.join("temp1_input"),
                b"70000".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 2);
        });
    }

    #[test]
    #[serial]
    fn find_fan_centos_and_temp_device() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path_centos.join("fan1_input"),
                b"1200".to_vec(), // duty
            )
            .await
            .unwrap();
            cc_fs::write(
                ctx.hwmon_path.join("temp1_input"),
                b"70000".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 2);
        });
    }

    #[test]
    #[serial]
    fn find_pwm_and_temp_centos_device() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();
            cc_fs::write(
                ctx.hwmon_path_centos.join("temp1_input"),
                b"70000".to_vec(), // temp
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 2);
        });
    }

    #[test]
    #[serial]
    fn find_pwm_device_norm_and_centos() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(
                ctx.hwmon_path.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();

            cc_fs::write(
                ctx.hwmon_path_centos.join("pwm1"),
                b"127".to_vec(), // duty
            )
            .await
            .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 2);
        });
    }

    #[test]
    #[serial]
    fn find_temp_device_norm_and_centos() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::write(ctx.hwmon_path.join("temp1_input"), b"70000".to_vec())
                .await
                .unwrap();

            cc_fs::write(ctx.hwmon_path_centos.join("temp1_input"), b"70000".to_vec())
                .await
                .unwrap();

            // when:
            let hwmon_paths = find_all_hwmon_device_paths_inner(&ctx.glob_paths);

            // then:
            teardown(&ctx).await;
            assert!(!hwmon_paths.is_empty());
            assert_eq!(hwmon_paths.len(), 2);
        });
    }

    // Goal: a SCSI disk's identity comes from `wwid`, so the UID no longer
    // depends on the probe-order sysfs path. Method: write a realistic
    // `naa.` wwid and assert it is returned verbatim after normalization.
    #[test]
    #[serial]
    fn scsi_wwid_is_preferred_identifier() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(device_dir.join("wwid"), b"naa.5000c500a1b2c3d4\n".to_vec())
                .await
                .unwrap();

            let wwid = get_scsi_device_wwid(&ctx.hwmon_path).await;

            teardown(&ctx).await;
            assert_eq!(wwid, Some("naa.5000c500a1b2c3d4".to_string()));
        });
    }

    // Goal: the kernel escapes a trailing NUL run in the VPD page as the
    // literal text `\0`, and pads with spaces. Both must be stripped so the
    // UID does not move if that escaping changes. Real sample taken from a
    // JMicron USB SATA bridge.
    #[test]
    #[serial]
    fn scsi_wwid_strips_escaped_nul_padding() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(
                device_dir.join("wwid"),
                b"t10.JMicron Tech            DD5641988389C\\0\\0\\0\n".to_vec(),
            )
            .await
            .unwrap();

            let wwid = get_scsi_device_wwid(&ctx.hwmon_path).await;

            teardown(&ctx).await;
            assert_eq!(
                wwid,
                Some("t10.JMicron Tech            DD5641988389C".to_string())
            );
        });
    }

    // Goal: a wwid that normalizes to nothing must not be treated as an
    // identity, otherwise every such device hashes to the same UID. Negative
    // space for `scsi_wwid_is_preferred_identifier`.
    #[test]
    #[serial]
    fn scsi_wwid_blank_is_not_an_identifier() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(device_dir.join("wwid"), b"  \\0\\0 \n".to_vec())
                .await
                .unwrap();

            let wwid = get_scsi_device_wwid(&ctx.hwmon_path).await;

            teardown(&ctx).await;
            assert_eq!(wwid, None);
        });
    }

    // Goal: devices with no `wwid` (everything that is not an sd disk) keep
    // their existing identity chain untouched, so no other device class
    // re-keys. Method: no wwid file at all.
    #[test]
    #[serial]
    fn non_scsi_device_has_no_wwid() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();

            let wwid = get_scsi_device_wwid(&ctx.hwmon_path).await;

            teardown(&ctx).await;
            assert_eq!(wwid, None);
        });
    }

    // Goal: the whole point of the change. Two boots that renumber the SCSI
    // host must yield the same UID. Method: derive the id from two different
    // base paths carrying the same wwid, and assert equality; then assert a
    // different wwid still separates them (negative space).
    #[test]
    #[serial]
    fn scsi_uid_is_stable_across_host_renumbering() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let boot_one = ctx.hwmon_path.join("host6");
            let boot_two = ctx.hwmon_path.join("host7");
            let other = ctx.hwmon_path.join("host8");
            for (dir, wwid) in [
                (&boot_one, "naa.5000c500a1b2c3d4"),
                (&boot_two, "naa.5000c500a1b2c3d4"),
                (&other, "naa.5000c500dddddddd"),
            ] {
                let device_dir = dir.join("device");
                cc_fs::create_dir_all(&device_dir).await.unwrap();
                cc_fs::write(device_dir.join("wwid"), format!("{wwid}\n").into_bytes())
                    .await
                    .unwrap();
            }

            let id_one = get_device_unique_id(&boot_one, "ST6000VN0033-2EE").await;
            let id_two = get_device_unique_id(&boot_two, "ST6000VN0033-2EE").await;
            let id_other = get_device_unique_id(&other, "ST6000VN0033-2EE").await;

            teardown(&ctx).await;
            assert_eq!(id_one, id_two);
            assert_ne!(id_one, id_other);
        });
    }

    // Goal: the real page 0x80 layout from a Seagate ST2000LX001, whose `serial` attribute is
    // absent while the page holds the serial. Left-padded with spaces. This is the sample that
    // motivated the whole fallback, so it is pinned byte for byte.
    #[test]
    fn vpd_unit_serial_parses_a_real_space_padded_page() {
        let mut page = vec![0x00, 0x80, 0x00, 0x14];
        page.extend_from_slice(b"            WCC34KL1");

        assert_eq!(parse_vpd_unit_serial(&page), Some("WCC34KL1".to_string()));
    }

    // Goal: the other padding style seen in the wild. A JMicron USB bridge NUL-pads instead,
    // and those are real NUL bytes here, unlike the escaped `\0` text the wwid attribute shows.
    #[test]
    fn vpd_unit_serial_trims_nul_padding() {
        let mut page = vec![0x00, 0x80, 0x00, 0x10];
        page.extend_from_slice(b"DD5641988389C\0\0\0");

        assert_eq!(
            parse_vpd_unit_serial(&page),
            Some("DD5641988389C".to_string())
        );
    }

    // Goal: negative space. Anything that is not a well-formed page 0x80 must yield None so the
    // rung stays blank and `assign_unique` moves on, rather than inventing a serial from
    // unrelated bytes. Also proves no panic on short or truncated input.
    #[test]
    fn vpd_unit_serial_rejects_malformed_pages() {
        // Wrong page code (0x83 is Device Identification, not Unit Serial).
        assert_eq!(parse_vpd_unit_serial(&[0x00, 0x83, 0x00, 0x08, b'x']), None);
        // Shorter than the header.
        assert_eq!(parse_vpd_unit_serial(&[0x00, 0x80]), None);
        assert_eq!(parse_vpd_unit_serial(&[]), None);
        // Declares more payload than is present: clamp, do not panic.
        let mut truncated = vec![0x00, 0x80, 0xFF, 0xFF];
        truncated.extend_from_slice(b"AB");
        assert_eq!(parse_vpd_unit_serial(&truncated), Some("AB".to_string()));
        // All padding, so nothing identifying is left.
        let mut blank = vec![0x00, 0x80, 0x00, 0x04];
        blank.extend_from_slice(b"    ");
        assert_eq!(parse_vpd_unit_serial(&blank), None);
    }

    // Goal: the fallback must not disturb identity. A drive with no `serial` attribute but a
    // readable page 0x80 still resolves to its wwid, and its legacy id is still the sysfs path,
    // so the migration continues to fire. Guards the exact regression this design avoids:
    // folding page 0x80 into the serial tier would silently skip the migration.
    #[test]
    #[serial]
    fn vpd_serial_does_not_affect_either_identity_chain() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(device_dir.join("wwid"), b"naa.5000c500a9068285\n".to_vec())
                .await
                .unwrap();
            let mut page = vec![0x00, 0x80, 0x00, 0x14];
            page.extend_from_slice(b"            WCC34KL1");
            cc_fs::write(device_dir.join("vpd_pg80"), page)
                .await
                .unwrap();

            let vpd_serial = get_scsi_vpd_serial(&ctx.hwmon_path).await;
            let current = get_device_unique_id(&ctx.hwmon_path, "drive").await;
            let legacy = get_legacy_device_unique_id(&ctx.hwmon_path, "drive").await;

            teardown(&ctx).await;
            assert_eq!(vpd_serial, Some("WCC34KL1".to_string()));
            assert_eq!(current, "naa.5000c500a9068285".to_string());
            assert!(
                legacy.contains("hwmon"),
                "legacy must still be the sysfs path"
            );
            assert_ne!(current, legacy, "the migration must still fire");
        });
    }

    // Goal: a device that already resolves by serial has a stable UID and no bug, so the
    // wwid must NOT displace it. This is what keeps the migration from re-keying users who
    // were never affected. Method: supply both, assert the serial wins.
    #[test]
    #[serial]
    fn serial_outranks_scsi_wwid() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(device_dir.join("wwid"), b"naa.5000c500a1b2c3d4\n".to_vec())
                .await
                .unwrap();
            cc_fs::write(device_dir.join("serial"), b"WD-WCC4N0803777\n".to_vec())
                .await
                .unwrap();

            let unique_id = get_device_unique_id(&ctx.hwmon_path, "ST6000VN0033-2EE").await;

            teardown(&ctx).await;
            assert_eq!(unique_id, "WD-WCC4N0803777".to_string());
        });
    }

    // Goal: the migration must be a no-op for the unaffected cohort. A device with a serial
    // resolves identically under the old and new chains, so no settings are ever moved for
    // it. Guards the ordering above against a future edit that silently re-keys everyone.
    #[test]
    #[serial]
    fn device_with_a_serial_is_never_migrated() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(device_dir.join("wwid"), b"naa.5000c500a1b2c3d4\n".to_vec())
                .await
                .unwrap();
            cc_fs::write(device_dir.join("serial"), b"WD-WCC4N0803777\n".to_vec())
                .await
                .unwrap();

            let current = get_device_unique_id(&ctx.hwmon_path, "drive").await;
            let legacy = get_legacy_device_unique_id(&ctx.hwmon_path, "drive").await;

            teardown(&ctx).await;
            assert_eq!(current, legacy, "no serial-bearing device may be re-keyed");
        });
    }

    // Goal: negative space for the above. The affected cohort, a drive with no serial, DOES
    // change identifier, since that is the whole fix. Without this the previous test could
    // be satisfied by a chain that never consults the wwid at all.
    #[test]
    #[serial]
    fn device_without_a_serial_moves_onto_its_wwid() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let device_dir = ctx.hwmon_path.join("device");
            cc_fs::create_dir_all(&device_dir).await.unwrap();
            cc_fs::write(device_dir.join("wwid"), b"naa.5000c500a1b2c3d4\n".to_vec())
                .await
                .unwrap();

            let current = get_device_unique_id(&ctx.hwmon_path, "drive").await;
            let legacy = get_legacy_device_unique_id(&ctx.hwmon_path, "drive").await;

            teardown(&ctx).await;
            assert_eq!(current, "naa.5000c500a1b2c3d4".to_string());
            assert_ne!(current, legacy, "the affected cohort must move");
            assert!(legacy.contains("hwmon"), "legacy resolved by sysfs path");
        });
    }
}
