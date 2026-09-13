// SPDX-FileCopyrightText: 2025 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

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
        ProfileUID, Setting, SettingKind, TempSource,
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
        applied_profiles: Rc<RefCell<Vec<Vec<(Temp, Duty)>>>>,
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
            speed_profile: &[(f64, u8)],
        ) -> Result<()> {
            if self.should_fail.get() {
                return Err(anyhow!("Simulated failure to apply speed profile"));
            }
            self.applied_profiles
                .borrow_mut()
                .push(speed_profile.to_vec());
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

    /// One hwmon mock device with an Engine over it, plus every handle
    /// the tests assert on. The setup wrappers below pick what they need.
    struct MockHarness {
        device: DeviceLock,
        engine: Engine,
        config: Rc<Config>,
        set_speeds: Rc<RefCell<Vec<u8>>>,
        applied_profiles: Rc<RefCell<Vec<Vec<(Temp, Duty)>>>>,
        should_fail: Rc<Cell<bool>>,
        calibration_store: Rc<crate::calibration::CalibrationStore>,
        fan_state_map: Rc<crate::calibration::FanStateMap>,
    }

    fn setup_harness() -> MockHarness {
        let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
        let mut repos = Repositories::default();
        let set_speeds = Rc::new(RefCell::new(Vec::new()));
        let applied_profiles = Rc::new(RefCell::new(Vec::new()));
        let should_fail = Rc::new(Cell::new(false));

        // Create mock repository
        let mock_repo = Rc::new(MockRepository {
            device_type: DeviceType::Hwmon,
            set_speeds: Rc::clone(&set_speeds),
            applied_profiles: Rc::clone(&applied_profiles),
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
            Rc::clone(&calibration_store),
            Rc::clone(&fan_state_map),
            Rc::new(crate::overrides::OverridesController::empty()),
        );

        MockHarness {
            device,
            engine,
            config,
            set_speeds,
            applied_profiles,
            should_fail,
            calibration_store,
            fan_state_map,
        }
    }

    fn setup_single_device() -> (
        DeviceLock,
        Engine,
        Rc<Config>,
        Rc<RefCell<Vec<u8>>>,
        Rc<Cell<bool>>,
    ) {
        let h = setup_harness();
        (h.device, h.engine, h.config, h.set_speeds, h.should_fail)
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

    /// Rewrites an existing Standard Function with a new response delay, the way a
    /// user edit does, so the engine has to pick the change up on a live Profile.
    fn update_standard_function_response_delay(
        config: &Config,
        function_uid: &FunctionUID,
        response_delay: u8,
    ) {
        config
            .update_function(Function {
                uid: function_uid.clone(),
                name: "StandardFunction".to_string(),
                step_size_min: 2,
                step_size_max: 100,
                kind: FunctionKind::Standard {
                    deviance: Some(2.0),
                    only_downward: Some(false),
                    response_delay: Some(response_delay),
                },
                ..Default::default()
            })
            .unwrap();
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

    fn create_standard_function_with_asymmetric_steps(
        config: &Config,
        response_delay: u8,
        deviance: f64,
        only_downward: bool,
        step_size_min: Duty,
        step_size_max: Duty,
        step_size_min_decreasing: Duty,
        step_size_max_decreasing: Duty,
    ) -> FunctionUID {
        let function_uid = Uuid::new_v4().to_string();
        let function = Function {
            uid: function_uid.clone(),
            name: "StandardFunction".to_string(),
            step_size_min,
            step_size_max,
            step_size_min_decreasing,
            step_size_max_decreasing,
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

    /// Shared setup for the response delay edit tests: one Graph Profile driven by a
    /// Standard Function, applied to two fan channels on the same device. Two channels
    /// is the case that keeps the Profile scheduled while a single channel is being
    /// re-applied, so the processor metadata survives the edit.
    async fn setup_two_channel_delay_test(
        response_delay: u8,
    ) -> (
        DeviceLock,
        Engine,
        Rc<Config>,
        Rc<RefCell<Vec<Duty>>>,
        TempName,
        FunctionUID,
    ) {
        let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
        let fan1_name = create_controllable_fan(&device, "fan1");
        let fan2_name = create_controllable_fan(&device, "fan2");
        let temp_channel_name = create_temp(&device, "temp1");
        let device_uid = device.borrow().uid.clone();

        let function_uid = create_standard_function(&config, response_delay, 2.0, false);
        let profile_uid = create_graph_profile_with_temp_source_and_function(
            &config,
            vec![(20.0, 25), (40.0, 50), (60.0, 75), (80.0, 100)],
            TempSource {
                device_uid: device_uid.clone(),
                temp_name: temp_channel_name.clone(),
            },
            &function_uid,
        );
        for channel_name in [&fan1_name, &fan2_name] {
            let setting = Setting {
                channel_name: channel_name.clone(),
                kind: SettingKind::Profile {
                    profile_uid: profile_uid.clone(),
                },
            };
            // The engine re-applies Profiles from the device settings on a Function
            // edit, so the setting has to be in the config as well.
            config.set_device_setting(&device_uid, &setting);
            engine
                .set_config_setting(&device_uid, &setting)
                .await
                .unwrap();
        }
        (
            device,
            engine,
            config,
            set_speeds,
            temp_channel_name,
            function_uid,
        )
    }

    #[test]
    #[serial]
    fn test_raised_response_delay_applies_to_multi_channel_profile() {
        cc_fs::test_runtime(async {
            // Goal: verify that raising a Function's response delay takes effect for a
            // Profile applied to more than one channel. Method: settle at the baseline
            // with a 2 cycle delay, raise it to 10 cycles, then jump the temp and assert
            // that no new duty is applied before the new delay has elapsed. The
            // hysteresis stack is keyed by Profile and outlives the edit here, so a
            // stale stack size would keep the old, shorter delay in force.
            let (device, engine, config, set_speeds, temp_name, function_uid) =
                setup_two_channel_delay_test(2).await;
            process_cycles(&engine, &device, &temp_name, 20., 4).await;

            // When: the response delay is raised while both channels stay scheduled.
            update_standard_function_response_delay(&config, &function_uid, 10);
            engine.function_updated(&function_uid).await;

            // Then: a temp jump must not reach the fans within the old, shorter delay.
            process_cycles(&engine, &device, &temp_name, 60., 5).await;
            let speeds_within_old_delay = set_speeds.borrow().clone();
            assert!(
                speeds_within_old_delay.contains(&75).not(),
                "the raised response delay should still be holding the duty back: \
                 {speeds_within_old_delay:?}"
            );

            // And: the new duty lands once the new delay has elapsed.
            process_cycles(&engine, &device, &temp_name, 60., 8).await;
            let speeds_after_new_delay = set_speeds.borrow().clone();
            assert_eq!(
                speeds_after_new_delay.last(),
                Some(&75),
                "duty should change after the raised response delay: {speeds_after_new_delay:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_lowered_response_delay_applies_to_multi_channel_profile() {
        cc_fs::test_runtime(async {
            // Goal: the negative space of the test above. Lowering the delay has to
            // shorten an already filled stack, otherwise the old, longer delay keeps
            // holding the duty back. Method: settle at the baseline with a 10 cycle
            // delay, lower it to 2 cycles, then jump the temp and assert the duty lands
            // well inside the old delay.
            let (device, engine, config, set_speeds, temp_name, function_uid) =
                setup_two_channel_delay_test(10).await;
            process_cycles(&engine, &device, &temp_name, 20., 12).await;

            // When: the response delay is lowered while both channels stay scheduled.
            update_standard_function_response_delay(&config, &function_uid, 2);
            engine.function_updated(&function_uid).await;

            // Then: the temp jump reaches the fans within the new, shorter delay.
            process_cycles(&engine, &device, &temp_name, 60., 4).await;
            let speeds_within_old_delay = set_speeds.borrow().clone();
            assert_eq!(
                speeds_within_old_delay.last(),
                Some(&75),
                "duty should change after the lowered response delay: \
                 {speeds_within_old_delay:?}"
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

    /// Runs `cycle_count` processing cycles with the temp source held at
    /// `temp_celsius`. Each cycle gets its own scope so spawned speed tasks
    /// complete before the next one starts.
    async fn process_cycles(
        engine: &Engine,
        device: &DeviceLock,
        temp_name: &TempName,
        temp_celsius: Temp,
        cycle_count: usize,
    ) {
        for _ in 0..cycle_count {
            set_temp_status(device, temp_name, temp_celsius);
            let cycle_result: Result<()> = moro_local::async_scope!(|scope| {
                engine.process_scheduled_speeds(scope);
                Ok(())
            })
            .await;
            cycle_result.unwrap();
        }
    }

    /// The curve shared by the ramp continuation tests. 45C interpolates to
    /// 34% and 75C to 65%, giving a 31% span for a 1% step to work through.
    fn ramp_test_curve() -> Vec<(Temp, Duty)> {
        vec![(40.0, 30), (60.0, 45), (75.0, 65), (85.0, 100)]
    }

    /// Shared setup for the ramp continuation tests: one controllable fan and
    /// one temp channel on a single device, running a graph profile over
    /// `ramp_test_curve` with the function `create_function` builds.
    async fn setup_ramp_test(
        create_function: impl FnOnce(&Config) -> FunctionUID,
    ) -> (DeviceLock, Engine, TempName, Rc<RefCell<Vec<Duty>>>) {
        let (device, engine, config, set_speeds, _should_fail) = setup_single_device();
        let fan_channel_name = create_controllable_fan(&device, "fan1");
        let temp_channel_name = create_temp(&device, "temp1");
        let device_uid = device.borrow().uid.clone();

        let function_uid = create_function(&config);
        let profile_uid = create_graph_profile_with_temp_source_and_function(
            &config,
            ramp_test_curve(),
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
        (device, engine, temp_channel_name, set_speeds)
    }

    #[test]
    #[serial]
    fn test_standard_function_continues_stepping_down_with_small_steps() {
        cc_fs::test_runtime(async {
            // Goal: reproduce issue #602. With a max decreasing step of 1 the
            // limiter needs many cycles to reach the target, but the hysteresis
            // gate stops feeding the chain once the temp goes flat, so only the
            // 30 cycle safety latch advances the fan. Verify the limiter steps
            // once per cycle instead.
            // Method: settle at 75C, drop to 45C, hold it flat for 20 cycles
            // (well under the latch) and inspect only the duties applied after
            // the settle.
            // The reporter's function: 1% max step down, 3s delay, 2C deviance.
            let (device, engine, temp_channel_name, set_speeds) = setup_ramp_test(|config| {
                create_standard_function_with_asymmetric_steps(config, 3, 2.0, false, 1, 100, 1, 1)
            })
            .await;

            process_cycles(&engine, &device, &temp_channel_name, 75., 6).await;
            let settled_count = set_speeds.borrow().len();
            assert_eq!(
                set_speeds.borrow().last(),
                Some(&65),
                "should settle on the 75C target before the drop"
            );

            process_cycles(&engine, &device, &temp_channel_name, 45., 20).await;

            let speeds = set_speeds.borrow().clone();
            let ramp = &speeds[settled_count..];
            // 20 held cycles minus the 2 the hysteresis stack needs to accept
            // the new temp, so exactly one step per cycle from there on.
            assert_eq!(ramp.len(), 18, "limiter must step once per cycle: {ramp:?}");
            assert_eq!(ramp[0], 64, "first step down from 65%: {ramp:?}");
            assert!(
                ramp.windows(2).all(|w| w[0] - w[1] == 1),
                "every step must be exactly the 1% max decreasing step: {ramp:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_standard_function_continues_stepping_up_with_small_steps() {
        cc_fs::test_runtime(async {
            // Goal: the same continuation must work while ramping up. With
            // only_downward=false the existing upward bypass never runs, so
            // without the fix an upward ramp stalls on the latch too.
            // Method: settle at 45C, raise to 75C, hold flat for 20 cycles.
            let (device, engine, temp_channel_name, set_speeds) = setup_ramp_test(|config| {
                create_standard_function_with_asymmetric_steps(config, 3, 2.0, false, 1, 1, 1, 1)
            })
            .await;

            // 10 cycles: the seeded 20C applies 30%, then the ramp to the 45C
            // target of 34% takes 4 more single-percent steps.
            process_cycles(&engine, &device, &temp_channel_name, 45., 10).await;
            let settled_count = set_speeds.borrow().len();
            assert_eq!(
                set_speeds.borrow().last(),
                Some(&34),
                "should settle on the 45C target before the climb"
            );

            process_cycles(&engine, &device, &temp_channel_name, 75., 20).await;

            let speeds = set_speeds.borrow().clone();
            let ramp = &speeds[settled_count..];
            // 20 held cycles minus the 2 the hysteresis stack needs to accept
            // the new temp, so exactly one step per cycle from there on.
            assert_eq!(ramp.len(), 18, "limiter must step once per cycle: {ramp:?}");
            assert_eq!(ramp[0], 35, "first step up from 34%: {ramp:?}");
            assert!(
                ramp.windows(2).all(|w| w[1] - w[0] == 1),
                "every step must be exactly the 1% max increasing step: {ramp:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_standard_function_stops_stepping_at_target() {
        cc_fs::test_runtime(async {
            // Goal: the continuation must terminate. Once the ramp reaches the
            // curve target it must not overshoot, oscillate, or keep writing.
            // Method: give the 31 step ramp 80 cycles, far more than it needs.
            let (device, engine, temp_channel_name, set_speeds) = setup_ramp_test(|config| {
                create_standard_function_with_asymmetric_steps(config, 3, 2.0, false, 1, 100, 1, 1)
            })
            .await;

            process_cycles(&engine, &device, &temp_channel_name, 75., 6).await;
            let settled_count = set_speeds.borrow().len();

            process_cycles(&engine, &device, &temp_channel_name, 45., 80).await;

            let speeds = set_speeds.borrow().clone();
            let ramp = &speeds[settled_count..];
            assert_eq!(
                ramp.last(),
                Some(&34),
                "ramp must settle exactly on the 45C target: {ramp:?}"
            );
            assert!(
                ramp.iter().all(|&duty| (34..65).contains(&duty)),
                "ramp must stay between the start and target duty: {ramp:?}"
            );
            assert!(
                ramp.windows(2).all(|w| w[1] <= w[0]),
                "a downward ramp must never step back up: {ramp:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_standard_function_holds_within_deadband_when_not_clamped() {
        cc_fs::test_runtime(async {
            // Goal: regression guard. The continuation must not turn the
            // hysteresis gate into a no-op. Without a max clamp the limiter
            // reaches the target in a single write, so a later temp drift
            // inside the deviance band must apply nothing at all.
            // Method: settle at 60C, drift 1C (inside the 2C band), hold for
            // 20 cycles, which stays under the safety latch.
            let (device, engine, temp_channel_name, set_speeds) = setup_ramp_test(|config| {
                create_standard_function_with_steps(config, 3, 2.0, false, 1, 100)
            })
            .await;

            process_cycles(&engine, &device, &temp_channel_name, 60., 6).await;
            let settled_count = set_speeds.borrow().len();
            assert_eq!(
                set_speeds.borrow().last(),
                Some(&45),
                "should settle on the 60C target before the drift"
            );

            process_cycles(&engine, &device, &temp_channel_name, 61., 20).await;

            let speeds = set_speeds.borrow().clone();
            assert!(
                speeds[settled_count..].is_empty(),
                "drift inside the deviance band must not apply anything: {:?}",
                &speeds[settled_count..]
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
        let h = setup_harness();
        (h.device, h.engine, h.calibration_store)
    }

    /// Build an Engine over one hwmon mock device, returning the config
    /// and the repo's recorded fixed-speed writes. Mirrors
    /// `setup_calibrated_device` but exposes the `config` and
    /// `set_speeds` handles a snapshot/restore test needs to assert on.
    fn setup_engine_with_speed_recorder() -> (Engine, Rc<Config>, DeviceUID, Rc<RefCell<Vec<u8>>>) {
        let h = setup_harness();
        let device_uid = h.device.borrow().uid.clone();
        (h.engine, h.config, device_uid, h.set_speeds)
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
        // Goal: hottest_temp walks every device's latest status and
        // returns the highest temp with the identity of the sensor it
        // came from, plus how many sensors are at or above the limit, so
        // a temp gate names the offending reading and reports breadth
        // rather than a bare number.
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
            status.temps.push(TempStatus {
                name: "t3".to_string(),
                temp: 80.0,
            });
            device
                .borrow_mut()
                .initialize_status_history_with(status, 1.0);

            let hottest = engine.hottest_temp(70.0).await;
            assert!(
                (hottest.celsius - 80.0).abs() < f64::EPSILON,
                "expected 80.0, got {}",
                hottest.celsius
            );
            assert!(
                hottest.sensor.contains("t3"),
                "sensor label must name the hottest reading (t3), got {}",
                hottest.sensor
            );
            assert!(
                hottest.sensor.contains("t1").not(),
                "sensor label must not name the cooler reading (t1), got {}",
                hottest.sensor
            );
            // t2 (72.5) and t3 (80.0) are >= 70.0; t1 (45.0) is not.
            assert_eq!(hottest.over_limit_count, 2);
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
    fn start_calibration_diagnosis_blocked_by_active_alert() {
        // Goal: an alert already Active on the channel aborts the sweep in
        // preflight: the alert was not caused by calibration, so the fan
        // itself is suspect. Nothing is persisted or registered.
        cc_fs::test_runtime(async {
            use crate::calibration::{CalibrationAlertGate, DiagnosisFailure};
            struct StubGate;
            impl CalibrationAlertGate for StubGate {
                fn active_alert_for_channel(
                    &self,
                    _device_uid: &str,
                    channel_name: &str,
                ) -> Option<String> {
                    (channel_name == "fan1").then(|| "Fan Alarm".to_string())
                }
            }
            let (device, engine, calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            engine.set_alert_gate(Rc::new(StubGate));
            let err = engine
                .start_calibration_diagnosis(device_uid.clone(), "fan1".to_string())
                .await
                .expect_err("active alert blocks the sweep");
            assert!(matches!(err, DiagnosisFailure::BlockedByAlert { .. }));
            let key: crate::calibration::ChannelKey = (device_uid, "fan1".to_string());
            assert!(!calibration_store.has(&key));
            assert!(!engine.is_calibration_in_progress(&key));
        });
    }

    #[test]
    #[serial]
    fn calibration_batch_begin_rejects_active_alert() {
        // Goal: a batch listing a channel with an Active alert is rejected
        // upfront, naming the offending alert, and no batch is installed; a
        // batch on an unblocked channel still begins.
        cc_fs::test_runtime(async {
            use crate::calibration::CalibrationAlertGate;
            struct StubGate;
            impl CalibrationAlertGate for StubGate {
                fn active_alert_for_channel(
                    &self,
                    _device_uid: &str,
                    channel_name: &str,
                ) -> Option<String> {
                    (channel_name == "fan1").then(|| "Fan Alarm".to_string())
                }
            }
            let (device, engine, _calibration_store) = setup_calibrated_device();
            let device_uid = device.borrow().uid.clone();
            engine.set_alert_gate(Rc::new(StubGate));
            let err = engine
                .begin_calibration_batch(vec![(device_uid.clone(), "fan1".to_string())], 1)
                .expect_err("blocked channel rejects the batch");
            assert!(err.to_string().contains("Fan Alarm"));
            // The rejected batch was never installed, so a clean one begins.
            engine
                .begin_calibration_batch(vec![(device_uid, "fan2".to_string())], 1)
                .expect("unblocked channel begins");
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

    /// A fan whose channel supports a firmware-internal curve, the
    /// prerequisite for the `set_graph_profile` hardware branch.
    fn create_firmware_curve_fan(device: &DeviceLock, fan_name: &str) -> ChannelName {
        let fan_channel_name = fan_name.to_string();
        device.borrow_mut().info.channels.insert(
            fan_channel_name.clone(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Speed(SpeedOptions {
                    fixed_enabled: true,
                    extension: Some(crate::device::ChannelExtensionNames::AutoHWCurve),
                    ..Default::default()
                }),
            },
        );
        fan_channel_name
    }

    /// Turn on the user-facing firmware-controlled profile toggle.
    fn enable_hw_curve(config: &Config, device_uid: &UID, channel_name: &str) {
        use crate::setting::{CCChannelSettings, CCDeviceSettings, ChannelExtensions};
        let mut cc_settings = CCDeviceSettings::default();
        cc_settings.channel_settings.insert(
            channel_name.to_string(),
            CCChannelSettings {
                extension: Some(ChannelExtensions::AutoHWCurve {
                    auto_hw_curve_enabled: true,
                }),
                ..Default::default()
            },
        );
        config.set_cc_settings_for_device(device_uid, &cc_settings);
    }

    /// Apply a Graph profile to a firmware-curve channel and return the
    /// points the repo received.
    async fn apply_firmware_curve_profile(
        h: &MockHarness,
        points: Vec<(Temp, Duty)>,
    ) -> Vec<(Temp, Duty)> {
        let fan = create_firmware_curve_fan(&h.device, "fan1");
        let temp = create_temp(&h.device, "temp1");
        let device_uid = h.device.borrow().uid.clone();
        enable_hw_curve(&h.config, &device_uid, &fan);
        let profile_uid = create_graph_profile_with_temp_source(
            &h.config,
            points,
            TempSource {
                device_uid: device_uid.clone(),
                temp_name: temp,
            },
        );
        h.engine
            .set_profile(&device_uid, &fan, &profile_uid)
            .await
            .expect("firmware profile applies");
        let applied = h.applied_profiles.borrow();
        assert_eq!(applied.len(), 1, "exactly one curve write expected");
        applied[0].clone()
    }

    #[test]
    #[serial]
    fn firmware_curve_maps_points_through_calibration() {
        // Goal: a calibrated channel draws its curve in true-duty, which
        // the firmware cannot interpret. The hardware branch must write
        // device-duty instead, or the firmware runs the fan below its
        // calibrated floor. Expected values follow from the fixture's
        // linear curve (rpm = duty * 20, floor 100 rpm at duty 5):
        // true 10 -> 14, true 50 -> 52. The 0 and 100 endpoints are
        // preserved exactly so an off-point stays off (AMD's zero-RPM
        // detection depends on it) and full duty stays full.
        cc_fs::test_runtime(async {
            let h = setup_harness();
            let device_uid = h.device.borrow().uid.clone();
            h.calibration_store.insert_unsaved(
                (device_uid, "fan1".to_string()),
                sample_smooth_calibration(),
            );

            let applied = apply_firmware_curve_profile(
                &h,
                vec![(30.0, 0), (50.0, 10), (70.0, 50), (90.0, 100)],
            )
            .await;

            assert_eq!(
                applied,
                vec![(30.0, 0), (50.0, 14), (70.0, 52), (90.0, 100)]
            );
        });
    }

    #[test]
    #[serial]
    fn firmware_curve_passes_through_when_uncalibrated() {
        // Goal: without a calibration the curve must reach the repo
        // byte-identical to what the user drew. This is the regression
        // lock for every existing firmware-curve user.
        cc_fs::test_runtime(async {
            let h = setup_harness();
            let points = vec![(30.0, 20), (50.0, 40), (70.0, 100)];

            let applied = apply_firmware_curve_profile(&h, points.clone()).await;

            assert_eq!(applied, points);
        });
    }

    #[test]
    #[serial]
    fn firmware_curve_passes_through_for_stepped_calibration() {
        // Goal: a stepped channel has no forward map (duties pass
        // through in software mode too), so the firmware curve must not
        // be rewritten.
        cc_fs::test_runtime(async {
            let h = setup_harness();
            let device_uid = h.device.borrow().uid.clone();
            let mut stepped = sample_smooth_calibration();
            stepped.curve_kind = crate::calibration::CurveKind::Stepped;
            h.calibration_store
                .insert_unsaved((device_uid, "fan1".to_string()), stepped);
            let points = vec![(30.0, 20), (50.0, 40), (70.0, 100)];

            let applied = apply_firmware_curve_profile(&h, points.clone()).await;

            assert_eq!(applied, points);
        });
    }

    #[test]
    #[serial]
    fn firmware_curve_forgets_stale_dispatcher_state() {
        // Goal: the dispatcher's kick/sustain state describes writes the
        // daemon makes itself. Once the firmware owns the channel it is
        // stale, and the status augmenter's cache tier would otherwise
        // keep displaying the last commanded true-duty.
        use crate::calibration::{ChannelEntry, FanState};
        cc_fs::test_runtime(async {
            let h = setup_harness();
            let device_uid = h.device.borrow().uid.clone();
            let key = (device_uid, "fan1".to_string());
            h.fan_state_map.replace(
                key.clone(),
                ChannelEntry {
                    state: FanState::On,
                    under_diagnosis: false,
                    commanded_true_duty: Some(42),
                },
            );
            assert_eq!(h.fan_state_map.commanded_true_duty(&key), Some(42));

            let _ = apply_firmware_curve_profile(&h, vec![(30.0, 20), (70.0, 100)]).await;

            assert!(
                h.fan_state_map.commanded_true_duty(&key).is_none(),
                "entering firmware control must drop the dispatcher state"
            );
        });
    }

    #[test]
    #[serial]
    fn duty_floor_comes_from_channel_info_and_repository() {
        // Goal: the sweep's floor is the stricter of what the channel
        // reports (a liquidctl pump header's min_duty, say) and what the
        // repository clamps behind the daemon's back (an RDNA3 card's
        // PMFW speed range). Channels that report no minimum and sit on
        // a repository without a clamp keep a floor of 0, which is the
        // pre-floor duty sequence.
        use crate::calibration::DiagnosisHost;
        cc_fs::test_runtime(async {
            let h = setup_harness();
            let device_uid = h.device.borrow().uid.clone();
            create_controllable_fan(&h.device, "fan1");
            h.device.borrow_mut().info.channels.insert(
                "pump".to_string(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        min_duty: 20,
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                },
            );

            assert_eq!(h.engine.duty_floor(&device_uid, "fan1"), 0);
            assert_eq!(h.engine.duty_floor(&device_uid, "pump"), 20);
            // Unknown channel and unknown device both degrade to no floor.
            assert_eq!(h.engine.duty_floor(&device_uid, "nope"), 0);
            assert_eq!(
                h.engine.duty_floor(&"no-such-device".to_string(), "fan1"),
                0
            );
        });
    }
}

#[cfg(test)]
mod lcd_shutdown_tests {
    use crate::cc_fs;
    use crate::config::Config;
    use crate::device::{
        ChannelInfo, ChannelKind, Device, DeviceInfo, DeviceType, DeviceUID, LcdInfo, UID,
    };
    use crate::engine::main::Engine;
    use crate::paths;
    use crate::repositories::repository::{DeviceList, DeviceLock, Repositories, Repository};
    use crate::setting::{LcdModeKind, LcdSettings, Setting, SettingKind};
    use anyhow::Result;
    use async_trait::async_trait;
    use mime;
    use serial_test::serial;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    const LCD_CHANNEL: &str = "lcd";

    /// Records what actually reached the screen, so a test can tell the stock
    /// shutdown image apart from a user's own.
    struct LcdRecorder {
        applied: Rc<RefCell<Vec<LcdSettings>>>,
    }

    #[async_trait(?Send)]
    impl Repository for LcdRecorder {
        fn device_type(&self) -> DeviceType {
            DeviceType::Liquidctl
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
        async fn apply_setting_reset(&self, _d: &UID, _c: &str) -> Result<()> {
            Ok(())
        }
        async fn apply_setting_manual_control(&self, _d: &UID, _c: &str) -> Result<()> {
            Ok(())
        }
        async fn apply_setting_speed_fixed(&self, _d: &UID, _c: &str, _s: u8) -> Result<()> {
            Ok(())
        }
        async fn apply_setting_speed_profile(
            &self,
            _d: &UID,
            _c: &str,
            _t: &crate::setting::TempSource,
            _p: &[(f64, u8)],
        ) -> Result<()> {
            Ok(())
        }
        async fn apply_setting_lighting(
            &self,
            _d: &UID,
            _c: &str,
            _l: &crate::setting::LightingSettings,
        ) -> Result<()> {
            Ok(())
        }
        async fn apply_setting_lcd(&self, _d: &UID, _c: &str, lcd: &LcdSettings) -> Result<()> {
            self.applied.borrow_mut().push(lcd.clone());
            Ok(())
        }
        async fn apply_setting_pwm_mode(&self, _d: &UID, _c: &str, _m: u8) -> Result<()> {
            Ok(())
        }
        async fn reinitialize_devices(&self) {}
    }

    fn setup_lcd_engine() -> (Engine, Rc<Config>, DeviceUID, Rc<RefCell<Vec<LcdSettings>>>) {
        let applied = Rc::new(RefCell::new(Vec::new()));
        let mut repos = Repositories::default();
        repos.liquidctl = Some(Rc::new(LcdRecorder {
            applied: Rc::clone(&applied),
        }));

        let mut info = DeviceInfo::default();
        info.channels.insert(
            LCD_CHANNEL.to_string(),
            ChannelInfo {
                label: None,
                kind: ChannelKind::Lcd {
                    modes: Vec::new(),
                    info: Some(LcdInfo {
                        screen_width: 320,
                        screen_height: 320,
                        max_image_size_bytes: 10_000_000,
                        gif_supported: true,
                    }),
                },
            },
        );
        let device = Rc::new(RefCell::new(Device::new(
            "Test LCD Device".to_string(),
            DeviceType::Liquidctl,
            0,
            None,
            info,
            None,
            1.0,
        )));
        let device_uid = device.borrow().uid.clone();
        let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
        devices.insert(device_uid.clone(), device);

        let all_devices = Rc::new(devices);
        let config = Rc::new(Config::init_default_config().unwrap());
        config.create_device_list(&all_devices);
        let engine = Engine::new(
            all_devices,
            &Rc::new(repos),
            Rc::clone(&config),
            Rc::new(crate::calibration::CalibrationStore::empty()),
            Rc::new(crate::calibration::FanStateMap::new()),
            Rc::new(crate::overrides::OverridesController::empty()),
        );
        (engine, config, device_uid, applied)
    }

    fn lcd_setting(mode: LcdModeKind) -> Setting {
        Setting {
            channel_name: LCD_CHANNEL.to_string(),
            kind: SettingKind::Lcd {
                lcd: LcdSettings {
                    brightness: None,
                    orientation: None,
                    colors: Vec::new(),
                    mode,
                },
            },
        }
    }

    /// Goal: a live Temp screen with no shutdown setting gets the stock image, which is
    /// the baseline the bug report expects to hold after a removal.
    #[test]
    #[serial]
    fn temp_mode_without_a_shutdown_setting_gets_the_stock_image() {
        cc_fs::test_runtime(async {
            let (engine, config, device_uid, applied) = setup_lcd_engine();
            config.set_device_setting(
                &device_uid,
                &lcd_setting(LcdModeKind::Temp { temp_source: None }),
            );

            engine.apply_lcd_shutdown_images().await;

            assert_eq!(applied.borrow().len(), 1, "the stock image must be applied");
        });
    }

    /// Goal: two screens must not share one image file. A single shared name let the second
    /// upload overwrite the first screen's picture, and both settings then named that file.
    #[test]
    #[serial]
    fn each_channel_gets_its_own_live_image_file() {
        cc_fs::test_runtime(async {
            let (engine, _config, device_uid, _applied) = setup_lcd_engine();
            let first = engine
                .save_lcd_image(
                    &device_uid,
                    "lcd1",
                    &mime::IMAGE_PNG,
                    b"screen-one".to_vec(),
                )
                .await
                .unwrap();
            let second = engine
                .save_lcd_image(
                    &device_uid,
                    "lcd2",
                    &mime::IMAGE_PNG,
                    b"screen-two".to_vec(),
                )
                .await
                .unwrap();

            assert_ne!(first, second, "each channel needs its own file");
            let kept = cc_fs::read_image(std::path::Path::new(&first))
                .await
                .unwrap();
            assert_eq!(
                kept, b"screen-one",
                "the second screen's upload must not overwrite the first"
            );
        });
    }

    /// Goal: a gif keeps its extension, since the retrieved content type is read back off the
    /// path. The old check matched one fixed filename, which a per-channel path never is.
    #[test]
    #[serial]
    fn a_gif_keeps_its_extension_on_a_per_channel_path() {
        cc_fs::test_runtime(async {
            let (engine, _config, device_uid, _applied) = setup_lcd_engine();
            let path = engine
                .save_lcd_image(&device_uid, LCD_CHANNEL, &mime::IMAGE_GIF, b"gif".to_vec())
                .await
                .unwrap();

            assert!(path.ends_with(".gif"), "a gif must stay a gif: {path}");
        });
    }

    /// Goal: someone deletes the shutdown image by hand. The stored setting then describes a
    /// file that is not there, so it must be dropped and the stock image used instead.
    #[test]
    #[serial]
    fn a_hand_removed_shutdown_image_drops_its_setting_and_uses_the_stock_image() {
        cc_fs::test_runtime(async {
            let (engine, config, device_uid, applied) = setup_lcd_engine();
            config.set_device_setting(
                &device_uid,
                &lcd_setting(LcdModeKind::Temp { temp_source: None }),
            );
            let user_image = paths::config_dir().join("lcd_shutdown/gone.png");
            config.set_lcd_shutdown_setting(
                &device_uid,
                LCD_CHANNEL,
                &LcdSettings {
                    brightness: None,
                    orientation: None,
                    colors: Vec::new(),
                    mode: LcdModeKind::Image {
                        image_file_processed: Some(user_image.to_str().unwrap().to_string()),
                    },
                },
            );
            // The file was never written: this is the hand-deleted state.
            assert_eq!(config.get_all_lcd_shutdown_settings().unwrap().len(), 1);

            engine.apply_lcd_shutdown_images().await;

            assert!(
                config.get_all_lcd_shutdown_settings().unwrap().is_empty(),
                "the stale shutdown setting must be dropped from the config"
            );
            assert_eq!(
                applied.borrow().len(),
                1,
                "the stock image must stand in for the missing one"
            );
        });
    }

    /// Goal: negative space. A shutdown image that is still on disk must be applied and its
    /// setting kept, or the pruning would eat working configurations.
    #[test]
    #[serial]
    fn a_present_shutdown_image_is_applied_and_kept() {
        cc_fs::test_runtime(async {
            let (engine, config, device_uid, applied) = setup_lcd_engine();
            config.set_device_setting(
                &device_uid,
                &lcd_setting(LcdModeKind::Temp { temp_source: None }),
            );
            let user_image = paths::config_dir().join("lcd_shutdown/present.png");
            cc_fs::create_dir_all(&paths::config_dir().join("lcd_shutdown"))
                .await
                .unwrap();
            cc_fs::write(&user_image, b"not-a-real-png".to_vec())
                .await
                .unwrap();
            config.set_lcd_shutdown_setting(
                &device_uid,
                LCD_CHANNEL,
                &LcdSettings {
                    brightness: None,
                    orientation: None,
                    colors: Vec::new(),
                    mode: LcdModeKind::Image {
                        image_file_processed: Some(user_image.to_str().unwrap().to_string()),
                    },
                },
            );

            engine.apply_lcd_shutdown_images().await;

            assert_eq!(
                config.get_all_lcd_shutdown_settings().unwrap().len(),
                1,
                "a working shutdown setting must survive"
            );
            assert_eq!(
                applied.borrow().len(),
                1,
                "the user's image must be applied"
            );
        });
    }

    /// Goal: negative space. A user's own picture that is still on disk is theirs to keep,
    /// so widening the fallback must not paint the stock image over a working screen.
    #[test]
    #[serial]
    fn a_present_user_image_is_left_alone() {
        cc_fs::test_runtime(async {
            let (engine, config, device_uid, applied) = setup_lcd_engine();
            let picture = paths::config_dir().join("lcd_images/mine.png");
            cc_fs::create_dir_all(&paths::config_dir().join("lcd_images"))
                .await
                .unwrap();
            cc_fs::write(&picture, b"not-a-real-png".to_vec())
                .await
                .unwrap();
            config.set_device_setting(
                &device_uid,
                &lcd_setting(LcdModeKind::Image {
                    image_file_processed: Some(picture.to_str().unwrap().to_string()),
                }),
            );

            engine.apply_lcd_shutdown_images().await;

            assert!(
                applied.borrow().is_empty(),
                "a screen showing an existing image must be left as the user set it"
            );
        });
    }

    /// Goal: negative space. None and Liquid mean the device drives its own screen, so the
    /// engine must keep its hands off regardless of the Image widening.
    #[test]
    #[serial]
    fn device_driven_modes_are_left_alone() {
        cc_fs::test_runtime(async {
            for mode in [LcdModeKind::None, LcdModeKind::Liquid] {
                let (engine, config, device_uid, applied) = setup_lcd_engine();
                config.set_device_setting(&device_uid, &lcd_setting(mode));

                engine.apply_lcd_shutdown_images().await;

                assert!(
                    applied.borrow().is_empty(),
                    "a device-driven screen must not get the stock image"
                );
            }
        });
    }

    /// Goal: the reported bug. An externally driven screen (coolerdash and friends) has its
    /// current setting rewritten to the shutdown image when that image is applied. After the
    /// user removes the shutdown image, the stock embedded image must still take over.
    #[test]
    #[serial]
    fn stock_image_returns_after_removing_an_applied_shutdown_image() {
        cc_fs::test_runtime(async {
            let (engine, config, device_uid, applied) = setup_lcd_engine();
            // An external service owns the screen: its image lives outside the config dir.
            config.set_device_setting(
                &device_uid,
                &lcd_setting(LcdModeKind::Image {
                    image_file_processed: Some("/run/coolerdash/live.png".to_string()),
                }),
            );
            // The user sets their own shutdown image.
            let user_image = paths::config_dir().join("lcd_shutdown/user.png");
            cc_fs::create_dir_all(&paths::config_dir().join("lcd_shutdown"))
                .await
                .unwrap();
            cc_fs::write(&user_image, b"not-a-real-png".to_vec())
                .await
                .unwrap();
            let shutdown = LcdSettings {
                brightness: None,
                orientation: None,
                colors: Vec::new(),
                mode: LcdModeKind::Image {
                    image_file_processed: Some(user_image.to_str().unwrap().to_string()),
                },
            };
            config.set_lcd_shutdown_setting(&device_uid, LCD_CHANNEL, &shutdown);

            // First shutdown: the user's image is applied, and because the screen was
            // externally driven the engine persists it as the current setting.
            engine.apply_lcd_shutdown_images().await;
            assert_eq!(
                applied.borrow().len(),
                1,
                "the user's image must be applied"
            );

            // The user removes the shutdown image: the file goes and the setting goes.
            cc_fs::remove_file(&user_image).await.unwrap();
            config.remove_lcd_shutdown_setting(&device_uid, LCD_CHANNEL);
            applied.borrow_mut().clear();

            // Second shutdown: nothing is configured, so the stock image must be used.
            engine.apply_lcd_shutdown_images().await;

            assert_eq!(
                applied.borrow().len(),
                1,
                "the stock embedded shutdown image must be applied once the user's is removed"
            );
        });
    }
}
