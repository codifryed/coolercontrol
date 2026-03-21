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
        ChannelInfo, ChannelName, Device, DeviceInfo, DeviceType, DeviceUID, Duty, SpeedOptions,
        Status, Temp, TempName, TempStatus, UID,
    };
    use crate::engine::main::Engine;
    use crate::repositories::repository::{DeviceList, DeviceLock, Repositories, Repository};
    use crate::setting::{
        Function, FunctionType, FunctionUID, LcdSettings, LightingSettings, Profile, ProfileType,
        ProfileUID, TempSource,
    };
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use serial_test::serial;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::rc::Rc;
    use uuid::Uuid;

    // Mock repository for testing
    struct MockRepository {
        device_type: DeviceType,
        set_speeds: Rc<RefCell<Vec<u8>>>,
        should_fail: Rc<Cell<bool>>,
    }

    #[async_trait(?Send)]
    impl Repository for MockRepository {
        fn device_type(&self) -> DeviceType {
            self.device_type
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

    fn create_controllable_fan(device: &DeviceLock, fan_name: &str) -> ChannelName {
        let fan_channel_name = fan_name.to_string();
        device.borrow_mut().info.channels.insert(
            fan_channel_name.clone(),
            ChannelInfo {
                speed_options: Some(SpeedOptions {
                    fixed_enabled: true,
                    ..Default::default()
                }),
                ..Default::default()
            },
        );
        fan_channel_name
    }

    fn create_temp(device: &DeviceLock, temp_name: &str) -> TempName {
        let temp_channel_name = temp_name.to_string();
        let mut status = Status::default();
        status.temps.push(TempStatus {
            name: temp_channel_name.clone(),
            temp: 20.0,
        });
        device
            .borrow_mut()
            .initialize_status_history_with(status, 1.0);
        temp_channel_name
    }

    fn create_two_temps(
        device: &DeviceLock,
        temp1_name: &str,
        temp2_name: &str,
    ) -> (TempName, TempName) {
        let temp1_channel_name = temp1_name.to_string();
        let temp2_channel_name = temp2_name.to_string();
        let mut status = Status::default();
        status.temps.push(TempStatus {
            name: temp1_channel_name.clone(),
            temp: 20.0,
        });
        status.temps.push(TempStatus {
            name: temp2_channel_name.clone(),
            temp: 20.0,
        });
        device
            .borrow_mut()
            .initialize_status_history_with(status, 1.0);
        (temp1_channel_name, temp2_channel_name)
    }

    fn create_graph_profile_with_temp_source(
        config: &Config,
        speed_profile: Vec<(Temp, Duty)>,
        temp_source: TempSource,
    ) -> ProfileUID {
        let profile_uid = Uuid::new_v4().to_string();
        let profile = Profile {
            uid: profile_uid.clone(),
            name: "Test Profile".to_string(),
            p_type: ProfileType::Graph,
            speed_profile: Some(speed_profile),
            temp_source: Some(temp_source),
            ..Default::default()
        };
        config.set_profile(profile).unwrap();
        profile_uid
    }

    fn create_graph_profile_with_temp_source_and_function(
        config: &Config,
        speed_profile: Vec<(Temp, Duty)>,
        temp_source: TempSource,
        function_uid: &FunctionUID,
    ) -> ProfileUID {
        let profile_uid = Uuid::new_v4().to_string();
        let profile = Profile {
            uid: profile_uid.clone(),
            name: "Test Profile".to_string(),
            p_type: ProfileType::Graph,
            speed_profile: Some(speed_profile),
            temp_source: Some(temp_source),
            function_uid: function_uid.clone(),
            ..Default::default()
        };
        config.set_profile(profile).unwrap();
        profile_uid
    }

    fn create_identity_function(
        config: &Config,
        duty_minimum: u8,
        duty_maximum: u8,
    ) -> FunctionUID {
        let function_uid = Uuid::new_v4().to_string();
        let function = Function {
            uid: function_uid.clone(),
            name: "Function1".to_string(),
            f_type: FunctionType::Identity,
            step_size_min: duty_minimum,
            step_size_max: duty_maximum,
            ..Default::default()
        };
        config.set_function(function).unwrap();
        function_uid
    }

    fn create_standard_function(
        config: &Config,
        response_delay: u8,
        deviance: f64,
        only_downward: bool,
    ) -> FunctionUID {
        let function_uid = Uuid::new_v4().to_string();
        let function = Function {
            uid: function_uid.clone(),
            name: "StandardFunction".to_string(),
            f_type: FunctionType::Standard,
            step_size_min: 2,
            step_size_max: 100,
            response_delay: Some(response_delay),
            deviance: Some(deviance),
            only_downward: Some(only_downward),
            ..Default::default()
        };
        config.set_function(function).unwrap();
        function_uid
    }

    fn create_standard_function_with_steps(
        config: &Config,
        response_delay: u8,
        deviance: f64,
        only_downward: bool,
        step_size_min: Duty,
        step_size_max: Duty,
    ) -> FunctionUID {
        let function_uid = Uuid::new_v4().to_string();
        let function = Function {
            uid: function_uid.clone(),
            name: "StandardFunction".to_string(),
            f_type: FunctionType::Standard,
            step_size_min,
            step_size_max,
            response_delay: Some(response_delay),
            deviance: Some(deviance),
            only_downward: Some(only_downward),
            ..Default::default()
        };
        config.set_function(function).unwrap();
        function_uid
    }

    fn set_temp_status(device: &DeviceLock, temp_name: &TempName, temp: Temp) {
        let mut status = Status::default();
        status.temps.push(TempStatus {
            name: temp_name.clone(),
            temp,
        });
        device.borrow_mut().set_status(status);
    }

    fn set_two_temp_status(
        device: &DeviceLock,
        temp1_name: &TempName,
        temp1: Temp,
        temp2_name: &TempName,
        temp2: Temp,
    ) {
        let mut status = Status::default();
        status.temps.push(TempStatus {
            name: temp1_name.clone(),
            temp: temp1,
        });
        status.temps.push(TempStatus {
            name: temp2_name.clone(),
            temp: temp2,
        });
        device.borrow_mut().set_status(status);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 50), (50.0, 75), (70.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
            );

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
                    set_temp_status(&device, &temp_channel_name, temp);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 50), (50.0, 75), (70.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
            );

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                set_temp_status(&device, &temp_channel_name, 50.);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 50), (50.0, 75), (70.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
            );

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
                    set_temp_status(&device, &temp_channel_name, 50.);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Setup Function with duty thresholds
            let function_uid = create_identity_function(&config, 5, 10);

            // Set up a profile
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(30.0, 50), (50.0, 75), (100.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

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
                    set_temp_status(&device, &temp_channel_name, temp);
                    engine.process_scheduled_speeds(scope);
                    temp += 1.;
                }
                // temp change to test the maximum duty threshold
                temp = 50.;
                // it takes 4 iterations using the maximum duty threshold of 10%,
                // to hit the target duty of 95%. The rest of the iterations are just to confirm
                // that the duty stays there.
                for i in 0..20 {
                    set_temp_status(&device, &temp_channel_name, temp);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Setup Function with duty thresholds
            let function_uid = create_identity_function(&config, 5, 10);

            // Set up a profile
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(30.0, 50), (50.0, 75), (100.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

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
                    set_temp_status(&device, &temp_channel_name, temp);
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
            let fan1_channel = create_controllable_fan(&device, "fan1");
            let fan2_channel = create_controllable_fan(&device, "fan2");

            let (temp1_channel, temp2_channel) = create_two_temps(&device, "temp1", "temp2");
            let device_uid = device.borrow().uid.clone();

            // Set up two different profiles
            let profile1_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 50), (50.0, 75), (70.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp1_channel.clone(),
                },
            );

            let profile2_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(40.0, 60), (60.0, 80), (80.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp2_channel.clone(),
                },
            );

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
                set_two_temp_status(&device, &temp1_channel, 50., &temp2_channel, 60.);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Set up two profiles
            let profile1_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 50), (50.0, 75), (70.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
            );
            let profile2_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 30), (50.0, 50), (70.0, 70)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
            );

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                set_temp_status(&device, &temp_channel_name, 50.);
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
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
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // Set up a profile
            let profile_uid = create_graph_profile_with_temp_source(
                &config,
                vec![(30.0, 50), (50.0, 75), (70.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
            );

            // Schedule the profile
            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                set_temp_status(&device, &temp_channel_name, 50.);
                // First run - should succeed
                engine.process_scheduled_speeds(scope);

                // Simulate device failure & a new duty to set
                set_temp_status(&device, &temp_channel_name, 30.);
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

    #[test]
    #[serial]
    fn test_standard_function_zero_delay_response() {
        cc_fs::test_runtime(async {
            // Goal: verify that response_delay=0 applies speed on the very first cycle
            // after a temp change, with no extra cycle delay.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            let function_uid = create_standard_function(&config, 0, 2.0, false);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(20.0, 25), (40.0, 50), (60.0, 75), (80.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When: set temp and process once
            let scope_result = moro_local::async_scope!(|scope| {
                set_temp_status(&device, &temp_channel_name, 40.);
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;

            // Then: speed applied on first cycle
            assert!(scope_result.is_ok());
            assert_eq!(
                set_speeds.borrow().clone(),
                vec![50],
                "with response_delay=0, speed should apply on first cycle"
            );
        });
    }

    #[test]
    #[serial]
    fn test_standard_function_delay_respected() {
        cc_fs::test_runtime(async {
            // Goal: verify that response_delay=3 causes the speed to change only after
            // 3 processing cycles, not before.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            let function_uid = create_standard_function(&config, 3, 2.0, false);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(20.0, 25), (40.0, 50), (60.0, 75), (80.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When: set initial temp, then change and wait for delay
            let scope_result = moro_local::async_scope!(|scope| {
                // First cycle: applies initial temp right away (first-run path)
                set_temp_status(&device, &temp_channel_name, 20.);
                engine.process_scheduled_speeds(scope);

                // Change temp - should not apply immediately due to delay
                set_temp_status(&device, &temp_channel_name, 60.);
                engine.process_scheduled_speeds(scope);
                engine.process_scheduled_speeds(scope);

                // Third cycle after change - delay of 3 met
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;

            // Then: initial speed applied, then delayed speed after 3 cycles
            assert!(scope_result.is_ok());
            let speeds = set_speeds.borrow().clone();
            assert_eq!(
                speeds.first(),
                Some(&25),
                "first cycle should apply initial temp speed"
            );
            assert_eq!(
                speeds.last(),
                Some(&75),
                "speed should change after response delay"
            );
        });
    }

    #[test]
    #[serial]
    fn test_standard_function_spike_normalization() {
        cc_fs::test_runtime(async {
            // Goal: verify that a transient spike (outside tolerance) followed by a return
            // to baseline is normalized and does not cause a speed change.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            let function_uid = create_standard_function(&config, 3, 2.0, false);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(20.0, 25), (40.0, 50), (60.0, 75), (80.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When: establish baseline, spike, return to baseline
            let scope_result = moro_local::async_scope!(|scope| {
                // First run applies initial temp (20C from create_temp)
                set_temp_status(&device, &temp_channel_name, 40.);
                engine.process_scheduled_speeds(scope);

                // Fill stack to establish 40C as baseline
                for _ in 0..3 {
                    set_temp_status(&device, &temp_channel_name, 40.);
                    engine.process_scheduled_speeds(scope);
                }
                let speeds_before_spike = set_speeds.borrow().len();

                // Spike to 44C (outside 2.0 deviance of 40C) then return to 41C (within tolerance)
                set_temp_status(&device, &temp_channel_name, 44.);
                engine.process_scheduled_speeds(scope);
                set_temp_status(&device, &temp_channel_name, 41.);
                engine.process_scheduled_speeds(scope);
                set_temp_status(&device, &temp_channel_name, 41.);
                engine.process_scheduled_speeds(scope);

                let speeds_after_spike = set_speeds.borrow().len();
                // Speed should not have changed because the spike was normalized
                assert_eq!(
                    speeds_before_spike, speeds_after_spike,
                    "spike normalization should prevent speed change"
                );
                Ok(())
            })
            .await;

            assert!(scope_result.is_ok());
        });
    }

    #[test]
    #[serial]
    fn test_only_downward_continues_climbing_with_small_steps() {
        cc_fs::test_runtime(async {
            // Goal: verify that with only_downward=true and a small step_size_max,
            // the fan continues stepping up toward the target even when the temp
            // dips slightly below its peak. The duty-based comparison should keep
            // bypassing hysteresis as long as the curve demands a higher duty.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // step_size_max=5 so the fan can only climb 5% per cycle.
            // step_size_min=2 so small changes are still applied.
            let function_uid = create_standard_function_with_steps(&config, 0, 2.0, true, 2, 5);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(20.0, 25), (40.0, 50), (60.0, 75), (80.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // Process cycles with separate scopes so spawned speed tasks complete.
            // Jump to 60C, then dip to 58C. The fan should keep climbing.
            set_temp_status(&device, &temp_channel_name, 60.);
            moro_local::async_scope!(|scope| {
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await
            .unwrap();

            for _ in 0..15 {
                set_temp_status(&device, &temp_channel_name, 58.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let speeds = set_speeds.borrow().clone();
            assert!(!speeds.is_empty(), "at least one speed should be applied");
            let final_duty = *speeds.last().unwrap();
            // At 58C, curve interpolates to ~73%. With the first cycle applying
            // 75% (at 60C), duty stays at 75% since target (73) < current (75).
            assert!(
                final_duty >= 70,
                "fan should reach near target duty (~73%), got {final_duty}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_only_downward_delays_decrease() {
        cc_fs::test_runtime(async {
            // Goal: verify that when temp drops with only_downward=true,
            // hysteresis delay IS respected and fan duty does NOT drop
            // immediately. The duty-based bypass should NOT fire because
            // the target duty is lower than the current duty.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // response_delay=5 gives ideal_stack_size=5 (5s / 1s poll_rate)
            let function_uid = create_standard_function_with_steps(&config, 5, 2.0, true, 2, 100);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(20.0, 25), (50.0, 50), (90.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // Warm up at 80C: first cycle applies immediately (first-run path),
            // subsequent cycles fill the hysteresis stack.
            for _ in 0..7 {
                set_temp_status(&device, &temp_channel_name, 80.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let warmup_speeds = set_speeds.borrow().clone();
            assert!(
                !warmup_speeds.is_empty(),
                "warmup should apply at least one speed"
            );
            let warmup_duty = *warmup_speeds.last().unwrap();
            let warmup_speed_count = warmup_speeds.len();

            // Drop temp to 50C. Run 3 cycles (fewer than response_delay=5).
            // The hysteresis stack still has 80C entries at the front,
            // so duty should NOT change yet.
            for _ in 0..3 {
                set_temp_status(&device, &temp_channel_name, 50.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let after_partial_drop = set_speeds.borrow().clone();
            // No new speeds should have been applied during the partial drop.
            assert_eq!(
                after_partial_drop.len(),
                warmup_speed_count,
                "no new speeds should be applied before delay elapses"
            );
            assert_eq!(
                *after_partial_drop.last().unwrap(),
                warmup_duty,
                "duty should NOT have dropped yet (hysteresis delay not elapsed)"
            );

            // Run 2 more cycles at 50C (total 5 since drop).
            // Now the stack is fully flushed with 50C and duty should change.
            for _ in 0..2 {
                set_temp_status(&device, &temp_channel_name, 50.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let final_speeds = set_speeds.borrow().clone();
            let final_duty = *final_speeds.last().unwrap();
            assert!(
                final_duty < warmup_duty,
                "duty should have dropped after hysteresis delay elapsed, \
                 warmup={warmup_duty}, final={final_duty}"
            );
            assert_eq!(
                final_duty, 50,
                "duty should match the profile target at 50C"
            );
        });
    }

    #[test]
    #[serial]
    fn test_only_downward_no_oscillation_with_temp_noise() {
        cc_fs::test_runtime(async {
            // Goal: verify that after a downward duty step, small temperature
            // fluctuations do NOT cause the fan to oscillate up and down.
            // The bypass requires target_duty >= last_duty + step_min, so
            // noise that creates duty diffs below step_min is filtered out.
            // Profile slope at 50C: (100-50)/(90-50) = 1.25 duty/degree.
            // Noise of ±2C = ~2.5% duty change, below step_min=5.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // step_size_min=5 so bypass needs >= 5% duty diff to fire.
            // ±2C noise creates ~2.5% duty diff, well below threshold.
            let function_uid = create_standard_function_with_steps(&config, 3, 2.0, true, 5, 100);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                vec![(20.0, 25), (50.0, 50), (90.0, 100)],
                TempSource {
                    device_uid: device_uid.clone(),
                    temp_name: temp_channel_name.clone(),
                },
                &function_uid,
            );

            engine
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // Warm up at 80C.
            for _ in 0..5 {
                set_temp_status(&device, &temp_channel_name, 80.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            // Wait for delay to elapse at 50C so duty drops.
            for _ in 0..5 {
                set_temp_status(&device, &temp_channel_name, 50.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let duty_after_drop = *set_speeds.borrow().last().unwrap();
            let count_after_drop = set_speeds.borrow().len();

            // Simulate 10 cycles of small temp noise around 50C (±2C).
            // With deviance=2.0, these are within tolerance. The bypass
            // threshold (step_min=5) prevents bypass for the ~2.5% duty
            // differences this noise creates.
            let noise_temps = [51., 49., 52., 48., 50., 51., 49., 50., 52., 50.];
            for &temp in &noise_temps {
                set_temp_status(&device, &temp_channel_name, temp);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let final_count = set_speeds.borrow().len();
            // No new speeds should have been applied during noise period.
            assert_eq!(
                final_count, count_after_drop,
                "temp noise should not cause any duty changes"
            );
            assert_eq!(
                *set_speeds.borrow().last().unwrap(),
                duty_after_drop,
                "duty should remain stable despite temp noise"
            );
        });
    }
}
