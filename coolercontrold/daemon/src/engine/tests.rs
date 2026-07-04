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
        ChannelInfo, ChannelKind, ChannelName, Device, DeviceInfo, DeviceType, DeviceUID, Duty,
        SpeedOptions, Status, Temp, TempName, TempStatus, UID,
    };
    use crate::engine::main::Engine;
    use crate::repositories::repository::{DeviceList, DeviceLock, Repositories, Repository};
    use crate::setting::{
        Function, FunctionKind, FunctionUID, LcdSettings, LightingSettings, Profile, ProfileKind,
        ProfileUID, TempSource,
    };
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use serial_test::serial;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;
    use std::ops::Not;
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
        // Empty store + empty state map means every channel is uncalibrated,
        // so calibration::dispatch passes the duty straight through to the
        // mock repository. Existing engine assertions stay intact.
        let calibration_store = Rc::new(crate::calibration::CalibrationStore::empty());
        let fan_state_map = Rc::new(crate::calibration::FanStateMap::new());
        let engine = Engine::new(
            all_devices,
            &all_repos,
            Rc::clone(&config),
            calibration_store,
            fan_state_map,
            Rc::new(crate::overrides::OverridesController::empty()),
        );

        (device, engine, config, set_speeds, should_fail)
    }

    fn create_controllable_fan(device: &DeviceLock, fan_name: &str) -> ChannelName {
        let fan_channel_name = fan_name.to_string();
        device.borrow_mut().info.channels.insert(
            fan_channel_name.clone(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    fixed_enabled: true,
                    ..Default::default()
                }),
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
            kind: ProfileKind::Graph {
                speed_profile: Some(speed_profile),
                temp_source: Some(temp_source),
                temp_min: None,
                temp_max: None,
            },
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
            function_uid: function_uid.clone(),
            kind: ProfileKind::Graph {
                speed_profile: Some(speed_profile),
                temp_source: Some(temp_source),
                temp_min: None,
                temp_max: None,
            },
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
            step_size_min: 2,
            step_size_max: 100,
            kind: FunctionKind::Standard {
                deviance: Some(deviance),
                only_downward: Some(only_downward),
                response_delay: Some(response_delay),
            },
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
            step_size_min,
            step_size_max,
            kind: FunctionKind::Standard {
                deviance: Some(deviance),
                only_downward: Some(only_downward),
                response_delay: Some(response_delay),
            },
            ..Default::default()
        };
        config.set_function(function).unwrap();
        function_uid
    }

    fn create_identity_function_with_bypass(
        config: &Config,
        step_size_min: Duty,
        step_size_max: Duty,
        bypass_min_at_extremes: bool,
    ) -> FunctionUID {
        let function_uid = Uuid::new_v4().to_string();
        let function = Function {
            uid: function_uid.clone(),
            name: "BypassFunction".to_string(),
            step_size_min,
            step_size_max,
            bypass_min_at_extremes,
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

    #[test]
    #[serial]
    fn test_bypass_min_at_extremes_reaches_100_below_min_diff() {
        cc_fs::test_runtime(async {
            // Goal: end-to-end verification that bypass_min_at_extremes lets
            // the fan reach exactly 100% even when the final jump is below
            // step_size_min. With bypass disabled the fan would stay at the
            // last applied duty under 100.
            let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
            let fan_channel_name = create_controllable_fan(&device, "fan1");
            let temp_channel_name = create_temp(&device, "temp1");
            let device_uid = device.borrow().uid.clone();

            // step_size_min=20 so a +5 final jump would normally be filtered.
            let function_uid = create_identity_function_with_bypass(&config, 20, 100, true);
            let profile_uid = create_graph_profile_with_temp_source_and_function(
                &config,
                // Steep slope at the top: 80C -> 95%, 90C -> 100%.
                vec![(20.0, 0), (80.0, 95), (90.0, 100)],
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

            // Tick 1: 80C, target=95. First-application path applies 95.
            set_temp_status(&device, &temp_channel_name, 80.);
            moro_local::async_scope!(|scope| {
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await
            .unwrap();

            // Subsequent ticks at 90C: target=100, abs_diff=5, below step_min=20.
            // Without bypass this would stay at 95; with bypass it must reach 100.
            for _ in 0..3 {
                set_temp_status(&device, &temp_channel_name, 90.);
                moro_local::async_scope!(|scope| {
                    engine.process_scheduled_speeds(scope);
                    Ok(())
                })
                .await
                .unwrap();
            }

            let speeds = set_speeds.borrow().clone();
            assert!(!speeds.is_empty(), "at least one speed should be applied");
            assert_eq!(
                *speeds.last().unwrap(),
                100,
                "bypass should let the fan reach exactly 100% despite step_size_min=20"
            );
        });
    }

    /// Setup variant that returns an `Rc` handle to the engine's
    /// `CalibrationStore` so the test can inject calibration data
    /// after construction and observe the effect on the engine.
    #[test]
    fn log_device_channel_resolves_names() {
        // Goal: the engine's log form applies user overrides to both parts
        // (`Device (raw) | Channel (raw)`), keeps raw parts without an
        // override, and falls back to the UID for unknown devices.
        crate::rt::test_runtime(async {
            let device = Rc::new(RefCell::new(Device::new(
                "nct6798".to_string(),
                DeviceType::Hwmon,
                0,
                None,
                DeviceInfo::default(),
                None,
                1.0,
            )));
            let device_uid = device.borrow().uid.clone();
            let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
            devices.insert(device_uid.clone(), device);
            let all_devices = Rc::new(devices);
            let config = Rc::new(Config::init_default_config().unwrap());
            config.create_device_list(&all_devices);

            let tmp = tempfile::tempdir().unwrap();
            let overrides = Rc::new(
                crate::overrides::OverridesController::init_from(tmp.path().join("overrides.toml"))
                    .await,
            );
            overrides
                .set_device_name(&device_uid, "hint", Some("Motherboard"))
                .await
                .unwrap();
            overrides
                .set_channel_label(
                    &device_uid,
                    "hint",
                    &"fan1".to_string(),
                    None,
                    Some("Front Intake"),
                )
                .await
                .unwrap();

            let engine = Engine::new(
                Rc::clone(&all_devices),
                &Rc::new(Repositories::default()),
                config,
                Rc::new(crate::calibration::CalibrationStore::empty()),
                Rc::new(crate::calibration::FanStateMap::new()),
                overrides,
            );

            assert_eq!(
                engine.log_device_channel(&device_uid, "fan1"),
                "Motherboard (nct6798) | Front Intake (fan1)"
            );
            assert_eq!(
                engine.log_device_channel(&device_uid, "fan2"),
                "Motherboard (nct6798) | fan2"
            );
            let unknown_uid = "unknown-uid".to_string();
            assert_eq!(
                engine.log_device_channel(&unknown_uid, "fan1"),
                "unknown-uid | fan1"
            );
        });
    }

    fn setup_calibrated_device() -> (DeviceLock, Engine, Rc<crate::calibration::CalibrationStore>) {
        let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
        let mut repos = Repositories::default();
        let set_speeds = Rc::new(RefCell::new(Vec::new()));
        let should_fail = Rc::new(Cell::new(false));

        let mock_repo = Rc::new(MockRepository {
            device_type: DeviceType::Hwmon,
            set_speeds,
            should_fail,
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
        devices.insert(device_uid, Rc::clone(&device));

        let all_devices = Rc::new(devices);
        let all_repos = Rc::new(repos);
        let config = Rc::new(Config::init_default_config().unwrap());
        config.create_device_list(&all_devices);

        let calibration_store = Rc::new(crate::calibration::CalibrationStore::empty());
        let fan_state_map = Rc::new(crate::calibration::FanStateMap::new());
        let engine = Engine::new(
            all_devices,
            &all_repos,
            Rc::clone(&config),
            Rc::clone(&calibration_store),
            fan_state_map,
            Rc::new(crate::overrides::OverridesController::empty()),
        );

        (device, engine, calibration_store)
    }

    /// Build an Engine over one hwmon mock device, returning the config
    /// and the repo's recorded fixed-speed writes. Mirrors
    /// `setup_calibrated_device` but exposes the `config` and
    /// `set_speeds` handles a snapshot/restore test needs to assert on.
    fn setup_engine_with_speed_recorder() -> (Engine, Rc<Config>, DeviceUID, Rc<RefCell<Vec<u8>>>) {
        let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
        let mut repos = Repositories::default();
        let set_speeds = Rc::new(RefCell::new(Vec::new()));
        let mock_repo = Rc::new(MockRepository {
            device_type: DeviceType::Hwmon,
            set_speeds: Rc::clone(&set_speeds),
            should_fail: Rc::new(Cell::new(false)),
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
        let engine = Engine::new(
            all_devices,
            &all_repos,
            Rc::clone(&config),
            Rc::new(crate::calibration::CalibrationStore::empty()),
            Rc::new(crate::calibration::FanStateMap::new()),
            Rc::new(crate::overrides::OverridesController::empty()),
        );
        (engine, config, device_uid, set_speeds)
    }

    fn sample_smooth_calibration() -> crate::calibration::Calibration {
        use crate::calibration::{CurveKind, DutySample};
        let up: Vec<DutySample> = (0..21usize)
            .map(|i| DutySample {
                duty: u8::try_from(i).expect("fits in u8") * 5,
                rpm: 100 * u32::try_from(i).expect("fits in u32"),
            })
            .collect();
        let down = up.clone();
        crate::calibration::Calibration {
            up_curve: up,
            down_curve: down,
            kick_duration_ms: 500,
            min_start_duty: 5,
            min_sustain_duty: 5,
            min_stable_duty: 5,
            max_eff_duty: 95,
            rpm_max: 2000,
            curve_kind: CurveKind::Smooth,
            warnings: Vec::new(),
            was_rpm_only: false,
            kick_boost_override: None,
            kick_duration_override_ms: None,
            walk_after_kick_override: None,
            timestamp: chrono::Local::now(),
        }
    }

    fn push_channel_status(
        device: &DeviceLock,
        channel: ChannelName,
        rpm: Option<u32>,
        duty_device: f64,
    ) {
        // Append a fresh Status with one ChannelStatus carrying the
        // given device-duty (the value a repo would have observed from
        // hardware before the mapping pass runs).
        use crate::device::ChannelStatus;
        let mut status = Status::default();
        status.channels.push(ChannelStatus {
            name: channel,
            rpm,
            duty: Some(duty_device),
            ..Default::default()
        });
        device.borrow_mut().set_status(status);
    }

    #[test]
    #[serial]
    fn unmanaged_channel_restores_to_auto_not_manual() {
        // Goal: the sweep now sets pwm_enable=1, so the post-sweep
        // restore-to-auto is load-bearing. A channel that starts
        // Unmanaged, whether via the Default profile "0" or with no
        // stored setting, must come back Unmanaged: snapshot must
        // classify it as a reset-bound kind, restore must take the
        // reset path (write no manual duty to the repo), and the
        // stored setting must be left untouched. Verifies both the
        // snapshot classification and the restore routing.
        use crate::calibration::{DiagnosisHost, SnapshotKind};
        use crate::setting::{Setting, SettingKind, DEFAULT_PROFILE_UID};
        cc_fs::test_runtime(async {
            let (engine, config, device_uid, set_speeds) = setup_engine_with_speed_recorder();

            // Case 1: explicitly Unmanaged via the Default profile "0".
            let chan_default = "fan1".to_string();
            config.set_device_setting(
                &device_uid,
                &Setting {
                    channel_name: chan_default.clone(),
                    kind: SettingKind::Profile {
                        profile_uid: DEFAULT_PROFILE_UID.to_string(),
                    },
                },
            );
            let snapshot = engine.snapshot_setting(&device_uid, &chan_default);
            assert!(
                matches!(&snapshot.kind, SnapshotKind::Profile(uid) if uid == DEFAULT_PROFILE_UID),
                "Unmanaged (Default profile) must snapshot as Profile(\"0\"), got {:?}",
                snapshot.kind
            );
            engine
                .restore_setting(&snapshot)
                .await
                .expect("restore default profile");
            assert!(
                set_speeds.borrow().is_empty(),
                "restoring Unmanaged must take the reset path, not write a manual duty: {:?}",
                set_speeds.borrow()
            );
            let stored = config
                .get_device_channel_settings(&device_uid, &chan_default)
                .expect("setting present");
            assert!(
                matches!(
                    &stored.kind,
                    SettingKind::Profile { profile_uid } if profile_uid == DEFAULT_PROFILE_UID
                ),
                "calibration must leave the stored Unmanaged setting untouched"
            );

            // Case 2: Unmanaged by absence of any stored setting.
            let chan_unset = "fan2".to_string();
            let snapshot_unset = engine.snapshot_setting(&device_uid, &chan_unset);
            assert!(
                matches!(snapshot_unset.kind, SnapshotKind::None),
                "an unset channel must snapshot as None, got {:?}",
                snapshot_unset.kind
            );
            engine
                .restore_setting(&snapshot_unset)
                .await
                .expect("restore unset channel");
            assert!(
                set_speeds.borrow().is_empty(),
                "restoring an unset channel must not write a manual duty"
            );
        });
    }

    #[test]
    #[serial]
    fn apply_true_duty_rewrites_calibrated_smooth_channel() {
        // Goal: a calibrated smooth channel's latest device-duty value
        // gets replaced with its true-duty equivalent based on the
        // measured RPM. The rewrite runs inside Device::set_status's
        // existing Arc::make_mut via the engine-installed augmenter, so
        // the calibration is inserted into the store first and then
        // the status push triggers the mapping.
        cc_fs::test_runtime(async {
            let (device, _engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            calibration_store
                .insert_unsaved((device_uid, channel.clone()), sample_smooth_calibration());
            push_channel_status(&device, channel.clone(), Some(1000), 50.0);

            let observed = device.borrow().status_current().unwrap();
            let chan = observed
                .channels
                .iter()
                .find(|c| c.name == channel)
                .expect("channel present");
            // 1000 RPM on a 0..=2000 curve with rpm_floor at index 1 (=100):
            // (1000 - 100) / (2000 - 100) * 100 = ~47%.
            let true_duty = chan.duty.expect("duty rewritten");
            assert!(
                (40.0..=55.0).contains(&true_duty),
                "expected ~47% true-duty, got {true_duty}"
            );
        });
    }

    #[test]
    #[serial]
    fn apply_true_duty_leaves_uncalibrated_channel_alone() {
        // Goal: a channel with no calibration in the store keeps its
        // original device-duty value verbatim. This is the path most
        // users start on. The installed augmenter is a no-op when the
        // store has no entry for the channel.
        cc_fs::test_runtime(async {
            let (device, _engine, _calibration_store) = setup_calibrated_device();
            let channel = "fan1".to_string();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            push_channel_status(&device, channel.clone(), Some(1000), 50.0);

            let observed = device.borrow().status_current().unwrap();
            let chan = observed
                .channels
                .iter()
                .find(|c| c.name == channel)
                .expect("channel present");
            assert_eq!(chan.duty, Some(50.0));
        });
    }

    #[test]
    #[serial]
    fn apply_true_duty_uses_device_duty_when_rpm_missing() {
        // Goal: a calibrated channel with no RPM reading on the latest
        // sample still gets its duty replaced with the device-duty-
        // derived true-duty. Without an RPM cross-check the displayed
        // value is whatever the calibration's down-curve maps the
        // device-duty to (about 47% for a synthetic 0..=2000 linear
        // curve at device-duty 50%).
        cc_fs::test_runtime(async {
            let (device, _engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            calibration_store
                .insert_unsaved((device_uid, channel.clone()), sample_smooth_calibration());
            push_channel_status(&device, channel.clone(), None, 50.0);

            let observed = device.borrow().status_current().unwrap();
            let chan = observed
                .channels
                .iter()
                .find(|c| c.name == channel)
                .expect("channel present");
            let true_duty = chan.duty.expect("duty rewritten via device-duty path");
            assert!(
                (40.0..=55.0).contains(&true_duty),
                "expected ~47% device-duty-derived true-duty, got {true_duty}"
            );
        });
    }

    #[test]
    #[serial]
    fn apply_true_duty_keeps_device_value_when_rpm_diverges() {
        // The earlier cross-check used to switch to the RPM-derived
        // value when device-derived and rpm-derived disagreed by more
        // than the sanity threshold. That tripped false-positively on
        // firmware-kick fans, so we now always prefer device-derived
        // when present: the displayed value reflects what the daemon
        // wrote, not transient RPM dips.
        cc_fs::test_runtime(async {
            let (device, _engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            calibration_store
                .insert_unsaved((device_uid, channel.clone()), sample_smooth_calibration());
            // Daemon wrote device-duty 50% (would imply ~47% true), and
            // the fan reports 0 RPM. The display must stay at the
            // device-derived value rather than dropping to 0%.
            push_channel_status(&device, channel.clone(), Some(0), 50.0);

            let observed = device.borrow().status_current().unwrap();
            let chan = observed
                .channels
                .iter()
                .find(|c| c.name == channel)
                .expect("channel present");
            let true_duty = chan.duty.expect("duty rewritten");
            assert!(
                (40.0..=55.0).contains(&true_duty),
                "expected ~47% device-derived true-duty, got {true_duty}"
            );
        });
    }

    #[test]
    #[serial]
    fn diagnosis_host_current_rpm_reads_status_history() {
        // Goal: the Engine's DiagnosisHost::current_rpm trait method
        // returns the latest RPM from the device's status_history.
        cc_fs::test_runtime(async {
            use crate::calibration::DiagnosisHost as _;
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            push_channel_status(&device, channel.clone(), Some(1234), 50.0);

            let observed = engine.current_rpm(&device_uid, &channel).await;
            assert_eq!(observed, Some(1234));
        });
    }

    #[test]
    #[serial]
    fn diagnosis_host_current_rpm_none_for_unknown_channel() {
        // Goal: querying a channel not present in the latest status
        // yields None so the diagnoser records a zero sample.
        cc_fs::test_runtime(async {
            use crate::calibration::DiagnosisHost as _;
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            push_channel_status(&device, "fan1".to_string(), Some(800), 30.0);

            let observed = engine.current_rpm(&device_uid, "fan-missing").await;
            assert_eq!(observed, None);
        });
    }

    #[test]
    #[serial]
    fn diagnosis_host_max_temp_finds_hottest_value() {
        // Goal: max_temp_celsius walks every device's latest status
        // and returns the highest temp.
        cc_fs::test_runtime(async {
            use crate::calibration::DiagnosisHost as _;
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: "t1".to_string(),
                temp: 45.0,
            });
            status.temps.push(TempStatus {
                name: "t2".to_string(),
                temp: 72.5,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);

            let observed = engine.max_temp_celsius().await;
            assert!(
                (observed - 72.5).abs() < f64::EPSILON,
                "expected 72.5, got {observed}"
            );
        });
    }

    #[test]
    #[serial]
    fn diagnosis_host_snapshot_returns_none_for_unset_channel() {
        // Goal: a channel with no persisted setting snapshots as
        // SnapshotKind::None so the restore step is a no-op reset.
        cc_fs::test_runtime(async {
            use crate::calibration::{DiagnosisHost as _, SnapshotKind};
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let snapshot = engine.snapshot_setting(&device_uid, "fan1");
            assert_eq!(snapshot.kind, SnapshotKind::None);
            assert_eq!(snapshot.device_uid, device_uid);
            assert_eq!(snapshot.channel_name, "fan1");
        });
    }

    #[test]
    #[serial]
    fn start_calibration_diagnosis_preflight_rejects_hot_system() {
        // Goal: end-to-end engine entry point. When the most recent
        // status shows a hot temp, the diagnoser short-circuits in
        // pre-flight, no calibration is persisted, and the registry
        // entry is cleared so subsequent attempts can run.
        cc_fs::test_runtime(async {
            use crate::calibration::DiagnosisFailure;
            let (device, engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            // Seed a hot ambient temperature so preflight refuses.
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: "cpu".to_string(),
                temp: 80.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let err = engine
                .start_calibration_diagnosis(device_uid.clone(), "fan1".to_string())
                .await
                .expect_err("preflight rejects hot system");
            assert!(matches!(err, DiagnosisFailure::PreflightTempTooHigh { .. }));
            let key: crate::calibration::ChannelKey = (device_uid, "fan1".to_string());
            assert!(!calibration_store.has(&key));
            assert!(!engine.is_calibration_in_progress(&key));
        });
    }

    #[test]
    #[serial]
    fn calibration_batch_drives_each_channel_to_terminal() {
        // Goal: begin + drive a batch end to end. With a hot system every
        // sweep short-circuits in preflight, so the driver marks each
        // entry Failed and the batch finishes inactive, proving the queue
        // advances through every channel and ends.
        cc_fs::test_runtime(async {
            let (device, engine, _store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: "cpu".to_string(),
                temp: 80.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let channels = vec![
                (device_uid.clone(), "fan1".to_string()),
                (device_uid.clone(), "fan2".to_string()),
            ];
            engine
                .begin_calibration_batch(channels, 1)
                .expect("batch begins");
            engine.drive_calibration_batch().await;
            let status = engine
                .calibration_batch_status()
                .expect("batch status present");
            assert!(!status.active);
            assert_eq!(status.entries.len(), 2);
            assert_eq!(status.entries[0].phase, "failed");
            assert_eq!(status.entries[1].phase, "failed");
        });
    }

    #[test]
    #[serial]
    fn calibration_batch_drives_a_concurrent_group() {
        // Goal: with concurrency 2 the driver runs both channels in one
        // group (the moro_local scope), and both still reach a terminal
        // phase and the batch ends inactive. Hot system => both fail
        // preflight, proving the concurrent path drives and joins.
        cc_fs::test_runtime(async {
            let (device, engine, _store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: "cpu".to_string(),
                temp: 80.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let channels = vec![
                (device_uid.clone(), "fan1".to_string()),
                (device_uid.clone(), "fan2".to_string()),
            ];
            engine
                .begin_calibration_batch(channels, 2)
                .expect("batch begins");
            engine.drive_calibration_batch().await;
            let status = engine
                .calibration_batch_status()
                .expect("batch status present");
            assert!(!status.active);
            assert_eq!(status.entries.len(), 2);
            assert_eq!(status.entries[0].phase, "failed");
            assert_eq!(status.entries[1].phase, "failed");
        });
    }

    #[test]
    #[serial]
    fn calibration_batch_rejects_second_while_active() {
        // Goal: the engine refuses a second batch while one is active and
        // allows a fresh one once the first has gone inactive.
        cc_fs::test_runtime(async {
            let (device, engine, _store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let mut status = Status::default();
            status.temps.push(TempStatus {
                name: "cpu".to_string(),
                temp: 80.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);
            let channels = vec![(device_uid.clone(), "fan1".to_string())];
            engine
                .begin_calibration_batch(channels.clone(), 1)
                .expect("first begin");
            let err = engine
                .begin_calibration_batch(channels.clone(), 1)
                .expect_err("second begin conflicts");
            assert!(err.to_string().contains("already in progress"));
            engine.drive_calibration_batch().await;
            engine
                .begin_calibration_batch(channels, 1)
                .expect("begin allowed after finish");
        });
    }

    #[test]
    #[serial]
    fn cancel_calibration_returns_false_when_not_running() {
        // Goal: the engine's cancel entry point returns false when
        // nothing is in flight; the REST layer maps this to a 404.
        cc_fs::test_runtime(async {
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let key: crate::calibration::ChannelKey = (device_uid, "fan1".to_string());
            assert!(!engine.cancel_calibration_diagnosis(&key));
        });
    }

    #[test]
    #[serial]
    fn set_calibration_overrides_returns_none_for_missing_calibration() {
        // Goal: the engine method returns Ok(None) when no calibration
        // is stored, so the REST handler maps to 404. The persistence
        // branch is exercised end-to-end (it writes to the configured
        // calibration file, which the unit test environment cannot
        // create).
        cc_fs::test_runtime(async {
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let key: crate::calibration::ChannelKey = (device_uid, "fan1".to_string());
            let result = engine
                .set_calibration_overrides(&key, Some(true), Some(5000), Some(false))
                .await
                .expect("ok");
            assert!(result.is_none(), "no calibration stored -> None");
        });
    }

    #[test]
    #[serial]
    fn apply_true_duty_skips_stepped_calibration() {
        // Goal: a calibrated channel whose curve was classified as
        // Stepped keeps its device-duty value (mapping disabled). The
        // user sees the raw device value, matching how the dispatch
        // layer also leaves stepped channels in passthrough mode.
        cc_fs::test_runtime(async {
            use crate::calibration::CurveKind;
            let (device, _engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            device
                .borrow_mut()
                .initialize_status_history_with(Status::default(), 1.0);
            let mut cal = sample_smooth_calibration();
            cal.curve_kind = CurveKind::Stepped;
            calibration_store.insert_unsaved((device_uid, channel.clone()), cal);
            push_channel_status(&device, channel.clone(), Some(1000), 50.0);

            let observed = device.borrow().status_current().unwrap();
            let chan = observed
                .channels
                .iter()
                .find(|c| c.name == channel)
                .expect("channel present");
            assert_eq!(chan.duty, Some(50.0));
        });
    }

    /// Seed `status_history` with copies of the given (rpm, duty) values
    /// for one channel. Drives the backfill / clear tests: they need
    /// every history entry to share the same shape, which
    /// `initialize_status_history_with` provides via its zero-fill of
    /// fields whose presence matches the first status.
    fn seed_history_with_channel(
        device: &DeviceLock,
        channel: ChannelName,
        rpm: Option<u32>,
        duty: Option<f64>,
    ) {
        use crate::device::ChannelStatus;
        let mut initial = Status::default();
        initial.channels.push(ChannelStatus {
            name: channel,
            rpm,
            duty,
            ..Default::default()
        });
        device
            .borrow_mut()
            .initialize_status_history_with(initial, 1.0);
    }

    #[test]
    #[serial]
    fn backfill_history_fills_none_entries_on_smooth_calibration() {
        // Goal: on a previously RPM-only channel, the backfill walks
        // every entry whose `duty` is None and fills it via
        // `rpm_to_true_duty`. The function reports `true` so the caller
        // can flip `was_rpm_only` and prompt the UI to reload.
        cc_fs::test_runtime(async {
            let (device, engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            seed_history_with_channel(&device, channel.clone(), Some(1000), None);
            let calibration = sample_smooth_calibration();
            calibration_store
                .insert_unsaved((device_uid.clone(), channel.clone()), calibration.clone());

            let filled = engine.backfill_history_duties_from_calibration(
                &(device_uid, channel.clone()),
                &calibration,
            );
            assert!(filled, "backfill must report filling entries");

            let observed = device.borrow();
            assert!(
                observed.status_history.is_empty().not(),
                "history must be populated"
            );
            for status in observed.status_history.iter() {
                let chan = status
                    .channels
                    .iter()
                    .find(|c| c.name == channel)
                    .expect("channel present in every entry");
                assert!(
                    chan.duty.is_some(),
                    "every entry's duty must be filled, got None"
                );
            }
        });
    }

    #[test]
    #[serial]
    fn backfill_history_returns_false_for_stepped_calibration() {
        // Goal: a stepped calibration has no duty mapping, so the
        // backfill is a no-op and reports `false`. The caller leaves
        // `was_rpm_only` at its default and does not prompt the UI.
        cc_fs::test_runtime(async {
            use crate::calibration::CurveKind;
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            seed_history_with_channel(&device, channel.clone(), Some(1000), None);
            let mut calibration = sample_smooth_calibration();
            calibration.curve_kind = CurveKind::Stepped;

            let filled = engine.backfill_history_duties_from_calibration(
                &(device_uid, channel.clone()),
                &calibration,
            );
            assert!(filled.not(), "stepped calibration must not fill anything");

            let observed = device.borrow();
            for status in observed.status_history.iter() {
                let chan = status
                    .channels
                    .iter()
                    .find(|c| c.name == channel)
                    .expect("channel present");
                assert!(chan.duty.is_none(), "stepped backfill must leave duty=None");
            }
        });
    }

    #[test]
    #[serial]
    fn backfill_history_returns_false_when_no_none_entries() {
        // Goal: a normal (non-RPM-only) channel already has duty
        // values throughout history. Backfill must not overwrite them
        // and must report `false` so the caller does not set
        // `was_rpm_only`.
        cc_fs::test_runtime(async {
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            seed_history_with_channel(&device, channel.clone(), Some(1000), Some(60.0));
            let calibration = sample_smooth_calibration();

            let filled = engine.backfill_history_duties_from_calibration(
                &(device_uid, channel.clone()),
                &calibration,
            );
            assert!(
                filled.not(),
                "no None entries existed, backfill must report false"
            );
        });
    }

    #[test]
    #[serial]
    fn backfill_history_skips_channels_without_rpm() {
        // Goal: an entry whose `rpm` is None cannot be mapped, so the
        // backfill leaves it alone. The overall return value reflects
        // whether any other entry was successfully filled; here, none.
        cc_fs::test_runtime(async {
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            seed_history_with_channel(&device, channel.clone(), None, None);
            let calibration = sample_smooth_calibration();

            let filled = engine.backfill_history_duties_from_calibration(
                &(device_uid, channel.clone()),
                &calibration,
            );
            assert!(
                filled.not(),
                "with no rpm available, backfill must report false"
            );
            let observed = device.borrow();
            for status in observed.status_history.iter() {
                let chan = status
                    .channels
                    .iter()
                    .find(|c| c.name == channel)
                    .expect("channel present");
                assert!(chan.duty.is_none(), "duty must stay None when rpm is None");
            }
        });
    }

    #[test]
    #[serial]
    fn backfill_history_returns_false_for_missing_device() {
        // Goal: an unknown device uid must not panic; the caller (the
        // diagnoser completion path) might race with device removal, so
        // the helper returns false cleanly.
        cc_fs::test_runtime(async {
            let (_device, engine, _calibration_store) = setup_calibrated_device();
            let calibration = sample_smooth_calibration();
            let filled = engine.backfill_history_duties_from_calibration(
                &("unknown-device".to_string(), "fan1".to_string()),
                &calibration,
            );
            assert!(filled.not(), "missing device must not fill anything");
        });
    }

    #[test]
    #[serial]
    fn clear_history_resets_duty_for_channel() {
        // Goal: clear walks every entry and resets `duty` to None for
        // the named channel. Drives the calibration-delete path on an
        // RPM-only channel so the chart reverts to its pre-calibration
        // state once the UI reloads.
        cc_fs::test_runtime(async {
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            let channel = "fan1".to_string();
            seed_history_with_channel(&device, channel.clone(), Some(1000), Some(45.0));

            engine.clear_history_duties_for_channel(&(device_uid, channel.clone()));

            let observed = device.borrow();
            assert!(
                observed.status_history.is_empty().not(),
                "history must be populated"
            );
            for status in observed.status_history.iter() {
                let chan = status
                    .channels
                    .iter()
                    .find(|c| c.name == channel)
                    .expect("channel present");
                assert!(chan.duty.is_none(), "clear must reset every entry to None");
            }
        });
    }

    #[test]
    #[serial]
    fn clear_history_leaves_other_channels_untouched() {
        // Goal: clear is per-channel. Other channels in the same device
        // must keep their duty values so deleting one channel's
        // calibration does not collateral-damage adjacent fans.
        cc_fs::test_runtime(async {
            use crate::device::ChannelStatus;
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            // Build a status with two channels and seed history from it.
            let mut initial = Status::default();
            initial.channels.push(ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1000),
                duty: Some(40.0),
                ..Default::default()
            });
            initial.channels.push(ChannelStatus {
                name: "fan2".to_string(),
                rpm: Some(1500),
                duty: Some(55.0),
                ..Default::default()
            });
            device
                .borrow_mut()
                .initialize_status_history_with(initial, 1.0);

            engine.clear_history_duties_for_channel(&(device_uid, "fan1".to_string()));

            let observed = device.borrow();
            for status in observed.status_history.iter() {
                let fan1 = status
                    .channels
                    .iter()
                    .find(|c| c.name == "fan1")
                    .expect("fan1 present");
                let fan2 = status
                    .channels
                    .iter()
                    .find(|c| c.name == "fan2")
                    .expect("fan2 present");
                assert!(fan1.duty.is_none(), "fan1 must be cleared");
                assert!(
                    fan2.duty.is_some(),
                    "fan2 must keep its duty (not in clear scope)"
                );
            }
        });
    }
}
