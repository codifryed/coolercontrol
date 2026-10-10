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
use crate::repositories::hwmon::devices::DEVICE_NAMES_APPLE;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonDriverInfo};
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::rc::Rc;

/// Which Apple fan driver a hwmon device is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppleDriver {
    /// `applesmc` on Intel Macs, with the `fanN_manual` files it had before Linux 7.3.
    IntelLegacy,
}

/// The fan handler of one Apple device, built once its channels are settled.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AppleFans {
    IntelLegacy(AppleMacSMC),
}

/// The Apple driver behind a hwmon device, or `None` for every other device.
pub fn detect(device_name: &str) -> Option<AppleDriver> {
    DEVICE_NAMES_APPLE
        .contains(&device_name)
        .then_some(AppleDriver::IntelLegacy)
}

impl AppleDriver {
    /// Every fan the driver exposes, including ones the user has disabled.
    pub async fn init_fans(self, base_path: &Path, io: &DeviceIo) -> Vec<HwmonChannelInfo> {
        match self {
            Self::IntelLegacy => AppleMacSMC::init_fans(base_path, io).await,
        }
    }

    /// The handler for the fans that were kept.
    pub async fn fans(
        self,
        base_path: &Path,
        channels: &Vec<HwmonChannelInfo>,
        device_name: &str,
    ) -> AppleFans {
        match self {
            Self::IntelLegacy => {
                AppleFans::IntelLegacy(AppleMacSMC::new(base_path, channels, device_name).await)
            }
        }
    }
}

impl AppleFans {
    pub async fn read_one_fan_status(
        &self,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<ChannelStatus> {
        match self {
            Self::IntelLegacy(smc) => smc.read_one_fan_status(driver, channel).await,
        }
    }

    pub async fn read_one_fan_rpm_only(
        &self,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<Option<u32>> {
        match self {
            Self::IntelLegacy(smc) => smc.read_one_fan_rpm_only(driver, channel).await,
        }
    }

    pub async fn set_fan_duty(&self, channel_number: u8, duty: Duty, io: &DeviceIo) -> Result<()> {
        match self {
            Self::IntelLegacy(smc) => smc.set_fan_duty(channel_number, duty, io).await,
        }
    }

    pub async fn set_to_manual_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        match self {
            Self::IntelLegacy(smc) => smc.set_to_manual_control(channel_number, io).await,
        }
    }

    pub async fn set_to_auto_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        match self {
            Self::IntelLegacy(smc) => smc.set_to_auto_control(channel_number, io).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: only Apple drivers get an Apple handler. Method: the Apple names and an ordinary
    /// motherboard chip.
    #[test]
    fn only_apple_drivers_are_detected() {
        assert_eq!(detect("applesmc"), Some(AppleDriver::IntelLegacy));
        assert_eq!(detect("macsmc-hwmon"), Some(AppleDriver::IntelLegacy));
        assert_eq!(detect("nct6798"), None);
        assert_eq!(detect(""), None);
    }
}
