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
use crate::cc_fs;
use crate::config::Config;
use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, DeviceUID,
    DriverInfo, DriverType, Duty, SpeedOptions, Status, Temp, TempInfo, TempName, TempStatus,
    TypeIndex, UID,
};
use crate::repositories::failsafe::{self, FailsafeStatusData, MISSING_STATUS_THRESHOLD};
use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
use crate::repositories::hwmon::devices::{DEVICE_NAMES_APPLE, HWMON_DEVICE_NAME_BLACKLIST};
use crate::repositories::hwmon::{auto_curve, devices, drivetemp, fans, power, temps, thinkpad};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::repositories::utils::apply_device_command_delay;
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use bitflags::bitflags;
use heck::ToTitleCase;
use log::{debug, error, info, log, trace, warn};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use strum::{Display, EnumString};
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::{sleep, Instant};

/// Fraction of `poll_rate` a device preload is allowed before the
/// slow-device arm fires. Anchored so that at the minimum poll rate
/// (0.5 s) the budget reproduces the original 350 ms value, and stays
/// strictly below `main_loop::SNAPSHOT_TIMEOUT_RATIO` (0.8) so the
/// repo's failsafe-apply branch runs before the snapshot is taken.
const READ_PERMIT_RATIO: f64 = 0.7;

/// Derives the read permit timeout from `poll_rate`. Pure helper so
/// the ratio is testable without constructing a full `HwmonRepo`.
fn device_read_permit_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * READ_PERMIT_RATIO)
}

/// Derives the write permit timeout from `poll_rate`. Pure helper so
/// the formula is testable without constructing a full `HwmonRepo`.
/// `MISSING_STATUS_THRESHOLD` is a small `usize` (8) that fits within
/// `u8::MAX`, so the cast to `f64` is lossless.
#[allow(clippy::cast_precision_loss)]
fn device_write_permit_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * MISSING_STATUS_THRESHOLD as f64)
}

/// Fraction of `poll_rate` allowed for the drivetemp ATA power-state
/// ioctl. Kept strictly below `READ_PERMIT_RATIO` so on timeout there
/// is still budget for the fallback temp read before the outer read
/// permit arm fires. Hardware-healthy ATA power-state checks complete
/// in milliseconds; any value >> that is a wedged controller.
const DRIVETEMP_IOCTL_RATIO: f64 = 0.4;

/// Derives the drivetemp ioctl timeout from `poll_rate`. Pure helper
/// so the ratio is testable without constructing a full `HwmonRepo`.
fn drivetemp_ioctl_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * DRIVETEMP_IOCTL_RATIO)
}

/// The `drivetemp` kernel module is non-standard and used for getting temps for HDDs. Part of its
/// implementation blocks temperature reads when the drive is spinning up which causes significant
/// read delays. Since this is pretty normal behavior for this driver, we handle it differently.
static DRIVETEMP: &str = "drivetemp";

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum HwmonChannelType {
    Fan,
    Temp,
    Load,
    Freq,
    Power,
    PowerCap, // RAPL
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HwmonChannelInfo {
    pub hwmon_type: HwmonChannelType,
    pub number: u8,
    pub pwm_enable_default: Option<u8>,
    pub name: String,
    pub label: Option<String>,
    pub auto_curve: AutoCurveInfo,
    pub caps: HwmonChannelCapabilities,
    // Paths that are often used are saved to avoid cloning
    pub pwm_path: Option<PathBuf>,
    pub rpm_path: Option<PathBuf>,
    pub temp_path: Option<PathBuf>,
}

impl Default for HwmonChannelInfo {
    fn default() -> Self {
        Self {
            hwmon_type: HwmonChannelType::Fan,
            number: 1,
            pwm_enable_default: None,
            name: String::new(),
            label: None,
            auto_curve: AutoCurveInfo::None,
            caps: HwmonChannelCapabilities::empty(),
            pwm_path: None,
            rpm_path: None,
            temp_path: None,
        }
    }
}

bitflags! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct HwmonChannelCapabilities: u32 {
        const FAN_WRITABLE = 1;
        const PWM = 1 << 1;
        const RPM = 1 << 2;
        const PWM_MODE = 1 << 3;
        // Specialities
        const APPLE_SMC = 1 << 15;
    }
}

impl HwmonChannelCapabilities {
    pub fn is_fan_controllable(&self) -> bool {
        self.contains(HwmonChannelCapabilities::FAN_WRITABLE)
    }

    pub fn has_pwm(&self) -> bool {
        self.contains(HwmonChannelCapabilities::PWM)
    }

    pub fn has_rpm(&self) -> bool {
        self.contains(HwmonChannelCapabilities::RPM)
    }

    pub fn has_pwm_mode(&self) -> bool {
        self.contains(HwmonChannelCapabilities::PWM_MODE)
    }

    pub fn is_apple_smc(&self) -> bool {
        self.contains(HwmonChannelCapabilities::APPLE_SMC)
    }

    pub fn is_non_controllable_rpm_fan(&self) -> bool {
        self.contains(HwmonChannelCapabilities::RPM)
            && !self.contains(HwmonChannelCapabilities::FAN_WRITABLE)
    }
}

/// Indicated support for hwmon auto curves (firmware profiles)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AutoCurveInfo {
    None,
    PWM { point_length: u8 },
    Temp { temp_lengths: HashMap<TempName, u8> },
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct HwmonDriverInfo {
    pub name: String,
    pub path: PathBuf,
    pub model: Option<String>,
    pub u_id: UID,
    pub channels: Vec<HwmonChannelInfo>,
    /// this is used specifically for the `drivetemp` module,
    /// which has an associated block device path if found.
    pub block_dev_path: Option<PathBuf>,
    pub apple_smc: AppleMacSMC,
}

/// A Repository for `HWMon` Devices
pub struct HwmonRepo {
    config: Rc<Config>,
    devices: HashMap<DeviceUID, (DeviceLock, Rc<HwmonDriverInfo>)>,
    preloaded_statuses: RefCell<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    failsafe_statuses: RefCell<HashMap<TypeIndex, FailsafeStatusData>>,

    /// Permits for each `HWMon` device. This is useful for slower devices.
    /// `liqctld` already has an in-built device queue - where only one read or write
    /// request can be sent to the device at a time. This is that same idea but for hwmon devices.
    /// This also ensures that polling loops don't overlap and stack if the device hasn't finished
    /// responding from the previous polling loop.
    ///
    /// Stored as `Rc<Semaphore>` so the Semaphore can be cloned into
    /// a detached `spawn_local` task that re-acquires it across the
    /// command delay after a preload, without blocking the current
    /// preload's completion. The single-threaded runtime means `Rc`
    /// is sufficient; `acquire()` borrows the Semaphore through the
    /// task's async state machine (self-reference is fine because
    /// the task owns both the Rc and the permit).
    device_permits: HashMap<TypeIndex, Rc<Semaphore>>,

    /// Used to avoid logging a device-delay warning more than once and not on startup
    delay_logged: HashMap<TypeIndex, Cell<u8>>,

    /// Liquidctl driver `HWMon` paths, to be used to filter out duplicate `HWMon` devices
    lc_hwmon_paths: Vec<PathBuf>,

    /// Cached per-device command delay in milliseconds. Loaded at startup from config.
    device_delays: HashMap<DeviceUID, u16>,

    /// Snapshot of the read-permit timeout. `poll_rate` only changes on
    /// daemon restart, so this value is constant for the repo's lifetime
    /// and is computed once in `new` to avoid per-poll f64 math and a
    /// `RefCell` borrow on the config hot path.
    device_read_permit_timeout: Duration,

    /// Snapshot of the write-permit timeout. Constant for the repo's
    /// lifetime; see `device_read_permit_timeout`.
    device_write_permit_timeout: Duration,

    /// Snapshot of the drivetemp ioctl timeout. Constant for the
    /// repo's lifetime; bounds the `HDIO_DRIVE_CMD` ioctl that runs
    /// on the blocking pool during each preload tick.
    drivetemp_ioctl_timeout: Duration,
}

impl HwmonRepo {
    pub fn new(config: Rc<Config>, lc_locations: Vec<String>) -> Self {
        // `poll_rate` is captured at daemon startup and cannot change
        // without a restart, so the derived permit timeouts are frozen
        // here for the repo's lifetime.
        let poll_rate = config.get_settings().map(|s| s.poll_rate).unwrap_or(1.0);
        let device_read_permit_timeout = device_read_permit_timeout_for(poll_rate);
        let device_write_permit_timeout = device_write_permit_timeout_for(poll_rate);
        let drivetemp_ioctl_timeout = drivetemp_ioctl_timeout_for(poll_rate);
        Self {
            config,
            devices: HashMap::new(),
            preloaded_statuses: RefCell::new(HashMap::new()),
            failsafe_statuses: RefCell::new(HashMap::new()),
            device_permits: HashMap::new(),
            delay_logged: HashMap::new(),
            lc_hwmon_paths: lc_locations
                .into_iter()
                .filter(|loc| loc.contains("hwmon/hwmon"))
                // blocking is fine during initialization:
                .filter_map(|loc| cc_fs::canonicalize(loc).ok())
                .collect(),
            device_delays: HashMap::new(),
            device_read_permit_timeout,
            device_write_permit_timeout,
            drivetemp_ioctl_timeout,
        }
    }

    fn load_device_delays(&mut self) {
        for uid in self.devices.keys() {
            let delay_millis = self
                .config
                .get_cc_settings_for_device(uid)
                .ok()
                .flatten()
                .map_or(0, |s| s.extensions.delay_millis);
            if delay_millis > 0 {
                self.device_delays.insert(uid.clone(), delay_millis);
            }
        }
    }

    fn device_delay(&self, device_uid: &UID) -> u16 {
        self.device_delays.get(device_uid).copied().unwrap_or(0)
    }

    /// Checks if the path matches a liquidctl device path.
    ///
    /// By default, `CoolerControl` will hide `HWMon` devices that are already detected
    /// by liquidctl. Liquidctl offers more features, like RGB & LCD control, that `HWMon`
    /// drivers don't.
    ///
    /// Liquidctl uses `HWMon` in their backend for many of their supported devices. This
    /// allows us to verify which one of the liquidctl devices have an exact path match to
    /// a `HWMon` device we've detected. The canonicalized path resolves the `HWMon` path
    /// to a very specific location in the system and device model, so false positives are
    /// near impossible.
    ///
    /// Additionally, liquidctl gives us a hidraw based `HWMon` path, and we use a `HWMon`
    /// class based path. Both of these paths are canonicalized to the same "real" path,
    /// negating any initial subsystem differences.
    fn path_matches_liquidctl_device(&self, base_path: &Path) -> bool {
        cc_fs::canonicalize(base_path).is_ok_and(|dev_path| self.lc_hwmon_paths.contains(&dev_path))
    }

    /// Maps driver infos to our Devices
    /// `ThinkPads` need special handling, see:
    /// [Kernel Docs](https://www.kernel.org/doc/html/latest/admin-guide/laptops/thinkpad-acpi.html#fan-control-and-monitoring-fan-speed-fan-enable-disable)
    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    async fn map_into_our_device_model(
        &mut self,
        hwmon_drivers: Vec<HwmonDriverInfo>,
    ) -> Result<()> {
        let poll_rate = self.config.get_settings()?.poll_rate;
        for (index, driver) in hwmon_drivers.into_iter().enumerate() {
            let temps = driver
                .channels
                .iter()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: channel.label.as_ref().map_or_else(
                                || channel.name.to_title_case(),
                                |l| l.to_title_case(),
                            ),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut profile_max_length = 21; // Default
            let mut channels = HashMap::new();
            let mut thinkpad_fan_control = (
                driver.name == devices::DEVICE_NAME_THINK_PAD
                // first check if this is a ThinkPad
            )
                .then_some(false);
            for channel in &driver.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Fan => {
                        if thinkpad_fan_control.is_some() && channel.number == 1 {
                            thinkpad_fan_control = Some(
                                // verify if fan control for this ThinkPad is enabled or not:
                                fans::set_pwm_enable(2, &driver.path, channel).await.is_ok(),
                            );
                        }
                        let extension = match &channel.auto_curve {
                            AutoCurveInfo::None => None,
                            AutoCurveInfo::PWM { point_length } => {
                                if point_length < &profile_max_length {
                                    profile_max_length = *point_length;
                                }
                                Some(ChannelExtensionNames::AutoHWCurve)
                            }
                            AutoCurveInfo::Temp { temp_lengths } => {
                                for point_length in temp_lengths.values() {
                                    if point_length < &profile_max_length {
                                        profile_max_length = *point_length;
                                    }
                                }
                                Some(ChannelExtensionNames::AutoHWCurve)
                            }
                        };
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            speed_options: Some(SpeedOptions {
                                fixed_enabled: channel
                                    .caps
                                    .contains(HwmonChannelCapabilities::FAN_WRITABLE),
                                extension,
                                ..Default::default()
                            }),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Power => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    _ => (), // other channel types are handled differently or don't have info
                }
            }
            let device_info = DeviceInfo {
                temps,
                channels,
                temp_min: 0,
                temp_max: 150,
                profile_max_length,
                model: driver.model.clone(),
                thinkpad_fan_control,
                driver_info: DriverInfo {
                    drv_type: DriverType::Kernel,
                    name: devices::get_device_driver_name(&driver.path).await,
                    version: sysinfo::System::kernel_version(),
                    locations: Self::get_driver_locations(&driver.path).await,
                },
                ..Default::default()
            };
            let type_index = (index + 1) as u8;
            let (mut channel_statuses, _) = fans::extract_fan_statuses(&driver).await;
            let (power_statuses, _) = power::extract_power_status(&driver).await;
            channel_statuses.extend(power_statuses);
            let (temp_statuses, _) = temps::extract_temp_statuses(&driver).await;
            let (channel_failsafes, temp_failsafes) =
                failsafe::create_failsafe_data(&channel_statuses, &temp_statuses);
            if let Some(fsd) = FailsafeStatusData::new(channel_failsafes, temp_failsafes) {
                self.failsafe_statuses.borrow_mut().insert(type_index, fsd);
            }
            self.preloaded_statuses.borrow_mut().insert(
                type_index,
                (channel_statuses.clone(), temp_statuses.clone()),
            );
            let mut device = Device::new(
                driver.name.clone(),
                DeviceType::Hwmon,
                type_index,
                None,
                device_info,
                Some(driver.u_id.clone()),
                poll_rate,
            );
            let status = Status {
                channels: channel_statuses,
                temps: temp_statuses,
                ..Default::default()
            };
            device.initialize_status_history_with(status, poll_rate);
            self.device_permits
                .insert(type_index, Rc::new(Semaphore::new(1)));
            self.delay_logged.insert(type_index, Cell::new(0));
            self.devices.insert(
                device.uid.clone(),
                (Rc::new(RefCell::new(device)), Rc::new(driver)),
            );
        }
        Ok(())
    }

    /// Gets the info necessary to apply setting to the device channel
    fn get_hwmon_info(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<(&Rc<HwmonDriverInfo>, &HwmonChannelInfo, TypeIndex)> {
        let (device_lock, hwmon_driver) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        let channel_info = hwmon_driver
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| format!("Searching for channel name: {channel_name}"))?;
        Ok((hwmon_driver, channel_info, device_lock.borrow().type_index))
    }

    async fn get_driver_locations(base_path: &Path) -> Vec<String> {
        let hwmon_path = base_path.to_str().unwrap_or_default().to_owned();
        let device_path = devices::get_static_device_path_str(base_path);
        let mut locations = vec![hwmon_path, device_path.unwrap_or_default()];
        if let Some(mod_alias) = devices::get_device_mod_alias(base_path).await {
            locations.push(mod_alias);
        }
        if let Some(hid_phys) = devices::get_device_hid_phys(base_path).await {
            locations.push(hid_phys);
        }
        locations
    }

    /// Reads channel and temp statuses for one device and upserts them
    /// into the preloaded cache per channel as each read completes.
    /// Fast channels on the device become visible to downstream the
    /// same tick they are read, even if a later channel on the same
    /// device is slow. Failing reads leave their cache entries
    /// untouched so downstream keeps seeing the last known good
    /// value, not a fabricated 0. Each sink also flips a
    /// pre-allocated `fresh_this_tick` bool inside `FailsafeStatusData`
    /// so the select! timeout arm on subsequent ticks can recognize
    /// partial upserts as fresh instead of ticking every channel
    /// blindly. Staleness and failsafe substitution are handled per
    /// channel in `tick_staleness_and_log`, invoked at end-of-tick.
    async fn preload_device_statuses(&self, type_index: TypeIndex, driver: &Rc<HwmonDriverInfo>) {
        // Clear the fresh-this-tick flags at the start of this
        // preload attempt. Any subsequent timeout arm that fires
        // while this preload is still running reads the flags as
        // they get set by the streaming sinks. `is_failsafed` and
        // `stale_ticks` persist across preloads.
        self.reset_fresh_this_tick(type_index);

        // Each sink lives in a scoped block so its transient &mut
        // borrows end before the next extractor runs. Each sink
        // toggles a pre-allocated bool per channel and upserts the
        // status. No allocation, no name cloning in the hot path.
        let _fan_failure = {
            let mut fan_sink = |status: ChannelStatus| {
                self.mark_channel_fresh(type_index, &status.name);
                self.upsert_single_channel(type_index, status);
            };
            if driver.apple_smc.detected {
                driver
                    .apple_smc
                    .stream_fan_statuses(driver, &mut fan_sink)
                    .await
            } else {
                fans::stream_fan_statuses(driver, &mut fan_sink).await
            }
        };
        let _power_failure = {
            let mut power_sink = |status: ChannelStatus| {
                self.mark_channel_fresh(type_index, &status.name);
                self.upsert_single_channel(type_index, status);
            };
            power::stream_power_status(driver, &mut power_sink).await
        };
        let _temp_failure = {
            let mut temp_sink = |status: TempStatus| {
                self.mark_temp_fresh(type_index, &status.name);
                self.upsert_single_temp(type_index, status);
            };
            if drivetemp::is_suspended(driver.block_dev_path.as_ref(), self.drivetemp_ioctl_timeout)
                .await
            {
                drivetemp::stream_default_suspended_temps(driver, &mut temp_sink);
                false
            } else {
                temps::stream_temp_statuses(driver, &mut temp_sink).await
            }
        };

        self.tick_staleness_and_log(type_index, &driver.name);
    }

    /// Clears the `fresh_this_tick` flags for `type_index`. Called
    /// at the start of each preload attempt so the flags reflect
    /// only the in-flight attempt.
    fn reset_fresh_this_tick(&self, type_index: TypeIndex) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        if let Some(fsd) = fsd_map.get_mut(&type_index) {
            fsd.reset_fresh_this_tick();
        }
    }

    /// Marks a channel as freshly upserted in the current preload
    /// attempt. The bool flip is keyed by pre-allocated name entries
    /// in `FailsafeStatusData`, so the hot path allocates nothing.
    fn mark_channel_fresh(&self, type_index: TypeIndex, name: &str) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        if let Some(fsd) = fsd_map.get_mut(&type_index) {
            fsd.mark_channel_fresh(name);
        }
    }

    /// Mirror of `mark_channel_fresh` for temps.
    fn mark_temp_fresh(&self, type_index: TypeIndex, name: &str) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        if let Some(fsd) = fsd_map.get_mut(&type_index) {
            fsd.mark_temp_fresh(name);
        }
    }

    /// Inserts one fresh channel status into the preloaded cache for
    /// `type_index`, replacing any prior entry with the same name or
    /// appending when absent. Short critical section: one `RefMut`
    /// borrow per channel, released before the next extractor yield.
    fn upsert_single_channel(&self, type_index: TypeIndex, fresh: ChannelStatus) {
        let mut preloaded = self.preloaded_statuses.borrow_mut();
        let (channels, _) = preloaded
            .entry(type_index)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let len_before = channels.len();
        if let Some(entry) = channels.iter_mut().find(|c| c.name == fresh.name) {
            *entry = fresh;
            debug_assert_eq!(channels.len(), len_before);
        } else {
            channels.push(fresh);
            debug_assert_eq!(channels.len(), len_before + 1);
        }
    }

    /// Mirror of `upsert_single_channel` for temp statuses.
    fn upsert_single_temp(&self, type_index: TypeIndex, fresh: TempStatus) {
        let mut preloaded = self.preloaded_statuses.borrow_mut();
        let (_, temps) = preloaded
            .entry(type_index)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let len_before = temps.len();
        if let Some(entry) = temps.iter_mut().find(|t| t.name == fresh.name) {
            *entry = fresh;
            debug_assert_eq!(temps.len(), len_before);
        } else {
            temps.push(fresh);
            debug_assert_eq!(temps.len(), len_before + 1);
        }
    }

    /// Calls `FailsafeStatusData::tick_per_channel_staleness` with
    /// the per-channel state already tracked inside `fsd`, and
    /// emits one-shot transition logs at the newly-failsafing and
    /// fully-recovered boundaries. Called from the end of
    /// `preload_device_statuses` and from the select! timeout arm.
    /// The timeout-arm caller sees whatever the currently running
    /// or most recently completed preload has upserted via
    /// `mark_channel_fresh` / `mark_temp_fresh`, so channels that
    /// already have real fresh values in the cache are not ticked.
    fn tick_staleness_and_log(&self, type_index: TypeIndex, driver_name: &str) {
        let mut fsd_map = self.failsafe_statuses.borrow_mut();
        let Some(fsd) = fsd_map.get_mut(&type_index) else {
            return;
        };
        let mut preloaded = self.preloaded_statuses.borrow_mut();
        let (channels, temps) = preloaded
            .entry(type_index)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        let (newly_failsafing, just_recovered) = fsd.tick_per_channel_staleness(channels, temps);
        if newly_failsafing {
            error!(
                "Significant issue retrieving status for hwmon \
                 device: {driver_name}. Substituting failsafe \
                 values for stale channels."
            );
        }
        if just_recovered {
            info!(
                "Recovered from failsafe for hwmon device: {driver_name}. \
                 Resuming normal status reads."
            );
        }
    }

    /// Logging slow devices is triggered once the polling loop overlaps and the
    /// `DEVICE_READ_PERMIT_TIMEOUT` is reached.
    /// This only outputs a log on the 2nd occurrence, which then avoids outputting a log during
    /// initialization where some devices are under extra load, but makes sure to log it if it
    /// happens during normal polling loop operations.
    fn log_slow_device(&self, type_index: TypeIndex, driver_name: &str) {
        let slow_device_trigger_count = self.delay_logged.get(&type_index).unwrap().get();
        if slow_device_trigger_count > 1 {
            return;
        }
        if slow_device_trigger_count == 1 {
            let log_level = if driver_name == DRIVETEMP {
                log::Level::Debug
            } else {
                log::Level::Warn
            };
            log!(
                log_level,
                "Slow HWMon Device detected for: {driver_name}. \
                This device may be slow to update and respond."
            );
        }
        self.delay_logged
            .get(&type_index)
            .unwrap()
            .replace(slow_device_trigger_count + 1);
    }

    async fn get_permit_with_write_timeout(
        &self,
        type_index: TypeIndex,
        driver_name: &str,
        channel_name: &str,
    ) -> Result<SemaphorePermit<'_>> {
        tokio::select! {
            () = sleep(self.device_write_permit_timeout) => Err(anyhow!(
                "TIMEOUT HWMon device: {driver_name} channel: {channel_name}; waiting to apply \
                fan speed. There will be significant issues handling this device due to extreme lag."
            )),
            device_permit = self.device_permits.get(&type_index).unwrap().acquire() =>
                device_permit.map_err(|e| anyhow!(e)),
        }
    }

    /// Spawns a detached task that re-acquires the device permit and
    /// holds it for the command delay. No-op when `delay_millis == 0`.
    /// The handoff lets `preload_statuses` return as soon as the reads
    /// complete while still gating subsequent writes (and the next
    /// preload) behind the device's configured settle time.
    fn spawn_command_delay_holder(&self, type_index: TypeIndex, delay_millis: u16) {
        if delay_millis == 0 {
            return;
        }
        let Some(permit) = self.device_permits.get(&type_index) else {
            return;
        };
        let permit = Rc::clone(permit);
        tokio::task::spawn_local(async move {
            // The permit borrows from `permit` through this .await.
            // The async state machine stores both the Rc and the
            // SemaphorePermit, so the self-reference is sound for
            // the life of this task.
            let Ok(_held) = permit.acquire().await else {
                // Semaphore closed; daemon is shutting down. Drop
                // silently and let the runtime tear down.
                return;
            };
            apply_device_command_delay(delay_millis).await;
            // `_held` is dropped here, releasing the permit.
        });
    }
}

#[async_trait(?Send)]
impl Repository for HwmonRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::Hwmon
    }

    #[allow(clippy::too_many_lines)]
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();

        let base_paths = devices::find_all_hwmon_device_paths();
        if base_paths.is_empty() {
            info!(
                "No HWMon devices were found, try installing lm-sensors and running sensors-detect"
            );
            return Ok(());
        }
        debug!("Detected HWMon device paths: {base_paths:?}");
        let mut hwmon_drivers: Vec<HwmonDriverInfo> = Vec::new();
        let settings = self.config.get_settings()?;
        for path in base_paths {
            debug!("Processing HWMon device path: {}", path.display());
            let device_name = devices::get_device_name(&path).await;
            debug!("Detected Device Name: {device_name}");
            if HWMON_DEVICE_NAME_BLACKLIST.contains(&device_name.trim()) {
                continue;
            }
            if settings.hide_duplicate_devices && self.path_matches_liquidctl_device(&path) {
                info!(
                    "Skipping HWMon detected device: {device_name} due to an existing \
                    duplicate liquidctl device"
                );
                continue;
            }
            let u_id = devices::get_device_unique_id(&path, &device_name).await;
            debug!("Detected UID: {u_id}");
            let device_uid =
                Device::create_uid_from(&device_name, DeviceType::Hwmon, 0, Some(&u_id));
            let cc_device_setting = self
                .config
                .get_cc_settings_for_device(&device_uid)
                .ok()
                .flatten();
            if cc_device_setting.as_ref().is_some_and(|s| s.disable) {
                info!("Skipping disabled device: {device_name} with UID: {device_uid}");
                continue;
            }
            let disabled_channels =
                cc_device_setting.map_or_else(Vec::new, |setting| setting.get_disabled_channels());
            let mut channels = vec![];
            if DEVICE_NAMES_APPLE.contains(&device_name.as_str()) {
                AppleMacSMC::init_fans(&path, &mut channels, &disabled_channels).await;
            } else {
                match fans::init_fans(&path, &device_name).await {
                    Ok(fans) => channels.extend(
                        fans.into_iter()
                            .filter(|fan| disabled_channels.contains(&fan.name).not())
                            .collect::<Vec<HwmonChannelInfo>>(),
                    ),
                    Err(err) => error!("Error initializing Hwmon Fans: {err}"),
                }
            }
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(
                    temps
                        .into_iter()
                        .filter(|temp| disabled_channels.contains(&temp.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing Hwmon Temps: {err}"),
            }
            match power::init_power(&path).await {
                Ok(power) => channels.extend(
                    power
                        .into_iter()
                        .filter(|power| disabled_channels.contains(&power.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing Hwmon Power: {err}"),
            }
            if channels.is_empty() {
                debug!(
                    "No fans, temps, or power detected under {}, skipping.",
                    path.display()
                );
                continue;
            }
            let block_dev_path = if device_name == DRIVETEMP && settings.drivetemp_suspend {
                drivetemp::get_verified_block_device_path(&path)
                    .inspect_err(|err| warn!("Error getting block device path: {err}"))
                    .ok()
            } else {
                None
            };
            let apple_smc = if DEVICE_NAMES_APPLE.contains(&device_name.as_str()) {
                AppleMacSMC::new(&path, &channels, &device_name).await
            } else {
                AppleMacSMC::not_applicable()
            };
            let pci_device_names = devices::get_device_pci_names(&path).await;
            let model = devices::get_device_model_name(&path).await.or_else(|| {
                pci_device_names.and_then(|names| names.subdevice_name.or(names.device_name))
            });
            debug!("Detected Device Model: {model:?}");
            let hwmon_driver_info = HwmonDriverInfo {
                name: device_name,
                path,
                model,
                u_id,
                channels,
                block_dev_path,
                apple_smc,
            };
            hwmon_drivers.push(hwmon_driver_info);
        }
        devices::handle_duplicate_device_names(&mut hwmon_drivers).await;
        // re-sorted by name to help keep some semblance of order after reboots & device changes.
        hwmon_drivers.sort_by(|d1, d2| d1.name.cmp(&d2.name));

        self.map_into_our_device_model(hwmon_drivers).await?;
        self.load_device_delays();

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(uid.clone(), (device.borrow().clone(), hwmon_info.clone()));
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Hwmon Devices: {init_devices:?}");
        } else {
            let device_map: HashMap<_, _> = init_devices
                .iter()
                .map(|d| {
                    (
                        d.1 .0.name.clone(),
                        HashMap::from([
                            (
                                "driver name",
                                vec![d.1 .0.info.driver_info.name.clone().unwrap_or_default()],
                            ),
                            (
                                "driver version",
                                vec![d.1 .0.info.driver_info.version.clone().unwrap_or_default()],
                            ),
                            ("locations", d.1 .0.info.driver_info.locations.clone()),
                            ("channels", {
                                let mut ch: Vec<_> = d.1 .0.info.channels.keys().cloned().collect();
                                ch.sort();
                                ch
                            }),
                            ("temps", {
                                let mut t: Vec<_> = d.1 .0.info.temps.keys().cloned().collect();
                                t.sort();
                                t
                            }),
                        ]),
                    )
                })
                .collect();
            info!(
                "Initialized Hwmon Devices: {}",
                serde_json::to_string(&device_map).unwrap_or_default()
            );
        }
        trace!(
            "Time taken to initialize all Hwmon devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("HWMON Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices
            .values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        moro_local::async_scope!(|scope| {
            for (uid, (device_lock, driver)) in &self.devices {
                let type_index = device_lock.borrow().type_index;
                let delay = self.device_delay(uid);
                let self = Rc::clone(&self);
                let read_permit_timeout = self.device_read_permit_timeout;
                scope.spawn(async move {
                    tokio::select! {
                        () = sleep(read_permit_timeout) => {
                            // Permit still held: no new preload ran this
                            // tick. The in-progress fresh set reflects
                            // whatever the currently-running or most
                            // recently completed preload has upserted so
                            // far. Channels already upserted stay fresh;
                            // channels not yet reached by the in-flight
                            // preload tick up toward failsafe. Prevents
                            // stale values from being served forever
                            // when a device hangs, while avoiding blanket
                            // ticks on channels that have fresh cached
                            // values.
                            self.log_slow_device(type_index, &driver.name);
                            self.tick_staleness_and_log(type_index, &driver.name);
                        },
                        Ok(device_permit) = self.device_permits.get(&type_index).unwrap().acquire() => {
                            // Queue the post-read delay holder before
                            // running the preload so any write (or next
                            // preload) that arrives during the reads
                            // queues behind the delay holder in the
                            // Semaphore's FIFO waiter list. This
                            // preserves the device's settle time between
                            // the read and the next command, matching
                            // the old "hold permit through delay"
                            // semantics, while still letting this scope
                            // complete as soon as the reads are done.
                            self.spawn_command_delay_holder(type_index, delay);
                            self.preload_device_statuses(type_index, driver).await;
                            drop(device_permit);
                        },
                    }
                });
            }
        }).await;
        trace!(
            "STATUS PRELOAD Time taken for all HWMON devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        for (device, _) in self.devices.values() {
            let preloaded_statuses_map = self.preloaded_statuses.borrow();
            let device_index = device.borrow().type_index;
            let preloaded_statuses = preloaded_statuses_map.get(&device_index);
            let Some((channels, temps)) = preloaded_statuses.cloned() else {
                error!("There is no status preloaded for this device: {device_index}");
                continue;
            };
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            trace!(
                "Hwmon device: {} status was updated with: {status:?}",
                device.borrow().name
            );
            device.borrow_mut().set_status(status);
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (device_uid, (device_lock, hwmon_driver)) in &self.devices {
            let type_index = device_lock.borrow().type_index;
            for channel_info in &hwmon_driver.channels {
                if channel_info.hwmon_type != HwmonChannelType::Fan {
                    continue;
                }
                debug!(
                    "Applying HWMON device: {device_uid} channel: {}; \
                    Resetting to Original fan control mode",
                    channel_info.name
                );
                let device_permit = self
                    .get_permit_with_write_timeout(
                        type_index,
                        &hwmon_driver.name,
                        &channel_info.name,
                    )
                    .await?;
                let _ =
                    fans::set_pwm_enable_to_default_or_auto(&hwmon_driver.path, channel_info).await;
                drop(device_permit);
            }
        }
        info!("HWMON Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let (hwmon_driver, channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        debug!(
            "Applying HWMON device: {device_uid} channel: {channel_name}; Resetting to Original fan control mode"
        );
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        let result = if hwmon_driver.apple_smc.detected {
            hwmon_driver
                .apple_smc
                .set_to_auto_control(channel_info.number)
                .await
        } else {
            fans::set_pwm_enable_to_default_or_auto(&hwmon_driver.path, channel_info).await
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_manual_control(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<()> {
        let (hwmon_driver, channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        let result = if hwmon_driver.apple_smc.detected {
            hwmon_driver
                .apple_smc
                .set_to_manual_control(channel_info.number)
                .await
        } else {
            fans::set_pwm_enable(
                fans::PWM_ENABLE_MANUAL_VALUE,
                &hwmon_driver.path,
                channel_info,
            )
            .await
            .map_err(|err| {
                anyhow!(
                    "Error on {}:{channel_name} for Manual Control - {err}",
                    hwmon_driver.name
                )
            })
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: Duty,
    ) -> Result<()> {
        let (hwmon_driver, channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {speed_fixed}"));
        }
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        debug!(
            "Applying HWMON device: {device_uid} channel: {channel_name}; Fixed Speed: {speed_fixed}"
        );
        let result = if hwmon_driver.name == devices::DEVICE_NAME_THINK_PAD {
            thinkpad::apply_speed_fixed(&self.config, hwmon_driver, channel_info, speed_fixed).await
        } else if hwmon_driver.apple_smc.detected {
            hwmon_driver
                .apple_smc
                .set_fan_duty(channel_info.number, speed_fixed)
                .await
        } else {
            fans::set_pwm_duty(&hwmon_driver.path, channel_info, speed_fixed)
                .await
                .map_err(|err| {
                    anyhow!(
                        "Error on {}:{channel_name} for duty {speed_fixed} - {err}",
                        hwmon_driver.name
                    )
                })
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(Temp, Duty)],
    ) -> Result<()> {
        let (hwmon_driver, fan_channel_info, type_index) =
            self.get_hwmon_info(device_uid, channel_name)?;
        if fan_channel_info.auto_curve == AutoCurveInfo::None {
            return Err(anyhow!(
                "Applying Internal Profile Error: device_uid: {device_uid} channel: {channel_name} does not support auto curves."
            ));
        }
        if &temp_source.device_uid != device_uid {
            return Err(anyhow!(
                "Applying Internal Profile Error: temp_source device_uid: {} does not match this device. \
                Auto curves temperature sources must be internal to the device.",
                temp_source.device_uid
            ));
        }
        let temp_channel_info = hwmon_driver
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Temp
                    && channel.name == temp_source.temp_name
            })
            .with_context(|| {
                format!("Searching for temp channel name: {}", temp_source.temp_name)
            })?;
        let _device_permit = self
            .get_permit_with_write_timeout(type_index, &hwmon_driver.name, channel_name)
            .await?;
        debug!(
            "Applying HWMON device: {device_uid} channel: {channel_name}; Speed Profile: {speed_profile:?}"
        );
        let result = auto_curve::apply_curve(
            &hwmon_driver.path,
            fan_channel_info,
            speed_profile,
            temp_channel_info,
            &hwmon_driver.name,
        )
        .await
        .map_err(|err| {
            anyhow!(
                "Error on {}:{channel_name} for speed profile {speed_profile:?} - {err}",
                hwmon_driver.name
            )
        });
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Lighting settings are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying LCD settings are not supported for HWMON devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying pwm_mode setting is no longer supported for HWMON devices"
        ))
        // let (hwmon_driver, channel_info) = self.get_hwmon_info(device_uid, channel_name)?;
        // info!(
        //     "Applying HWMON device: {} channel: {}; PWM Mode: {}",
        //     device_uid, channel_name, pwm_mode
        // );
        // fans::set_pwm_mode(&hwmon_driver.path, channel_info, Some(pwm_mode)).await
    }

    async fn prepare_for_sleep(&self) {
        for (device_uid, (_device_lock, hwmon_driver)) in &self.devices {
            if hwmon_driver.name != devices::DEVICE_NAME_THINK_PAD {
                continue;
            }
            for channel_info in &hwmon_driver.channels {
                if channel_info.hwmon_type != HwmonChannelType::Fan {
                    continue;
                }
                if channel_info.caps.is_fan_controllable().not() {
                    continue;
                }
                info!(
                    "Setting ThinkPad device: {device_uid} channel: {} to auto mode for sleep",
                    channel_info.name
                );
                if let Err(err) = fans::set_pwm_enable(
                    fans::PWM_ENABLE_AUTO_VALUE,
                    &hwmon_driver.path,
                    channel_info,
                )
                .await
                {
                    warn!(
                        "Failed to set auto mode for ThinkPad device: {device_uid} \
                         channel: {} before sleep: {err}",
                        channel_info.name
                    );
                }
            }
        }
    }

    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }
}

#[cfg(test)]
mod preload_tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::failsafe::{self, MISSING_DUTY_FAILSAFE, MISSING_RPM_FAILSAFE};
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    const TEST_TYPE_INDEX: TypeIndex = 1;

    struct PreloadContext {
        test_base_path: PathBuf,
    }

    async fn setup() -> PreloadContext {
        let base = format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4());
        let path = Path::new(&base).to_path_buf();
        cc_fs::create_dir_all(&path).await.unwrap();
        PreloadContext {
            test_base_path: path,
        }
    }

    async fn teardown(ctx: &PreloadContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).await.unwrap();
    }

    fn new_test_repo() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        HwmonRepo::new(config, vec![])
    }

    fn fan_channel_with_paths(number: u8, name: &str, base_path: &Path) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            pwm_enable_default: None,
            name: name.to_string(),
            label: None,
            caps: HwmonChannelCapabilities::PWM | HwmonChannelCapabilities::RPM,
            auto_curve: AutoCurveInfo::None,
            pwm_path: Some(base_path.join(format!("pwm{number}"))),
            rpm_path: Some(base_path.join(format!("fan{number}_input"))),
            temp_path: None,
        }
    }

    fn driver_with_channels(
        base_path: &Path,
        channels: Vec<HwmonChannelInfo>,
    ) -> Rc<HwmonDriverInfo> {
        Rc::new(HwmonDriverInfo {
            name: "test_driver".to_string(),
            path: base_path.to_path_buf(),
            model: None,
            u_id: String::new(),
            channels,
            block_dev_path: None,
            apple_smc: AppleMacSMC::default(),
        })
    }

    /// Seeds the failsafe map for `type_index` using initial statuses
    /// as if the device had successfully preloaded at init time.
    fn seed_failsafe(
        repo: &HwmonRepo,
        type_index: TypeIndex,
        channel_statuses: &[ChannelStatus],
        temp_statuses: &[TempStatus],
    ) {
        let (channel_failsafes, temp_failsafes) =
            failsafe::create_failsafe_data(channel_statuses, temp_statuses);
        if let Some(fsd) = FailsafeStatusData::new(channel_failsafes, temp_failsafes) {
            repo.failsafe_statuses.borrow_mut().insert(type_index, fsd);
        }
    }

    #[test]
    #[serial]
    fn preload_upserts_fresh_channel_in_place() {
        // Two successive preloads with fresh fan readings must replace
        // the cache entry in place rather than duplicating it.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // when: two consecutive preloads.
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            cc_fs::write(base.join("fan1_input"), b"1800".to_vec())
                .await
                .unwrap();
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: cache has exactly one entry, with the latest value.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                assert_eq!(channels[0].name, "fan1");
                assert_eq!(channels[0].rpm, Some(1800));
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_preserves_cache_on_single_channel_failure() {
        // When one of two fan channels fails its PWM read while the
        // other succeeds, the successful entry updates and the failing
        // entry keeps its prior last-known-good value.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            // both readable in the initial tick
            cc_fs::write(base.join("pwm1"), b"64".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"900".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("pwm2"), b"200".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan2_input"), b"2500".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(
                base,
                vec![
                    fan_channel_with_paths(1, "fan1", base),
                    fan_channel_with_paths(2, "fan2", base),
                ],
            );
            let repo = new_test_repo();
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[], &[]);

            // given: first preload populates both entries
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            // when: fan1 updates, fan2 now fails (pwm2 removed)
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            cc_fs::remove_file(base.join("pwm2")).await.unwrap();
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: fan1 updated, fan2 preserved at 2500.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 2);
                let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
                assert_eq!(fan1.rpm, Some(1200));
                let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
                assert_eq!(fan2.rpm, Some(2500));
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_applies_failsafe_only_when_threshold_exceeded() {
        // Drives the failsafe counter past MISSING_STATUS_THRESHOLD via
        // repeated failing preloads. Once active, the overlay replaces
        // the absent channel's cache entry with its failsafe value.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();

            // given: initial successful read to seed cache + failsafe data.
            let seed_status = ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            };
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[seed_status], &[]);
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // when: remove pwm1 so every subsequent preload fails, and
            // drive the counter above MISSING_STATUS_THRESHOLD.
            cc_fs::remove_file(base.join("pwm1")).await.unwrap();
            for _ in 0..=MISSING_STATUS_THRESHOLD {
                repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            }

            // then: the cache now holds the failsafe values for fan1,
            // because the overlay substituted them while the threshold
            // was exceeded and the channel did not report.
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
                assert_eq!(fan1.rpm, Some(MISSING_RPM_FAILSAFE));
                assert_eq!(fan1.duty, Some(MISSING_DUTY_FAILSAFE));
            }
            teardown(&ctx).await;
        });
    }

    #[test]
    #[serial]
    fn preload_recovery_clears_failsafe_on_success() {
        // After the per-channel stale counter trips the threshold and
        // fan1's cache entry is substituted with its failsafe value,
        // a fully successful preload must reset the counter to 0 and
        // the fresh read's values must replace the failsafe values.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            let seed_status = ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            };
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[seed_status], &[]);
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            cc_fs::remove_file(base.join("pwm1")).await.unwrap();
            for _ in 0..=MISSING_STATUS_THRESHOLD {
                repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;
            }
            // Verify failsafe is active for fan1 on the per-channel
            // path.
            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
                assert!(fsd.was_failsafing);
                let fan1_state = &fsd.channel_state["fan1"];
                assert!((fan1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
                assert!(fan1_state.is_failsafed);
            }

            // when: pwm1 comes back and preload succeeds.
            cc_fs::write(base.join("pwm1"), b"200".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"2000".to_vec())
                .await
                .unwrap();
            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            // then: per-channel counter cleared, not failsafing, and
            // fresh values in the cache.
            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
                assert!(fsd.was_failsafing.not());
                let fan1_state = &fsd.channel_state["fan1"];
                assert_eq!(fan1_state.stale_ticks, 0);
                assert!(fan1_state.is_failsafed.not());
            }
            {
                let preloaded = repo.preloaded_statuses.borrow();
                let (channels, _) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
                assert_eq!(channels.len(), 1);
                let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
                assert_eq!(fan1.rpm, Some(2000));
            }
            teardown(&ctx).await;
        });
    }

    // --- per-channel staleness wiring ---

    /// Seeds `fresh_this_tick` flags on the failsafe state for the
    /// given names, simulating what the streaming sinks would have
    /// upserted during the current preload attempt.
    fn mark_fresh(
        repo: &HwmonRepo,
        type_index: TypeIndex,
        channel_names: &[&str],
        temp_names: &[&str],
    ) {
        let mut fsd_map = repo.failsafe_statuses.borrow_mut();
        let fsd = fsd_map.get_mut(&type_index).unwrap();
        fsd.reset_fresh_this_tick();
        for name in channel_names {
            fsd.mark_channel_fresh(name);
        }
        for name in temp_names {
            fsd.mark_temp_fresh(name);
        }
    }

    #[test]
    #[serial]
    fn timeout_arm_respects_fresh_this_tick_flags() {
        // Simulates a still-running preload that has upserted fan1
        // but is stuck before fan2 / temp1. Repeated timeout-arm
        // firings must leave fan1's counter at 0 and its cache value
        // intact, while fan2 and temp1 tick up and fail over to
        // their failsafes once the threshold is crossed.
        let repo = new_test_repo();
        let seed_channels = vec![
            ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            },
            ChannelStatus {
                name: "fan2".to_string(),
                rpm: Some(900),
                duty: Some(30.0),
                ..Default::default()
            },
        ];
        let seed_temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 40.0,
        }];
        seed_failsafe(&repo, TEST_TYPE_INDEX, &seed_channels, &seed_temps);
        repo.preloaded_statuses
            .borrow_mut()
            .insert(TEST_TYPE_INDEX, (seed_channels, seed_temps));

        // In-flight preload state: only fan1 has been upserted.
        // Every tick re-applies the fresh flag (sink would fire once
        // per preload; since we only simulate the timeout-arm side,
        // the flag persists once set via `mark_channel_fresh`).
        for _ in 0..=MISSING_STATUS_THRESHOLD {
            mark_fresh(&repo, TEST_TYPE_INDEX, &["fan1"], &[]);
            repo.tick_staleness_and_log(TEST_TYPE_INDEX, "test_driver");
        }

        let fsd_map = repo.failsafe_statuses.borrow();
        let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
        let fan1_state = &fsd.channel_state["fan1"];
        assert_eq!(fan1_state.stale_ticks, 0);
        assert!(fan1_state.is_failsafed.not());
        let fan2_state = &fsd.channel_state["fan2"];
        assert!((fan2_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(fan2_state.is_failsafed);
        let temp1_state = &fsd.temp_state["temp1"];
        assert!((temp1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(temp1_state.is_failsafed);
        assert!(fsd.was_failsafing);
        drop(fsd_map);

        let preloaded = repo.preloaded_statuses.borrow();
        let (channels, temps) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(1200));
        let fan2 = channels.iter().find(|c| c.name == "fan2").unwrap();
        assert_eq!(fan2.rpm, Some(MISSING_RPM_FAILSAFE));
        assert_eq!(fan2.duty, Some(MISSING_DUTY_FAILSAFE));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!(
            (temp_entry.temp - crate::repositories::failsafe::MISSING_TEMP_FAILSAFE).abs()
                < f64::EPSILON
        );
    }

    #[test]
    #[serial]
    fn timeout_with_no_fresh_flags_ticks_everything() {
        // A preload that has not upserted anything (truly hung from
        // the start) leaves every `fresh_this_tick` flag false.
        // Every channel's counter must tick up and fail over once
        // the threshold is crossed.
        let repo = new_test_repo();
        let seed_channels = vec![ChannelStatus {
            name: "fan1".to_string(),
            rpm: Some(1200),
            duty: Some(50.0),
            ..Default::default()
        }];
        let seed_temps = vec![TempStatus {
            name: "temp1".to_string(),
            temp: 40.0,
        }];
        seed_failsafe(&repo, TEST_TYPE_INDEX, &seed_channels, &seed_temps);
        repo.preloaded_statuses
            .borrow_mut()
            .insert(TEST_TYPE_INDEX, (seed_channels, seed_temps));
        // No fresh flags set - explicit reset to simulate a fresh
        // preload attempt that never upserts anything.
        mark_fresh(&repo, TEST_TYPE_INDEX, &[], &[]);

        for _ in 0..=MISSING_STATUS_THRESHOLD {
            repo.tick_staleness_and_log(TEST_TYPE_INDEX, "test_driver");
        }

        let fsd_map = repo.failsafe_statuses.borrow();
        let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
        let fan1_state = &fsd.channel_state["fan1"];
        assert!((fan1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(fan1_state.is_failsafed);
        let temp1_state = &fsd.temp_state["temp1"];
        assert!((temp1_state.stale_ticks as usize) > MISSING_STATUS_THRESHOLD);
        assert!(temp1_state.is_failsafed);
        assert!(fsd.was_failsafing);
        drop(fsd_map);

        let preloaded = repo.preloaded_statuses.borrow();
        let (channels, temps) = preloaded.get(&TEST_TYPE_INDEX).unwrap();
        let fan1 = channels.iter().find(|c| c.name == "fan1").unwrap();
        assert_eq!(fan1.rpm, Some(MISSING_RPM_FAILSAFE));
        let temp_entry = temps.iter().find(|t| t.name == "temp1").unwrap();
        assert!(
            (temp_entry.temp - crate::repositories::failsafe::MISSING_TEMP_FAILSAFE).abs()
                < f64::EPSILON
        );
    }

    #[test]
    #[serial]
    fn preload_start_clears_fresh_this_tick_flags() {
        // The clear-at-start invariant: each preload attempt starts
        // with `fresh_this_tick` flags cleared, so flags left over
        // from a prior attempt cannot pretend to be fresh.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let base = &ctx.test_base_path;
            cc_fs::write(base.join("pwm1"), b"128".to_vec())
                .await
                .unwrap();
            cc_fs::write(base.join("fan1_input"), b"1200".to_vec())
                .await
                .unwrap();
            let driver = driver_with_channels(base, vec![fan_channel_with_paths(1, "fan1", base)]);
            let repo = new_test_repo();
            let seed_fan = ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1200),
                duty: Some(50.0),
                ..Default::default()
            };
            seed_failsafe(&repo, TEST_TYPE_INDEX, &[seed_fan], &[]);
            // Pre-populate the fresh flag for fan1 from a prior
            // (simulated) preload attempt.
            mark_fresh(&repo, TEST_TYPE_INDEX, &["fan1"], &[]);

            repo.preload_device_statuses(TEST_TYPE_INDEX, &driver).await;

            {
                let fsd_map = repo.failsafe_statuses.borrow();
                let fsd = fsd_map.get(&TEST_TYPE_INDEX).unwrap();
                // fan1 is fresh because this preload's sink fired,
                // not because of the pre-populated flag. Verified by
                // the stale_ticks counter staying at 0 (no tick up).
                assert_eq!(fsd.channel_state["fan1"].stale_ticks, 0);
                assert!(fsd.channel_state["fan1"].fresh_this_tick);
            }
            teardown(&ctx).await;
        });
    }
}

#[cfg(test)]
mod permit_timeout_tests {
    use super::*;

    #[test]
    fn read_permit_timeout_matches_legacy_at_min_poll_rate() {
        // Regression: at poll_rate = 0.5 s the formula must reproduce
        // the previous hard-coded 350 ms value.
        assert_eq!(
            device_read_permit_timeout_for(0.5),
            Duration::from_millis(350)
        );
    }

    #[test]
    fn read_permit_timeout_scales_with_poll_rate() {
        // The budget must widen proportionally for slower polls.
        assert_eq!(
            device_read_permit_timeout_for(1.0),
            Duration::from_millis(700)
        );
        assert_eq!(
            device_read_permit_timeout_for(5.0),
            Duration::from_millis(3500)
        );
    }

    #[test]
    fn write_permit_timeout_matches_legacy_at_default_poll_rate() {
        // Regression: at the default poll_rate = 1.0 s the formula
        // must reproduce the previous hard-coded 8 s value.
        assert_eq!(device_write_permit_timeout_for(1.0), Duration::from_secs(8));
    }

    #[test]
    fn write_permit_timeout_scales_with_poll_rate() {
        // The write timeout must track the failsafe wall time
        // exactly, i.e. MISSING_STATUS_THRESHOLD * poll_rate.
        assert_eq!(device_write_permit_timeout_for(0.5), Duration::from_secs(4));
        assert_eq!(
            device_write_permit_timeout_for(5.0),
            Duration::from_secs(40)
        );
    }

    #[test]
    fn read_permit_always_strictly_less_than_snapshot_budget() {
        // The invariant lets the repo's failsafe-apply branch run
        // before the main loop forces a snapshot. Ratios 0.7 vs 0.8
        // preserve 87.5 % at every poll rate.
        const SNAPSHOT_RATIO: f64 = 0.8;
        for poll_rate in [0.5_f64, 1.0, 5.0] {
            let read = device_read_permit_timeout_for(poll_rate);
            let snapshot = Duration::from_secs_f64(poll_rate * SNAPSHOT_RATIO);
            assert!(read < snapshot, "read must be < snapshot at {poll_rate}");
        }
    }

    #[test]
    fn drivetemp_ioctl_timeout_scales_with_poll_rate() {
        // The ioctl budget must scale proportionally with poll_rate
        // so a slow drivetemp check cannot consume more than its
        // share of the overall read permit at any valid poll rate.
        assert_eq!(drivetemp_ioctl_timeout_for(0.5), Duration::from_millis(200));
        assert_eq!(drivetemp_ioctl_timeout_for(1.0), Duration::from_millis(400));
        assert_eq!(drivetemp_ioctl_timeout_for(5.0), Duration::from_secs(2));
    }

    #[test]
    fn drivetemp_ioctl_timeout_always_strictly_less_than_read_permit() {
        // Invariant: on ioctl timeout the fallback temp read must
        // still have budget left before the outer read permit arm
        // fires. Ratios 0.4 vs 0.7 preserve 3/7 headroom at every
        // poll rate.
        for poll_rate in [0.5_f64, 1.0, 5.0] {
            let ioctl = drivetemp_ioctl_timeout_for(poll_rate);
            let read = device_read_permit_timeout_for(poll_rate);
            assert!(
                ioctl < read,
                "ioctl must be < read permit at poll_rate={poll_rate}"
            );
        }
    }
}

#[cfg(test)]
mod command_delay_handoff_tests {
    use super::*;
    use crate::cc_fs;
    use serial_test::serial;

    const TEST_TYPE_INDEX: TypeIndex = 1;

    fn new_test_repo_with_permit() -> HwmonRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        let mut repo = HwmonRepo::new(config, vec![]);
        repo.device_permits
            .insert(TEST_TYPE_INDEX, Rc::new(Semaphore::new(1)));
        repo
    }

    #[test]
    #[serial]
    fn delay_holder_is_noop_for_zero_delay() {
        // With delay_millis == 0 the handoff must not spawn a
        // delay-holder task. Even after yielding enough time for any
        // spawned task to run, the permit stays free.
        cc_fs::test_runtime(async {
            let repo = new_test_repo_with_permit();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, 0);
            sleep(Duration::from_millis(20)).await;
            let sem = repo.device_permits.get(&TEST_TYPE_INDEX).unwrap();
            assert!(
                sem.try_acquire().is_ok(),
                "permit should be free when delay is 0"
            );
        });
    }

    #[test]
    #[serial]
    fn delay_holder_is_noop_for_unknown_type_index() {
        // When no Semaphore exists for the given type_index (possible
        // if the device was never registered), the handoff must not
        // panic; it should return silently.
        cc_fs::test_runtime(async {
            let repo = new_test_repo_with_permit();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX + 1, 100);
            sleep(Duration::from_millis(20)).await;
            // Existing permit for TEST_TYPE_INDEX must remain free.
            let sem = repo.device_permits.get(&TEST_TYPE_INDEX).unwrap();
            assert!(sem.try_acquire().is_ok());
        });
    }

    #[test]
    #[serial]
    fn delay_holder_call_returns_immediately() {
        // The caller of spawn_command_delay_holder must not be stalled
        // on the delay. Verify by measuring wall clock around the call
        // with a long configured delay.
        cc_fs::test_runtime(async {
            let repo = new_test_repo_with_permit();
            let start = Instant::now();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, 500);
            let elapsed = start.elapsed();
            assert!(
                elapsed < Duration::from_millis(50),
                "caller stalled: elapsed={elapsed:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn delay_holder_gates_permit_for_delay_duration() {
        // Verifies the core invariant: after handoff, the permit is
        // held by the detached task for approximately delay_millis
        // and then released. Future preloads / writes that acquire
        // the same permit wait for the holder but not beyond.
        cc_fs::test_runtime(async {
            const DELAY_MS: u16 = 100;
            let repo = new_test_repo_with_permit();
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, DELAY_MS);
            // Yield so the spawn_local task can reach acquire_owned
            // before we probe the permit state.
            sleep(Duration::from_millis(10)).await;
            let sem = Rc::clone(repo.device_permits.get(&TEST_TYPE_INDEX).unwrap());
            assert!(
                sem.try_acquire().is_err(),
                "permit must be held by delay task"
            );
            // Wait out the delay plus a small margin and re-probe.
            sleep(Duration::from_millis(u64::from(DELAY_MS) + 50)).await;
            assert!(
                sem.try_acquire().is_ok(),
                "permit must be released once delay elapses"
            );
        });
    }

    #[test]
    #[serial]
    fn delay_holder_queued_before_later_writers() {
        // Verifies the FIFO invariant that makes spawning the delay
        // holder while the read permit is still held correct: a
        // writer that calls acquire() after the delay holder has
        // queued must wait for BOTH the preload's release AND the
        // delay. Without this, a write racing the preload's release
        // would bypass the device's configured settle time.
        cc_fs::test_runtime(async {
            const DELAY_MS: u16 = 100;
            let repo = new_test_repo_with_permit();
            let sem_rc = Rc::clone(repo.device_permits.get(&TEST_TYPE_INDEX).unwrap());

            // Simulate the preload holding the read permit.
            let preload_permit = sem_rc.acquire().await.unwrap();

            // Queue the delay holder behind the preload. Yield so
            // the spawn_local task's acquire() has reached the
            // waiter queue before the fake write arrives.
            repo.spawn_command_delay_holder(TEST_TYPE_INDEX, DELAY_MS);
            sleep(Duration::from_millis(10)).await;

            // Fake a writer that queues behind the delay holder.
            let sem_for_write = Rc::clone(&sem_rc);
            let write_acquired_at: Rc<RefCell<Option<Instant>>> = Rc::new(RefCell::new(None));
            let write_acquired_at_clone = Rc::clone(&write_acquired_at);
            let write_handle = tokio::task::spawn_local(async move {
                let _write_permit = sem_for_write.acquire().await.unwrap();
                *write_acquired_at_clone.borrow_mut() = Some(Instant::now());
            });
            sleep(Duration::from_millis(10)).await;

            // Release the preload permit; delay holder is next, the
            // writer is behind it.
            let release_at = Instant::now();
            drop(preload_permit);

            write_handle.await.unwrap();

            let acquired_at = write_acquired_at.borrow().expect("write acquired");
            let elapsed = acquired_at.duration_since(release_at);
            assert!(
                elapsed >= Duration::from_millis(u64::from(DELAY_MS)),
                "write acquired too fast: elapsed={elapsed:?}, expected >= {DELAY_MS}ms"
            );
        });
    }
}
