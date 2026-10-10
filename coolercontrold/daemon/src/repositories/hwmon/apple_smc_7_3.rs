// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Fan control on Intel Macs through the `applesmc` driver, as it is from Linux 7.3.
//!
//! Linux 7.3 moved the driver onto the standard hwmon files: `fanN_output` became
//! `fanN_target`, and `fanN_manual` became `pwmN_enable` (1 manual, 2 automatic). There is
//! still no `pwmN`, so a duty is written as a speed. The rest is what the module for the older
//! driver does, under the new file names:
//!
//! - manual: `fanN_min` to 0, so a speed below the firmware's floor is honoured, then
//!   `pwmN_enable` to 1.
//! - duty: that share of `fanN_max` as `fanN_target`, so 0% asks for 0 rpm.
//! - automatic: `fanN_min` back to its value at startup, then `pwmN_enable` to 2.
//!
//! See: `https://github.com/torvalds/linux/blob/master/drivers/hwmon/applesmc.c`

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
use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};

const PWM_ENABLE_MANUAL: u32 = 1;
const PWM_ENABLE_AUTOMATIC: u32 = 2;
/// The highest target ever written, the cap the module for the older driver applies.
const TARGET_RPM_MAX: RPM = 6_500;
macro_rules! format_fan_target { ($($arg:tt)*) => {{ format!("fan{}_target", $($arg)*) }}; }
macro_rules! format_fan_min { ($($arg:tt)*) => {{ format!("fan{}_min", $($arg)*) }}; }
macro_rules! format_fan_max { ($($arg:tt)*) => {{ format!("fan{}_max", $($arg)*) }}; }
macro_rules! format_pwm_enable { ($($arg:tt)*) => {{ format!("pwm{}_enable", $($arg)*) }}; }

// The driver rejects a speed of 0x4000 or more.
const _: () = assert!(TARGET_RPM_MAX < 0x4000);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct FanInfo {
    max_rpm: RPM,
    /// `fanN_min` as it was found, restored when the fan returns to automatic.
    startup_min_rpm: RPM,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppleSmc73 {
    path: PathBuf,
    /// Every controllable fan, by fan number.
    fans: HashMap<u8, FanInfo>,
    /// Set once the first failed `fanN_min` write has been logged.
    #[serde(skip)]
    min_failure_logged: Cell<bool>,
}

impl AppleSmc73 {
    pub async fn new(path: &Path, channels: &[HwmonChannelInfo], io: &DeviceIo) -> Self {
        let mut fans = HashMap::with_capacity(channels.len());
        for channel in channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            if channel.caps.is_apple_smc().not() {
                continue;
            }
            match read_fan_info(path, channel.number, io).await {
                Some(fan) => {
                    fans.insert(channel.number, fan);
                }
                // Detection read these moments ago, so this is a driver fault. Duty writes to
                // the fan are refused from here on, which names it again.
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
            min_failure_logged: Cell::new(false),
        }
    }

    pub async fn read_one_fan_status(
        &self,
        driver: &HwmonDriverInfo,
        channel: &HwmonChannelInfo,
    ) -> Option<ChannelStatus> {
        apple::read_fan_status(driver, channel, |rpm| {
            self.fans
                .get(&channel.number)
                .map_or(0., |fan| duty_from_rpm(fan.max_rpm, rpm))
        })
        .await
    }

    pub async fn set_fan_duty(&self, channel_number: u8, duty: Duty, io: &DeviceIo) -> Result<()> {
        let fan = self.fans.get(&channel_number).with_context(|| {
            format!(
                "No speed limits are known for fan{channel_number} at {}",
                self.path.display()
            )
        })?;
        let target_path = self.path.join(format_fan_target!(channel_number));
        apple::write_number(&target_path, rpm_from_duty(fan.max_rpm, duty), io).await
    }

    /// Takes the fan over. Its minimum goes to 0 first, or the firmware would hold the fan at
    /// its own floor whatever target is written.
    pub async fn set_to_manual_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        self.write_mode(channel_number, Some(0), PWM_ENABLE_MANUAL, io)
            .await
    }

    /// Returns the fan to the firmware, with the minimum it had at startup.
    pub async fn set_to_auto_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        let startup_min_rpm = self
            .fans
            .get(&channel_number)
            .map(|fan| fan.startup_min_rpm);
        self.write_mode(channel_number, startup_min_rpm, PWM_ENABLE_AUTOMATIC, io)
            .await
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

    /// Writes the fan's minimum, when one is known, and then its mode.
    ///
    /// Only the mode decides the result, as in the module for the older driver: a fan whose
    /// minimum cannot be written is still switched, and must still be given its duty.
    async fn write_mode(
        &self,
        channel_number: u8,
        min_rpm: Option<RPM>,
        pwm_enable: u32,
        io: &DeviceIo,
    ) -> Result<()> {
        debug_assert!(pwm_enable == PWM_ENABLE_MANUAL || pwm_enable == PWM_ENABLE_AUTOMATIC);
        if let Some(min_rpm) = min_rpm {
            let min_path = self.path.join(format_fan_min!(channel_number));
            if let Err(err) = apple::write_number(&min_path, min_rpm, io).await {
                self.log_first_min_failure(&err);
            }
        }
        let enable_path = self.path.join(format_pwm_enable!(channel_number));
        apple::write_number(&enable_path, pwm_enable, io).await
    }

    /// Said once: every later switch would fail the same way, and there is nothing to act on.
    fn log_first_min_failure(&self, err: &anyhow::Error) {
        if self.min_failure_logged.replace(true) {
            return;
        }
        info!("{err}. The firmware's minimum fan speed stays in force.");
    }
}

/// Marks the fan behind a `fanN_target` file as controllable, when its mode can be switched and
/// its speed and limits can be read.
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
        return;
    }
    if apple::is_writable(&base_path.join(format_pwm_enable!(channel_number))).not() {
        return;
    }
    if read_fan_info(base_path, channel_number, io).await.is_none() {
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

/// The fan's maximum and current minimum, or `None` when either cannot be read or there is no
/// maximum to map a duty onto.
async fn read_fan_info(base_path: &Path, channel_number: u8, io: &DeviceIo) -> Option<FanInfo> {
    let max_rpm = apple::read_rpm(&base_path.join(format_fan_max!(channel_number)), io).await?;
    let startup_min_rpm =
        apple::read_rpm(&base_path.join(format_fan_min!(channel_number)), io).await?;
    (max_rpm > 0).then_some(FanInfo {
        max_rpm,
        startup_min_rpm,
    })
}

/// The speed a duty stands for: that share of the fan's maximum, in steps of 10 rpm. The same
/// arithmetic as the module for the older driver, so a duty means the same speed on both.
#[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
fn rpm_from_duty(max_rpm: RPM, duty: Duty) -> RPM {
    let tens_of_rpm = (f64::from(duty.min(100)) * f64::from(max_rpm) / 1_000.).round();
    ((tens_of_rpm * 10.) as RPM).min(TARGET_RPM_MAX)
}

/// The duty a speed stands for, the inverse of `rpm_from_duty`.
fn duty_from_rpm(max_rpm: RPM, rpm: RPM) -> f64 {
    debug_assert!(max_rpm > 0);
    if rpm >= max_rpm {
        return 100.;
    }
    let tenths_of_percent = (f64::from(rpm) * 1_000. / f64::from(max_rpm)).round();
    (tenths_of_percent / 10.).round()
}

#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use uuid::Uuid;

    /// The `fanN_max` of the MacBookPro9,2 fan, from the reporter of issue 622.
    const MAX_RPM: RPM = 6200;

    /// A fake hwmon directory with one fan, as the driver shows it from Linux 7.3: in automatic
    /// mode at the firmware's floor.
    async fn seed_fan() -> PathBuf {
        let dir = PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()));
        cc_fs::create_dir_all(&dir).await.unwrap();
        for (file, value) in [
            ("fan1_target", "2000"),
            ("fan1_input", "3100"),
            ("fan1_min", "2000"),
            ("fan1_max", "6200"),
            ("pwm1_enable", "2"),
        ] {
            cc_fs::write(dir.join(file), value.as_bytes().to_vec())
                .await
                .unwrap();
        }
        dir
    }

    async fn read(dir: &Path, file: &str) -> String {
        cc_fs::read_sysfs(dir.join(file))
            .await
            .unwrap()
            .trim()
            .to_owned()
    }

    async fn handler_for(dir: &Path, io: &DeviceIo) -> (AppleSmc73, Vec<HwmonChannelInfo>) {
        let channels = AppleSmc73::init_fans(dir, io).await.unwrap();
        (AppleSmc73::new(dir, &channels, io).await, channels)
    }

    /// Goal: a duty means the same speed as on the older driver, 0% is 0 rpm, and the cap
    /// holds. Method: the speeds the reporter measured on a MacBookPro9,2 with the older
    /// driver, plus a fan faster than the cap.
    #[test]
    fn a_duty_is_a_share_of_the_maximum() {
        assert_eq!(rpm_from_duty(MAX_RPM, 0), 0);
        assert_eq!(rpm_from_duty(MAX_RPM, 30), 1860);
        assert_eq!(rpm_from_duty(MAX_RPM, 50), 3100);
        assert_eq!(rpm_from_duty(MAX_RPM, 70), 4340);
        assert_eq!(rpm_from_duty(MAX_RPM, 100), 6200);
        assert_eq!(rpm_from_duty(MAX_RPM, 255), 6200);
        assert_eq!(rpm_from_duty(9000, 100), TARGET_RPM_MAX);
    }

    /// Goal: the duty shown is the inverse of the one written and stays within 0 to 100%.
    /// Method: the same speeds, a stopped fan, and an overshoot.
    #[test]
    fn a_speed_reads_back_as_the_duty_that_set_it() {
        assert_eq!(duty_from_rpm(MAX_RPM, 0), 0.);
        assert_eq!(duty_from_rpm(MAX_RPM, 1860), 30.);
        assert_eq!(duty_from_rpm(MAX_RPM, 3100), 50.);
        assert_eq!(duty_from_rpm(MAX_RPM, 4340), 70.);
        assert_eq!(duty_from_rpm(MAX_RPM, 6200), 100.);
        assert_eq!(duty_from_rpm(MAX_RPM, 7000), 100.);
    }

    /// Goal: a fan is controllable from the files Linux 7.3 gives it, with no `fanN_manual` or
    /// `fanN_output`. Method: the files of a MacBookPro9,2 fan under the newer driver.
    #[test]
    #[serial]
    fn a_fan_with_a_target_and_a_mode_is_controllable() {
        cc_fs::test_runtime(async {
            let dir = seed_fan().await;
            let io = DeviceIo::default();

            let (handler, channels) = handler_for(&dir, &io).await;

            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].name, "fan1");
            assert!(channels[0].caps.is_apple_smc());
            assert!(channels[0].caps.is_fan_controllable());
            assert!(channels[0].caps.has_rpm());
            assert!(channels[0].caps.has_pwm().not());
            let fan = FanInfo {
                max_rpm: 6200,
                startup_min_rpm: 2000,
            };
            assert_eq!(handler.fans.get(&1), Some(&fan));
        });
    }

    /// Goal: a fan whose mode cannot be switched, or that reports no maximum, is shown
    /// read-only. Method: a read-only `pwm1_enable`, and a `fan1_max` of 0.
    #[test]
    #[serial]
    fn a_fan_that_cannot_be_switched_or_scaled_is_not_controllable() {
        cc_fs::test_runtime(async {
            let fixed_mode = seed_fan().await;
            cc_fs::set_permissions(
                fixed_mode.join("pwm1_enable"),
                Permissions::from_mode(0o444),
            )
            .await
            .unwrap();
            let no_maximum = seed_fan().await;
            cc_fs::write(no_maximum.join("fan1_max"), b"0".to_vec())
                .await
                .unwrap();
            let io = DeviceIo::default();

            let fixed_mode_fans = AppleSmc73::init_fans(&fixed_mode, &io).await.unwrap();
            let no_maximum_fans = AppleSmc73::init_fans(&no_maximum, &io).await.unwrap();

            cc_fs::remove_dir_all(&fixed_mode).await.unwrap();
            cc_fs::remove_dir_all(&no_maximum).await.unwrap();
            assert_eq!(fixed_mode_fans.len(), 1);
            assert!(fixed_mode_fans[0].caps.has_rpm());
            assert!(fixed_mode_fans[0].caps.is_fan_controllable().not());
            assert_eq!(no_maximum_fans.len(), 1);
            assert!(no_maximum_fans[0].caps.is_fan_controllable().not());
        });
    }

    /// Goal: the whole cycle writes what the older driver's module writes, under the new file
    /// names. Method: take the fan, set 50% then 0%, and return it, reading the files after
    /// each step.
    #[test]
    #[serial]
    fn a_fan_is_taken_driven_and_returned() {
        cc_fs::test_runtime(async {
            let dir = seed_fan().await;
            let io = DeviceIo::default();
            let (handler, _) = handler_for(&dir, &io).await;

            let taken = handler.set_to_manual_control(1, &io).await;
            let taken_min = read(&dir, "fan1_min").await;
            let taken_mode = read(&dir, "pwm1_enable").await;
            let half = handler.set_fan_duty(1, 50, &io).await;
            let half_target = read(&dir, "fan1_target").await;
            let zero = handler.set_fan_duty(1, 0, &io).await;
            let zero_target = read(&dir, "fan1_target").await;
            let unknown_fan = handler.set_fan_duty(2, 50, &io).await;
            let returned = handler.set_to_auto_control(1, &io).await;
            let returned_min = read(&dir, "fan1_min").await;
            let returned_mode = read(&dir, "pwm1_enable").await;

            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert!(taken.is_ok());
            assert_eq!(taken_min, "0");
            assert_eq!(taken_mode, "1");
            assert!(half.is_ok());
            assert_eq!(half_target, "3100");
            assert!(zero.is_ok());
            assert_eq!(zero_target, "0");
            assert!(unknown_fan.is_err());
            assert!(returned.is_ok());
            assert_eq!(returned_min, "2000");
            assert_eq!(returned_mode, "2");
        });
    }

    /// Goal: a minimum that cannot be written does not fail the switch to manual or back, as
    /// with the older driver. A failed switch keeps the engine from ever setting a duty, which
    /// would leave the fan in manual at a stale target. Method: a controllable fan whose
    /// `fan1_min` becomes a directory, which no write can land on, then both switches and a
    /// duty.
    #[test]
    #[serial]
    fn a_failed_minimum_write_does_not_fail_the_mode_switch() {
        cc_fs::test_runtime(async {
            let dir = seed_fan().await;
            let io = DeviceIo::default();
            let (handler, _) = handler_for(&dir, &io).await;
            cc_fs::remove_file(dir.join("fan1_min")).await.unwrap();
            cc_fs::create_dir_all(dir.join("fan1_min")).await.unwrap();

            let taken = handler.set_to_manual_control(1, &io).await;
            let taken_mode = read(&dir, "pwm1_enable").await;
            let half = handler.set_fan_duty(1, 50, &io).await;
            let half_target = read(&dir, "fan1_target").await;
            let returned = handler.set_to_auto_control(1, &io).await;
            let returned_mode = read(&dir, "pwm1_enable").await;
            cc_fs::remove_file(dir.join("pwm1_enable")).await.unwrap();
            cc_fs::create_dir_all(dir.join("pwm1_enable"))
                .await
                .unwrap();
            let mode_failure = handler.set_to_manual_control(1, &io).await;

            cc_fs::remove_dir_all(&dir).await.unwrap();
            assert!(taken.is_ok(), "{taken:?}");
            assert_eq!(taken_mode, "1");
            assert!(half.is_ok());
            assert_eq!(half_target, "3100");
            assert!(returned.is_ok(), "{returned:?}");
            assert_eq!(returned_mode, "2");
            assert!(handler.min_failure_logged.get());
            assert!(mode_failure.is_err());
        });
    }

    /// Goal: one status carries the speed and the duty it stands for, and an unreadable speed
    /// leaves the channel out. Method: a fan at half its maximum, then with its input gone.
    #[test]
    #[serial]
    fn a_status_carries_the_speed_and_its_duty() {
        cc_fs::test_runtime(async {
            let dir = seed_fan().await;
            let io = DeviceIo::default();
            let (handler, channels) = handler_for(&dir, &io).await;
            let driver = HwmonDriverInfo {
                path: dir.clone(),
                channels: channels.clone(),
                ..Default::default()
            };

            let readable = handler.read_one_fan_status(&driver, &channels[0]).await;
            cc_fs::remove_file(dir.join("fan1_input")).await.unwrap();
            let unreadable = handler.read_one_fan_status(&driver, &channels[0]).await;

            cc_fs::remove_dir_all(&dir).await.unwrap();
            let readable = readable.unwrap();
            assert_eq!(readable.rpm, Some(3100));
            assert_eq!(readable.duty, Some(50.));
            assert!(unreadable.is_none());
        });
    }
}
