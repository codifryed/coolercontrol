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
#[cfg(test)]
use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};
use anyhow::{anyhow, Result};
use log::{trace, warn};
use nix::libc;
use std::cmp::PartialEq;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::rc::Rc;
use std::time::Duration;

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
            return Err(anyhow!(
                "Block device path does not exist: {}",
                path.display()
            ));
        }
        Ok(path)
    })
}

/// Streams the default suspended temp value for every temp channel
/// on the driver to `sink`. Used when the drive is in standby; the
/// real temp file is not read because reading wakes the drive.
/// Production now calls `default_suspended_temp_for` per channel
/// under the device permit (see `hwmon_repo::preload_device_statuses`),
/// so this whole-driver streamer is kept for tests only.
#[cfg(test)]
pub fn stream_default_suspended_temps<F>(driver: &Rc<HwmonDriverInfo>, mut sink: F)
where
    F: FnMut(TempStatus),
{
    for channel in &driver.channels {
        if channel.hwmon_type != HwmonChannelType::Temp {
            continue;
        }
        sink(default_suspended_temp_for(channel));
    }
}

/// Returns the default suspended temp value for a single channel.
/// Per-channel callers (e.g. preload's per-channel permit loop)
/// use this directly so they do not iterate the whole driver list.
pub fn default_suspended_temp_for(channel: &HwmonChannelInfo) -> TempStatus {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Temp);
    TempStatus {
        name: channel.name.clone(),
        temp: DEFAULT_TEMP_WHEN_DRIVE_IS_SUSPENDED,
    }
}

/// If `drivetemp` state checks are enabled, checks the drive's power state.
///
/// The power-state check performs an `ioctl(HDIO_DRIVE_CMD, …)` which is
/// a synchronous kernel call that is **not** cancelled by `O_NONBLOCK`
/// and can block for seconds on a wedged ATA / SATA controller. This
/// wrapper bounds the blocking call with `timeout` so the caller never
/// stalls longer than its budget even if the drive is hung.
pub async fn is_suspended(block_device_path_opt: Option<&PathBuf>, timeout: Duration) -> bool {
    debug_assert!(timeout > Duration::ZERO);
    let Some(block_device_path) = block_device_path_opt else {
        return false;
    };
    let start_time = std::time::Instant::now();
    let is_suspended = match drive_power_state(block_device_path, timeout).await {
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
            "No block device name found in {}",
            block_hwmon_path.display()
        ));
    };
    let block_device_path = PathBuf::from("/dev").join(device_name);
    Ok(block_device_path)
}

#[cfg(not(feature = "io_uring"))]
async fn drive_power_state(dev_path: &Path, timeout: Duration) -> Result<PowerState> {
    // The ioctl is blocking and cannot be cancelled; offload to the
    // blocking pool and bound the await with a timeout. If the ioctl
    // hangs, the blocking thread is leaked until the kernel returns,
    // but the caller is released on time and the next poll tick can
    // proceed without the single-threaded runtime being stalled.
    let dev_path = dev_path.to_path_buf();
    run_blocking_with_timeout(timeout, move || drive_power_state_blocking(&dev_path)).await
}

/// Offloads a synchronous fallible closure to the Tokio blocking pool
/// and bounds the await with `timeout`. Extracted so the timeout path
/// is unit-testable without a real block device and so callers do not
/// duplicate the select/match boilerplate.
#[cfg(not(feature = "io_uring"))]
async fn run_blocking_with_timeout<F, T>(timeout: Duration, blocking_fn: F) -> Result<T>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    debug_assert!(timeout > Duration::ZERO);
    let handle = tokio::task::spawn_blocking(blocking_fn);
    match tokio::time::timeout(timeout, handle).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_err)) => Err(anyhow!("blocking task join error: {join_err}")),
        Err(_elapsed) => Err(anyhow!("blocking task timed out after {timeout:?}")),
    }
}

/// Synchronous body of `drive_power_state`. Opens the block device and
/// issues the `HDIO_DRIVE_CMD` ioctl to read the ATA power state. Runs
/// on the Tokio blocking pool via `drive_power_state`.
#[cfg(not(feature = "io_uring"))]
fn drive_power_state_blocking(dev_path: &Path) -> Result<PowerState> {
    use std::os::unix::fs::OpenOptionsExt;
    let block_dev_file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(dev_path)?;
    let fd = block_dev_file.as_raw_fd();
    if fd == -1 {
        return Err(anyhow!("Failed to open device"));
    }
    let mut query: [libc::c_uchar; 4] = [0; 4];

    // low level kernel ioctl
    #[allow(clippy::useless_conversion)]
    unsafe {
        query[0] = ATA_CHECKPOWERMODE;
        // try_into conversion is for musl libc compatability, where the ioctl signature uses c_uint
        if libc::ioctl(fd, IOCTL_DRIVE_CMD.try_into()?, query.as_mut_ptr()) != 0 {
            // Try the retired command if the current one failed
            query[0] = ATA_CHECKPOWERMODE_RETIRED;
            if libc::ioctl(fd, IOCTL_DRIVE_CMD.try_into()?, query.as_mut_ptr()) != 0 {
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
async fn drive_power_state(_path: impl AsRef<Path>, _timeout: Duration) -> Result<PowerState> {
    Err(anyhow!("Not yet implemented"))
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::hwmon::hwmon_repo::HwmonChannelInfo;
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    struct HwmonDeviceContext {
        test_dir: String,
        hwmon_path: PathBuf,
    }

    async fn setup() -> HwmonDeviceContext {
        let test_dir = TEST_BASE_PATH_STR.to_string() + Uuid::new_v4().to_string().as_str();
        let base_path_str = test_dir.clone() + "/hwmon/hwmon1/";
        let hwmon_path = Path::new(&base_path_str).to_path_buf();
        cc_fs::create_dir_all(&hwmon_path).await.unwrap();
        HwmonDeviceContext {
            test_dir,
            hwmon_path,
        }
    }

    async fn teardown(ctx: &HwmonDeviceContext) {
        cc_fs::remove_dir_all(&ctx.test_dir).await.unwrap();
    }

    #[test]
    #[serial]
    fn find_dev_path_empty() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // when:
            let dev_result = get_block_device_path(&ctx.hwmon_path);

            // then:
            teardown(&ctx).await;
            assert!(
                dev_result.is_err(),
                "Block device path is not empty: {dev_result:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn find_dev_path() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            cc_fs::create_dir_all(ctx.hwmon_path.join("device").join("block").join("sda"))
                .await
                .unwrap();

            // when:
            let dev_result = get_block_device_path(&ctx.hwmon_path);

            // then:
            teardown(&ctx).await;
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

    // --- stream_default_suspended_temps: sink contract ---

    fn make_temp_driver(channels: Vec<HwmonChannelInfo>) -> Rc<HwmonDriverInfo> {
        Rc::new(HwmonDriverInfo {
            name: "test_driver".to_string(),
            channels,
            ..Default::default()
        })
    }

    fn temp_channel(number: u8, name: &str) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            number,
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    #[serial]
    fn stream_default_suspended_temps_invokes_sink_for_each_temp() {
        // Verifies the sink is called once per Temp channel, with the
        // suspended default temp, in channel-definition order.
        let driver = make_temp_driver(vec![
            temp_channel(1, "temp1"),
            temp_channel(2, "temp2"),
            temp_channel(3, "temp3"),
        ]);

        let mut received: Vec<(String, f64)> = Vec::new();
        stream_default_suspended_temps(&driver, |status| {
            received.push((status.name, status.temp));
        });

        assert_eq!(received.len(), 3);
        assert_eq!(received[0].0, "temp1");
        assert_eq!(received[1].0, "temp2");
        assert_eq!(received[2].0, "temp3");
        for (_, temp) in &received {
            assert!((*temp - DEFAULT_TEMP_WHEN_DRIVE_IS_SUSPENDED).abs() < f64::EPSILON);
        }
    }

    #[test]
    #[serial]
    fn stream_default_suspended_temps_skips_non_temp_channels() {
        // Verifies non-Temp channels don't invoke the sink, preserving
        // the invariant that only Temp entries are produced.
        let driver = make_temp_driver(vec![
            temp_channel(1, "temp1"),
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                name: "fan1".to_string(),
                ..Default::default()
            },
        ]);

        let mut received: Vec<String> = Vec::new();
        stream_default_suspended_temps(&driver, |status| received.push(status.name));

        assert_eq!(received, vec!["temp1"]);
    }

    #[test]
    #[serial]
    fn stream_default_suspended_temps_no_invocation_when_no_channels() {
        // Verifies the sink is never called for a driver with no temp
        // channels.
        let driver = make_temp_driver(vec![]);

        let mut invocations: u32 = 0;
        stream_default_suspended_temps(&driver, |_| invocations += 1);

        assert_eq!(invocations, 0);
    }

    #[test]
    #[serial]
    #[ignore = "requires a real block device & sudo privileges"]
    fn get_driver_power_state() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let local_dev_path = PathBuf::from("/dev/sda");

            // when:
            let dev_result = drive_power_state(&local_dev_path, Duration::from_secs(1)).await;

            // then:
            teardown(&ctx).await;
            assert!(
                dev_result.is_ok(),
                "Error retrieving device power state: {dev_result:?}"
            );
            let power_state = dev_result.unwrap();
            assert_eq!(power_state, PowerState::Unknown,);
        });
    }

    // --- run_blocking_with_timeout: timeout & success semantics ---
    //
    // The helper is what bounds the blocking ioctl so the
    // single-threaded runtime isn't stalled on a wedged drive.
    // Verify that a fast closure returns its value, and a slow
    // closure produces a timeout error well before it would have
    // completed on its own.

    #[test]
    #[serial]
    fn run_blocking_with_timeout_returns_value_when_fast() {
        // Verifies the happy path: a closure that returns promptly
        // yields its value unchanged.
        cc_fs::test_runtime(async {
            let result: Result<u32> =
                run_blocking_with_timeout(Duration::from_secs(1), || Ok(42)).await;
            assert!(result.is_ok(), "expected Ok, got {result:?}");
            assert_eq!(result.unwrap(), 42);
        });
    }

    #[test]
    #[serial]
    fn run_blocking_with_timeout_times_out_on_slow_closure() {
        // Verifies the failure path: a closure that sleeps longer
        // than the timeout produces an Err within a bounded wall
        // clock, so the caller is not stalled on the blocking pool.
        cc_fs::test_runtime(async {
            let start = std::time::Instant::now();
            let result: Result<u32> = run_blocking_with_timeout(Duration::from_millis(100), || {
                std::thread::sleep(Duration::from_secs(5));
                Ok(0)
            })
            .await;
            let elapsed = start.elapsed();
            assert!(result.is_err(), "expected timeout Err, got {result:?}");
            // Generous upper bound: we only require the caller to
            // have returned well before the 5s sleep would have
            // completed; avoid flakiness on slow CI.
            assert!(
                elapsed < Duration::from_secs(2),
                "caller was stalled: elapsed={elapsed:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn run_blocking_with_timeout_propagates_inner_error() {
        // Verifies that an Err returned by the blocking closure is
        // surfaced to the caller verbatim and is not masked by the
        // timeout machinery.
        cc_fs::test_runtime(async {
            let result: Result<u32> =
                run_blocking_with_timeout(Duration::from_secs(1), || Err(anyhow!("inner failure")))
                    .await;
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("inner failure"),
                "expected inner error to propagate, got {err}"
            );
        });
    }
}
