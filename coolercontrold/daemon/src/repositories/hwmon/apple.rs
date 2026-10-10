// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Picks the Apple fan driver behind a hwmon device and hands its fan work to that driver's
//! module.
//!
//! Each driver has its own sysfs files and its own way of taking and returning control, so each
//! has its own module. Nothing here knows those files: this only decides which module applies.

use crate::device::{ChannelStatus, Duty};
use crate::repositories::hwmon::apple_mac_smc::AppleMacSMC;
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::devices::{DEVICE_NAME_APPLE_SMC, DEVICE_NAME_MAC_SMC};
use crate::repositories::hwmon::fans;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonDriverInfo};
use crate::repositories::hwmon::macsmc_hwmon::MacSmcHwmon;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::rc::Rc;

/// Which Apple fan driver a hwmon device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleDriver {
    /// `applesmc` on Intel Macs, with the `fanN_manual` files it had before Linux 7.3.
    IntelLegacy,
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
    Silicon(MacSmcHwmon),
}

/// The Apple driver behind a hwmon device, or `None` for every other device.
pub fn detect(device_name: &str) -> Option<AppleDriver> {
    match device_name {
        DEVICE_NAME_APPLE_SMC => Some(AppleDriver::IntelLegacy),
        DEVICE_NAME_MAC_SMC => Some(AppleDriver::Silicon),
        _ => None,
    }
}

impl AppleDriver {
    /// Every fan the driver exposes, including ones the user has disabled.
    pub async fn init_fans(self, base_path: &Path, io: &DeviceIo) -> Vec<HwmonChannelInfo> {
        match self {
            Self::IntelLegacy => AppleMacSMC::init_fans(base_path, io).await,
            Self::Silicon => MacSmcHwmon::init_fans(base_path, io).await,
        }
    }

    /// The driver's own files for one fan, as `fanN_<suffix>`. They carry the state `pwmN` and
    /// `pwmN_enable` carry elsewhere, so the hardware report prints them.
    pub fn fan_attribute_suffixes(self) -> &'static [&'static str] {
        match self {
            Self::IntelLegacy => &["output", "manual", "min", "max"],
            Self::Silicon => &["target", "min", "max"],
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
                AppleHandler::IntelLegacy(AppleMacSMC::new(base_path, channels).await)
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
            AppleHandler::Silicon(_) => fans::read_one_fan_rpm_only(driver, channel).await,
        }
    }

    pub async fn set_fan_duty(&self, channel_number: u8, duty: Duty, io: &DeviceIo) -> Result<()> {
        self.mark_taken(channel_number);
        match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.set_fan_duty(channel_number, duty, io).await,
            AppleHandler::Silicon(smc) => smc.set_fan_duty(channel_number, duty, io).await,
        }
    }

    pub async fn set_to_manual_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        self.mark_taken(channel_number);
        match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.set_to_manual_control(channel_number, io).await,
            // Nothing to switch: the first in-range target is what takes the fan.
            AppleHandler::Silicon(_) => Ok(()),
        }
    }

    /// Returns the fan to automatic control, whoever holds it. This is the user's reset, so it
    /// also recovers a fan an earlier run left in manual.
    pub async fn set_to_auto_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        let result = match &self.handler {
            AppleHandler::IntelLegacy(smc) => smc.set_to_auto_control(channel_number, io).await,
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
            AppleHandler::IntelLegacy(_) => true,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::hwmon::hwmon_repo::{HwmonChannelCapabilities, HwmonChannelType};
    use serial_test::serial;
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

    /// Goal: only Apple drivers get an Apple handler. Method: the Apple names and an ordinary
    /// motherboard chip.
    #[test]
    fn only_apple_drivers_are_detected() {
        assert_eq!(detect("applesmc"), Some(AppleDriver::IntelLegacy));
        assert_eq!(detect("macsmc_hwmon"), Some(AppleDriver::Silicon));
        // The platform driver's name, which no hwmon device carries.
        assert_eq!(detect("macsmc-hwmon"), None);
        assert_eq!(detect("nct6798"), None);
        assert_eq!(detect(""), None);
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
