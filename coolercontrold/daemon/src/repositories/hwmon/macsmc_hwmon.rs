// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fan control on Apple Silicon, through the `macsmc_hwmon` driver.
//!
//! The driver has no manual switch. Writing a speed between `fanN_min` and `fanN_max` to
//! `fanN_target` takes the fan over, and writing `0` returns it to the firmware. Anything else
//! is rejected, so the daemon cannot stop a fan itself: a 0% duty hands the fan back, and 1 to
//! 100% spans the fan's own minimum to maximum.
//!
//! `fanN_target` is writable only when the module was loaded with `fan_control=1`.
//! See: `https://github.com/torvalds/linux/blob/master/drivers/hwmon/macsmc-hwmon.c`

use crate::cc_fs;
use crate::device::{ChannelStatus, Duty, RPM};
use crate::repositories::hwmon::apple;
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::fans;
use crate::repositories::hwmon::hwmon_repo::{
    HwmonChannelCapabilities, HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
};
use anyhow::{Context, Result};
use log::{error, info};
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::{Path, PathBuf};

/// The target that returns a fan to the firmware.
const FIRMWARE_TARGET: RPM = 0;
macro_rules! format_fan_target { ($($arg:tt)*) => {{ format!("fan{}_target", $($arg)*) }}; }
macro_rules! format_fan_min { ($($arg:tt)*) => {{ format!("fan{}_min", $($arg)*) }}; }
macro_rules! format_fan_max { ($($arg:tt)*) => {{ format!("fan{}_max", $($arg)*) }}; }

/// The speeds the driver accepts as a target for one fan.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FanLimits {
    min_rpm: RPM,
    max_rpm: RPM,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacSmcHwmon {
    path: PathBuf,
    /// The limits of every controllable fan, by fan number.
    fans: HashMap<u8, FanLimits>,
    /// Set once the first hand-back at 0% has been logged.
    #[serde(skip)]
    hand_back_logged: Cell<bool>,
    /// Fans given an in-range target this session. From then on the driver knows what the SMC
    /// is doing with the fan, so a bare `0` lands.
    #[serde(skip)]
    synced_fans: RefCell<HashSet<u8>>,
}

impl MacSmcHwmon {
    pub async fn new(path: &Path, channels: &[HwmonChannelInfo], io: &DeviceIo) -> Self {
        let mut fans = HashMap::with_capacity(channels.len());
        for channel in channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            if channel.caps.is_apple_smc().not() {
                continue;
            }
            match read_limits(path, channel.number, io).await {
                Some(limits) => {
                    fans.insert(channel.number, limits);
                }
                // Detection read these moments ago, so this is a driver fault. Writes to the
                // fan are refused from here on, which names it again.
                None => error!(
                    "The speed limits of {} at {} can no longer be read.",
                    channel.name,
                    path.display()
                ),
            }
        }
        Self {
            path: path.to_path_buf(),
            fans,
            hand_back_logged: Cell::new(false),
            synced_fans: RefCell::new(HashSet::with_capacity(channels.len())),
        }
    }

    pub async fn read_one_fan_status(
        &self,
        driver: &HwmonDriverInfo,
        channel: &HwmonChannelInfo,
    ) -> Option<ChannelStatus> {
        apple::read_fan_status(driver, channel, |rpm| {
            self.duty_from_rpm(channel.number, rpm)
        })
        .await
    }

    /// Sets the fan's speed, or hands it back to the firmware at 0%.
    pub async fn set_fan_duty(&self, channel_number: u8, duty: Duty, io: &DeviceIo) -> Result<()> {
        let limits = self.fans.get(&channel_number).with_context(|| {
            format!(
                "No speed limits are known for fan{channel_number} at {}",
                self.path.display()
            )
        })?;
        let targets = duty_targets(*limits, duty, self.is_synced(channel_number));
        if targets[1] == Some(FIRMWARE_TARGET) {
            self.log_first_hand_back(channel_number);
        }
        self.write_targets(channel_number, targets, io).await
    }

    /// Returns the fan to the firmware.
    ///
    /// The driver acts on `0` only while it believes the fan is manual. It forgets that when it
    /// is reloaded, and the SMC does not. A target at the minimum first makes the two agree, so
    /// the `0` that follows always lands.
    pub async fn set_to_auto_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        let targets = hand_back_targets(self.fans.get(&channel_number));
        self.write_targets(channel_number, targets, io).await
    }

    /// Every fan the driver exposes, including ones the user has disabled.
    ///
    /// Says nothing about a fan it cannot control, and leaves an error to the caller: the
    /// hardware report calls this too.
    pub async fn init_fans(base_path: &Path, io: &DeviceIo) -> Result<Vec<HwmonChannelInfo>> {
        let mut fan_caps = HashMap::new();
        for entry in cc_fs::read_dir(base_path)? {
            let os_file_name = entry?.file_name();
            let file_name = os_file_name.to_str().context("File Name should be a str")?;
            detect_controllable_fan(base_path, file_name, &mut fan_caps, io).await;
            fans::detect_rpm(base_path, file_name, &mut fan_caps, io).await?;
        }
        Ok(apple::fan_channels(base_path, fan_caps).await)
    }

    /// Says why this device's fans are read-only. Repository init only, so it is logged once
    /// per start and never by a hardware report.
    pub fn log_fan_control_off(base_path: &Path, channels: &[HwmonChannelInfo]) {
        if fan_control_is_off(base_path, channels) {
            info!(
                "The macsmc_hwmon fans at {} are read-only: fan control needs the module loaded \
                 with fan_control=1.",
                base_path.display()
            );
        }
    }

    fn duty_from_rpm(&self, channel_number: u8, rpm: RPM) -> f64 {
        self.fans
            .get(&channel_number)
            .map_or(0., |limits| duty_from_rpm(*limits, rpm))
    }

    /// 0% reads as "off" but means "the firmware decides" here, so say it once.
    fn log_first_hand_back(&self, channel_number: u8) {
        if self.hand_back_logged.replace(true) {
            return;
        }
        info!(
            "fan{channel_number} at {} is set to 0%. The driver cannot stop a fan, so it is \
             handed back to the firmware, which may keep it spinning.",
            self.path.display()
        );
    }

    fn is_synced(&self, channel_number: u8) -> bool {
        self.synced_fans.borrow().contains(&channel_number)
    }

    /// Writes the targets in order. Every write is attempted, and the first failure is the one
    /// reported.
    async fn write_targets(
        &self,
        channel_number: u8,
        targets: [Option<RPM>; 2],
        io: &DeviceIo,
    ) -> Result<()> {
        let mut result = Ok(());
        for rpm in targets.into_iter().flatten() {
            result = result.and(self.write_target(channel_number, rpm, io).await);
        }
        result
    }

    async fn write_target(&self, channel_number: u8, rpm: RPM, io: &DeviceIo) -> Result<()> {
        debug_assert!(
            rpm == FIRMWARE_TARGET
                || self
                    .fans
                    .get(&channel_number)
                    .is_some_and(|limits| (limits.min_rpm..=limits.max_rpm).contains(&rpm)),
            "the driver rejects a target outside the fan's limits"
        );
        let target_path = self.path.join(format_fan_target!(channel_number));
        apple::write_number(&target_path, rpm, io).await?;
        if rpm != FIRMWARE_TARGET {
            self.synced_fans.borrow_mut().insert(channel_number);
        }
        Ok(())
    }
}

/// Marks the fan behind a `fanN_target` file as controllable, when the driver would accept a
/// target for it and reports the limits a duty is mapped onto.
async fn detect_controllable_fan(
    base_path: &Path,
    file_name: &str,
    fan_caps: &mut HashMap<u8, HwmonChannelCapabilities>,
    io: &DeviceIo,
) {
    let Some(channel_number) = apple::file_number(file_name, "fan", "_target") else {
        return;
    };
    if apple::is_writable(&base_path.join(format_fan_target!(channel_number))).not() {
        return; // `fan_control` is off.
    }
    if read_limits(base_path, channel_number, io).await.is_none() {
        return;
    }
    // The duty shown for a fan is derived from its speed, so the speed must be readable.
    let rpm = fans::get_fan_rpm(
        io,
        base_path,
        &channel_number,
        &HwmonChannelCapabilities::empty(),
        None,
        None,
        false,
    )
    .await;
    if rpm.is_none() {
        return;
    }
    let caps = fan_caps
        .entry(channel_number)
        .or_insert(HwmonChannelCapabilities::empty());
    caps.insert(HwmonChannelCapabilities::APPLE_SMC);
    caps.insert(HwmonChannelCapabilities::FAN_WRITABLE);
    caps.insert(HwmonChannelCapabilities::RPM);
}

/// True when a fan has a target file the driver registered read-only, which it does unless
/// `fan_control` is set.
fn fan_control_is_off(base_path: &Path, channels: &[HwmonChannelInfo]) -> bool {
    channels
        .iter()
        .filter(|channel| channel.hwmon_type == HwmonChannelType::Fan)
        .any(|channel| apple::is_read_only(&base_path.join(format_fan_target!(channel.number))))
}

/// The fan's limits, or `None` when they cannot be read or do not form a range. Hardware lies,
/// and a duty can only be mapped onto a real range.
async fn read_limits(base_path: &Path, channel_number: u8, io: &DeviceIo) -> Option<FanLimits> {
    let min_rpm = apple::read_rpm(&base_path.join(format_fan_min!(channel_number)), io).await?;
    let max_rpm = apple::read_rpm(&base_path.join(format_fan_max!(channel_number)), io).await?;
    (min_rpm < max_rpm).then_some(FanLimits { min_rpm, max_rpm })
}

/// The targets that return a fan to the firmware, in the order they are written. Without known
/// limits there is no in-range target to write first, so only the `0` is left.
fn hand_back_targets(limits: Option<&FanLimits>) -> [Option<RPM>; 2] {
    [limits.map(|limits| limits.min_rpm), Some(FIRMWARE_TARGET)]
}

/// The targets a duty is written as, in the order they are written.
///
/// The driver ignores a bare `0`, and still reports success, unless it believes the fan is
/// manual. After a reload or an unclean stop it does not, while the SMC may still hold the fan.
/// So the first 0% of a session is the whole hand-back, and a plain `0` serves once synced.
fn duty_targets(limits: FanLimits, duty: Duty, is_synced: bool) -> [Option<RPM>; 2] {
    let rpm = rpm_from_duty(limits, duty);
    if rpm == FIRMWARE_TARGET {
        if is_synced.not() {
            return hand_back_targets(Some(&limits));
        }
    }
    [None, Some(rpm)]
}

/// Maps a duty onto the fan's range. 0% is the firmware target: the driver takes no speed below
/// the minimum, so the fan cannot be stopped from here.
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn rpm_from_duty(limits: FanLimits, duty: Duty) -> RPM {
    debug_assert!(limits.min_rpm < limits.max_rpm);
    if duty == 0 {
        return FIRMWARE_TARGET;
    }
    let span = f64::from(limits.max_rpm - limits.min_rpm);
    let rpm = f64::from(limits.min_rpm) + span * f64::from(duty.min(100)) / 100.;
    (rpm.round() as RPM).clamp(limits.min_rpm, limits.max_rpm)
}

/// The duty a speed stands for, the inverse of `rpm_from_duty`. A fan the firmware runs below
/// the minimum, or has stopped, reads as 0%.
fn duty_from_rpm(limits: FanLimits, rpm: RPM) -> f64 {
    debug_assert!(limits.min_rpm < limits.max_rpm);
    if rpm <= limits.min_rpm {
        return 0.;
    }
    if rpm >= limits.max_rpm {
        return 100.;
    }
    let span = f64::from(limits.max_rpm - limits.min_rpm);
    (f64::from(rpm - limits.min_rpm) * 100. / span).round()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use uuid::Uuid;

    /// The M1 Max's limits, from the reporter of issue 622.
    const M1_MAX: FanLimits = FanLimits {
        min_rpm: 1499,
        max_rpm: 5348,
    };

    /// A fake hwmon directory with one fan, as the driver shows it with `fan_control=1`.
    async fn seed_fan(fan: u8, target: &str) -> PathBuf {
        let dir = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
        cc_fs::create_dir_all(&dir).await.unwrap();
        for (file, value) in [
            (format_fan_target!(fan), target),
            (format!("fan{fan}_input"), "1520"),
            (format_fan_min!(fan), "1499"),
            (format_fan_max!(fan), "5348"),
        ] {
            cc_fs::write(dir.join(file), value.as_bytes().to_vec())
                .await
                .unwrap();
        }
        dir
    }

    async fn read_target(dir: &Path, fan: u8) -> String {
        cc_fs::read_sysfs(dir.join(format_fan_target!(fan)))
            .await
            .unwrap()
            .trim()
            .to_owned()
    }

    async fn handler_for(dir: &Path, io: &DeviceIo) -> (MacSmcHwmon, Vec<HwmonChannelInfo>) {
        let channels = MacSmcHwmon::init_fans(dir, io).await.unwrap();
        (MacSmcHwmon::new(dir, &channels, io).await, channels)
    }

    /// Goal: 1 to 100% spans the fan's own range and 0% is the firmware target. Method: the
    /// speeds the reporter measured on an M1 Max, plus a duty past 100.
    #[test]
    fn a_duty_spans_min_to_max_and_zero_is_the_firmware() {
        assert_eq!(rpm_from_duty(M1_MAX, 0), 0);
        assert_eq!(rpm_from_duty(M1_MAX, 1), 1537);
        assert_eq!(rpm_from_duty(M1_MAX, 25), 2461);
        assert_eq!(rpm_from_duty(M1_MAX, 50), 3424);
        assert_eq!(rpm_from_duty(M1_MAX, 100), 5348);
        assert_eq!(rpm_from_duty(M1_MAX, 255), 5348);
    }

    /// Goal: the duty shown is the inverse of the one written, and a speed outside the range
    /// stays within 0 to 100%. Method: the same speeds, plus a stopped fan and an overshoot.
    #[test]
    fn a_speed_reads_back_as_the_duty_that_set_it() {
        assert_eq!(duty_from_rpm(M1_MAX, 0), 0.);
        assert_eq!(duty_from_rpm(M1_MAX, 1499), 0.);
        assert_eq!(duty_from_rpm(M1_MAX, 1537), 1.);
        assert_eq!(duty_from_rpm(M1_MAX, 3424), 50.);
        assert_eq!(duty_from_rpm(M1_MAX, 5348), 100.);
        assert_eq!(duty_from_rpm(M1_MAX, 6000), 100.);
    }

    /// Goal: a hand-back syncs the driver with an in-range target before the `0`, and still
    /// writes the `0` when the limits are unknown. Method: the write order itself, since a
    /// plain file only keeps the last value.
    #[test]
    fn a_hand_back_writes_the_minimum_then_zero() {
        assert_eq!(hand_back_targets(Some(&M1_MAX)), [Some(1499), Some(0)]);
        assert_eq!(hand_back_targets(None), [None, Some(0)]);
    }

    /// Goal: a fan is controllable from its target, speed and limits alone, with no
    /// `fanN_manual` file, which this driver never has. Method: the files of an M1 Max fan.
    #[test]
    #[serial]
    fn a_fan_with_a_writable_target_is_controllable() {
        cc_fs::test_runtime(async {
            let dir = seed_fan(1, "0").await;
            let io = DeviceIo::default();

            let (handler, channels) = handler_for(&dir, &io).await;

            let off = fan_control_is_off(&dir, &channels);
            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].name, "fan1");
            assert!(channels[0].caps.is_apple_smc());
            assert!(channels[0].caps.is_fan_controllable());
            assert!(channels[0].caps.has_rpm());
            assert!(channels[0].caps.has_pwm().not());
            assert_eq!(handler.fans.get(&1), Some(&M1_MAX));
            assert!(off.not());
        });
    }

    /// Goal: with `fan_control` off the fan is still shown, read-only, and that state is
    /// recognised so it can be explained. Method: the same fan with a read-only target.
    #[test]
    #[serial]
    fn a_fan_with_a_read_only_target_is_shown_but_not_controllable() {
        cc_fs::test_runtime(async {
            let dir = seed_fan(1, "0").await;
            cc_fs::set_permissions(dir.join("fan1_target"), Permissions::from_mode(0o444))
                .await
                .unwrap();
            let io = DeviceIo::default();

            let (handler, channels) = handler_for(&dir, &io).await;

            let off = fan_control_is_off(&dir, &channels);
            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert_eq!(channels.len(), 1);
            assert!(channels[0].caps.has_rpm());
            assert!(channels[0].caps.is_apple_smc().not());
            assert!(channels[0].caps.is_fan_controllable().not());
            assert!(handler.fans.is_empty());
            assert!(off);
        });
    }

    /// Goal: limits that do not form a range leave the fan read-only instead of mapping duties
    /// onto nonsense. Method: a minimum above the maximum, and a missing maximum.
    #[test]
    #[serial]
    fn a_fan_without_a_real_range_is_not_controllable() {
        cc_fs::test_runtime(async {
            let inverted = seed_fan(1, "0").await;
            cc_fs::write(inverted.join("fan1_max"), b"1000".to_vec())
                .await
                .unwrap();
            let unbounded = seed_fan(1, "0").await;
            cc_fs::remove_file(unbounded.join("fan1_max"))
                .await
                .unwrap();
            let io = DeviceIo::default();

            let inverted_fans = MacSmcHwmon::init_fans(&inverted, &io).await.unwrap();
            let unbounded_fans = MacSmcHwmon::init_fans(&unbounded, &io).await.unwrap();

            cc_fs::remove_dir_all(&inverted).await.unwrap();
            cc_fs::remove_dir_all(&unbounded).await.unwrap();
            assert_eq!(inverted_fans.len(), 1);
            assert!(inverted_fans[0].caps.is_fan_controllable().not());
            assert_eq!(unbounded_fans.len(), 1);
            assert!(unbounded_fans[0].caps.is_fan_controllable().not());
        });
    }

    /// Goal: a duty is written as a speed within the fan's range, and 0% as the firmware
    /// target. Method: 50% then 0% on a fake fan, reading the target back each time.
    #[test]
    #[serial]
    fn a_duty_is_written_as_a_target_and_zero_hands_the_fan_back() {
        cc_fs::test_runtime(async {
            let dir = seed_fan(1, "0").await;
            let io = DeviceIo::default();
            let (handler, _) = handler_for(&dir, &io).await;

            let half = handler.set_fan_duty(1, 50, &io).await;
            let half_target = read_target(&dir, 1).await;
            let zero = handler.set_fan_duty(1, 0, &io).await;
            let zero_target = read_target(&dir, 1).await;
            let unknown_fan = handler.set_fan_duty(2, 50, &io).await;

            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert!(half.is_ok());
            assert_eq!(half_target, "3424");
            assert!(zero.is_ok());
            assert_eq!(zero_target, "0");
            assert!(handler.hand_back_logged.get());
            assert!(unknown_fan.is_err());
        });
    }

    /// Goal: the first 0% of a session is the whole hand-back, since the driver may have
    /// forgotten a fan the SMC still holds, and a plain `0` follows once an in-range target has
    /// landed. A hand-back that failed is written in full again. Method: the targets planned
    /// before each duty on fake fans, since a plain file only keeps the last value: 0% twice,
    /// 0% after 50%, and 0% twice on a target no write can land on.
    #[test]
    #[serial]
    fn the_first_zero_of_a_session_is_a_whole_hand_back() {
        cc_fs::test_runtime(async {
            let io = DeviceIo::default();
            let planned = |handler: &MacSmcHwmon| duty_targets(M1_MAX, 0, handler.is_synced(1));
            let idle_dir = seed_fan(1, "3000").await;
            let (idle, _) = handler_for(&idle_dir, &io).await;
            let driven_dir = seed_fan(1, "0").await;
            let (driven, _) = handler_for(&driven_dir, &io).await;
            let broken_dir = seed_fan(1, "3000").await;
            let (broken, _) = handler_for(&broken_dir, &io).await;
            cc_fs::remove_file(broken_dir.join("fan1_target"))
                .await
                .unwrap();
            cc_fs::create_dir_all(broken_dir.join("fan1_target"))
                .await
                .unwrap();

            let first_zero = planned(&idle);
            let first_result = idle.set_fan_duty(1, 0, &io).await;
            let repeated_zero = planned(&idle);
            let repeated_result = idle.set_fan_duty(1, 0, &io).await;
            let idle_target = read_target(&idle_dir, 1).await;

            driven.set_fan_duty(1, 50, &io).await.unwrap();
            let zero_after_duty = planned(&driven);

            let failed_result = broken.set_fan_duty(1, 0, &io).await;
            let zero_after_failure = planned(&broken);

            for dir in [idle_dir, driven_dir, broken_dir] {
                cc_fs::remove_dir_all(&dir).await.unwrap();
            }
            assert_eq!(first_zero, [Some(1499), Some(0)]);
            assert!(first_result.is_ok());
            assert_eq!(repeated_zero, [None, Some(0)]);
            assert!(repeated_result.is_ok());
            assert_eq!(idle_target, "0");
            assert_eq!(zero_after_duty, [None, Some(0)]);
            assert!(failed_result.is_err());
            assert_eq!(zero_after_failure, [Some(1499), Some(0)]);
            assert_eq!(duty_targets(M1_MAX, 50, false), [None, Some(3424)]);
            assert_eq!(duty_targets(M1_MAX, 50, true), [None, Some(3424)]);
        });
    }

    /// Goal: returning a fan leaves the firmware target in place and touches no other file.
    /// Method: a fan held at 3000 rpm, then the directory listing after the hand-back.
    #[test]
    #[serial]
    fn a_hand_back_leaves_the_firmware_in_control() {
        cc_fs::test_runtime(async {
            let dir = seed_fan(1, "3000").await;
            let io = DeviceIo::default();
            let (handler, _) = handler_for(&dir, &io).await;

            let result = handler.set_to_auto_control(1, &io).await;

            let target = read_target(&dir, 1).await;
            let fan_min = cc_fs::read_sysfs(dir.join("fan1_min")).await.unwrap();
            let file_count = cc_fs::read_dir(&dir).unwrap().count();
            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert!(result.is_ok());
            assert_eq!(target, "0");
            assert_eq!(fan_min.trim(), "1499");
            assert_eq!(file_count, 4);
        });
    }

    /// Goal: one status carries the speed and the duty it stands for, a read-only fan carries
    /// the speed alone, and an unreadable speed leaves the channel out. Method: a controllable
    /// fan at a known speed, the same fan read as a read-only channel, then with its input gone.
    #[test]
    #[serial]
    fn a_status_carries_the_speed_and_its_duty() {
        cc_fs::test_runtime(async {
            let dir = seed_fan(1, "3424").await;
            cc_fs::write(dir.join("fan1_input"), b"3424".to_vec())
                .await
                .unwrap();
            let io = DeviceIo::default();
            let (handler, channels) = handler_for(&dir, &io).await;
            let driver = HwmonDriverInfo {
                path: dir.clone(),
                channels: channels.clone(),
                ..Default::default()
            };
            let read_only_channel = HwmonChannelInfo {
                caps: HwmonChannelCapabilities::RPM,
                ..channels[0].clone()
            };

            let controllable = handler.read_one_fan_status(&driver, &channels[0]).await;
            let read_only = handler
                .read_one_fan_status(&driver, &read_only_channel)
                .await;
            cc_fs::remove_file(dir.join("fan1_input")).await.unwrap();
            let unreadable = handler.read_one_fan_status(&driver, &channels[0]).await;

            cc_fs::remove_dir_all(&dir).await.unwrap();
            let controllable = controllable.unwrap();
            assert_eq!(controllable.rpm, Some(3424));
            assert_eq!(controllable.duty, Some(50.));
            let read_only = read_only.unwrap();
            assert_eq!(read_only.rpm, Some(3424));
            assert_eq!(read_only.duty, None);
            assert!(unreadable.is_none());
        });
    }
}
