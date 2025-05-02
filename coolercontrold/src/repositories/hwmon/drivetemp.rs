/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2025  Guy Boldon, Eren Simsek and contributors
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

//! This module is for handling drive temperatures from the `drivetemp` kernel module which
//! handles temps for HDDs and SSDs. Those drives can enter a low power state (suspend).
//!
//! To be able to dynamically handle fan control for these devices, this module attempts
//! to determine the current power mode of the connected drives.

use crate::cc_fs;
use crate::device::TempStatus;
use crate::repositories::hwmon::devices;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelType, HwmonDriverInfo};
use anyhow::{anyhow, Result};
use log::{trace, warn};
use nix::libc;
use std::cmp::PartialEq;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::rc::Rc;

static DEFAULT_TEMP_WHEN_DRIVE_IS_SUSPENDED: f64 = 0.;

/// Ioctl for special hdd commands
/// `/usr/include/linux/hdreg.h`
/// `#define HDIO_DRIVE_CMD 0x031f /* execute a special drive command */`
const IOCTL_DRIVE_CMD: libc::c_ulong = 0x031F;

/// Standard ATA command to check the power state
/// `/usr/include/linux/hdreg.h`
/// `#define WIN_CHECKPOWERMODE1 0xE5`
const ATA_CHECKPOWERMODE: libc::c_uchar = 0xE5;

/// Legacy ATA command to check the power state
/// `/usr/include/linux/hdreg.h`
/// `#define WIN_CHECKPOWERMODE2 0x98`
const ATA_CHECKPOWERMODE_RETIRED: libc::c_uchar = 0x98;

/// The power state of an ata device
#[derive(Debug, PartialEq, Eq)]
enum PowerState {
    /// The hdd is in the standby state (PM2, usually spun down)
    Standby,
    /// The hdd is in the idle state (PM1)
    Idle,
    /// The hdd is in the active or idle state (PM0 or PM1)
    ActiveIdle,
    /// Many drives use non-volatile cache to increase spin-down time.
    /// Deprecated and not used since the ata-3 standard
    // NVCacheSpinDown,
    /// Deprecated and not used since the ata-3 standard
    // NVCacheSpinUp,
    /// The state of the hdd is unknown (invalid ATA response)
    Unknown,
}

pub fn get_verified_block_device_path(path: &Path) -> Result<PathBuf> {
    get_block_device_path(path).and_then(|path| {
        if !path.exists() {
            return Err(anyhow!("Block device path does not exist: {path:?}"));
        }
        Ok(path)
    })
}

/// Returns the default value for drivers that are suspended for all available temperature channels
pub fn default_suspended_temps(driver: &Rc<HwmonDriverInfo>) -> Vec<TempStatus> {
    let mut temps = vec![];
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        temps.push(TempStatus {
            name: channel.name.clone(),
            temp: DEFAULT_TEMP_WHEN_DRIVE_IS_SUSPENDED,
        });
    }
    temps
}

/// If `drivetemp` state checks are enabled, checks the drive's power state.
pub async fn is_suspended(block_device_path_opt: Option<&PathBuf>) -> bool {
    let Some(block_device_path) = block_device_path_opt else {
        return false;
    };
    let start_time = std::time::Instant::now();
    let is_suspended = match drive_power_state(block_device_path).await {
        Ok(state) => state == PowerState::Standby,
        Err(err) => {
            warn!("Error getting drive power state: {err}");
            false
        }
    };
    trace!(
        "Time taken to determine drive power state: {:?}",
        start_time.elapsed()
    );
    is_suspended
}

fn get_block_device_path(path: &Path) -> Result<PathBuf> {
    let block_hwmon_path = devices::device_path(path).join("block");
    let mut block_device_name = None;
    for entry_result in cc_fs::read_dir(&block_hwmon_path)? {
        let entry = entry_result?;
        // There is usually only a single bock directory with the name of the device
        if entry.file_type()?.is_dir() {
            block_device_name = entry.file_name().to_str().map(ToOwned::to_owned);
            break;
        }
    }
    let Some(device_name) = block_device_name else {
        return Err(anyhow!(
            "No block device name found in {block_hwmon_path:?}"
        ));
    };
    let block_device_path = PathBuf::from("/dev").join(device_name);
    Ok(block_device_path)
}

#[cfg(not(feature = "io_uring"))]
async fn drive_power_state(dev_path: &Path) -> Result<PowerState> {
    let block_dev_file = tokio::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(dev_path)
        .await?;
    let fd = block_dev_file.as_raw_fd();
    if fd == -1 {
        return Err(anyhow!("Failed to open device"));
    }
    let mut query: [libc::c_uchar; 4] = [0; 4];

    // low level kernel ioctl
    unsafe {
        query[0] = ATA_CHECKPOWERMODE;
        if libc::ioctl(fd, IOCTL_DRIVE_CMD, query.as_mut_ptr()) != 0 {
            // Try the retired command if the current one failed
            query[0] = ATA_CHECKPOWERMODE_RETIRED;
            if libc::ioctl(fd, IOCTL_DRIVE_CMD, query.as_mut_ptr()) != 0 {
                return Err(anyhow!("Not a Block Device File"));
            }
        }
    }
    // These are based on ATA-3 standards (newer than what hdparm uses)
    Ok(match query[2] {
        0x00..=0x01 => PowerState::Standby,
        0x80..=0x83 => PowerState::Idle,
        0xFF => PowerState::ActiveIdle,
        _ => PowerState::Unknown,
    })
}

#[cfg(feature = "io_uring")]
async fn drive_power_state(path: impl AsRef<Path>) -> Result<PowerState> {
    Err(anyhow!("Not yet implemented"))
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    struct HwmonDeviceContext {
        test_dir: String,
        hwmon_path: PathBuf,
    }

    fn setup() -> HwmonDeviceContext {
        let test_dir = TEST_BASE_PATH_STR.to_string() + Uuid::new_v4().to_string().as_str();
        let base_path_str = test_dir.clone() + "/hwmon/hwmon1/";
        let hwmon_path = Path::new(&base_path_str).to_path_buf();
        cc_fs::create_dir_all(&hwmon_path).unwrap();
        HwmonDeviceContext {
            test_dir,
            hwmon_path,
        }
    }

    fn teardown(ctx: &HwmonDeviceContext) {
        cc_fs::remove_dir_all(&ctx.test_dir).unwrap();
    }

    #[test]
    #[serial]
    fn find_dev_path_empty() {
        let ctx = setup();
        cc_fs::test_runtime(async {
            // when:
            let dev_result = get_block_device_path(&ctx.hwmon_path);

            // then:
            teardown(&ctx);
            assert!(
                dev_result.is_err(),
                "Block device path is not empty: {dev_result:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn find_dev_path() {
        let ctx = setup();
        cc_fs::test_runtime(async {
            // given:
            cc_fs::create_dir_all(ctx.hwmon_path.join("device").join("block").join("sda")).unwrap();

            // when:
            let dev_result = get_block_device_path(&ctx.hwmon_path);

            // then:
            teardown(&ctx);
            assert!(
                dev_result.is_ok(),
                "Block device path is empty: {dev_result:?}"
            );
            let dev_path = dev_result.unwrap();
            assert_eq!(
                dev_path.to_str(),
                Some("/dev/sda"),
                "Resulting device path doesn't match: {dev_path:?}"
            );
        });
    }

    #[test]
    #[serial]
    #[ignore] // requires a real block device & sudo privileges
    fn get_driver_power_state() {
        let ctx = setup();
        cc_fs::test_runtime(async {
            // given:
            let local_dev_path = PathBuf::from("/dev/sda");

            // when:
            let dev_result = drive_power_state(&local_dev_path).await;

            // then:
            teardown(&ctx);
            assert!(
                dev_result.is_ok(),
                "Error retrieving device power state: {dev_result:?}"
            );
            let power_state = dev_result.unwrap();
            assert_eq!(power_state, PowerState::Unknown,);
        });
    }
}
