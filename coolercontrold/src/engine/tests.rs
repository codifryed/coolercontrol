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

// ! These are somewhat "integration" tests for the control engine of CoolerControl.
// ! The setup and tests are meant to cover the main control functions as well as the
// ! interaction of the various processors and functions together.

#[cfg(test)]
mod engine_tests {
    use crate::cc_fs;
    use crate::config::Config;
    use crate::device::{
        ChannelInfo, Device, DeviceInfo, DeviceType, DeviceUID, SpeedOptions, Status, TempStatus,
        UID,
    };
    use crate::engine::main::Engine;
    use crate::repositories::repository::{DeviceList, DeviceLock, Repositories, Repository};
    use crate::setting::{
        Function, FunctionType, LcdSettings, LightingSettings, Profile, ProfileType, TempSource,
    };
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use serial_test::serial;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;

    // Mock repository for testing
    struct MockRepository {
        device_type: DeviceType,
        set_speeds: Rc<RefCell<Vec<u8>>>,
        should_fail: Rc<Cell<bool>>,
    }

    #[async_trait(?Send)]
    impl Repository for MockRepository {
        fn device_type(&self) -> DeviceType {
            self.device_type.clone()
        }

        async fn initialize_devices(&mut self) -> Result<()> {
            Ok(())
        }

        async fn devices(&self) -> DeviceList {
            Vec::new()
        }

        async fn preload_statuses(self: Rc<Self>) {}

        async fn update_statuses(&self) -> Result<()> {
            Ok(())
        }

        async fn shutdown(&self) -> Result<()> {
            Ok(())
        }

        async fn apply_setting_reset(&self, _device_uid: &UID, _channel_name: &str) -> Result<()> {
            Ok(())
        }

        async fn apply_setting_manual_control(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
        ) -> Result<()> {
            Ok(())
        }

        async fn apply_setting_speed_fixed(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
            speed_fixed: u8,
        ) -> Result<()> {
            if self.should_fail.get() {
                return Err(anyhow!("Simulated failure to apply speed"));
            }
            self.set_speeds.borrow_mut().push(speed_fixed);
            Ok(())
        }

        async fn apply_setting_speed_profile(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
            _temp_source: &TempSource,
            _speed_profile: &[(f64, u8)],
        ) -> Result<()> {
            Ok(())
        }

        async fn apply_setting_lighting(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
            _lighting: &LightingSettings,
        ) -> Result<()> {
            Err(anyhow!("Lighting is not applicable for these tests"))
        }

        async fn apply_setting_lcd(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
            _lcd: &LcdSettings,
        ) -> Result<()> {
            Err(anyhow!("LCD is not applicable for these tests"))
        }

        async fn apply_setting_pwm_mode(
            &self,
            _device_uid: &UID,
            _channel_name: &str,
            _pwm_mode: u8,
        ) -> Result<()> {
            Ok(())
        }

        async fn reinitialize_devices(&self) {}
    }

    fn setup_single_device() -> (
        DeviceLock,
        Engine,
        Rc<Config>,
        Rc<RefCell<Vec<u8>>>,
        Rc<Cell<bool>>,
    ) {
        let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
        let mut repos = Repositories::default();
        let set_speeds = Rc::new(RefCell::new(Vec::new()));
        let should_fail = Rc::new(Cell::new(false));

        // Create mock repository
        let mock_repo = Rc::new(MockRepository {
            device_type: DeviceType::Hwmon,
            set_speeds: Rc::clone(&set_speeds),
            should_fail: Rc::clone(&should_fail),
        });
        repos.hwmon = Some(mock_repo);

        let device = Rc::new(RefCell::new(Device::new(
            "Test Device".to_string(),
            DeviceType::Hwmon,
            0,
            None,
            DeviceInfo::default(),
            None,
            1.0,
        )));

        let device_uid = device.borrow().uid.clone();
        devices.insert(device_uid.clone(), Rc::clone(&device));

        let all_devices = Rc::new(devices);
        let all_repos = Rc::new(repos);
        let config = Rc::new(Config::init_default_config().unwrap());
        config.create_device_list(&all_devices);
        let engine = Engine::new(all_devices, &all_repos, Rc::clone(&config));

        (device, engine, config, set_speeds, should_fail)
    }

    #[test]
    #[serial]
    fn test_no_application_without_settings() {
        cc_fs::test_runtime(async {
            // Given
            let (_device, engine, _config, set_speeds, _should_fail) = setup_single_device();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                for _ in 0..3 {
                    engine.process_scheduled_speeds(scope);
                }
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Note: Haven't set any profiles, so none set
            assert_eq!(set_speeds.borrow().len(), 0);
        });
    }

    #[test]
    #[serial]
    fn test_simple_profile_speeds() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 100.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = "profile123".to_string();
            let profile = Profile {
                uid: profile_uid.clone(),
                name: "Test Profile".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (70.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile).unwrap();

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                let mut temp = 30.;
                // Process speeds multiple times
                for _ in 0..3 {
                    let mut status = Status::default();
                    status.temps.push(TempStatus {
                        name: temp_channel_name.clone(),
                        temp,
                    });
                    device.borrow_mut().set_status(status);
                    engine.process_scheduled_speeds(scope);
                    temp += 20.;
                }
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // speeds from profile & default function
            assert_eq!(set_speeds.borrow().clone(), vec![50, 75, 100]);
        });
    }

    #[test]
    #[serial]
    fn test_initial_application() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 50.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = "profile123".to_string();
            let profile = Profile {
                uid: profile_uid.clone(),
                name: "Test Profile".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (70.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile).unwrap();

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Should have a speed applied immediately
            assert_eq!(set_speeds.borrow().clone(), vec![75]);
        });
    }

    #[test]
    #[serial]
    fn test_safety_latch_fires() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 50.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = "profile123".to_string();
            let profile = Profile {
                uid: profile_uid.clone(),
                name: "Test Profile".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (70.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile).unwrap();

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                // Process speeds many times
                for _ in 0..32 {
                    // Safety Latch fires after 30 secs (incl. poll-rate) have passed with no duty
                    let mut status = Status::default();
                    status.temps.push(TempStatus {
                        name: temp_channel_name.clone(),
                        temp: 50.,
                    });
                    device.borrow_mut().set_status(status);
                    engine.process_scheduled_speeds(scope);
                }
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Only fires twice, once at start and once from safety latch
            assert_eq!(set_speeds.borrow().clone(), vec![75, 75]);
        });
    }

    #[test]
    #[serial]
    fn test_duty_thresholds() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 50.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Setup Function with duty thresholds
            let function_uid = "function123".to_string();
            let function = Function {
                uid: function_uid.clone(),
                name: "Function1".to_string(),
                f_type: FunctionType::Identity,
                duty_minimum: 5,
                duty_maximum: 10,
                ..Default::default()
            };
            config.set_function(function).unwrap();

            // Set up a profile
            let profile_uid = "profile123".to_string();
            let profile = Profile {
                uid: profile_uid.clone(),
                name: "Test Profile".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (100.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                function_uid,
                ..Default::default()
            };
            config.set_profile(profile).unwrap();

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                // temp change to test the minimum duty threshold
                let mut temp = 30.;
                // takes 5 iterations to hit 35 degrees,
                // which then breaks the minimum duty threshold of 5%.
                for _ in 0..5 {
                    let mut status = Status::default();
                    status.temps.push(TempStatus {
                        name: temp_channel_name.clone(),
                        temp,
                    });
                    device.borrow_mut().set_status(status);
                    engine.process_scheduled_speeds(scope);
                    temp += 1.;
                }
                // temp change to test the maximum duty threshold
                temp = 50.;
                // it takes 4 iterations using the maximum duty threshold of 10%,
                // to hit the target duty of 95%. The rest of the iterations are just to confirm
                // that the duty stays there.
                for i in 0..20 {
                    let mut status = Status::default();
                    status.temps.push(TempStatus {
                        name: temp_channel_name.clone(),
                        temp,
                    });
                    device.borrow_mut().set_status(status);
                    engine.process_scheduled_speeds(scope);
                    if i < 3 {
                        temp += 15.;
                    }
                }
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Only fires twice, once at start and once from safety latch
            assert_eq!(set_speeds.borrow().clone(), vec![50, 55, 65, 75, 85, 95]);
        });
    }

    #[test]
    #[serial]
    fn test_safety_latch_fires_despite_duty_thresholds() {
        // This tests that when the safety latch fires, that it applies whatever duty should be set.
        // This also helps to make sure the target duty is hit, even if it's 1% away from the
        // currently applied duty.
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 50.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Setup Function with duty thresholds
            let function_uid = "function123".to_string();
            let function = Function {
                uid: function_uid.clone(),
                name: "Function1".to_string(),
                f_type: FunctionType::Identity,
                duty_minimum: 5,
                duty_maximum: 10,
                ..Default::default()
            };
            config.set_function(function).unwrap();

            // Set up a profile
            let profile_uid = "profile123".to_string();
            let profile = Profile {
                uid: profile_uid.clone(),
                name: "Test Profile".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (100.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                function_uid,
                ..Default::default()
            };
            config.set_profile(profile).unwrap();

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                let mut temp = 30.;
                // A small temp change brings the duty to just under the 5% min threshold.
                // When the safety latch fires, it should be for a <5% duty change.
                for i in 0..32 {
                    let mut status = Status::default();
                    status.temps.push(TempStatus {
                        name: temp_channel_name.clone(),
                        temp,
                    });
                    device.borrow_mut().set_status(status);
                    engine.process_scheduled_speeds(scope);
                    if i == 0 {
                        temp += 2.;
                    }
                }
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Only fires twice, once at start and once from safety latch
            assert_eq!(set_speeds.borrow().clone(), vec![50, 53]);
        });
    }

    #[test]
    #[serial]
    fn test_multiple_channel_profiles() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _) = setup_single_device();

            // Create a test device with multiple temperature sensors & fans
            let fan1_channel = "fan1".to_string();
            let fan2_channel = "fan2".to_string();
            device.borrow_mut().info.channels.insert(
                fan1_channel.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            device.borrow_mut().info.channels.insert(
                fan2_channel.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );

            let temp1_channel = "temp1".to_string();
            let temp2_channel = "temp2".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp1_channel.clone(),
                temp: 50.0,
            });
            status.temps.push(TempStatus {
                name: temp2_channel.clone(),
                temp: 60.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Set up two different profiles
            let profile1_uid = "profile1".to_string();
            let profile1 = Profile {
                uid: profile1_uid.clone(),
                name: "Profile 1".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (70.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp1_channel.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile1).unwrap();

            let profile2_uid = "profile2".to_string();
            let profile2 = Profile {
                uid: profile2_uid.clone(),
                name: "Profile 2".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(40.0, 60), (60.0, 80), (80.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp2_channel.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile2).unwrap();

            // Schedule both profiles
            engine
                .set_profile(&device_uid, &fan1_channel, &profile1_uid)
                .await
                .unwrap();
            engine
                .set_profile(&device_uid, &fan2_channel, &profile2_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Both fans should have speeds applied based on their respective profiles
            // Note: due to hashmap usage, fan order is non-deterministic
            assert!(
                set_speeds.borrow().clone() == vec![80, 75]
                    || set_speeds.borrow().clone() == vec![75, 80]
            );
        });
    }

    #[test]
    #[serial]
    fn test_profile_switching() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, _) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 50.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Set up two profiles
            let profile1_uid = "profile1".to_string();
            let profile1 = Profile {
                uid: profile1_uid.clone(),
                name: "Profile 1".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (70.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile1).unwrap();

            let profile2_uid = "profile2".to_string();
            let profile2 = Profile {
                uid: profile2_uid.clone(),
                name: "Profile 2".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 30), (50.0, 50), (70.0, 70)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile2).unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                // Start with profile 1
                engine
                    .set_profile(&device_uid, &fan_channel_name, &profile1_uid)
                    .await
                    .unwrap();
                engine.process_scheduled_speeds(scope);

                // Switch to profile 2
                engine
                    .set_profile(&device_uid, &fan_channel_name, &profile2_uid)
                    .await
                    .unwrap();
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Should see speeds from both profiles
            assert_eq!(set_speeds.borrow().clone(), vec![75, 50]);
        });
    }

    #[test]
    #[serial]
    fn test_invalid_profile_handling() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, _config, set_speeds, _) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let device_uid = device.borrow().uid.clone();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                // Try to set a non-existent profile
                let result = engine
                    .set_profile(&device_uid, &fan_channel_name, &"nonexistent".to_string())
                    .await;
                engine.process_scheduled_speeds(scope);
                result
            })
            .await;

            // Then
            assert!(scope_result.is_err());
            assert_eq!(set_speeds.borrow().len(), 0);
        });
    }

    #[test]
    #[serial]
    fn test_device_failure_handling() {
        cc_fs::test_runtime(async {
            // Given
            let (device, engine, config, set_speeds, should_fail) = setup_single_device();

            // Create a test device with temperature sensor & fan
            let fan_channel_name = "fan1".to_string();
            device.borrow_mut().info.channels.insert(
                fan_channel_name.clone(),
                ChannelInfo {
                    speed_options: Some(SpeedOptions {
                        manual_profiles_enabled: true,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
            );
            let temp_channel_name = "temp1".to_string();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: temp_channel_name.clone(),
                temp: 50.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = "profile123".to_string();
            let profile = Profile {
                uid: profile_uid.clone(),
                name: "Test Profile".to_string(),
                p_type: ProfileType::Graph,
                speed_profile: Some(vec![(30.0, 50), (50.0, 75), (70.0, 100)]),
                temp_source: Some(TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                }),
                ..Default::default()
            };
            config.set_profile(profile).unwrap();

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                // First run - should succeed
                engine.process_scheduled_speeds(scope);

                // Simulate device failure & a new duty to set
                let mut status = Status::default();
                status.temps.push(TempStatus {
                    name: temp_channel_name.clone(),
                    temp: 30.,
                });
                device.borrow_mut().set_status(status);
                should_fail.set(true);
                engine.process_scheduled_speeds(scope);

                // Reset failure state & engine should retry to apply
                should_fail.set(false);
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;

            // Then
            assert!(scope_result.is_ok());
            // Should see speeds from successful attempts only
            assert_eq!(set_speeds.borrow().clone(), vec![75, 50]);
        });
    }
}
