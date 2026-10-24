// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use chrono::{DateTime, Local};
use std::cell::RefCell;
use std::rc::Rc;

use crate::device::health::{FailsafeRef, UnreachableRef};
use crate::device::{ChannelAttribute, DeviceType, Duty, UID};
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use crate::Device;
use anyhow::Result;
use async_trait::async_trait;
use thiserror::Error;

pub type DeviceLock = Rc<RefCell<Device>>;
pub type DeviceList = Vec<DeviceLock>;

/// A Repository is used to access device hardware data
#[async_trait(?Send)]
pub trait Repository {
    fn device_type(&self) -> DeviceType;

    async fn initialize_devices(&mut self) -> Result<()>;

    /// Returns a reference to all the devices in this repository
    async fn devices(&self) -> DeviceList;

    /// This method should be called first before calling `update_statuses`, as it then doesn't
    /// need a write lock to the device itself and all repos can be done in parallel.
    /// Preloading keeps response times for clients really quick and dependable, despite any
    /// bad timing, like calling during the middle of an update which can cause inconsistent
    /// status responses
    async fn preload_statuses(self: Rc<Self>);

    /// This method should be called after `preload_statuses` to update the internal
    /// device status history with the last polled status, stamped `timestamp`: one instant
    /// for the whole tick, shared by every device.
    async fn update_statuses(&self, timestamp: DateTime<Local>) -> Result<()>;

    /// Hint to the repository to abort in-flight or queued work so
    /// the main loop's moro scope can exit promptly when shutdown
    /// is requested. The default no-op suits repositories with no
    /// long-running queue. Repositories that own permits or
    /// background tasks (hwmon) should override this to fire their
    /// internal cancellation token. `shutdown` still runs
    /// afterwards and performs final hardware resets (returning
    /// fans to auto control, etc.).
    async fn abort_pending(&self) {}

    async fn shutdown(&self) -> Result<()>;

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()>;

    /// This is used to enable manual fan control for a device channel.
    async fn apply_setting_manual_control(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<()>;

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()>;

    /// This is for device-internal profiles only, such as some AIOs.
    /// The temp source must then always belong to the device itself.
    /// Everything else is handled by `CoolerControl` itself.
    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(f64, u8)],
    ) -> Result<()>;
    async fn apply_setting_lighting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lighting: &LightingSettings,
    ) -> Result<()>;
    async fn apply_setting_lcd(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd: &LcdSettings,
    ) -> Result<()>;
    async fn apply_setting_pwm_mode(
        &self,
        device_uid: &UID,
        channel_name: &str,
        pwm_mode: u8,
    ) -> Result<()>;

    /// Prepare devices for system sleep by resetting to safe defaults.
    async fn prepare_for_sleep(&self) {}

    /// This is helpful/necessary after waking from sleep.
    async fn reinitialize_devices(&self);

    /// Returns the channels/temps this repository is currently serving failsafe
    /// values for. Default empty: only repositories with a failsafe layer
    /// override this.
    fn failsafing(&self) -> Vec<FailsafeRef> {
        Vec::new()
    }

    /// Devices of this repository whose driver has stopped answering, so neither reads nor writes
    /// reach them. Distinct from failsafing, which describes a live device with stale readings.
    ///
    /// Only repositories that isolate their device IO can report this; the rest cannot tell a
    /// wedged device from a slow one, so they report nothing.
    fn unreachable_devices(&self) -> Vec<UnreachableRef> {
        Vec::new()
    }

    /// Lowest non-zero duty an `apply_setting_speed_fixed` write can
    /// actually reach on this channel. Anything in `1..floor` is clamped
    /// up by the driver, so a calibration sweep of that band measures
    /// one operating point repeatedly. `0` means no clamping beyond what
    /// `SpeedOptions::min_duty` already reports; only repositories that
    /// clamp behind the daemon's back override this.
    fn duty_floor(&self, _device_uid: &UID, _channel_name: &str) -> Duty {
        0
    }

    /// Driver-reported attributes (limits, targets, sensor details) for one channel, read on
    /// demand. Unset or unreadable attributes are left out. Default empty: only repositories
    /// with hwmon-style attribute files override this.
    async fn channel_attributes(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
    ) -> Result<Vec<ChannelAttribute>> {
        Ok(Vec::new())
    }
}

#[derive(Default)]
pub struct Repositories {
    pub cpu: Option<Rc<dyn Repository>>,
    pub gpu: Option<Rc<dyn Repository>>,
    pub liquidctl: Option<Rc<dyn Repository>>,
    pub hwmon: Option<Rc<dyn Repository>>,
    pub external: Option<Rc<dyn Repository>>,
    pub custom_sensors: Option<Rc<dyn Repository>>,
}

impl Repositories {
    /// This is here to help with future refactorings and improvements.
    #[allow(dead_code)]
    pub fn get_by_type(&self, device_type: DeviceType) -> Option<Rc<dyn Repository>> {
        match device_type {
            DeviceType::CPU => Self::clone_rc(self.cpu.as_ref()),
            DeviceType::GPU => Self::clone_rc(self.gpu.as_ref()),
            DeviceType::Liquidctl => Self::clone_rc(self.liquidctl.as_ref()),
            DeviceType::Hwmon => Self::clone_rc(self.hwmon.as_ref()),
            DeviceType::ServicePlugin => Self::clone_rc(self.external.as_ref()),
            DeviceType::CustomSensors => Self::clone_rc(self.custom_sensors.as_ref()),
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Rc<dyn Repository>> {
        [
            // liquidctl device can take the longest, so they should be first
            &self.liquidctl,
            &self.cpu,
            &self.gpu,
            &self.hwmon,
            &self.external,
            // custom sensors should always be last
            &self.custom_sensors,
        ]
        .into_iter()
        .flatten()
    }

    #[allow(dead_code)]
    fn clone_rc(repo: Option<&Rc<dyn Repository>>) -> Option<Rc<dyn Repository>> {
        repo.map(Rc::clone)
    }
}

/// A repository with no devices. Its preload counts itself finished, after waiting at `meeting`
/// when there is one.
#[cfg(test)]
pub struct StubRepo {
    pub device_type: DeviceType,
    pub meeting: Option<Rc<tokio::sync::Barrier>>,
    pub finished: Rc<std::cell::Cell<u8>>,
}

#[cfg(test)]
#[async_trait(?Send)]
impl Repository for StubRepo {
    fn device_type(&self) -> DeviceType {
        self.device_type
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        Vec::new()
    }

    async fn preload_statuses(self: Rc<Self>) {
        if let Some(meeting) = &self.meeting {
            meeting.wait().await;
        }
        self.finished.set(self.finished.get() + 1);
    }

    async fn update_statuses(&self, _timestamp: DateTime<Local>) -> Result<()> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_reset(&self, _: &UID, _: &str) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_manual_control(&self, _: &UID, _: &str) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_speed_fixed(&self, _: &UID, _: &str, _: u8) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_speed_profile(
        &self,
        _: &UID,
        _: &str,
        _: &TempSource,
        _: &[(f64, u8)],
    ) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_lighting(&self, _: &UID, _: &str, _: &LightingSettings) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_lcd(&self, _: &UID, _: &str, _: &LcdSettings) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_pwm_mode(&self, _: &UID, _: &str, _: u8) -> Result<()> {
        Ok(())
    }

    async fn reinitialize_devices(&self) {}
}

/// Repository Initialization Errors
/// Particularly useful for handling different initialization situations.
#[derive(Debug, Clone, Error, PartialEq)]
pub enum InitError {
    #[error("Liquidctl Integration is Disabled")]
    LiqctldDisabled,

    #[error("Python Environment Error: {msg}")]
    PythonEnv { msg: String },

    #[error("Connection Error: {msg}")]
    Connection { msg: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stub(device_type: DeviceType) -> Option<Rc<dyn Repository>> {
        Some(Rc::new(StubRepo {
            device_type,
            meeting: None,
            finished: Rc::default(),
        }))
    }

    fn all_repos() -> Repositories {
        Repositories {
            cpu: stub(DeviceType::CPU),
            gpu: stub(DeviceType::GPU),
            liquidctl: stub(DeviceType::Liquidctl),
            hwmon: stub(DeviceType::Hwmon),
            external: stub(DeviceType::ServicePlugin),
            custom_sensors: stub(DeviceType::CustomSensors),
        }
    }

    #[test]
    fn liquidctl_comes_first_and_custom_sensors_last() {
        let types: Vec<DeviceType> = all_repos().iter().map(|repo| repo.device_type()).collect();
        assert_eq!(
            types,
            [
                DeviceType::Liquidctl,
                DeviceType::CPU,
                DeviceType::GPU,
                DeviceType::Hwmon,
                DeviceType::ServicePlugin,
                DeviceType::CustomSensors,
            ]
        );
    }

    #[test]
    fn a_missing_repo_is_skipped() {
        let repos = Repositories {
            gpu: None,
            ..all_repos()
        };
        assert!(repos
            .iter()
            .all(|repo| repo.device_type() != DeviceType::GPU));
        assert_eq!(repos.iter().count(), 5);
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn iterating_allocates_nothing() {
        let repos = all_repos();
        let (count, allocations) = crate::alloc_count::count_allocations(|| repos.iter().count());
        assert_eq!(count, 6);
        assert_eq!(allocations, 0);
    }
}
