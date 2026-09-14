// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::config::Config;
use crate::repositories::hwmon::device_io::DeviceIo;
use crate::repositories::hwmon::fans::{
    check_parsing_8, set_pwm_duty, set_pwm_enable_if_not_already, PWM_ENABLE_MANUAL_VALUE,
};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonDriverInfo};
use anyhow::{anyhow, Result};
use log::debug;
use std::path::Path;
use std::rc::Rc;

const PWM_ENABLE_THINKPAD_FULL_SPEED: u8 = 0;

/// The `thinkpad_acpi` module rejects every pwm write while its `fan_control`
/// option is off, which is the usual reason an apply fails on a `ThinkPad`.
/// Appended to those errors so the log says what to do about it.
pub const FAN_CONTROL_HINT: &str =
    " If fan control is disabled for this ThinkPad, enable it on the device's page.";

macro_rules! format_pwm_enable { ($($arg:tt)*) => {{ format!("pwm{}_enable", $($arg)*) }}; }

pub async fn apply_speed_fixed(
    config: &Rc<Config>,
    hwmon_driver: &Rc<HwmonDriverInfo>,
    channel_info: &HwmonChannelInfo,
    speed_fixed: u8,
) -> Result<()> {
    let result = if speed_fixed == 100 && config.get_settings()?.thinkpad_full_speed {
        set_to_full_speed(&hwmon_driver.path, channel_info, &hwmon_driver.io).await
    } else {
        set_manual_duty(hwmon_driver, channel_info, speed_fixed).await
    };
    result.map_err(|err| anyhow!("{err}{FAN_CONTROL_HINT}"))
}

async fn set_manual_duty(
    hwmon_driver: &Rc<HwmonDriverInfo>,
    channel_info: &HwmonChannelInfo,
    speed_fixed: u8,
) -> Result<()> {
    set_pwm_enable_if_not_already(
        PWM_ENABLE_MANUAL_VALUE,
        &hwmon_driver.path,
        channel_info,
        &hwmon_driver.io,
    )
    .await?;
    set_pwm_duty(
        &hwmon_driver.path,
        channel_info,
        speed_fixed,
        &hwmon_driver.io,
    )
    .await
    .map_err(|err| {
        anyhow!(
            "Error on {}:{} for duty {speed_fixed} - {err}",
            hwmon_driver.name,
            channel_info.name
        )
    })
}

/// This sets `pwm_enable` to 0. The effect of this is dependent on the device, but is primarily used
/// for `ThinkPads` where this means "full-speed". See:
/// [Kernel Doc](https://www.kernel.org/doc/html/latest/admin-guide/laptops/thinkpad-acpi.html#fan-control-and-monitoring-fan-speed-fan-enable-disable)
pub async fn set_to_full_speed(
    base_path: &Path,
    channel_info: &HwmonChannelInfo,
    io: &DeviceIo,
) -> Result<()> {
    // set to 100% first for consistent pwm duty-reporting behavior
    // (the driver doesn't automatically set the duty to 100% in full-speed mode)
    set_pwm_duty(base_path, channel_info, 100, io).await?;
    let path_pwm_enable = base_path.join(format_pwm_enable!(channel_info.number));
    let current_pwm_enable = io
        .read_value(&path_pwm_enable)
        .await
        .and_then(check_parsing_8)?;
    if current_pwm_enable != PWM_ENABLE_THINKPAD_FULL_SPEED {
        io.write_value(
            &path_pwm_enable,
            PWM_ENABLE_THINKPAD_FULL_SPEED.to_string().into_bytes(),
        )
                .await
                .inspect(|()| {
                    debug!("Applied pwm_enable for {} of {PWM_ENABLE_THINKPAD_FULL_SPEED}", path_pwm_enable.display());
                })
                .map_err(|err| {
                    anyhow!(
                        "Not able to set pwm_enable of {PWM_ENABLE_THINKPAD_FULL_SPEED}. \
                        Most likely because of a permissions issue or driver limitation; Error: {err}"
                    )
                })?;
    }
    Ok(())
}

/// Tests
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod tests {
    use super::*;
    use crate::cc_fs;
    use crate::config::Config;
    use crate::repositories::hwmon::fans::pwm_value_to_duty;
    use crate::repositories::hwmon::hwmon_repo::{
        AutoCurveInfo, HwmonChannelCapabilities, HwmonChannelType,
    };
    use crate::setting::CoolerControlSettings;
    use serial_test::serial;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;
    use uuid::Uuid;

    const TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

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
    fn thinkpad_apply_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_MANUAL_VALUE.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: Some(test_base_path.join("pwm1")),
                rpm_path: None,
                temp_path: None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            // set full_speed setting
            let cc_settings = CoolerControlSettings {
                thinkpad_full_speed: true,
                ..Default::default()
            };
            config.set_settings(&cc_settings);
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result = apply_speed_fixed(&config, &hwmon_info, &channel_info, 50).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, PWM_ENABLE_MANUAL_VALUE.to_string());
            assert_eq!(current_duty, 50.);
        });
    }

    // A failed write says what to do about it: fan control may be off.
    #[test]
    #[serial]
    fn thinkpad_apply_speed_error_carries_hint() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given: no pwm files, so every write fails like a locked ThinkPad
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: Some(ctx.test_base_path.join("pwm1")),
                rpm_path: None,
                temp_path: None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: ctx.test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result = apply_speed_fixed(&config, &hwmon_info, &channel_info, 50).await;

            // then:
            teardown(&ctx).await;
            let err = result.unwrap_err().to_string();
            assert!(err.contains(FAN_CONTROL_HINT), "error: {err}");
        });
    }

    #[test]
    #[serial]
    fn thinkpad_apply_speed_full_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_MANUAL_VALUE.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: Some(test_base_path.join("pwm1")),
                rpm_path: None,
                temp_path: None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            // set full_speed setting
            let cc_settings = CoolerControlSettings {
                thinkpad_full_speed: true,
                ..Default::default()
            };
            config.set_settings(&cc_settings);
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result = apply_speed_fixed(&config, &hwmon_info, &channel_info, 100).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(
                current_pwm_enable,
                PWM_ENABLE_THINKPAD_FULL_SPEED.to_string()
            );
            assert_eq!(current_duty, 100.);
        });
    }

    #[test]
    #[serial]
    fn thinkpad_apply_speed_after_full_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_THINKPAD_FULL_SPEED.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"255".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: Some(test_base_path.join("pwm1")),
                rpm_path: None,
                temp_path: None,
            };
            let config = Rc::new(Config::init_default_config().unwrap());
            // set full_speed setting
            let cc_settings = CoolerControlSettings {
                thinkpad_full_speed: true,
                ..Default::default()
            };
            config.set_settings(&cc_settings);
            let hwmon_info = Rc::new(HwmonDriverInfo {
                path: test_base_path.clone(),
                ..Default::default()
            });

            // when:
            let result = apply_speed_fixed(&config, &hwmon_info, &channel_info, 50).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(current_pwm_enable, PWM_ENABLE_MANUAL_VALUE.to_string(),);
            assert_eq!(current_duty, 50.);
        });
    }

    #[test]
    #[serial]
    fn thinkpad_set_full_speed() {
        cc_fs::test_runtime(async {
            let ctx = setup().await;
            // given:
            let test_base_path = &ctx.test_base_path;
            cc_fs::write(
                test_base_path.join("pwm1_enable"),
                PWM_ENABLE_MANUAL_VALUE.to_string().into_bytes(),
            )
            .await
            .unwrap();
            cc_fs::write(test_base_path.join("pwm1"), b"0".to_vec())
                .await
                .unwrap();
            let channel_info = HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                pwm_enable_default: Some(2),
                name: String::new(),
                label: None,
                caps: HwmonChannelCapabilities::FAN_WRITABLE,
                auto_curve: AutoCurveInfo::None,
                pwm_path: Some(test_base_path.join("pwm1")),
                rpm_path: None,
                temp_path: None,
            };

            // when:
            let result =
                set_to_full_speed(test_base_path, &channel_info, &DeviceIo::default()).await;

            // then:
            let current_pwm_enable = cc_fs::read_sysfs(&test_base_path.join("pwm1_enable"))
                .await
                .unwrap();
            let current_duty = cc_fs::read_sysfs_value(&test_base_path.join("pwm1"))
                .await
                .and_then(check_parsing_8)
                .map(pwm_value_to_duty)
                .unwrap();
            teardown(&ctx).await;
            assert!(result.is_ok());
            assert_eq!(
                current_pwm_enable,
                PWM_ENABLE_THINKPAD_FULL_SPEED.to_string()
            );
            assert_eq!(current_duty, 100.);
        });
    }
}
