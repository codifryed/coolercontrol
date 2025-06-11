#[cfg(test)]
mod tests {
    use crate::cc_fs;
    use crate::config::Config;
    use crate::device::{
        ChannelInfo, Device, DeviceInfo, DeviceType, DeviceUID, SpeedOptions, Status, TempStatus,
        UID,
    };
    use crate::processing::settings::SettingsController;
    use crate::repositories::repository::{DeviceList, DeviceLock, Repositories, Repository};
    use crate::setting::{LcdSettings, LightingSettings, Profile, ProfileType, TempSource};
    use anyhow::{anyhow, Result};
    use async_trait::async_trait;
    use serial_test::serial;
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::rc::Rc;

    // Mock repository for testing
    struct MockRepository {
        device_type: DeviceType,
        set_speeds: Rc<RefCell<Vec<u8>>>,
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
        SettingsController,
        Rc<Config>,
        Rc<RefCell<Vec<u8>>>,
    ) {
        let mut devices: HashMap<DeviceUID, DeviceLock> = HashMap::new();
        let mut repos = Repositories::default();
        let set_speeds = Rc::new(RefCell::new(Vec::new()));

        // Create mock repository
        let mock_repo = Rc::new(MockRepository {
            device_type: DeviceType::Hwmon,
            set_speeds: Rc::clone(&set_speeds),
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
        let settings_controller =
            SettingsController::new(all_devices, &all_repos, Rc::clone(&config));

        (device, settings_controller, config, set_speeds)
    }

    #[test]
    #[serial]
    fn test_no_application_without_settings() {
        cc_fs::test_runtime(async {
            // Given
            let (_device, settings_controller, _config, set_speeds) = setup_single_device();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                for _ in 0..3 {
                    settings_controller.process_scheduled_speeds(scope);
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
            let (device, settings_controller, config, set_speeds) = setup_single_device();

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
            settings_controller
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
                    settings_controller.process_scheduled_speeds(scope);
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
            let (device, settings_controller, config, set_speeds) = setup_single_device();

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
            settings_controller
                .set_profile(&device_uid, &fan_channel_name, &profile_uid)
                .await
                .unwrap();

            // When
            let scope_result = moro_local::async_scope!(|scope| {
                settings_controller.process_scheduled_speeds(scope);
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
            let (device, settings_controller, config, set_speeds) = setup_single_device();

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
            settings_controller
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
                    settings_controller.process_scheduled_speeds(scope);
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
}
