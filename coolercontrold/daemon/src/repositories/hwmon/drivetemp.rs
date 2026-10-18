// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
use log::{debug, info, trace, warn};
use nix::libc;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::cmp::PartialEq;
use std::ops::Not;
use std::os::fd::{AsRawFd, RawFd};
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::rc::Rc;
use std::time::Duration;

static DEFAULT_TEMP_WHEN_DRIVE_IS_SUSPENDED: f64 = 0.;

/// State for `drivetemp` hwmon entries: resolved block device path
/// and a runtime-only dedup flag for the "ioctl unsupported" log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrivetempState {
    pub block_dev_path: Option<PathBuf>,
    #[serde(skip)]
    pub ioctl_unsupported_logged: Cell<bool>,
}

impl Default for DrivetempState {
    fn default() -> Self {
        Self {
            block_dev_path: None,
            ioctl_unsupported_logged: Cell::new(false),
        }
    }
}

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

/// Result of the bounded `drive_power_state` call, kept structured so
/// callers can discriminate the three failure modes without
/// string-matching the error message.
#[derive(Debug)]
enum BlockingTimeoutError {
    /// The ioctl exceeded its wall-clock budget; the blocking thread
    /// is leaked until the kernel returns, but the caller is freed.
    TimedOut(Duration),
    /// The Tokio blocking task itself failed to join (typically a
    /// panic inside the closure).
    Join(crate::rt::JoinError),
    /// The closure ran to completion but returned `Err`, e.g. the
    /// device file could not be opened or both ATA power-state
    /// commands returned non-zero.
    Inner(anyhow::Error),
}

impl std::fmt::Display for BlockingTimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TimedOut(elapsed) => {
                write!(f, "blocking task timed out after {elapsed:?}")
            }
            Self::Join(err) => write!(f, "blocking task join error: {err}"),
            Self::Inner(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for BlockingTimeoutError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::TimedOut(_) => None,
            Self::Join(err) => Some(err),
            Self::Inner(err) => Some(err.as_ref()),
        }
    }
}

/// If `drivetemp` state checks are enabled, checks the drive's power state.
///
/// The power-state check performs an `ioctl(HDIO_DRIVE_CMD, …)` which is
/// a synchronous kernel call that is **not** cancelled by `O_NONBLOCK`
/// and can block for seconds on a wedged ATA / SATA controller. This
/// wrapper bounds the blocking call with `timeout` so the caller never
/// stalls longer than its budget even if the drive is hung.
///
/// Fallback semantics on the error branches:
/// - `TimedOut`: assume suspended (`true`). The timeout itself is
///   evidence the drive can't answer right now; falling through to
///   the sysfs temp read would only let the kernel block us harder
///   during a spin-up / spin-down. Returns a 0 C placeholder for
///   this tick; the in-flight coalesce guard sees a fresh mark and
///   the failsafe wall stays clear.
/// - `Inner`: legacy behaviour (`false`); a drive answering neither
///   command may still have a working sysfs temp file. Logged once
///   per drive via `state.ioctl_unsupported_logged`.
/// - `Join`: rare panic in the blocking task. Fall through to sysfs
///   like `Inner`, logged at `warn!`.
pub async fn is_suspended(state: &DrivetempState, timeout: Duration) -> bool {
    debug_assert!(timeout > Duration::ZERO);
    let Some(block_device_path) = state.block_dev_path.as_ref() else {
        return false;
    };
    let start_time = std::time::Instant::now();
    let is_suspended = match drive_power_state(block_device_path, timeout).await {
        Ok(power_state) => power_state == PowerState::Standby,
        Err(BlockingTimeoutError::TimedOut(elapsed)) => {
            debug!(
                "Drive power-state ioctl exceeded {elapsed:?} for {}; \
                 assuming suspended for this tick",
                block_device_path.display()
            );
            true
        }
        Err(BlockingTimeoutError::Inner(err)) => {
            if state.ioctl_unsupported_logged.replace(true).not() {
                info!(
                    "Drive {} does not support any ATA power-state command \
                     ({err}); falling back to direct sysfs reads, which cannot \
                     tell a spun-down drive from an awake one",
                    block_device_path.display()
                );
            }
            false
        }
        Err(BlockingTimeoutError::Join(err)) => {
            warn!("Drive power-state blocking task panicked: {err}");
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

async fn drive_power_state(
    dev_path: &Path,
    timeout: Duration,
) -> std::result::Result<PowerState, BlockingTimeoutError> {
    // The ioctl is blocking and cannot be cancelled; offload to the
    // blocking pool and bound the await with a timeout. If the ioctl
    // hangs, the blocking thread is leaked until the kernel returns,
    // but the caller is released on time and the next poll tick can
    // proceed without the single-threaded runtime being stalled.
    let dev_path = dev_path.to_path_buf();
    run_blocking_with_timeout(timeout, move || {
        drive_power_state_blocking(&dev_path, timeout)
    })
    .await
}

/// Offloads a synchronous fallible closure to the Tokio blocking pool
/// and bounds the await with `timeout`. Extracted so the timeout path
/// is unit-testable without a real block device and so callers do not
/// duplicate the select/match boilerplate. Returns the typed
/// `BlockingTimeoutError` so callers can discriminate timeouts (an
/// expected event during HDD spin-up) from genuine errors.
async fn run_blocking_with_timeout<F, T>(
    timeout: Duration,
    blocking_fn: F,
) -> std::result::Result<T, BlockingTimeoutError>
where
    F: FnOnce() -> Result<T> + Send + 'static,
    T: Send + 'static,
{
    debug_assert!(timeout > Duration::ZERO);
    match crate::rt::timeout(timeout, crate::rt::spawn_blocking(blocking_fn)).await {
        Ok(Ok(Ok(value))) => Ok(value),
        Ok(Ok(Err(inner))) => Err(BlockingTimeoutError::Inner(inner)),
        Ok(Err(join_err)) => Err(BlockingTimeoutError::Join(join_err)),
        Err(_elapsed) => Err(BlockingTimeoutError::TimedOut(timeout)),
    }
}

/// `scsi/sg.h`. Issues a SCSI command on an open block device.
const IOCTL_SG_IO: libc::c_ulong = 0x2285;

/// SAT opcode wrapping an ATA command in a SCSI CDB, so it survives a transport with no
/// legacy HDIO ioctls.
const SCSI_ATA_PASS_THROUGH_16: u8 = 0x85;

/// CDB byte 1: ATA protocol 3 (Non-data) in bits 3:1.
const SAT_PROTOCOL_NON_DATA: u8 = 3 << 1;

/// CDB byte 2: returns the ATA output registers in sense data even on success. Without it the
/// power mode is unreadable.
const SAT_FLAG_CK_COND: u8 = 0x20;

/// Descriptor code for the ATA Status Return descriptor in descriptor-format sense.
const SENSE_DESC_ATA_RETURN: u8 = 0x09;

/// Declared length of a complete ATA Status Return descriptor, minus its 2 byte header. Less
/// than this means truncated output registers, so `+5` belongs to whatever follows.
const SENSE_DESC_ATA_RETURN_LEN: usize = 12;

/// ATA status `ERR` bit. Set means the output registers carry no power mode, so a zero count
/// must not read as `Standby`.
const ATA_STATUS_ERR: u8 = 0x01;

/// Descriptor-format sense response codes (current and deferred).
const SENSE_FORMAT_DESCRIPTOR_CURRENT: u8 = 0x72;
const SENSE_FORMAT_DESCRIPTOR_DEFERRED: u8 = 0x73;

/// Big enough for the 8 byte sense header plus the 14 byte ATA return descriptor.
const SENSE_BUFFER_LEN: usize = 32;
const _: () = assert!(SENSE_BUFFER_LEN >= 22);

/// `sg_io_hdr_t` from `scsi/sg.h`. Field order and types must match the header exactly.
#[repr(C)]
struct SgIoHdr {
    interface_id: libc::c_int,
    dxfer_direction: libc::c_int,
    cmd_len: libc::c_uchar,
    mx_sb_len: libc::c_uchar,
    iovec_count: libc::c_ushort,
    dxfer_len: libc::c_uint,
    dxferp: *mut libc::c_void,
    cmdp: *mut libc::c_uchar,
    sbp: *mut libc::c_uchar,
    timeout: libc::c_uint,
    flags: libc::c_uint,
    pack_id: libc::c_int,
    usr_ptr: *mut libc::c_void,
    status: libc::c_uchar,
    masked_status: libc::c_uchar,
    msg_status: libc::c_uchar,
    sb_len_wr: libc::c_uchar,
    host_status: libc::c_ushort,
    driver_status: libc::c_ushort,
    resid: libc::c_int,
    duration: libc::c_uint,
    info: libc::c_uint,
}

/// ATA-3 values, newer than what hdparm uses. Shared by the HDIO and SAT paths so the two
/// cannot disagree about a given byte.
fn power_state_from_ata_count(count: u8) -> PowerState {
    match count {
        0x00..=0x01 => PowerState::Standby,
        0x80..=0x83 => PowerState::Idle,
        0xFF => PowerState::ActiveIdle,
        _ => PowerState::Unknown,
    }
}

/// Split from the ioctl so the parsing is testable without a SAS HBA. `None` for sense that is
/// not descriptor format, lacks an ATA Status Return descriptor, is truncated, or reports an error.
fn ata_count_from_sense(sense: &[u8]) -> Option<u8> {
    let response_code = *sense.first()?;
    let is_current = response_code == SENSE_FORMAT_DESCRIPTOR_CURRENT;
    let is_deferred = response_code == SENSE_FORMAT_DESCRIPTOR_DEFERRED;
    if is_current.not() && is_deferred.not() {
        return None;
    }
    let additional_length = usize::from(*sense.get(7)?);
    let end = additional_length.saturating_add(8).min(sense.len());
    let mut offset = 8;
    // Each descriptor is a 2 byte header plus its declared length. The loop is bounded
    // by `end`, and a zero-length descriptor still advances by 2, so it terminates.
    while offset + 1 < end {
        let descriptor_code = sense[offset];
        let descriptor_length = usize::from(sense[offset + 1]);
        if descriptor_code == SENSE_DESC_ATA_RETURN {
            // [code, length, flags, error, count_ext, count, lba x6, device, status]
            if descriptor_length < SENSE_DESC_ATA_RETURN_LEN {
                return None;
            }
            let status = *sense.get(offset + 13)?;
            if status & ATA_STATUS_ERR != 0 {
                return None;
            }
            return sense.get(offset + 5).copied();
        }
        offset += descriptor_length + 2;
    }
    None
}

/// Returns `None` when the driver rejects it, the normal outcome for any drive not directly
/// attached via libata.
fn drive_power_state_via_hdio(fd: RawFd) -> Option<PowerState> {
    let mut query: [libc::c_uchar; 4] = [0; 4];
    // SAFETY: `fd` is an open block device owned by the caller and outlives this call.
    // `query` is a 4 byte buffer matching what HDIO_DRIVE_CMD reads and writes.
    // The try_into conversion is for musl, where the ioctl signature uses c_uint.
    #[allow(clippy::useless_conversion)]
    unsafe {
        let request = IOCTL_DRIVE_CMD.try_into().ok()?;
        query[0] = ATA_CHECKPOWERMODE;
        if libc::ioctl(fd, request, query.as_mut_ptr()) != 0 {
            query[0] = ATA_CHECKPOWERMODE_RETIRED;
            if libc::ioctl(fd, request, query.as_mut_ptr()) != 0 {
                return None;
            }
        }
    }
    Some(power_state_from_ata_count(query[2]))
}

/// Needed because `HDIO_DRIVE_CMD` is serviced only by libata, so drives behind a SAS HBA
/// reject it and the suspend check silently degrades to "always awake". Same mechanism as
/// `smartctl -d sat`.
fn drive_power_state_via_sat(fd: RawFd, timeout: Duration) -> Option<PowerState> {
    let mut cdb: [u8; 16] = [0; 16];
    cdb[0] = SCSI_ATA_PASS_THROUGH_16;
    cdb[1] = SAT_PROTOCOL_NON_DATA;
    cdb[2] = SAT_FLAG_CK_COND;
    cdb[14] = ATA_CHECKPOWERMODE;
    let mut sense: [u8; SENSE_BUFFER_LEN] = [0; SENSE_BUFFER_LEN];
    let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    let mut header = SgIoHdr {
        interface_id: i32::from(b'S'),
        dxfer_direction: -1, // SG_DXFER_NONE
        cmd_len: u8::try_from(cdb.len()).ok()?,
        mx_sb_len: u8::try_from(sense.len()).ok()?,
        iovec_count: 0,
        dxfer_len: 0,
        dxferp: std::ptr::null_mut(),
        cmdp: cdb.as_mut_ptr(),
        sbp: sense.as_mut_ptr(),
        timeout: timeout_ms,
        flags: 0,
        pack_id: 0,
        usr_ptr: std::ptr::null_mut(),
        status: 0,
        masked_status: 0,
        msg_status: 0,
        sb_len_wr: 0,
        host_status: 0,
        driver_status: 0,
        resid: 0,
        duration: 0,
        info: 0,
    };
    assert_eq!(usize::from(header.cmd_len), cdb.len());
    assert_eq!(usize::from(header.mx_sb_len), sense.len());
    // SAFETY: `fd` is an open block device owned by the caller. `header` points at
    // `cdb` and `sense`, both live for the whole call, and their lengths are recorded
    // in `cmd_len` / `mx_sb_len` so the kernel cannot overrun either.
    // The try_into conversion is for musl, where the ioctl signature uses c_uint.
    #[allow(clippy::useless_conversion)]
    let result = unsafe {
        let request = IOCTL_SG_IO.try_into().ok()?;
        libc::ioctl(fd, request, std::ptr::addr_of_mut!(header))
    };
    if result != 0 {
        return None;
    }
    // Never trust the device's own count as a bound.
    let written = usize::from(header.sb_len_wr).min(sense.len());
    ata_count_from_sense(&sense[..written]).map(power_state_from_ata_count)
}

/// Synchronous body of `drive_power_state`, run on the blocking pool. Prefers the legacy
/// `HDIO_DRIVE_CMD` ioctl and falls back to SAT for transports without it.
fn drive_power_state_blocking(dev_path: &Path, timeout: Duration) -> Result<PowerState> {
    use std::os::unix::fs::OpenOptionsExt;
    let block_dev_file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(dev_path)?;
    let fd = block_dev_file.as_raw_fd();
    if fd == -1 {
        return Err(anyhow!("Failed to open device"));
    }
    if let Some(power_state) = drive_power_state_via_hdio(fd) {
        return Ok(power_state);
    }
    if let Some(power_state) = drive_power_state_via_sat(fd, timeout) {
        return Ok(power_state);
    }
    Err(anyhow!(
        "the driver rejected both HDIO_DRIVE_CMD and SCSI ATA PASS-THROUGH"
    ))
}

/// Tests
#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::hwmon::hwmon_repo::HwmonChannelInfo;
    use serial_test::serial;

    struct HwmonDeviceContext {
        /// Owns the fixture directory, which is removed when the context drops.
        _dir: tempfile::TempDir,
        hwmon_path: PathBuf,
    }

    async fn setup() -> HwmonDeviceContext {
        let dir = tempfile::tempdir().unwrap();
        let hwmon_path = dir.path().join("hwmon/hwmon1");
        cc_fs::create_dir_all(&hwmon_path).await.unwrap();
        HwmonDeviceContext {
            _dir: dir,
            hwmon_path,
        }
    }

    #[test]
    #[serial]
    fn find_dev_path_empty() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // when:
            let dev_result = get_block_device_path(&ctx.hwmon_path);

            // then:
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
            drivetemp: DrivetempState::default(),
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
            // given:
            let local_dev_path = PathBuf::from("/dev/sda");

            // when:
            let dev_result = drive_power_state(&local_dev_path, Duration::from_secs(1)).await;

            // then:
            assert!(
                dev_result.is_ok(),
                "Error retrieving device power state: {dev_result:?}"
            );
            let power_state = dev_result.unwrap();
            assert_eq!(power_state, PowerState::Unknown,);
        });
    }

    // Goal: both power-state paths decode the ATA count register identically, so a drive
    // behind an HBA cannot be classified differently from the same drive on a SATA port.
    // Covers every documented band plus the negative space between them.
    #[test]
    fn ata_count_maps_to_documented_power_states() {
        assert_eq!(power_state_from_ata_count(0x00), PowerState::Standby);
        assert_eq!(power_state_from_ata_count(0x01), PowerState::Standby);
        assert_eq!(power_state_from_ata_count(0x80), PowerState::Idle);
        assert_eq!(power_state_from_ata_count(0x83), PowerState::Idle);
        assert_eq!(power_state_from_ata_count(0xFF), PowerState::ActiveIdle);
        // Between the bands: not a value the standard assigns.
        assert_eq!(power_state_from_ata_count(0x02), PowerState::Unknown);
        assert_eq!(power_state_from_ata_count(0x7F), PowerState::Unknown);
        assert_eq!(power_state_from_ata_count(0x84), PowerState::Unknown);
    }

    /// Descriptor-format sense carrying an ATA Status Return descriptor whose count
    /// register holds `count`. Shaped as the kernel returns it for a CK_COND request.
    fn ata_sense_with_count(count: u8) -> Vec<u8> {
        let mut sense = vec![0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0E];
        sense.extend_from_slice(&[SENSE_DESC_ATA_RETURN, 0x0C, 0x00, 0x00, 0x00, count]);
        sense.extend_from_slice(&[0; 8]);
        sense
    }

    // Goal: the SAT path's only untestable-on-this-machine step is the ioctl itself, so
    // the sense parsing is verified directly. A standby drive must decode as Standby.
    #[test]
    fn ata_count_is_read_from_descriptor_sense() {
        assert_eq!(
            ata_count_from_sense(&ata_sense_with_count(0x00)),
            Some(0x00)
        );
        assert_eq!(
            ata_count_from_sense(&ata_sense_with_count(0xFF)),
            Some(0xFF)
        );
        let standby = ata_count_from_sense(&ata_sense_with_count(0x00)).unwrap();
        assert_eq!(power_state_from_ata_count(standby), PowerState::Standby);
    }

    // Goal: the ATA descriptor is not required to come first, so the walk must skip
    // preceding descriptors using their declared lengths rather than assuming offset 8.
    #[test]
    fn ata_count_is_found_after_another_descriptor() {
        let mut sense = vec![0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x16];
        // A 6 byte Information descriptor (code 0x00) ahead of the ATA one.
        sense.extend_from_slice(&[0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
        sense.extend_from_slice(&[SENSE_DESC_ATA_RETURN, 0x0C, 0x00, 0x00, 0x00, 0x83]);
        sense.extend_from_slice(&[0; 8]);

        assert_eq!(ata_count_from_sense(&sense), Some(0x83));
    }

    // Goal: negative space. Anything that is not descriptor-format sense carrying an ATA
    // descriptor must yield None so the caller falls through rather than inventing a
    // power state from unrelated bytes.
    #[test]
    fn ata_count_rejects_sense_without_an_ata_descriptor() {
        // Fixed-format sense (0x70) is not descriptor format.
        let mut fixed = vec![0x70];
        fixed.extend_from_slice(&[0; 20]);
        assert_eq!(ata_count_from_sense(&fixed), None);
        // Descriptor format, but only an Information descriptor.
        let mut other = vec![0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x06];
        other.extend_from_slice(&[0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);
        assert_eq!(ata_count_from_sense(&other), None);
        // Empty and truncated buffers must not panic.
        assert_eq!(ata_count_from_sense(&[]), None);
        assert_eq!(ata_count_from_sense(&[0x72, 0x00]), None);
        // Declares an ATA descriptor but is cut off before the count byte.
        let truncated = vec![0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x0E, 0x09, 0x0C];
        assert_eq!(ata_count_from_sense(&truncated), None);
    }

    // Goal: a short ATA descriptor means truncated registers, so +5 belongs to the next
    // descriptor. Decoding it would read 0x00 as Standby and suppress the temp read.
    #[test]
    fn ata_count_rejects_a_short_ata_descriptor() {
        // Declares length 0x06 rather than 0x0C, then a second descriptor behind it.
        let mut sense = vec![0x72, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10];
        sense.extend_from_slice(&[SENSE_DESC_ATA_RETURN, 0x06, 0x00, 0x00, 0x00, 0x00]);
        sense.extend_from_slice(&[0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00]);

        assert_eq!(ata_count_from_sense(&sense), None);
    }

    // Goal: ERR set means no power mode in the registers. A zero count must not decode as
    // Standby, which would report 0 C for a drive that is awake and heating.
    #[test]
    fn ata_count_rejects_a_descriptor_reporting_an_ata_error() {
        let mut errored = ata_sense_with_count(0x00);
        errored[8 + 3] = 0x04; // error register: ABRT
        errored[8 + 13] = ATA_STATUS_ERR; // status register: ERR

        assert_eq!(ata_count_from_sense(&errored), None);

        // Negative space: the same descriptor without ERR still decodes.
        let mut ok = ata_sense_with_count(0x00);
        ok[8 + 13] = 0x50; // DRDY | DSC, no ERR
        assert_eq!(ata_count_from_sense(&ok), Some(0x00));
    }

    // --- run_blocking_with_timeout: timeout & success semantics ---
    //
    // The helper is what bounds the blocking ioctl so the
    // single-threaded runtime isn't stalled on a wedged drive.
    // Verify that a fast closure returns its value, that a slow
    // closure produces a typed `TimedOut` within a bounded wall
    // clock, and that an inner failure is surfaced as the typed
    // `Inner` variant so callers can discriminate without
    // string-matching.

    #[test]
    #[serial]
    #[cfg(feature = "gated-tests")]
    fn run_blocking_with_timeout_returns_value_when_fast() {
        // Verifies the happy path: a closure that returns promptly
        // yields its value unchanged.
        cc_fs::test_runtime(async {
            let result: std::result::Result<u32, BlockingTimeoutError> =
                run_blocking_with_timeout(Duration::from_secs(1), || Ok(42)).await;
            assert!(result.is_ok(), "expected Ok, got {result:?}");
            assert_eq!(result.unwrap(), 42);
        });
    }

    #[test]
    #[serial]
    #[cfg(feature = "gated-tests")]
    fn run_blocking_with_timeout_times_out_on_slow_closure() {
        // Verifies the failure path: a closure that sleeps longer
        // than the timeout produces the typed `TimedOut` variant
        // within a bounded wall clock, so the caller is neither
        // stalled on the blocking pool nor forced to string-match.
        cc_fs::test_runtime(async {
            let start = std::time::Instant::now();
            let result: std::result::Result<u32, BlockingTimeoutError> =
                run_blocking_with_timeout(Duration::from_millis(100), || {
                    std::thread::sleep(Duration::from_secs(5));
                    Ok(0)
                })
                .await;
            let elapsed = start.elapsed();
            assert!(
                matches!(result, Err(BlockingTimeoutError::TimedOut(_))),
                "expected TimedOut, got {result:?}"
            );
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
    #[cfg(feature = "gated-tests")]
    fn run_blocking_with_timeout_propagates_inner_error() {
        // Verifies that an Err returned by the blocking closure is
        // surfaced verbatim as the typed `Inner` variant and is not
        // masked by the timeout machinery or the `Join` variant.
        cc_fs::test_runtime(async {
            let result: std::result::Result<u32, BlockingTimeoutError> =
                run_blocking_with_timeout(Duration::from_secs(1), || Err(anyhow!("inner failure")))
                    .await;
            assert!(
                matches!(result, Err(BlockingTimeoutError::Inner(_))),
                "expected Inner, got {result:?}"
            );
            let err = result.unwrap_err().to_string();
            assert!(
                err.contains("inner failure"),
                "expected inner error to propagate, got {err}"
            );
        });
    }

    // --- is_suspended: fallback semantics ---
    //
    // Failsafes were tripping because a 400 ms ioctl timeout fell
    // through to a sysfs read that itself blocked during HDD
    // spin-up. With the typed-error split, `TimedOut` now returns
    // `true` (skip sysfs) and only `Inner` keeps the legacy "try
    // sysfs anyway" behaviour. The `Inner` log fires once per
    // drive so systems with many drives aren't spammed.

    #[test]
    #[serial]
    fn is_suspended_returns_false_when_block_path_is_none() {
        // Verifies that without a block device path, the suspension
        // check short-circuits to `false` (try sysfs normally).
        cc_fs::test_runtime(async {
            let state = DrivetempState::default();
            let suspended = is_suspended(&state, Duration::from_secs(1)).await;
            assert!(suspended.not());
            assert!(
                state.ioctl_unsupported_logged.get().not(),
                "no log should fire when path is absent"
            );
        });
    }

    #[test]
    #[serial]
    #[cfg(feature = "gated-tests")]
    fn is_suspended_returns_false_on_inner_error_and_logs_once_per_drive() {
        // Verifies the `Inner` fallback: when the drive doesn't
        // support the ioctl (here simulated by a path that cannot
        // be opened), `is_suspended` returns `false` so the caller
        // falls back to direct sysfs reads, and the dedup `Cell`
        // flips to `true` after the first call so subsequent calls
        // do not log again.
        cc_fs::test_runtime(async {
            let state = DrivetempState {
                block_dev_path: Some(PathBuf::from("/dev/__cc_test_nonexistent_block_device__")),
                ioctl_unsupported_logged: Cell::new(false),
            };

            let first = is_suspended(&state, Duration::from_secs(1)).await;
            assert!(first.not(), "first call: expected false on Inner");
            assert!(
                state.ioctl_unsupported_logged.get(),
                "first call should flip the dedup flag"
            );

            let second = is_suspended(&state, Duration::from_secs(1)).await;
            assert!(second.not(), "second call: expected false on Inner");
            assert!(
                state.ioctl_unsupported_logged.get(),
                "dedup flag must remain set across subsequent calls"
            );
        });
    }
}
