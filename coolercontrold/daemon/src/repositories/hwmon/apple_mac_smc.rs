// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::cc_fs;
use crate::cc_fs::ReadIndex;
use crate::device::{ChannelStatus, Duty, RPM};
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::fans;
use crate::repositories::hwmon::hwmon_repo::{
    AutoCurveInfo, ChannelReadSlots, HwmonChannelCapabilities, HwmonChannelInfo, HwmonChannelType,
    HwmonDriverInfo,
};
use anyhow::{anyhow, Context, Result};
use log::{debug, error, log_enabled, warn};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const DEFAULT_MIN_FAN_SPEED: RPM = 600;
const DEFAULT_MAX_FAN_SPEED: RPM = 6_500;
const FAN_AUTO_CONTROL: u8 = 0;
const FAN_MANUAL_CONTROL: u8 = 1;
const PATTERN_FAN_OUTPUT_FILE_NUMBER: &str = r"^fan(?P<number>\d+)_output$";
macro_rules! format_fan_manual { ($($arg:tt)*) => {{ format!("fan{}_manual", $($arg)*) }}; }
macro_rules! format_fan_min { ($($arg:tt)*) => {{ format!("fan{}_min", $($arg)*) }}; }
macro_rules! format_fan_max { ($($arg:tt)*) => {{ format!("fan{}_max", $($arg)*) }}; }
macro_rules! format_fan_output { ($($arg:tt)*) => {{ format!("fan{}_output", $($arg)*) }}; }
macro_rules! format_fan_input { ($($arg:tt)*) => {{ format!("fan{}_input", $($arg)*) }}; }

/// This is a `HWMon` repository extension for Apple hardware supported by the Linux Kernel.
///
/// In particular the `applesmc` driver, which used in Intel-based Apple computers.
/// See: `https://github.com/torvalds/linux/blob/master/drivers/hwmon/applesmc.c`
///
/// Apple Silicon has its own module, `macsmc_hwmon`.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AppleMacSMC {
    path: PathBuf,
    fans: HashMap<u8, AppleFanInfo>,
}

impl AppleMacSMC {
    pub async fn new(path: &Path, channels: &Vec<HwmonChannelInfo>) -> Self {
        let mut fans = HashMap::new();
        for channel in channels {
            if channel.hwmon_type != HwmonChannelType::Fan || channel.caps.is_apple_smc().not() {
                continue;
            }
            let max_rpm = Self::get_fan_max(path, channel.number, false)
                .await
                .unwrap_or(DEFAULT_MAX_FAN_SPEED);
            let default_min_rpm = Self::get_fan_min(path, channel.number, false)
                .await
                .unwrap_or(DEFAULT_MIN_FAN_SPEED);
            fans.insert(
                channel.number,
                AppleFanInfo {
                    max_rpm,
                    default_min_rpm,
                },
            );
        }
        Self {
            path: path.to_path_buf(),
            fans,
        }
    }

    /// Says why a fan cannot be controlled, or cannot be stopped. Repository init only, so it
    /// is logged once per start and never by a hardware report.
    pub async fn log_fan_limits(base_path: &Path, channels: &[HwmonChannelInfo]) {
        for channel in channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            if channel.caps.is_apple_smc() {
                let _ = Self::fan_min_is_writable(base_path, channel.number);
            } else if cc_fs::metadata(base_path.join(format_fan_output!(channel.number))).is_ok() {
                // Detection turned this fan down. Asking again, aloud, names what is missing.
                let _ = Self::is_controllable(base_path, channel.number, true).await;
            }
        }
    }

    /// Returns every detected fan, including ones the user has disabled. The
    /// caller drops those, so both hwmon branches record the same exclusions.
    ///
    /// Says nothing about a fan it cannot control, and leaves an error to the caller: the
    /// hardware report calls this too.
    pub async fn init_fans(base_path: &Path, io: &DeviceIo) -> Result<Vec<HwmonChannelInfo>> {
        let dir_entries = cc_fs::read_dir(base_path)?;
        let mut fan_caps = HashMap::new();
        for entry in dir_entries {
            let os_file_name = entry?.file_name();
            let file_name = os_file_name.to_str().context("File Name should be a str")?;
            Self::detect_apple_smc_fans(base_path, file_name, &mut fan_caps).await?;
            fans::detect_rpm(base_path, file_name, &mut fan_caps, io).await?;
        }
        let mut fans = Self::caps_to_hwmon_fans(base_path, fan_caps).await?;
        fans.sort_by_key(|c| c.number);
        debug!(
            "Apple SMC fans detected: {fans:?} for {}",
            base_path.display()
        );
        Ok(fans)
    }

    /// Detects which fans of this `applesmc` device are controllable
    async fn detect_apple_smc_fans(
        base_path: &Path,
        file_name: &str,
        fan_caps: &mut HashMap<u8, HwmonChannelCapabilities>,
    ) -> Result<()> {
        let regex_output_file = Regex::new(PATTERN_FAN_OUTPUT_FILE_NUMBER)?;
        if regex_output_file.is_match(file_name).not() {
            return Ok(()); // skip if not an applicable fan file
        }
        let channel_number: u8 = regex_output_file
            .captures(file_name)
            .context("Fan Number should exist")?
            .name("number")
            .context("Number Group should exist")?
            .as_str()
            .parse()?;
        // Quiet: the hardware report detects too. `log_fan_limits` gives the reason at start.
        if Self::is_controllable(base_path, channel_number, false)
            .await
            .not()
        {
            return Ok(());
        }
        let caps = fan_caps
            .entry(channel_number)
            .or_insert(HwmonChannelCapabilities::empty());
        // APPLE_SMC cap means it fulfills all our base requirements for fan control,
        //  otherwise it will likely be detected as a rpm-only non-controllable fan
        // Note: all rpm endpoints must be exposed for us to interpolate Duty and support this driver
        caps.insert(HwmonChannelCapabilities::APPLE_SMC);
        caps.insert(HwmonChannelCapabilities::FAN_WRITABLE);
        caps.insert(HwmonChannelCapabilities::RPM);
        Ok(())
    }

    /// Whether the fan behind a `fanN_output` file has everything control needs. With
    /// `log_error`, says what is missing.
    async fn is_controllable(base_path: &Path, channel_number: u8, log_error: bool) -> bool {
        if Self::fan_output_is_writable(base_path, channel_number, log_error).not() {
            return false;
        }
        // Detection reads each attribute once, so this cache closes with the probe.
        let rpm = fans::get_fan_rpm(
            &DeviceIo::default(),
            base_path,
            &channel_number,
            &HwmonChannelCapabilities::empty(),
            None,
            None,
            log_error,
        )
        .await;
        if rpm.is_none() {
            return false; // no indicator of speed
        }
        if Self::fan_manual_is_writable(base_path, channel_number, log_error).not() {
            return false;
        }
        // Both limits are needed to map a duty onto a speed.
        Self::get_fan_max(base_path, channel_number, log_error)
            .await
            .is_some()
            && Self::get_fan_min(base_path, channel_number, log_error)
                .await
                .is_some()
    }

    fn fan_output_is_writable(base_path: &Path, channel_number: u8, log_error: bool) -> bool {
        let output_path = base_path.join(format_fan_output!(channel_number));
        let output_writable = cc_fs::metadata(&output_path)
            .inspect_err(|_| {
                if log_error {
                    error!(
                        "Fan_output file metadata is not readable: {}",
                        output_path.display()
                    );
                }
            })
            // This check should be sufficient, as we're running as root:
            .is_ok_and(|att| att.permissions().readonly().not());
        if log_error && output_writable.not() {
            warn!(
                "Apple SMC fan at {} is NOT writable - \
            Fan control is not currently supported by the installed driver.",
                output_path.display()
            );
        }
        output_writable
    }

    fn fan_manual_is_writable(base_path: &Path, channel_number: u8, log_error: bool) -> bool {
        let manual_path = base_path.join(format_fan_manual!(channel_number));
        let manual_writable = cc_fs::metadata(&manual_path)
            .inspect_err(|_| {
                if log_error {
                    error!(
                        "Fan_manual file metadata is not readable: {}",
                        manual_path.display()
                    );
                }
            })
            // This check should be sufficient, as we're running as root:
            .is_ok_and(|att| att.permissions().readonly().not());
        if log_error && manual_writable.not() {
            warn!(
                "Apple SMC fan manual at {} is NOT writable - \
            Fan control is not currently supported by the installed driver.",
                manual_path.display()
            );
        }
        manual_writable
    }

    fn fan_min_is_writable(base_path: &Path, channel_number: u8) -> bool {
        let min_path = base_path.join(format_fan_min!(channel_number));
        let min_writable = cc_fs::metadata(&min_path)
            .inspect_err(|_| {
                error!(
                    "Fan_min file metadata is not readable: {}",
                    min_path.display()
                );
            })
            // This check should be sufficient, as we're running as root:
            .is_ok_and(|att| att.permissions().readonly().not());
        if min_writable.not() {
            warn!(
                "Apple SMC fan min at {} is NOT writable - \
            0 rpm and other minimum speed changes will not be possible.",
                min_path.display()
            );
        }
        min_writable
    }

    /// Converts fan capabilities to `HwmonChannelInfo`
    async fn caps_to_hwmon_fans(
        base_path: &Path,
        fan_caps: HashMap<u8, HwmonChannelCapabilities>,
    ) -> Result<Vec<HwmonChannelInfo>> {
        let mut fans = vec![];
        for (channel_number, fan_cap) in fan_caps {
            let channel_name = fans::get_fan_channel_name(channel_number);
            let label = fans::get_fan_channel_label(base_path, &channel_number).await;
            let rpm_path = fan_cap
                .has_rpm()
                .then(|| base_path.join(format_fan_input!(channel_number)));
            fans.push(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: channel_number,
                pwm_enable_default: None,
                name: channel_name,
                label,
                caps: fan_cap,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            });
        }
        Ok(fans)
    }

    /// Reads the Apple SMC duty + rpm for one channel and returns
    /// the resulting `ChannelStatus`, or `None` if any expected
    /// field failed. Mirror of `fans::read_one_fan_status` for the
    /// Apple SMC code path; pulled out so the preload loop can
    /// acquire the device permit per channel.
    pub async fn read_one_fan_status(
        &self,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<ChannelStatus> {
        debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
        let fan_duty = if channel.caps.is_apple_smc() {
            self.get_fan_duty(
                &driver.io,
                channel.number,
                channel.rpm_path.as_deref(),
                channel.read_slot.rpm,
            )
            .await
        } else {
            None
        };
        let fan_rpm = if channel.caps.has_rpm() {
            fans::get_fan_rpm(
                &driver.io,
                &driver.path,
                &channel.number,
                &channel.caps,
                channel.rpm_path.as_deref(),
                channel.read_slot.rpm,
                log_enabled!(log::Level::Debug),
            )
            .await
        } else {
            None
        };
        let expected_duty_failed = channel.caps.is_apple_smc() && fan_duty.is_none();
        let expected_rpm_failed = channel.caps.has_rpm() && fan_rpm.is_none();
        if expected_duty_failed || expected_rpm_failed {
            // Omit the entry so the upstream failsafe overlay can
            // substitute safe values. See the comment on
            // `fans::read_fan_statuses` for the rationale.
            return None;
        }
        Some(ChannelStatus {
            name: channel.name.clone(),
            rpm: fan_rpm,
            duty: fan_duty,
            ..Default::default()
        })
    }

    /// Reads only the RPM portion of an Apple SMC fan channel.
    /// Mirrors `fans::read_one_fan_rpm_only` for the slow-device
    /// preload path; see that function's doc for the rationale.
    pub async fn read_one_fan_rpm_only(
        &self,
        driver: &Rc<HwmonDriverInfo>,
        channel: &HwmonChannelInfo,
    ) -> Option<Option<u32>> {
        debug_assert_eq!(channel.hwmon_type, HwmonChannelType::Fan);
        if channel.caps.has_rpm().not() {
            return Some(None);
        }
        let fan_rpm = fans::get_fan_rpm(
            &driver.io,
            &driver.path,
            &channel.number,
            &channel.caps,
            channel.rpm_path.as_deref(),
            channel.read_slot.rpm,
            false,
        )
        .await?;
        Some(Some(fan_rpm))
    }

    /// Every Apple SMC fan channel, buffered. Test-only: production reads one channel at a time
    /// under the device permit so each fresh reading upserts into the preloaded cache.
    #[cfg(test)]
    pub async fn extract_fan_statuses(
        &self,
        driver: &Rc<HwmonDriverInfo>,
    ) -> (Vec<ChannelStatus>, bool) {
        let mut fans = Vec::new();
        let mut any_failure = false;
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            match self.read_one_fan_status(driver, channel).await {
                Some(status) => fans.push(status),
                None => any_failure = true,
            }
        }
        (fans, any_failure)
    }

    pub async fn set_to_auto_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        let fan_min_default = self
            .fans
            .get(&channel_number)
            .map_or(DEFAULT_MIN_FAN_SPEED, |info| info.default_min_rpm);
        let fan_min_path = self.path.join(format_fan_min!(channel_number));
        let fan_manual_path = self.path.join(format_fan_manual!(channel_number));
        if let Err(e) = io
            .write_value(&fan_min_path, fan_min_default.to_string().into_bytes())
            .await
        {
            warn!(
                "Unable to set Fan Min value {fan_min_default} for {} Reason: {e}",
                fan_min_path.display()
            );
        }
        io.write_value(&fan_manual_path, FAN_AUTO_CONTROL.to_string().into_bytes())
            .await
            .map_err(|err| {
                anyhow!(
                    "Unable to set Fan Manual value {FAN_AUTO_CONTROL} for {} Reason: {err}",
                    fan_min_path.display()
                )
            })
    }

    pub async fn set_to_manual_control(&self, channel_number: u8, io: &DeviceIo) -> Result<()> {
        let fan_min_path = self.path.join(format_fan_min!(channel_number));
        let fan_manual_path = self.path.join(format_fan_manual!(channel_number));
        if let Err(e) = io.write_value(&fan_min_path, b"0".to_vec()).await {
            warn!(
                "Unable to set Fan Min value 0 for {}. The driver will not allow you to set fan speeds to 0. Reason: {e}",
                fan_min_path.display()
            );
        }
        io.write_value(
            &fan_manual_path,
            FAN_MANUAL_CONTROL.to_string().into_bytes(),
        )
        .await
        .map_err(|err| {
            anyhow!(
                "Unable to set Fan Manual value {FAN_MANUAL_CONTROL} for {} Reason: {err}",
                fan_min_path.display()
            )
        })
    }

    async fn get_fan_min(base_path: &Path, channel_number: u8, log_error: bool) -> Option<RPM> {
        let fan_min_path = base_path.join(format_fan_min!(channel_number));
        cc_fs::read_sysfs_value(&fan_min_path)
            .await
            .and_then(fans::check_parsing_32)
            // Edge case where on spin-up the output is max value until it begins moving
            .map(|rpm| if rpm >= u32::from(u16::MAX) { 0 } else { rpm })
            .inspect_err(|err| {
                if log_error {
                    warn!(
                        "Could not read fan min rpm value at {}: {err}",
                        fan_min_path.display()
                    );
                }
            })
            .ok()
    }

    async fn get_fan_max(base_path: &Path, channel_number: u8, log_error: bool) -> Option<RPM> {
        let fan_max_path = base_path.join(format_fan_max!(channel_number));
        cc_fs::read_sysfs_value(&fan_max_path)
            .await
            .and_then(fans::check_parsing_32)
            // Edge case where on spin-up the output is max value until it begins moving
            .map(|rpm| if rpm >= u32::from(u16::MAX) { 0 } else { rpm })
            .inspect_err(|err| {
                if log_error {
                    warn!(
                        "Could not read fan max rpm value at {}: {err}",
                        fan_max_path.display()
                    );
                }
            })
            .ok()
    }

    pub async fn get_fan_duty(
        &self,
        io: &DeviceIo,
        channel_number: u8,
        rpm_path: Option<&Path>,
        slot: Option<ReadIndex>,
    ) -> Option<f64> {
        fans::get_fan_rpm(
            io,
            &self.path,
            &channel_number,
            &HwmonChannelCapabilities::empty(),
            rpm_path,
            slot,
            log_enabled!(log::Level::Debug),
        )
        .await
        .and_then(|rpm| self.interpolate_duty_from_rpm(channel_number, rpm))
    }

    pub async fn set_fan_duty(&self, channel_number: u8, speed: Duty, io: &DeviceIo) -> Result<()> {
        let rpm = self.interpolate_rpm_from_duty(channel_number, speed);
        Self::set_fan_output(&self.path, channel_number, rpm, io).await
    }

    async fn set_fan_output(
        path: &Path,
        channel_number: u8,
        rpm: RPM,
        io: &DeviceIo,
    ) -> Result<()> {
        let fan_output_path = path.join(format_fan_output!(channel_number));
        io.write_value(&fan_output_path, rpm.to_string().into_bytes())
            .await
            .map_err(|err| {
                anyhow!(
                    "Unable to set Fan Output value {rpm} for {} Reason: {err}",
                    fan_output_path.display()
                )
            })
    }

    /// Interpolates a duty value from a given RPM value.
    /// We also round the result to a single decimal point and then round to the nearest integer.
    /// This is to avoid floating point precision issues and gives us a more accurate result.
    fn interpolate_duty_from_rpm(&self, channel_number: u8, rpm: RPM) -> Option<f64> {
        self.fans.get(&channel_number).map(|fan| {
            if rpm == 0 {
                return 0.;
            } else if rpm >= fan.max_rpm {
                return 100.;
            }
            // give us single decimal point rounding precision, aka 21.3%
            let max_rpm_points = f64::from(fan.max_rpm) / 1_000.;
            let duty = (f64::from(rpm) / max_rpm_points).round() / 10.;
            duty.clamp(0.0, 100.0).round()
        })
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn interpolate_rpm_from_duty(&self, channel_number: u8, duty: Duty) -> RPM {
        self.fans.get(&channel_number).map_or_else(
            || {
                error!("Apple SCM fan channel: {channel_number} not found. Using maximum speed.");
                DEFAULT_MAX_FAN_SPEED
            },
            |fan| {
                let max_rpm_points = f64::from(fan.max_rpm) / 1_000.;
                let rpm = ((f64::from(duty) * max_rpm_points).round() * 10.).round() as RPM;
                rpm.clamp(0, DEFAULT_MAX_FAN_SPEED)
            },
        )
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
struct AppleFanInfo {
    max_rpm: RPM,
    default_min_rpm: RPM,
}

/// Tests
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::repositories::hwmon::drivetemp;
    use serial_test::serial;
    use std::fs::Permissions;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";
    const COMPLETE_FAN: [(&str, &str); 5] = [
        ("fan1_output", "2500"),
        ("fan1_input", "2500"),
        ("fan1_manual", "0"),
        ("fan1_min", "600"),
        ("fan1_max", "6500"),
    ];

    struct HwmonFileContext {
        test_base_path: PathBuf,
    }

    async fn setup() -> HwmonFileContext {
        let test_base_path =
            Path::new(&(TEST_BASE_PATH_STR.to_string() + &Uuid::new_v4().to_string()))
                .to_path_buf();
        cc_fs::create_dir_all(&test_base_path).await.unwrap();
        HwmonFileContext { test_base_path }
    }

    async fn teardown(ctx: &HwmonFileContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).await.unwrap();
    }

    #[test]
    #[serial]
    fn test_interpolate_duty_from_rpm_zero() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let duty = apple_smc.interpolate_duty_from_rpm(1, 0);

        // then:
        assert_eq!(duty, Some(0.0));
    }

    #[test]
    #[serial]
    fn test_interpolate_duty_from_rpm_max() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let duty = apple_smc.interpolate_duty_from_rpm(1, 5000);

        // then:
        assert_eq!(duty, Some(100.0));
    }

    #[test]
    #[serial]
    fn test_interpolate_duty_from_rpm_above_max() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let duty = apple_smc.interpolate_duty_from_rpm(1, 6000);

        // then:
        assert_eq!(duty, Some(100.0));
    }

    #[test]
    #[serial]
    fn test_interpolate_duty_from_rpm_mid() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let duty = apple_smc.interpolate_duty_from_rpm(1, 2500);

        // then:
        assert_eq!(duty, Some(50.0));
    }

    #[test]
    #[serial]
    fn test_interpolate_duty_from_rpm_not_found() {
        // given:
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans: HashMap::new(),
        };

        // when:
        let duty = apple_smc.interpolate_duty_from_rpm(1, 2500);

        // then:
        assert_eq!(duty, None);
    }

    #[test]
    #[serial]
    fn test_interpolate_rpm_from_duty_zero() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let rpm = apple_smc.interpolate_rpm_from_duty(1, 0);

        // then:
        assert_eq!(rpm, 0);
    }

    #[test]
    #[serial]
    fn test_interpolate_rpm_from_duty_max() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let rpm = apple_smc.interpolate_rpm_from_duty(1, 100);

        // then:
        assert_eq!(rpm, 5000);
    }

    #[test]
    #[serial]
    fn test_interpolate_rpm_from_duty_mid() {
        // given:
        let mut fans = HashMap::new();
        fans.insert(
            1,
            AppleFanInfo {
                max_rpm: 5000,
                default_min_rpm: 600,
            },
        );
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans,
        };

        // when:
        let rpm = apple_smc.interpolate_rpm_from_duty(1, 50);

        // then:
        assert_eq!(rpm, 2500);
    }

    #[test]
    #[serial]
    fn test_interpolate_rpm_from_duty_not_found() {
        // given:
        let apple_smc = AppleMacSMC {
            path: PathBuf::new(),
            fans: HashMap::new(),
        };

        // when:
        let rpm = apple_smc.interpolate_rpm_from_duty(1, 50);

        // then:
        assert_eq!(rpm, DEFAULT_MAX_FAN_SPEED);
    }

    #[test]
    #[serial]
    fn test_detect_apple_smc_fans_no_output_file() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            let mut fan_caps = HashMap::new();

            // when:
            let result =
                AppleMacSMC::detect_apple_smc_fans(test_base_path, "fan1_input", &mut fan_caps)
                    .await;

            // then:
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert!(fan_caps.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_detect_apple_smc_fans_complete() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_manual"), b"0".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();
            let mut fan_caps = HashMap::new();

            // when:
            let result =
                AppleMacSMC::detect_apple_smc_fans(test_base_path, "fan1_output", &mut fan_caps)
                    .await;

            // then:
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(fan_caps.len(), 1);
            let caps = fan_caps.get(&1).unwrap();
            assert!(caps.is_apple_smc());
            assert!(caps.is_fan_controllable());
            assert!(caps.has_rpm());
        });
    }

    #[test]
    #[serial]
    fn test_detect_apple_smc_fans_missing_manual() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();
            let mut fan_caps = HashMap::new();

            // when:
            let result =
                AppleMacSMC::detect_apple_smc_fans(test_base_path, "fan1_output", &mut fan_caps)
                    .await;

            // then:
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert!(fan_caps.is_empty());
        });
    }

    /// The verdict on fan1 asked quietly, as detection does, and aloud, as the start-up log
    /// does.
    async fn both_verdicts(dir: &Path) -> (bool, bool) {
        (
            AppleMacSMC::is_controllable(dir, 1, false).await,
            AppleMacSMC::is_controllable(dir, 1, true).await,
        )
    }

    /// A fan is controllable only with every one of its files, and detection must reach the
    /// same verdict as the start-up log that explains it. Takes one file at a time away from
    /// a complete fan and asks both ways.
    #[test]
    #[serial]
    fn a_fan_missing_any_file_is_not_controllable() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let dir = &ctx.test_base_path;
            for (name, value) in COMPLETE_FAN {
                cc_fs::write(dir.join(name), value.as_bytes().to_vec())
                    .await
                    .unwrap();
            }

            let complete = both_verdicts(dir).await;
            let mut incomplete = Vec::with_capacity(COMPLETE_FAN.len());
            for (name, value) in COMPLETE_FAN {
                cc_fs::remove_file(dir.join(name)).await.unwrap();
                incomplete.push((name, both_verdicts(dir).await));
                cc_fs::write(dir.join(name), value.as_bytes().to_vec())
                    .await
                    .unwrap();
            }
            let restored = both_verdicts(dir).await;

            teardown(&ctx).await;
            assert_eq!(complete, (true, true));
            for (name, verdicts) in incomplete {
                assert_eq!(verdicts, (false, false), "without {name}");
            }
            assert_eq!(restored, (true, true));
        });
    }

    /// The driver marks a file it will not take writes on as read-only, and control needs to
    /// write both the speed and the mode. Locks each of the two in turn.
    #[test]
    #[serial]
    fn a_fan_with_a_read_only_control_file_is_not_controllable() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let dir = &ctx.test_base_path;
            for (name, value) in COMPLETE_FAN {
                cc_fs::write(dir.join(name), value.as_bytes().to_vec())
                    .await
                    .unwrap();
            }

            let mut locked = Vec::with_capacity(2);
            for name in ["fan1_output", "fan1_manual"] {
                cc_fs::set_permissions(dir.join(name), Permissions::from_mode(0o444))
                    .await
                    .unwrap();
                locked.push((name, both_verdicts(dir).await));
                cc_fs::set_permissions(dir.join(name), Permissions::from_mode(0o644))
                    .await
                    .unwrap();
            }
            let unlocked = both_verdicts(dir).await;

            teardown(&ctx).await;
            for (name, verdicts) in locked {
                assert_eq!(verdicts, (false, false), "with {name} read-only");
            }
            assert_eq!(unlocked, (true, true));
        });
    }

    #[test]
    #[serial]
    fn test_init_apple_fans() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_manual"), b"0".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_output"), b"3000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_input"), b"3000".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_manual"), b"0".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_min"), b"700".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_max"), b"7000".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::init_fans(test_base_path, &DeviceIo::default()).await;

            // then:
            teardown(&ctx).await;
            assert!(result.is_ok());
            let fans = result.unwrap();
            assert_eq!(fans.len(), 2);
            assert_eq!(fans[0].number, 1);
            assert_eq!(fans[0].name, "fan1");
            assert_eq!(fans[1].number, 2);
            assert_eq!(fans[1].name, "fan2");
        });
    }

    /// Disabled channels are dropped by the caller, not here, so that the same
    /// place also records the exclusion for the hardware report.
    #[test]
    #[serial]
    fn test_init_fans_returns_disabled_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_manual"), b"0".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();
            // when:
            let channels = AppleMacSMC::init_fans(test_base_path, &DeviceIo::default())
                .await
                .unwrap();

            // then:
            teardown(&ctx).await;
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].name, "fan1");
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_min() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::get_fan_min(test_base_path, 1, false).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(result, Some(600));
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_min_edge_case_max_value() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("fan1_min"),
                b"4294967295".to_vec(), // u32::MAX
            )
            .await
            .unwrap();

            // when:
            let result = AppleMacSMC::get_fan_min(test_base_path, 1, false).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(result, Some(0));
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_max() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::get_fan_max(test_base_path, 1, false).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(result, Some(6500));
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_max_edge_case_max_value() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("fan1_max"),
                b"4294967295".to_vec(), // u32::MAX
            )
            .await
            .unwrap();

            // when:
            let result = AppleMacSMC::get_fan_max(test_base_path, 1, false).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(result, Some(0));
        });
    }

    #[test]
    #[serial]
    fn test_set_to_auto_control() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"0".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_manual"), b"1".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 6500,
                    default_min_rpm: 600,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };

            // when:
            let result = apple_smc.set_to_auto_control(1, &DeviceIo::default()).await;

            // then:
            let fan_min = cc_fs::read_sysfs(test_base_path.join("fan1_min"))
                .await
                .unwrap();
            let fan_manual = cc_fs::read_sysfs(test_base_path.join("fan1_manual"))
                .await
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(fan_min.trim(), "600");
            assert_eq!(fan_manual.trim(), "0");
        });
    }

    #[test]
    #[serial]
    fn test_set_to_manual_control() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_manual"), b"0".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 6500,
                    default_min_rpm: 600,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };

            // when:
            let result = apple_smc
                .set_to_manual_control(1, &DeviceIo::default())
                .await;

            // then:
            let fan_min = cc_fs::read_sysfs(test_base_path.join("fan1_min"))
                .await
                .unwrap();
            let fan_manual = cc_fs::read_sysfs(test_base_path.join("fan1_manual"))
                .await
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(fan_min.trim(), "0");
            assert_eq!(fan_manual.trim(), "1");
        });
    }

    #[test]
    #[serial]
    fn test_set_fan_duty() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"0".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 5000,
                    default_min_rpm: 600,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };

            // when:
            let result = apple_smc.set_fan_duty(1, 50, &DeviceIo::default()).await;

            // then:
            let fan_output = cc_fs::read_sysfs(test_base_path.join("fan1_output"))
                .await
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(fan_output.trim(), "2500");
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_duty() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 5000,
                    default_min_rpm: 600,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };

            // when:
            let result = apple_smc
                .get_fan_duty(&DeviceIo::default(), 1, None, None)
                .await;

            // then:
            teardown(&ctx).await;
            assert_eq!(result, Some(50.0));
        });
    }

    #[test]
    #[serial]
    fn test_extract_fan_statuses() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_input"), b"3000".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 5000,
                    default_min_rpm: 600,
                },
            );
            fans.insert(
                2,
                AppleFanInfo {
                    max_rpm: 6000,
                    default_min_rpm: 700,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };
            let channels = vec![
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 1,
                    pwm_enable_default: None,
                    name: "fan1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::APPLE_SMC
                        | HwmonChannelCapabilities::FAN_WRITABLE
                        | HwmonChannelCapabilities::RPM,
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: Some(test_base_path.join("fan1_input")),
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 2,
                    pwm_enable_default: None,
                    name: "fan2".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::APPLE_SMC
                        | HwmonChannelCapabilities::FAN_WRITABLE
                        | HwmonChannelCapabilities::RPM,
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: Some(test_base_path.join("fan2_input")),
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
            ];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: channels.clone(),
                drivetemp: drivetemp::DrivetempState::default(),
                apple_fans: None,
                io: DeviceIo::default(),
            });

            // when:
            let (statuses, any_failure) = apple_smc.extract_fan_statuses(&driver).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 2);
            assert_eq!(statuses[0].name, "fan1");
            assert_eq!(statuses[0].rpm, Some(2500));
            assert_eq!(statuses[0].duty, Some(50.0));
            assert_eq!(statuses[1].name, "fan2");
            assert_eq!(statuses[1].rpm, Some(3000));
            assert_eq!(statuses[1].duty, Some(50.0));
        });
    }

    #[test]
    #[serial]
    fn test_new_apple_smc() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();
            let channels = vec![HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: "fan1".to_string(),
                label: None,
                caps: HwmonChannelCapabilities::APPLE_SMC
                    | HwmonChannelCapabilities::FAN_WRITABLE
                    | HwmonChannelCapabilities::RPM,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: Some(test_base_path.join("fan1_input")),
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            }];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(apple_smc.fans.len(), 1);
            let fan_info = apple_smc.fans.get(&1).unwrap();
            assert_eq!(fan_info.max_rpm, 6500);
            assert_eq!(fan_info.default_min_rpm, 600);
        });
    }

    #[test]
    #[serial]
    fn test_new_apple_smc_with_defaults() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            let channels = vec![HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: "fan1".to_string(),
                label: None,
                caps: HwmonChannelCapabilities::APPLE_SMC
                    | HwmonChannelCapabilities::FAN_WRITABLE
                    | HwmonChannelCapabilities::RPM,
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: Some(test_base_path.join("fan1_input")),
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            }];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels).await;

            // then:
            teardown(&ctx).await;
            assert_eq!(apple_smc.fans.len(), 1);
            let fan_info = apple_smc.fans.get(&1).unwrap();
            assert_eq!(fan_info.max_rpm, DEFAULT_MAX_FAN_SPEED);
            assert_eq!(fan_info.default_min_rpm, DEFAULT_MIN_FAN_SPEED);
        });
    }

    #[test]
    #[serial]
    fn test_fan_output_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"2500".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_output_is_writable(test_base_path, 1, true);

            // then:
            teardown(&ctx).await;
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_output_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_output_is_writable(test_base_path, 1, true);

            // then:
            teardown(&ctx).await;
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_manual_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_manual"), b"0".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_manual_is_writable(test_base_path, 1, true);

            // then:
            teardown(&ctx).await;
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_manual_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_manual_is_writable(test_base_path, 1, true);

            // then:
            teardown(&ctx).await;
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_min_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_min_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx).await;
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_min_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_min_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx).await;
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_set_fan_output() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"0".to_vec())
                .await
                .unwrap();

            // when:
            let result =
                AppleMacSMC::set_fan_output(test_base_path, 1, 3000, &DeviceIo::default()).await;

            // then:
            let fan_output = cc_fs::read_sysfs(test_base_path.join("fan1_output"))
                .await
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(fan_output.trim(), "3000");
        });
    }

    #[test]
    #[serial]
    fn test_caps_to_hwmon_fans() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            let mut fan_caps = HashMap::new();
            fan_caps.insert(
                1,
                HwmonChannelCapabilities::APPLE_SMC
                    | HwmonChannelCapabilities::FAN_WRITABLE
                    | HwmonChannelCapabilities::RPM,
            );
            fan_caps.insert(2, HwmonChannelCapabilities::RPM);

            // when:
            let result = AppleMacSMC::caps_to_hwmon_fans(test_base_path, fan_caps).await;

            // then:
            teardown(&ctx).await;
            assert!(result.is_ok());
            let fans = result.unwrap();
            assert_eq!(fans.len(), 2);
        });
    }

    #[test]
    #[serial]
    fn test_caps_to_hwmon_fans_with_label() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_label"), b"Left Fan".to_vec())
                .await
                .unwrap();
            let mut fan_caps = HashMap::new();
            fan_caps.insert(
                1,
                HwmonChannelCapabilities::APPLE_SMC
                    | HwmonChannelCapabilities::FAN_WRITABLE
                    | HwmonChannelCapabilities::RPM,
            );

            // when:
            let result = AppleMacSMC::caps_to_hwmon_fans(test_base_path, fan_caps).await;

            // then:
            teardown(&ctx).await;
            assert!(result.is_ok());
            let fans = result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].label, Some("Left Fan".to_string()));
        });
    }

    #[test]
    #[serial]
    fn test_new_skips_non_apple_smc_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            let channels = vec![
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 1,
                    pwm_enable_default: None,
                    name: "fan1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::RPM, // Not APPLE_SMC
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: None,
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Temp,
                    number: 1,
                    pwm_enable_default: None,
                    name: "temp1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::APPLE_SMC,
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: None,
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
            ];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels).await;

            // then:
            teardown(&ctx).await;
            assert!(apple_smc.fans.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_extract_fan_statuses_skips_non_fan_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 5000,
                    default_min_rpm: 600,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };
            let channels = vec![
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 1,
                    pwm_enable_default: None,
                    name: "fan1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::APPLE_SMC
                        | HwmonChannelCapabilities::FAN_WRITABLE
                        | HwmonChannelCapabilities::RPM,
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: None,
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Temp,
                    number: 1,
                    pwm_enable_default: None,
                    name: "temp1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::empty(),
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: None,
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
            ];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: channels.clone(),
                drivetemp: drivetemp::DrivetempState::default(),
                apple_fans: None,
                io: DeviceIo::default(),
            });

            // when:
            let (statuses, any_failure) = apple_smc.extract_fan_statuses(&driver).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].name, "fan1");
        });
    }

    #[test]
    #[serial]
    fn test_extract_fan_statuses_rpm_only_fan() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans: HashMap::new(),
            };
            let channels = vec![HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: None,
                name: "fan1".to_string(),
                label: None,
                caps: HwmonChannelCapabilities::RPM, // RPM only, not APPLE_SMC
                auto_curve: AutoCurveInfo::None,
                pwm_path: None,
                rpm_path: None,
                temp_path: None,
                read_slot: ChannelReadSlots::default(),
            }];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: channels.clone(),
                drivetemp: drivetemp::DrivetempState::default(),
                apple_fans: None,
                io: DeviceIo::default(),
            });

            // when:
            let (statuses, any_failure) = apple_smc.extract_fan_statuses(&driver).await;

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].name, "fan1");
            assert_eq!(statuses[0].rpm, Some(2500));
            assert_eq!(statuses[0].duty, None); // No duty for RPM-only fan
        });
    }

    // --- extract_fan_statuses: ordering and failures ---

    #[test]
    #[serial]
    fn extract_fan_statuses_preserves_channel_order() {
        // Verifies the streaming variant invokes the sink once per
        // successful channel in channel-definition order.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            cc_fs::write(test_base_path.join("fan2_input"), b"3000".to_vec())
                .await
                .unwrap();
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 5000,
                    default_min_rpm: 600,
                },
            );
            fans.insert(
                2,
                AppleFanInfo {
                    max_rpm: 6000,
                    default_min_rpm: 700,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };
            let caps = HwmonChannelCapabilities::APPLE_SMC
                | HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::RPM;
            let channels = vec![
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 1,
                    pwm_enable_default: None,
                    name: "fan1".to_string(),
                    label: None,
                    caps: caps.clone(),
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: Some(test_base_path.join("fan1_input")),
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 2,
                    pwm_enable_default: None,
                    name: "fan2".to_string(),
                    label: None,
                    caps,
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: Some(test_base_path.join("fan2_input")),
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
            ];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels,
                drivetemp: drivetemp::DrivetempState::default(),
                apple_fans: None,
                io: DeviceIo::default(),
            });

            // when:
            let (statuses, any_failure) = apple_smc.extract_fan_statuses(&driver).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            teardown(&ctx).await;
            assert!(any_failure.not());
            assert_eq!(received, vec!["fan1", "fan2"]);
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_skips_failed_channels() {
        // Verifies the sink is not invoked for a channel whose RPM
        // read fails, and any_failure is set.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            // no fan2_input, but fan2 has RPM cap -> expected read fails
            let mut fans = HashMap::new();
            fans.insert(
                1,
                AppleFanInfo {
                    max_rpm: 5000,
                    default_min_rpm: 600,
                },
            );
            fans.insert(
                2,
                AppleFanInfo {
                    max_rpm: 6000,
                    default_min_rpm: 700,
                },
            );
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans,
            };
            let caps = HwmonChannelCapabilities::APPLE_SMC
                | HwmonChannelCapabilities::FAN_WRITABLE
                | HwmonChannelCapabilities::RPM;
            let channels = vec![
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 1,
                    pwm_enable_default: None,
                    name: "fan1".to_string(),
                    label: None,
                    caps: caps.clone(),
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: Some(test_base_path.join("fan1_input")),
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    number: 2,
                    pwm_enable_default: None,
                    name: "fan2".to_string(),
                    label: None,
                    caps,
                    auto_curve: AutoCurveInfo::None,
                    pwm_path: None,
                    rpm_path: Some(test_base_path.join("fan2_input")),
                    temp_path: None,
                    read_slot: ChannelReadSlots::default(),
                },
            ];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels,
                drivetemp: drivetemp::DrivetempState::default(),
                apple_fans: None,
                io: DeviceIo::default(),
            });

            // when:
            let (statuses, any_failure) = apple_smc.extract_fan_statuses(&driver).await;
            let received: Vec<String> = statuses.into_iter().map(|s| s.name).collect();

            // then:
            teardown(&ctx).await;
            assert!(any_failure);
            assert_eq!(received, vec!["fan1"]);
        });
    }

    #[test]
    #[serial]
    fn extract_fan_statuses_empty_when_no_channels() {
        // Verifies the sink is never invoked for a driver with no
        // fan channels, and any_failure is false.
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            let test_base_path = &ctx.test_base_path;
            let apple_smc = AppleMacSMC {
                path: test_base_path.clone(),
                fans: HashMap::new(),
            };
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: vec![],
                drivetemp: drivetemp::DrivetempState::default(),
                apple_fans: None,
                io: DeviceIo::default(),
            });

            let (statuses, any_failure) = apple_smc.extract_fan_statuses(&driver).await;

            teardown(&ctx).await;
            assert!(statuses.is_empty());
            assert!(any_failure.not());
        });
    }
}
