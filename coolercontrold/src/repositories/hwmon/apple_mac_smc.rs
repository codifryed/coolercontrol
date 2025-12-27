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
use crate::device::{ChannelName, ChannelStatus, Duty, RPM};
use crate::repositories::hwmon::devices::DEVICE_NAME_MAC_SMC;
use crate::repositories::hwmon::fans;
use crate::repositories::hwmon::hwmon_repo::{
    AutoCurveInfo, HwmonChannelCapabilities, HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
};
use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, warn};
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
const PATTERN_FAN_TARGET_FILE_NUMBER: &str = r"^fan(?P<number>\d+)_target$";
macro_rules! format_fan_manual { ($($arg:tt)*) => {{ format!("fan{}_manual", $($arg)*) }}; }
macro_rules! format_fan_min { ($($arg:tt)*) => {{ format!("fan{}_min", $($arg)*) }}; }
macro_rules! format_fan_max { ($($arg:tt)*) => {{ format!("fan{}_max", $($arg)*) }}; }
macro_rules! format_fan_output { ($($arg:tt)*) => {{ format!("fan{}_output", $($arg)*) }}; }
/// `macsmc-hwmon` uses a more appropriate `fanN_target` instead of `fanN_output`
macro_rules! format_fan_target { ($($arg:tt)*) => {{ format!("fan{}_target", $($arg)*) }}; }

/// This is a `HWMon` repository extension for Apple hardware supported by the Linux Kernel.
///
/// In particular the `applesmc` driver, which used in Intel-based Apple computers.
/// See: `https://github.com/torvalds/linux/blob/master/drivers/hwmon/applesmc.c`
///
/// Apple Silicon support (M1+) will is supported in the 6.19 kernel.
/// See `https://github.com/torvalds/linux/blob/master/drivers/hwmon/macsmc-hwmon.c`
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct AppleMacSMC {
    pub detected: bool,
    path: PathBuf,
    is_mac_smc: bool,
    fans: HashMap<u8, AppleFanInfo>,
}

impl AppleMacSMC {}

impl AppleMacSMC {
    pub async fn new(path: &Path, channels: &Vec<HwmonChannelInfo>, device_name: &str) -> Self {
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
            detected: true,
            path: path.to_path_buf(),
            is_mac_smc: device_name == DEVICE_NAME_MAC_SMC,
            fans,
        }
    }

    pub fn not_applicable() -> Self {
        Self::default()
    }

    pub async fn init_fans(
        base_path: &Path,
        channels: &mut Vec<HwmonChannelInfo>,
        disabled_channels: &[ChannelName],
    ) {
        match Self::init_apple_fans(base_path).await {
            Ok(fans) => channels.extend(
                fans.into_iter()
                    .filter(|fan| disabled_channels.contains(&fan.name).not())
                    .collect::<Vec<HwmonChannelInfo>>(),
            ),
            Err(err) => error!("Error initializing Apple Mac SMC Fans: {err}"),
        }
    }

    async fn init_apple_fans(base_path: &Path) -> Result<Vec<HwmonChannelInfo>> {
        let dir_entries = cc_fs::read_dir(base_path)?;
        let mut fan_caps = HashMap::new();
        for entry in dir_entries {
            let os_file_name = entry?.file_name();
            let file_name = os_file_name.to_str().context("File Name should be a str")?;
            Self::detect_apple_smc_fans(base_path, file_name, &mut fan_caps).await?;
            fans::detect_rpm(base_path, file_name, &mut fan_caps).await?;
        }
        let mut fans = Self::caps_to_hwmon_fans(base_path, fan_caps).await?;
        fans.sort_by(|c1, c2| c1.number.cmp(&c2.number));
        debug!(
            "Apple SMC fans detected: {fans:?} for {}",
            base_path.display()
        );
        Ok(fans)
    }

    /// Detects which fans of this `applesmc` or `macsmc-hwmon` device are controllable
    async fn detect_apple_smc_fans(
        base_path: &Path,
        file_name: &str,
        fan_caps: &mut HashMap<u8, HwmonChannelCapabilities>,
    ) -> Result<()> {
        let regex_output_file = Regex::new(PATTERN_FAN_OUTPUT_FILE_NUMBER)?;
        let regex_target_file = Regex::new(PATTERN_FAN_TARGET_FILE_NUMBER)?;
        let mut is_output_file = false;
        let channel_number: u8 = {
            if regex_output_file.is_match(file_name) {
                is_output_file = true;
                regex_output_file
                    .captures(file_name)
                    .context("Fan Number should exist")?
                    .name("number")
                    .context("Number Group should exist")?
                    .as_str()
                    .parse()?
            } else {
                if regex_target_file.is_match(file_name).not() {
                    return Ok(()); // skip if not an applicable fan file
                }
                regex_target_file
                    .captures(file_name)
                    .context("Fan Number should exist")?
                    .name("number")
                    .context("Number Group should exist")?
                    .as_str()
                    .parse()?
            }
        };
        if is_output_file {
            if Self::fan_output_is_writable(base_path, channel_number).not() {
                return Ok(()); // skip if fan_output file isn't writable
            }
        } else if Self::fan_target_is_writable(base_path, channel_number).not() {
            return Ok(()); // skip if fan_target file isn't writable
        }
        if fans::get_fan_rpm(base_path, &channel_number, true)
            .await
            .is_none()
        {
            return Ok(()); // skip if fan_input file isn't readable (no indicator of speed)
        }
        if Self::fan_manual_is_writable(base_path, channel_number).not() {
            return Ok(()); // skip if fan_manual file isn't writable
        }
        if Self::get_fan_max(base_path, channel_number, true)
            .await
            .is_none()
        {
            return Ok(()); // skip if fan_max file isn't readable
        }
        if Self::get_fan_min(base_path, channel_number, true)
            .await
            .is_none()
        {
            return Ok(()); // skip if fan_min file isn't readable
        }
        let _ = Self::fan_min_is_writable(base_path, channel_number); // check for writability
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

    fn fan_output_is_writable(base_path: &Path, channel_number: u8) -> bool {
        let output_path = base_path.join(format_fan_output!(channel_number));
        let output_writable = cc_fs::metadata(&output_path)
            .inspect_err(|_| {
                error!(
                    "Fan_output file metadata is not readable: {}",
                    output_path.display()
                );
            })
            // This check should be sufficient, as we're running as root:
            .is_ok_and(|att| att.permissions().readonly().not());
        if output_writable.not() {
            warn!(
                "Apple SMC fan at {} is NOT writable - \
            Fan control is not currently supported by the installed driver.",
                output_path.display()
            );
        }
        output_writable
    }

    fn fan_target_is_writable(base_path: &Path, channel_number: u8) -> bool {
        let target_path = base_path.join(format_fan_target!(channel_number));
        let target_writable = cc_fs::metadata(&target_path)
            .inspect_err(|_| {
                error!(
                    "Fan_target file metadata is not readable: {}",
                    target_path.display()
                );
            })
            // This check should be sufficient, as we're running as root:
            .is_ok_and(|att| att.permissions().readonly().not());
        if target_writable.not() {
            warn!(
                "Mac SMC fan at {} is NOT writable - \
            Fan control is not currently supported by the installed driver.",
                target_path.display()
            );
        }
        target_writable
    }

    fn fan_manual_is_writable(base_path: &Path, channel_number: u8) -> bool {
        let manual_path = base_path.join(format_fan_manual!(channel_number));
        let manual_writable = cc_fs::metadata(&manual_path)
            .inspect_err(|_| {
                error!(
                    "Fan_manual file metadata is not readable: {}",
                    manual_path.display()
                );
            })
            // This check should be sufficient, as we're running as root:
            .is_ok_and(|att| att.permissions().readonly().not());
        if manual_writable.not() {
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
            if fan_cap.is_non_controllable_rpm_fan() {
                info!(
                    "Uncontrollable RPM-only fan found at {}/fan{channel_number}_input",
                    base_path.display()
                );
            }
            fans.push(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: channel_number,
                pwm_enable_default: None,
                name: channel_name,
                label,
                caps: fan_cap,
                auto_curve: AutoCurveInfo::None,
            });
        }
        Ok(fans)
    }

    pub async fn extract_fan_statuses(&self, driver: &Rc<HwmonDriverInfo>) -> Vec<ChannelStatus> {
        let mut fans = vec![];
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Fan {
                continue;
            }
            let fan_duty = if channel.caps.is_apple_smc() {
                self.get_fan_duty(channel.number).await
            } else {
                None
            };
            let fan_rpm = if channel.caps.has_rpm() {
                fans::get_fan_rpm(&driver.path, &channel.number, false).await
            } else {
                None
            };
            fans.push(ChannelStatus {
                name: channel.name.clone(),
                rpm: fan_rpm,
                duty: fan_duty,
                ..Default::default()
            });
        }
        fans
    }
    pub async fn set_to_auto_control(&self, channel_number: u8) -> Result<()> {
        let fan_min_default = self
            .fans
            .get(&channel_number)
            .map_or(DEFAULT_MIN_FAN_SPEED, |info| info.default_min_rpm);
        let fan_min_path = self.path.join(format_fan_min!(channel_number));
        let fan_manual_path = self.path.join(format_fan_manual!(channel_number));
        if let Err(e) = cc_fs::write_string(&fan_min_path, fan_min_default.to_string()).await {
            warn!(
                "Unable to set Fan Min value {fan_min_default} for {} Reason: {e}",
                fan_min_path.display()
            );
        }
        cc_fs::write_string(&fan_manual_path, FAN_AUTO_CONTROL.to_string())
            .await
            .map_err(|err| {
                anyhow!(
                    "Unable to set Fan Manual value {FAN_AUTO_CONTROL} for {} Reason: {err}",
                    fan_min_path.display()
                )
            })
    }

    pub async fn set_to_manual_control(&self, channel_number: u8) -> Result<()> {
        let fan_min_path = self.path.join(format_fan_min!(channel_number));
        let fan_manual_path = self.path.join(format_fan_manual!(channel_number));
        if let Err(e) = cc_fs::write_string(&fan_min_path, "0".to_string()).await {
            warn!(
                "Unable to set Fan Min value 0 for {}. The driver will not allow you to set fan speeds to 0. Reason: {e}",
                fan_min_path.display()
            );
        }
        cc_fs::write_string(&fan_manual_path, FAN_MANUAL_CONTROL.to_string())
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
        cc_fs::read_sysfs(&fan_min_path)
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
        cc_fs::read_sysfs(&fan_max_path)
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

    pub async fn get_fan_duty(&self, channel_number: u8) -> Option<f64> {
        fans::get_fan_rpm(&self.path, &channel_number, false)
            .await
            .and_then(|rpm| self.interpolate_duty_from_rpm(channel_number, rpm))
    }

    pub async fn set_fan_duty(&self, channel_number: u8, speed: Duty) -> Result<()> {
        let rpm = self.interpolate_rpm_from_duty(channel_number, speed);
        if self.is_mac_smc {
            Self::set_fan_target(&self.path, channel_number, rpm).await
        } else {
            Self::set_fan_output(&self.path, channel_number, rpm).await
        }
    }

    async fn set_fan_output(path: &Path, channel_number: u8, rpm: RPM) -> Result<()> {
        let fan_output_path = path.join(format_fan_output!(channel_number));
        cc_fs::write_string(&fan_output_path, rpm.to_string())
            .await
            .map_err(|err| {
                anyhow!(
                    "Unable to set Fan Output value {rpm} for {} Reason: {err}",
                    fan_output_path.display()
                )
            })
    }

    async fn set_fan_target(path: &Path, channel_number: u8, rpm: RPM) -> Result<()> {
        let fan_target_path = path.join(format_fan_target!(channel_number));
        cc_fs::write_string(&fan_target_path, rpm.to_string())
            .await
            .map_err(|err| {
                anyhow!(
                    "Unable to set Fan Target value {rpm} for {} Reason: {err}",
                    fan_target_path.display()
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
    use serial_test::serial;
    use std::path::{Path, PathBuf};
    use uuid::Uuid;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    struct HwmonFileContext {
        test_base_path: PathBuf,
    }

    fn setup() -> HwmonFileContext {
        let test_base_path =
            Path::new(&(TEST_BASE_PATH_STR.to_string() + &Uuid::new_v4().to_string()))
                .to_path_buf();
        cc_fs::create_dir_all(&test_base_path).unwrap();
        HwmonFileContext { test_base_path }
    }

    fn teardown(ctx: &HwmonFileContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).unwrap();
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            detected: true,
            path: PathBuf::new(),
            is_mac_smc: false,
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
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            let mut fan_caps = HashMap::new();

            // when:
            let result =
                AppleMacSMC::detect_apple_smc_fans(test_base_path, "fan1_input", &mut fan_caps)
                    .await;

            // then:
            teardown(&ctx);
            assert!(result.is_ok());
            assert!(fan_caps.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_detect_apple_smc_fans_complete() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            teardown(&ctx);
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
            let ctx = setup();
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
            teardown(&ctx);
            assert!(result.is_ok());
            assert!(fan_caps.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_init_apple_fans() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            let result = AppleMacSMC::init_apple_fans(test_base_path).await;

            // then:
            teardown(&ctx);
            assert!(result.is_ok());
            let fans = result.unwrap();
            assert_eq!(fans.len(), 2);
            assert_eq!(fans[0].number, 1);
            assert_eq!(fans[0].name, "fan1");
            assert_eq!(fans[1].number, 2);
            assert_eq!(fans[1].name, "fan2");
        });
    }

    #[test]
    #[serial]
    fn test_init_fans_with_disabled_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            let mut channels = vec![];
            let disabled_channels = vec!["fan1".to_string()];

            // when:
            AppleMacSMC::init_fans(test_base_path, &mut channels, &disabled_channels).await;

            // then:
            teardown(&ctx);
            assert!(channels.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_min() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::get_fan_min(test_base_path, 1, false).await;

            // then:
            teardown(&ctx);
            assert_eq!(result, Some(600));
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_min_edge_case_max_value() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            teardown(&ctx);
            assert_eq!(result, Some(0));
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_max() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_max"), b"6500".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::get_fan_max(test_base_path, 1, false).await;

            // then:
            teardown(&ctx);
            assert_eq!(result, Some(6500));
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_max_edge_case_max_value() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            teardown(&ctx);
            assert_eq!(result, Some(0));
        });
    }

    #[test]
    #[serial]
    fn test_set_to_auto_control() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
                fans,
            };

            // when:
            let result = apple_smc.set_to_auto_control(1).await;

            // then:
            let fan_min = cc_fs::read_sysfs(test_base_path.join("fan1_min"))
                .await
                .unwrap();
            let fan_manual = cc_fs::read_sysfs(test_base_path.join("fan1_manual"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(fan_min.trim(), "600");
            assert_eq!(fan_manual.trim(), "0");
        });
    }

    #[test]
    #[serial]
    fn test_set_to_manual_control() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
                fans,
            };

            // when:
            let result = apple_smc.set_to_manual_control(1).await;

            // then:
            let fan_min = cc_fs::read_sysfs(test_base_path.join("fan1_min"))
                .await
                .unwrap();
            let fan_manual = cc_fs::read_sysfs(test_base_path.join("fan1_manual"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(fan_min.trim(), "0");
            assert_eq!(fan_manual.trim(), "1");
        });
    }

    #[test]
    #[serial]
    fn test_set_fan_duty() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
                fans,
            };

            // when:
            let result = apple_smc.set_fan_duty(1, 50).await;

            // then:
            let fan_output = cc_fs::read_sysfs(test_base_path.join("fan1_output"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(fan_output.trim(), "2500");
        });
    }

    #[test]
    #[serial]
    fn test_get_fan_duty() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
                fans,
            };

            // when:
            let result = apple_smc.get_fan_duty(1).await;

            // then:
            teardown(&ctx);
            assert_eq!(result, Some(50.0));
        });
    }

    #[test]
    #[serial]
    fn test_extract_fan_statuses() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
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
                },
            ];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: channels.clone(),
                block_dev_path: None,
                apple_smc: AppleMacSMC::not_applicable(),
            });

            // when:
            let statuses = apple_smc.extract_fan_statuses(&driver).await;

            // then:
            teardown(&ctx);
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
            let ctx = setup();
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
            }];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels, "applesmc").await;

            // then:
            teardown(&ctx);
            assert!(apple_smc.detected);
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
            let ctx = setup();
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
            }];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels, "applesmc").await;

            // then:
            teardown(&ctx);
            assert!(apple_smc.detected);
            assert_eq!(apple_smc.fans.len(), 1);
            let fan_info = apple_smc.fans.get(&1).unwrap();
            assert_eq!(fan_info.max_rpm, DEFAULT_MAX_FAN_SPEED);
            assert_eq!(fan_info.default_min_rpm, DEFAULT_MIN_FAN_SPEED);
        });
    }

    #[test]
    #[serial]
    fn test_not_applicable() {
        // when:
        let apple_smc = AppleMacSMC::not_applicable();

        // then:
        assert!(!apple_smc.detected);
        assert!(apple_smc.fans.is_empty());
    }

    #[test]
    #[serial]
    fn test_fan_output_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"2500".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_output_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_output_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_output_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_target_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_target"), b"2500".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_target_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_target_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_target_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_manual_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_manual"), b"0".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_manual_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_manual_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_manual_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_min_is_writable() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_min"), b"600".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::fan_min_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(result);
        });
    }

    #[test]
    #[serial]
    fn test_fan_min_is_writable_not_exists() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;

            // when:
            let result = AppleMacSMC::fan_min_is_writable(test_base_path, 1);

            // then:
            teardown(&ctx);
            assert!(!result);
        });
    }

    #[test]
    #[serial]
    fn test_set_fan_output() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_output"), b"0".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::set_fan_output(test_base_path, 1, 3000).await;

            // then:
            let fan_output = cc_fs::read_sysfs(test_base_path.join("fan1_output"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(fan_output.trim(), "3000");
        });
    }

    #[test]
    #[serial]
    fn test_set_fan_target() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_target"), b"0".to_vec())
                .await
                .unwrap();

            // when:
            let result = AppleMacSMC::set_fan_target(test_base_path, 1, 3500).await;

            // then:
            let fan_target = cc_fs::read_sysfs(test_base_path.join("fan1_target"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(fan_target.trim(), "3500");
        });
    }

    #[test]
    #[serial]
    fn test_set_fan_duty_mac_smc() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_target"), b"0".to_vec())
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: true,
                fans,
            };

            // when:
            let result = apple_smc.set_fan_duty(1, 50).await;

            // then:
            let fan_target = cc_fs::read_sysfs(test_base_path.join("fan1_target"))
                .await
                .unwrap();
            teardown(&ctx);
            assert!(result.is_ok());
            assert_eq!(fan_target.trim(), "2500");
        });
    }

    #[test]
    #[serial]
    fn test_detect_apple_smc_fans_with_target_file() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_target"), b"2500".to_vec())
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
                AppleMacSMC::detect_apple_smc_fans(test_base_path, "fan1_target", &mut fan_caps)
                    .await;

            // then:
            teardown(&ctx);
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
    fn test_caps_to_hwmon_fans() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            teardown(&ctx);
            assert!(result.is_ok());
            let fans = result.unwrap();
            assert_eq!(fans.len(), 2);
        });
    }

    #[test]
    #[serial]
    fn test_caps_to_hwmon_fans_with_label() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            teardown(&ctx);
            assert!(result.is_ok());
            let fans = result.unwrap();
            assert_eq!(fans.len(), 1);
            assert_eq!(fans[0].label, Some("Left Fan".to_string()));
        });
    }

    #[test]
    #[serial]
    fn test_new_mac_smc_device() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
            }];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels, DEVICE_NAME_MAC_SMC).await;

            // then:
            teardown(&ctx);
            assert!(apple_smc.detected);
            assert!(apple_smc.is_mac_smc);
            assert_eq!(apple_smc.fans.len(), 1);
        });
    }

    #[test]
    #[serial]
    fn test_new_skips_non_apple_smc_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Temp,
                    number: 1,
                    pwm_enable_default: None,
                    name: "temp1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::APPLE_SMC,
                    auto_curve: AutoCurveInfo::None,
                },
            ];

            // when:
            let apple_smc = AppleMacSMC::new(test_base_path, &channels, "applesmc").await;

            // then:
            teardown(&ctx);
            assert!(apple_smc.detected);
            assert!(apple_smc.fans.is_empty());
        });
    }

    #[test]
    #[serial]
    fn test_extract_fan_statuses_skips_non_fan_channels() {
        cc_fs::test_runtime(async {
            let ctx = setup();
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
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
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
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Temp,
                    number: 1,
                    pwm_enable_default: None,
                    name: "temp1".to_string(),
                    label: None,
                    caps: HwmonChannelCapabilities::empty(),
                    auto_curve: AutoCurveInfo::None,
                },
            ];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: channels.clone(),
                block_dev_path: None,
                apple_smc: AppleMacSMC::not_applicable(),
            });

            // when:
            let statuses = apple_smc.extract_fan_statuses(&driver).await;

            // then:
            teardown(&ctx);
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].name, "fan1");
        });
    }

    #[test]
    #[serial]
    fn test_extract_fan_statuses_rpm_only_fan() {
        cc_fs::test_runtime(async {
            let ctx = setup();
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(test_base_path.join("fan1_input"), b"2500".to_vec())
                .await
                .unwrap();
            let apple_smc = AppleMacSMC {
                detected: true,
                path: test_base_path.clone(),
                is_mac_smc: false,
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
            }];
            let driver = Rc::new(HwmonDriverInfo {
                name: "applesmc".to_string(),
                path: test_base_path.clone(),
                model: None,
                u_id: "test_uid".to_string(),
                channels: channels.clone(),
                block_dev_path: None,
                apple_smc: AppleMacSMC::not_applicable(),
            });

            // when:
            let statuses = apple_smc.extract_fan_statuses(&driver).await;

            // then:
            teardown(&ctx);
            assert_eq!(statuses.len(), 1);
            assert_eq!(statuses[0].name, "fan1");
            assert_eq!(statuses[0].rpm, Some(2500));
            assert_eq!(statuses[0].duty, None); // No duty for RPM-only fan
        });
    }
}
