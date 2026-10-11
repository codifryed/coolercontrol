// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Picks the Apple fan driver behind a hwmon device and hands its fan work to that driver's
//! module.
//!
//! Each driver has its own sysfs files and its own way of taking and returning control, so each
//! has its own module. This file decides which module applies, remembers which fans the daemon
//! has taken, and holds the few file helpers the modules share.

use crate::cc_fs;
use crate::device::{ChannelStatus, Duty, RPM};
use crate::hardware_support::{self, ChannelDiagnosis};
use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
use crate::repositories::hwmon::apple_smc_7_3::AppleSmc73;
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::devices::{DEVICE_NAME_APPLE_SMC, DEVICE_NAME_MAC_SMC};
use crate::repositories::hwmon::fans;
use crate::repositories::hwmon::hwmon_repo::{
    HwmonChannelCapabilities, HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
};
use crate::repositories::hwmon::macsmc_hwmon::MacSmcHwmon;
use anyhow::{anyhow, Context, Result};
use log::log_enabled;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::Path;
use std::rc::Rc;

/// Which Apple fan driver a hwmon device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleDriver {
    /// `applesmc` on Intel Macs, with the `fanN_manual` files it had before Linux 7.3.
    IntelLegacy,
    /// `applesmc` on Intel Macs, with the `pwmN_enable` files it has from Linux 7.3.
    IntelLinux73,
    /// `macsmc_hwmon` on Apple Silicon.
    Silicon,
}

/// The fan handler of one Apple device, built once its channels are settled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppleFans {
    handler: AppleHandler,
    /// Fans the daemon has taken over. Shutdown returns only these, so a fan left to the
    /// firmware or to another tool is not written to.
    #[serde(skip)]
    taken_fans: RefCell<HashSet<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum AppleHandler {
    IntelLegacy(AppleMacSMC),
    IntelLinux73(AppleSmc73),
    Silicon(MacSmcHwmon),
}

/// The Apple driver behind a hwmon device, or `None` for every other device.
pub fn detect(device_name: &str, base_path: &Path) -> Option<AppleDriver> {
    match device_name {
        DEVICE_NAME_APPLE_SMC => Some(detect_intel(base_path)),
        DEVICE_NAME_MAC_SMC => Some(AppleDriver::Silicon),
        _ => None,
    }
}

/// Which generation of `applesmc` this is.
///
/// Decided by the files the driver exposes and never by the kernel version, so a backported
/// driver is handled right. Only a positive match is the newer driver: anything else stays with
/// the module that has always handled `applesmc`.
fn detect_intel(base_path: &Path) -> AppleDriver {
    let Ok(entries) = cc_fs::read_dir(base_path) else {
        return AppleDriver::IntelLegacy;
    };
    let mut has_pwm_enable = false;
    for entry in entries.flatten() {
        let os_file_name = entry.file_name();
        let Some(file_name) = os_file_name.to_str() else {
            continue;
        };
        if file_number(file_name, "fan", "_manual").is_some() {
            return AppleDriver::IntelLegacy;
        }
        if file_number(file_name, "pwm", "_enable").is_some() {
            has_pwm_enable = true;
        }
    }
    if has_pwm_enable {
        AppleDriver::IntelLinux73
    } else {
        AppleDriver::IntelLegacy
    }
}

impl AppleDriver {
    /// Every fan the driver exposes, including ones the user has disabled. An error is the
    /// caller's to log: the repository does, the hardware report does not.
    pub async fn init_fans(self, base_path: &Path, io: &DeviceIo) -> Result<Vec<HwmonChannelInfo>> {
        let mut fans = match self {
            Self::IntelLegacy => AppleMacSMC::init_fans(base_path, io).await,
            Self::IntelLinux73 => AppleSmc73::init_fans(base_path, io).await,
            Self::Silicon => MacSmcHwmon::init_fans(base_path, io).await,
        }?;
        for fan in &mut fans {
            // A fan that can be driven has no read-only control file, so only the rest are asked.
            if fan.caps.is_fan_controllable() {
                continue;
            }
            let read_only_files = self.read_only_control_files(base_path, fan.number);
            if read_only_files.is_empty().not() {
                fan.caps
                    .insert(HwmonChannelCapabilities::APPLE_SMC_READ_ONLY);
            }
        }
        Ok(fans)
    }

    /// The files the daemon writes to control one fan, as the text either side of its number.
    fn control_files(self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::IntelLegacy => &[("fan", "_output"), ("fan", "_manual")],
            Self::IntelLinux73 => &[("fan", "_target"), ("pwm", "_enable")],
            Self::Silicon => &[("fan", "_target")],
        }
    }

    /// The names of one fan's control files that the driver registered read-only. Any one of
    /// them keeps the fan from being controlled.
    pub fn read_only_control_files(self, base_path: &Path, channel_number: u8) -> Vec<String> {
        self.control_files()
            .iter()
            .map(|(prefix, suffix)| format!("{prefix}{channel_number}{suffix}"))
            .filter(|file_name| is_read_only(&base_path.join(file_name)))
            .collect()
    }

    /// The driver's own files for one fan, as `fanN_<suffix>`. They carry the state `pwmN` and
    /// `pwmN_enable` carry elsewhere, so the hardware report prints them.
    pub fn fan_attribute_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::IntelLegacy => &["output", "manual", "min", "max"],
            Self::IntelLinux73 | Self::Silicon => &["target", "min", "max"],
        }
    }

    /// The handler for the fans that were kept. Repository init only: this is also where a
    /// device says why its fans cannot be controlled.
    pub async fn fans(
        self,
        base_path: &Path,
        channels: &Vec<HwmonChannelInfo>,
        io: &DeviceIo,
    ) -> AppleFans {
        let handler = match self {
            Self::IntelLegacy => {
                AppleMacSMC::log_fan_limits(base_path, channels).await;
                AppleHandler::IntelLegacy(AppleMacSMC::new(base_path, channels).await)
            }
            Self::IntelLinux73 => {
                AppleHandler::IntelLinux73(AppleSmc73::new(base_path, channels, io).await)
            }
            Self::Silicon => {
                MacSmcHwmon::log_fan_control_off(base_path, channels);
                AppleHandler::Silicon(MacSmcHwmon::new(base_path, channels, io).await)
            }
        };
        AppleFans {
            handler,
            taken_fans: RefCell::new(HashSet::with_capacity(channels.len())),
        }
    }
}

impl AppleFans {
    pub async fn read_one_fan_status(
        &self,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<ChannelStatus> {
        match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.read_one_fan_status(driver, channel).await,
            AppleHandler::IntelLinux73(smc) => smc.read_one_fan_status(driver, channel).await,
            AppleHandler::Silicon(smc) => smc.read_one_fan_status(driver, channel).await,
        }
    }

    pub async fn read_one_fan_rpm_only(
        &self,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<Option<u32>> {
        match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.read_one_fan_rpm_only(driver, channel).await,
            AppleHandler::IntelLinux73(_) | AppleHandler::Silicon(_) => {
                fans::read_one_fan_rpm_only(driver, channel).await
            }
        }
    }

    pub async fn set_fan_duty(&self, channel_number: u8, duty: Duty, io: &DeviceIo) -> Result<()> {
        self.mark_taken(channel_number);
        match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.set_fan_duty(channel_number, duty, io).await,
            AppleHandler::IntelLinux73(smc) => smc.set_fan_duty(channel_number, duty, io).await,
            AppleHandler::Silicon(smc) => smc.set_fan_duty(channel_number, duty, io).await,
        }
    }

    pub async fn set_to_manual_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        self.mark_taken(channel_number);
        match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.set_to_manual_control(channel_number, io).await,
            AppleHandler::IntelLinux73(smc) => smc.set_to_manual_control(channel_number, io).await,
            // Nothing to switch: the first in-range target is what takes the fan.
            AppleHandler::Silicon(_) => Ok(()),
        }
    }

    /// Returns the fan to automatic control, whoever holds it. This is the user's reset, so it
    /// also recovers a fan an earlier run left in manual.
    pub async fn set_to_auto_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        // The Apple Silicon hand-back writes a speed first, which takes a fan the firmware was
        // running. If the rest then fails the fan is held, so shutdown must try it again.
        if matches!(self.handler, AppleHandler::Silicon(_)) {
            self.mark_taken(channel_number);
        }
        let result = match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.set_to_auto_control(channel_number, io).await,
            AppleHandler::IntelLinux73(smc) => smc.set_to_auto_control(channel_number, io).await,
            AppleHandler::Silicon(smc) => smc.set_to_auto_control(channel_number, io).await,
        };
        if result.is_ok() {
            self.taken_fans.borrow_mut().remove(&channel_number);
        }
        result
    }

    /// Returns the fan to automatic control if the daemon took it, for shutdown. A fan the
    /// daemon never drove is left exactly as it is.
    pub async fn release_fan(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        if self.is_taken(channel_number) {
            self.set_to_auto_control(channel_number, io).await
        } else {
            Ok(())
        }
    }

    /// Whether the duty read back can stand in for the last one written, which is what lets the
    /// writer skip a write that would change nothing.
    ///
    /// Not on Apple Silicon: the duty is derived from the speed, so a fan held at its minimum
    /// reads 0% exactly like one the firmware runs. Skipping the 0% write would leave it held.
    pub fn read_back_tracks_target(&self) -> bool {
        match &self.handler {
            AppleHandler::IntelLegacy(_) | AppleHandler::IntelLinux73(_) => true,
            AppleHandler::Silicon(_) => false,
        }
    }

    fn is_taken(&self, channel_number: u8) -> bool {
        self.taken_fans.borrow().contains(&channel_number)
    }

    /// Marked before the write, since a write that fails part way has still changed the fan.
    fn mark_taken(&self, channel_number: u8) {
        self.taken_fans.borrow_mut().insert(channel_number);
    }
}

/// The number in a `<prefix>N<suffix>` file name, such as `fan2_target`, or `None` for any
/// other file.
pub fn file_number(file_name: &str, prefix: &str, suffix: &str) -> Option<u8> {
    file_name
        .strip_prefix(prefix)?
        .strip_suffix(suffix)?
        .parse()
        .ok()
}

/// The diagnosis of an Apple driver's fan, or `None` for any other device.
///
/// These fans are judged by the driver's own control files and carry no pwm evidence: no Apple
/// driver has a `pwmN`, so its absence says nothing about them.
pub fn diagnose_fan(device_name: &str, channel: &HwmonChannelInfo) -> Option<ChannelDiagnosis> {
    if matches!(device_name, DEVICE_NAME_APPLE_SMC | DEVICE_NAME_MAC_SMC).not() {
        return None;
    }
    debug_assert!(
        (channel.caps.is_fan_controllable() && channel.caps.is_apple_smc_read_only()).not(),
        "a fan with a read-only control file cannot be controllable"
    );
    if channel.caps.is_apple_smc_read_only() {
        return Some(hardware_support::diagnose_read_only_driver_channel());
    }
    Some(hardware_support::diagnose_driver_channel(
        channel.caps.is_fan_controllable(),
    ))
}

/// Whether the driver registered the file read-only. A missing file is not read-only.
pub fn is_read_only(path: &Path) -> bool {
    cc_fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly())
}

/// Whether the driver registered the file writable. Running as root, the mode bits are the
/// whole answer.
pub fn is_writable(path: &Path) -> bool {
    cc_fs::metadata(path).is_ok_and(|metadata| metadata.permissions().readonly().not())
}

pub async fn read_rpm(path: &Path, io: &DeviceIo) -> Option<RPM> {
    io.read_one(None, path)
        .await
        .and_then(fans::check_parsing_32)
        .ok()
}

pub async fn write_number(path: &Path, value: u32, io: &DeviceIo) -> Result<()> {
    io.write_value(path, value.to_string().into_bytes())
        .await
        .map_err(|err| anyhow!("Unable to write {value} to {}: {err}", path.display()))
}

/// Every fan in the directory of a driver that controls its fans through `fanN_target`.
///
/// `files_allow_control` is the driver's own answer for one such fan. The fan is controllable
/// when they do and its speed reads, since the duty shown for it is derived from that speed.
/// Every other fan is still listed with its speed.
pub async fn detect_target_fans(
    base_path: &Path,
    io: &DeviceIo,
    files_allow_control: impl AsyncFn(u8) -> bool,
) -> Result<Vec<HwmonChannelInfo>> {
    let no_caps = HwmonChannelCapabilities::empty();
    let mut fan_caps = HashMap::new();
    for entry in cc_fs::read_dir(base_path)? {
        let os_file_name = entry?.file_name();
        let file_name = os_file_name.to_str().context("File Name should be a str")?;
        if let Some(channel_number) = file_number(file_name, "fan", "_target") {
            let is_controllable = files_allow_control(channel_number).await
                && fans::get_fan_rpm(io, base_path, &channel_number, &no_caps, None, None, false)
                    .await
                    .is_some();
            if is_controllable {
                fan_caps
                    .entry(channel_number)
                    .or_insert(HwmonChannelCapabilities::empty())
                    .insert(
                        HwmonChannelCapabilities::APPLE_SMC
                            | HwmonChannelCapabilities::FAN_WRITABLE
                            | HwmonChannelCapabilities::RPM,
                    );
            }
        }
        fans::detect_rpm(base_path, file_name, &mut fan_caps, io).await?;
    }
    Ok(fan_channels(base_path, fan_caps).await)
}

/// The channels of the fans found, in fan order. Every one has a readable speed.
pub async fn fan_channels(
    base_path: &Path,
    fan_caps: HashMap<u8, HwmonChannelCapabilities>,
) -> Vec<HwmonChannelInfo> {
    let mut fans = Vec::with_capacity(fan_caps.len());
    for (channel_number, caps) in fan_caps {
        debug_assert!(caps.has_rpm());
        fans.push(HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number: channel_number,
            name: fans::get_fan_channel_name(channel_number),
            label: fans::get_fan_channel_label(base_path, &channel_number).await,
            rpm_path: Some(base_path.join(format!("fan{channel_number}_input"))),
            caps,
            ..Default::default()
        });
    }
    fans.sort_by_key(|fan| fan.number);
    fans
}

/// A fan's speed, and the duty that speed stands for when the fan is controllable.
///
/// One read serves both, as these drivers report no duty. `None` when the speed cannot be read,
/// so the failsafe can stand in for the whole channel.
pub async fn read_fan_status(
    driver: &HwmonDriverInfo,
    channel: &HwmonChannelInfo,
    duty_from_rpm: impl FnOnce(RPM) -> f64,
) -> Option<ChannelStatus> {
    debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
    debug_assert!(channel.caps.has_rpm());
    let rpm = fans::get_fan_rpm(
        &driver.io,
        &driver.path,
        &channel.number,
        &channel.caps,
        channel.rpm_path.as_deref(),
        channel.read_slot.rpm,
        log_enabled!(log::Level::Debug),
    )
    .await?;
    let duty = channel.caps.is_apple_smc().then(|| duty_from_rpm(rpm));
    Some(ChannelStatus {
        name: channel.name.clone(),
        rpm: Some(rpm),
        duty,
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware_support::ChannelVerdict;
    use crate::repositories::hwmon::hwmon_repo::{HwmonChannelCapabilities, HwmonChannelType};
    use serial_test::serial;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use uuid::Uuid;

    async fn seed(dir: &Path, files: &[(&str, &str)]) {
        cc_fs::create_dir_all(dir).await.unwrap();
        for (file, value) in files {
            cc_fs::write(dir.join(file), value.as_bytes().to_vec())
                .await
                .unwrap();
        }
    }

    async fn read(dir: &Path, file: &str) -> String {
        cc_fs::read_sysfs(dir.join(file))
            .await
            .unwrap()
            .trim()
            .to_owned()
    }

    fn apple_fan(number: u8, dir: &Path) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Fan,
            number,
            name: format!("fan{number}"),
            caps: HwmonChannelCapabilities::APPLE_SMC
                | HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::RPM,
            rpm_path: Some(dir.join(format!("fan{number}_input"))),
            ..Default::default()
        }
    }

    fn test_dir() -> PathBuf {
        PathBuf::from(format!("/tmp/coolercontrol-tests-{}", Uuid::new_v4()))
    }

    /// Goal: only a file of the exact shape names a fan. Method: a target file's neighbours in
    /// the directory, and names that only resemble it.
    #[test]
    fn only_a_matching_file_name_carries_a_fan_number() {
        assert_eq!(file_number("fan1_target", "fan", "_target"), Some(1));
        assert_eq!(file_number("fan12_target", "fan", "_target"), Some(12));
        assert_eq!(file_number("pwm2_enable", "pwm", "_enable"), Some(2));
        assert_eq!(file_number("fan1_input", "fan", "_target"), None);
        assert_eq!(file_number("fan_target", "fan", "_target"), None);
        assert_eq!(file_number("fanx_target", "fan", "_target"), None);
        assert_eq!(file_number("pwm1_target", "fan", "_target"), None);
        assert_eq!(file_number("fan300_target", "fan", "_target"), None);
    }

    /// Goal: only Apple drivers get an Apple handler. Method: the Apple names and an ordinary
    /// motherboard chip, with a directory that cannot be read.
    #[test]
    fn only_apple_drivers_are_detected() {
        let nowhere = Path::new("/nonexistent");
        assert_eq!(detect("applesmc", nowhere), Some(AppleDriver::IntelLegacy));
        assert_eq!(detect("macsmc_hwmon", nowhere), Some(AppleDriver::Silicon));
        // The platform driver's name, which no hwmon device carries.
        assert_eq!(detect("macsmc-hwmon", nowhere), None);
        assert_eq!(detect("nct6798", nowhere), None);
        assert_eq!(detect("", nowhere), None);
    }

    /// Goal: the two generations of `applesmc` are told apart by their files, and only a
    /// positive match is the newer one. Method: the fan files of each generation, a directory
    /// with both mode files, and one with neither. Other drivers' files do not matter.
    #[test]
    #[serial]
    fn applesmc_generations_are_told_apart_by_their_files() {
        cc_fs::test_runtime(async {
            let base = test_dir();
            let before_7_3 = base.join("before");
            let from_7_3 = base.join("from");
            let both = base.join("both");
            let neither = base.join("neither");
            let silicon = base.join("silicon");
            seed(&before_7_3, &[("fan1_manual", "0"), ("fan1_output", "0")]).await;
            seed(&from_7_3, &[("pwm1_enable", "2"), ("fan1_target", "0")]).await;
            seed(&both, &[("pwm1_enable", "2"), ("fan1_manual", "0")]).await;
            seed(&neither, &[("fan1_input", "2000")]).await;
            seed(&silicon, &[("pwm1_enable", "2"), ("fan1_target", "0")]).await;

            let detected = [
                detect("applesmc", &before_7_3),
                detect("applesmc", &from_7_3),
                detect("applesmc", &both),
                detect("applesmc", &neither),
                detect("macsmc_hwmon", &silicon),
                detect("nct6798", &from_7_3),
            ];

            cc_fs::remove_dir_all(&base).await.unwrap();
            assert_eq!(
                detected,
                [
                    Some(AppleDriver::IntelLegacy),
                    Some(AppleDriver::IntelLinux73),
                    Some(AppleDriver::IntelLegacy),
                    Some(AppleDriver::IntelLegacy),
                    Some(AppleDriver::Silicon),
                    None,
                ]
            );
        });
    }

    /// Goal: a driver is asked only about a fan with a target file, and its yes counts only
    /// when the fan's speed reads, while every fan with a speed is listed either way. Method:
    /// one fan per case, detected with a driver that allows all but the last.
    #[test]
    #[serial]
    fn a_target_fan_is_controllable_when_allowed_and_its_speed_reads() {
        cc_fs::test_runtime(async {
            let dir = test_dir();
            let files = [
                ("fan1_target", "0"),
                ("fan1_input", "1500"),
                ("fan2_target", "0"),
                ("fan3_input", "1500"),
                ("fan4_target", "0"),
                ("fan4_input", "1500"),
            ];
            seed(&dir, &files).await;
            let asked = RefCell::new(Vec::with_capacity(3));

            let fans = detect_target_fans(&dir, &DeviceIo::default(), async |channel_number| {
                asked.borrow_mut().push(channel_number);
                channel_number != 4
            })
            .await
            .unwrap();

            cc_fs::remove_dir_all(&dir).await.unwrap();
            let mut asked = asked.into_inner();
            asked.sort_unstable();
            assert_eq!(asked, [1, 2, 4]);
            let detected: Vec<(u8, bool)> = fans
                .iter()
                .map(|fan| (fan.number, fan.caps.is_fan_controllable()))
                .collect();
            assert_eq!(detected, [(1, true), (3, false), (4, false)]);
        });
    }

    /// Goal: a device that cannot be read is the caller's to report, since the repository logs
    /// it and the hardware report must not. Method: every driver against a directory that is
    /// missing, and against an empty one for contrast.
    #[test]
    #[serial]
    fn an_unreadable_device_is_an_error_for_the_caller() {
        cc_fs::test_runtime(async {
            let base = test_dir();
            let empty = base.join("empty");
            let missing = base.join("missing");
            seed(&empty, &[]).await;
            let io = DeviceIo::default();
            let drivers = [
                AppleDriver::IntelLegacy,
                AppleDriver::IntelLinux73,
                AppleDriver::Silicon,
            ];

            let mut results = Vec::with_capacity(drivers.len());
            for driver in drivers {
                let from_empty = driver.init_fans(&empty, &io).await;
                let from_missing = driver.init_fans(&missing, &io).await;
                results.push((driver, from_empty, from_missing));
            }

            cc_fs::remove_dir_all(&base).await.unwrap();
            for (driver, from_empty, from_missing) in results {
                assert!(from_empty.is_ok_and(|fans| fans.is_empty()), "{driver:?}");
                assert!(from_missing.is_err(), "{driver:?}");
            }
        });
    }

    /// The files of a fan the driver can control, and the control files among them.
    fn complete_fan(
        driver: AppleDriver,
    ) -> (
        &'static [(&'static str, &'static str)],
        &'static [&'static str],
    ) {
        match driver {
            AppleDriver::IntelLegacy => (
                &[
                    ("fan1_input", "2500"),
                    ("fan1_output", "2500"),
                    ("fan1_manual", "0"),
                    ("fan1_min", "600"),
                    ("fan1_max", "6200"),
                ],
                &["fan1_output", "fan1_manual"],
            ),
            AppleDriver::IntelLinux73 => (
                &[
                    ("fan1_input", "2500"),
                    ("fan1_target", "2500"),
                    ("pwm1_enable", "2"),
                    ("fan1_min", "600"),
                    ("fan1_max", "6200"),
                ],
                &["fan1_target", "pwm1_enable"],
            ),
            AppleDriver::Silicon => (
                &[
                    ("fan1_input", "1500"),
                    ("fan1_target", "0"),
                    ("fan1_min", "1499"),
                    ("fan1_max", "5348"),
                ],
                &["fan1_target"],
            ),
        }
    }

    /// Whether fan1 was detected controllable, whether it was recorded read-only, and which of
    /// its control files are read-only now.
    async fn fan1_state(
        driver: AppleDriver,
        dir: &Path,
        io: &DeviceIo,
    ) -> (bool, bool, Vec<String>) {
        let fans = driver.init_fans(dir, io).await.unwrap();
        assert_eq!(fans.len(), 1, "{driver:?}");
        (
            fans[0].caps.is_fan_controllable(),
            fans[0].caps.is_apple_smc_read_only(),
            driver.read_only_control_files(dir, 1),
        )
    }

    /// Goal: a fan the driver locked is told apart from one it offers no control for, since
    /// the two mean different things to the user. Method: for every driver a complete fan,
    /// then each of its control files read-only in turn, then each one missing.
    #[test]
    #[serial]
    fn a_read_only_control_file_is_recorded_on_the_fan() {
        cc_fs::test_runtime(async {
            let base = test_dir();
            let io = DeviceIo::default();
            let mut complete = Vec::new();
            let mut locked = Vec::new();
            let mut missing = Vec::new();
            for driver in [
                AppleDriver::IntelLegacy,
                AppleDriver::IntelLinux73,
                AppleDriver::Silicon,
            ] {
                let (files, control_files) = complete_fan(driver);
                let dir = base.join(format!("{driver:?}"));
                seed(&dir, files).await;
                complete.push((driver, fan1_state(driver, &dir, &io).await));
                for control_file in control_files {
                    let path = dir.join(control_file);
                    cc_fs::set_permissions(&path, Permissions::from_mode(0o444))
                        .await
                        .unwrap();
                    locked.push((driver, *control_file, fan1_state(driver, &dir, &io).await));
                    cc_fs::remove_file(&path).await.unwrap();
                    missing.push((driver, *control_file, fan1_state(driver, &dir, &io).await));
                    seed(&dir, files).await;
                }
            }

            cc_fs::remove_dir_all(&base).await.unwrap();
            for (driver, state) in complete {
                assert_eq!(state, (true, false, vec![]), "{driver:?}");
            }
            for (driver, file, state) in locked {
                let expected = (false, true, vec![file.to_string()]);
                assert_eq!(state, expected, "{driver:?} with {file} read-only");
            }
            for (driver, file, state) in missing {
                assert_eq!(state, (false, false, vec![]), "{driver:?} without {file}");
            }
        });
    }

    /// Goal: an Apple fan's verdict comes from its driver's own control and never carries pwm
    /// evidence, and no other device is judged this way. Method: a fan in each state under
    /// both Apple names, and the same fans under another name.
    #[test]
    fn an_apple_fan_is_diagnosed_without_pwm_evidence() {
        let fan = |caps: HwmonChannelCapabilities| HwmonChannelInfo {
            caps,
            ..apple_fan(1, Path::new("/unused"))
        };
        let states = [
            (
                apple_fan(1, Path::new("/unused")),
                ChannelVerdict::Controllable,
            ),
            (
                fan(HwmonChannelCapabilities::RPM | HwmonChannelCapabilities::APPLE_SMC_READ_ONLY),
                ChannelVerdict::PwmReadOnly,
            ),
            (
                fan(HwmonChannelCapabilities::RPM),
                ChannelVerdict::NotSupportedByDriver,
            ),
        ];

        for (channel, verdict) in &states {
            let expected = Some(ChannelDiagnosis {
                verdict: *verdict,
                evidence: None,
            });
            assert_eq!(diagnose_fan("applesmc", channel), expected);
            assert_eq!(diagnose_fan("macsmc_hwmon", channel), expected);
            assert_eq!(diagnose_fan("nct6798", channel), None);
        }
    }

    /// Goal: shutdown leaves a fan the daemon never drove alone, and returns one it took.
    /// Method: an `applesmc` fan another tool holds in manual. Releasing it writes nothing
    /// until the daemon has taken manual control itself, and nothing again once returned.
    #[test]
    #[serial]
    fn release_only_returns_fans_the_daemon_took() {
        cc_fs::test_runtime(async {
            let dir = test_dir();
            let files = [
                ("fan1_input", "3100"),
                ("fan1_min", "2000"),
                ("fan1_max", "6200"),
                ("fan1_manual", "0"),
            ];
            seed(&dir, &files).await;
            let io = DeviceIo::default();
            let apple_fans = AppleDriver::IntelLegacy
                .fans(&dir, &vec![apple_fan(1, &dir)], &io)
                .await;
            let held_by_another_tool = [("fan1_min", "0"), ("fan1_manual", "1")];
            seed(&dir, &held_by_another_tool).await;

            let untouched = apple_fans.release_fan(1, &io).await;
            let untouched_manual = read(&dir, "fan1_manual").await;
            let untouched_min = read(&dir, "fan1_min").await;

            apple_fans.set_to_manual_control(1, &io).await.unwrap();
            let taken = apple_fans.release_fan(1, &io).await;
            let taken_manual = read(&dir, "fan1_manual").await;
            let taken_min = read(&dir, "fan1_min").await;

            seed(&dir, &held_by_another_tool).await;
            let returned = apple_fans.release_fan(1, &io).await;
            let returned_manual = read(&dir, "fan1_manual").await;
            cc_fs::remove_dir_all(&dir).await.unwrap();

            assert!(untouched.is_ok());
            assert_eq!(untouched_manual, "1");
            assert_eq!(untouched_min, "0");
            assert!(taken.is_ok());
            assert_eq!(taken_manual, "0");
            assert_eq!(taken_min, "2000");
            assert!(returned.is_ok());
            assert_eq!(returned_manual, "1");
        });
    }

    /// Goal: an Apple Silicon reset that fails leaves the fan recorded as taken, so shutdown
    /// tries the hand-back again, and one that succeeds leaves nothing to return. Method: a fan
    /// the daemon never drove, reset while its target is a directory, which no write can land
    /// on. Then the file is put back and the fan released twice.
    #[test]
    #[serial]
    fn a_failed_silicon_reset_is_retried_at_shutdown() {
        cc_fs::test_runtime(async {
            let dir = test_dir();
            let (files, _) = complete_fan(AppleDriver::Silicon);
            seed(&dir, files).await;
            let io = DeviceIo::default();
            let apple_fans = AppleDriver::Silicon
                .fans(&dir, &vec![apple_fan(1, &dir)], &io)
                .await;
            let target_path = dir.join("fan1_target");
            cc_fs::remove_file(&target_path).await.unwrap();
            cc_fs::create_dir_all(&target_path).await.unwrap();

            let failed_reset = apple_fans.set_to_auto_control(1, &io).await;
            let taken_after_failure = apple_fans.is_taken(1);

            cc_fs::remove_dir_all(&target_path).await.unwrap();
            seed(&dir, &[("fan1_target", "1499")]).await;
            let retried = apple_fans.release_fan(1, &io).await;
            let retried_target = read(&dir, "fan1_target").await;
            let taken_after_retry = apple_fans.is_taken(1);

            seed(&dir, &[("fan1_target", "3000")]).await;
            let released_again = apple_fans.release_fan(1, &io).await;
            let untouched_target = read(&dir, "fan1_target").await;
            cc_fs::remove_dir_all(&dir).await.unwrap();

            assert!(failed_reset.is_err());
            assert!(taken_after_failure);
            assert!(retried.is_ok());
            assert_eq!(retried_target, "0");
            assert!(taken_after_retry.not());
            assert!(released_again.is_ok());
            assert_eq!(untouched_target, "3000");
        });
    }

    /// Goal: a duty write alone counts as taking the fan, and the user's reset returns a fan
    /// whether or not the daemon took it. Method: write a duty then release, and reset a fan
    /// that was never driven.
    #[test]
    #[serial]
    fn a_duty_write_takes_the_fan_and_a_reset_always_returns_it() {
        cc_fs::test_runtime(async {
            let dir = test_dir();
            let files = [
                ("fan1_input", "3100"),
                ("fan1_min", "2000"),
                ("fan1_max", "6200"),
                ("fan1_manual", "1"),
                ("fan1_output", "0"),
            ];
            seed(&dir, &files).await;
            let io = DeviceIo::default();
            let apple_fans = AppleDriver::IntelLegacy
                .fans(&dir, &vec![apple_fan(1, &dir)], &io)
                .await;

            let reset = apple_fans.set_to_auto_control(1, &io).await;
            let reset_manual = read(&dir, "fan1_manual").await;

            seed(&dir, &[("fan1_manual", "1")]).await;
            apple_fans.set_fan_duty(1, 50, &io).await.unwrap();
            let released = apple_fans.release_fan(1, &io).await;
            let released_manual = read(&dir, "fan1_manual").await;
            let fan_output = read(&dir, "fan1_output").await;
            cc_fs::remove_dir_all(&dir).await.unwrap();

            assert!(reset.is_ok());
            assert_eq!(reset_manual, "0");
            assert_eq!(fan_output, "3100");
            assert!(released.is_ok());
            assert_eq!(released_manual, "0");
        });
    }
}
