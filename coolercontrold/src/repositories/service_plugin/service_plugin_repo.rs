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

use crate::config::{Config, DEFAULT_CONFIG_DIR};
use crate::device::{
    ChannelInfo, ChannelName, ChannelStatus, DeviceType, DeviceUID, TempInfo, TempName, TempStatus,
    TypeIndex, UID,
};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::repositories::service_plugin::device_service::v1::device_service_client::DeviceServiceClient;
use crate::repositories::service_plugin::device_service::v1::{
    health_response, HealthRequest, InitializeDevicesRequest, ListDevicesRequest, ShutdownRequest,
};
use crate::repositories::service_plugin::models;
use crate::repositories::service_plugin::service_config::{ServiceConfig, ServiceType};
use crate::repositories::service_plugin::service_management::manager::{
    Manager, ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_management::{ServiceId, ServiceIdExt};
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use crate::{cc_fs, ENV_CC_LOG};
use anyhow::Result;
use async_trait::async_trait;
use const_format::concatcp;
use log::{debug, error, info, trace, warn, LevelFilter};
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Not;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::time::{sleep, Instant};
use toml_edit::DocumentMut;
use tonic::transport::Channel;

type ServiceDeviceID = String;

pub const DEFAULT_PLUGINS_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/plugins");
const SERVICE_CONFIG_FILE_NAME: &str = "config.toml";
pub const CC_PLUGIN_USER: &str = "cc-plugin-user";
const TIMEOUT_SERVICE_START_SECONDS: usize = 5;

#[derive(Debug)]
struct DeviceServiceInfo {
    id: ServiceId,
    config: ServiceConfig,
    version: String,
    client: RefCell<DeviceServiceClient<Channel>>,
    devices: HashMap<ServiceDeviceID, models::v1::Device>,
}

pub struct ServicePluginRepo {
    config: Rc<Config>,
    service_manager: Manager,
    services: HashMap<ServiceId, DeviceServiceInfo>,
    devices: HashMap<DeviceUID, (DeviceLock, Rc<DeviceServiceInfo>)>,
    preloaded_statuses: RefCell<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    device_permits: HashMap<TypeIndex, Semaphore>,
}

impl ServicePluginRepo {
    pub fn new(config: Rc<Config>) -> Result<Self> {
        let service_manager = Manager::detect()?;
        Ok(Self {
            config,
            service_manager,
            services: HashMap::new(),
            devices: HashMap::new(),
            preloaded_statuses: RefCell::new(HashMap::new()),
            device_permits: HashMap::new(),
        })
    }

    async fn find_service_configs(&self) -> HashMap<ServiceId, ServiceConfig> {
        let plugins_dir = Path::new(DEFAULT_PLUGINS_PATH);
        let mut services = HashMap::new();
        let Ok(dir_entries) = cc_fs::read_dir(plugins_dir) else {
            debug!("Error reading plugins directory: {DEFAULT_PLUGINS_PATH}");
            if let Err(err) = cc_fs::create_dir_all(plugins_dir) {
                error!("Error creating plugins directory: {DEFAULT_PLUGINS_PATH} Reason: {err}");
            }
            return services;
        };
        // cycle through subdirectories looking for a config.toml file
        for entry in dir_entries {
            let Ok(dir_entry) = entry else {
                continue;
            };
            let path = dir_entry.path();
            if path.is_dir().not() {
                continue;
            }
            let service_config_file = path.join(SERVICE_CONFIG_FILE_NAME);
            if service_config_file.exists() {
                let Ok(config_content) = cc_fs::read_txt(&service_config_file).await else {
                    error!(
                        "Error reading plugin config file: {}",
                        service_config_file.display()
                    );
                    continue;
                };
                let Ok(document) = config_content.parse::<DocumentMut>() else {
                    error!(
                        "Error Parsing TOML configuration file, check the syntax: {}",
                        service_config_file.display()
                    );
                    continue;
                };
                match ServiceConfig::from_document(&document) {
                    Ok(config) => {
                        if services.contains_key(&config.id) {
                            error!(
                                "Service Name {} already registered. Skipping {}",
                                config.id,
                                service_config_file.display()
                            );
                            continue;
                        }
                        services.insert(config.id.clone(), config);
                    }
                    Err(err) => {
                        error!(
                            "Error parsing service config file: {} Reason: {err}",
                            service_config_file.display()
                        );
                    }
                }
            }
        }
        services
    }

    fn env_log_level() -> Vec<(String, String)> {
        match log::max_level() {
            LevelFilter::Off => {
                vec![(
                    ENV_CC_LOG.to_string(),
                    LevelFilter::Off.to_string().to_uppercase(),
                )]
            }
            LevelFilter::Error => {
                vec![(
                    ENV_CC_LOG.to_string(),
                    LevelFilter::Error.to_string().to_uppercase(),
                )]
            }
            LevelFilter::Warn => {
                vec![(
                    ENV_CC_LOG.to_string(),
                    LevelFilter::Warn.to_string().to_uppercase(),
                )]
            }
            LevelFilter::Info => {
                vec![(
                    ENV_CC_LOG.to_string(),
                    LevelFilter::Info.to_string().to_uppercase(),
                )]
            }
            LevelFilter::Debug => {
                vec![(
                    ENV_CC_LOG.to_string(),
                    LevelFilter::Debug.to_string().to_uppercase(),
                )]
            }
            LevelFilter::Trace => {
                vec![(
                    ENV_CC_LOG.to_string(),
                    LevelFilter::Trace.to_string().to_uppercase(),
                )]
            }
        }
    }
}

#[async_trait(?Send)]
impl Repository for ServicePluginRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::ServicePlugin
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Service Plugins Initialization");
        let start_initialization = Instant::now();
        let services = self.find_service_configs().await;
        for (service_id, service_config) in services {
            let username = service_config
                .privileged
                .not()
                .then_some(CC_PLUGIN_USER.to_string());
            // This is the only way to reload this daemon unit if already installed:
            let _ = self.service_manager.remove(&service_id).await;
            self.service_manager
                .add(ServiceDefinition {
                    service_id: service_id.clone(),
                    executable: service_config.executable.clone(),
                    args: service_config.args.clone(),
                    username,
                    wrk_dir: None,
                    envs: Some(Self::env_log_level()),
                    disable_restart_on_failure: false,
                })
                .await?;
            if let Err(err) = self.service_manager.start(&service_id).await {
                error!("Error starting plugin service: {err}");
                continue;
            }
            // check the service status and wait for 5 seconds for the service to be Running before giving up:
            let mut wait_secs = 0;
            while wait_secs < TIMEOUT_SERVICE_START_SECONDS {
                // We wait a moment in case the service crashes right after startup
                sleep(Duration::from_secs(1)).await;
                let status = self.service_manager.status(&service_id).await;
                if status.is_ok_and(|status| status == ServiceStatus::Running) {
                    break;
                }
                wait_secs += 1;
            }
            if wait_secs == TIMEOUT_SERVICE_START_SECONDS {
                error!(
                    "Service {service_id} did not start within {TIMEOUT_SERVICE_START_SECONDS} seconds"
                );
                continue;
            }
            if service_config.service_type == ServiceType::Integration {
                continue;
            }
            let unix_path = format!("unix://{}", service_config.uds.display());
            match DeviceServiceClient::connect(unix_path).await {
                Ok(mut client) => {
                    let mut version = String::new();
                    let mut retries = 0;
                    while retries < TIMEOUT_SERVICE_START_SECONDS {
                        let request = tonic::Request::new(HealthRequest {});
                        let result = client.health(request).await;
                        match result {
                            Ok(response) => {
                                let health_response = response.into_inner();
                                match health_response.status() {
                                    health_response::Status::Ok => {
                                        debug!("Service {} is healthy", health_response.name);
                                    }
                                    health_response::Status::Warning => {
                                        warn!("Service {} has warnings", health_response.name);
                                    }
                                    health_response::Status::Error => {
                                        error!("Service {} has errors", health_response.name);
                                    }
                                    _ => {
                                        error!(
                                            "Service {service_id} has unknown status. Shutting Service down."
                                        );
                                        let shutdown_request =
                                            tonic::Request::new(ShutdownRequest {});
                                        if let Err(status) = client.shutdown(shutdown_request).await
                                        {
                                            error!(
                                                "Error shutting down plugin service: {service_id} - {status}"
                                            );
                                        }
                                        continue;
                                    }
                                }
                                version = health_response.version;
                                break;
                            }
                            Err(status) => {
                                debug!("Health request returned status: {status}, retrying...");
                                retries += 1;
                            }
                        }
                    }
                    let request = tonic::Request::new(InitializeDevicesRequest {});
                    let result = client.initialize_devices(request).await;
                    if let Err(err) = result {
                        error!("Error initializing devices for {service_id}: {err}");
                        continue;
                    }
                    let request = tonic::Request::new(ListDevicesRequest {});
                    let result = client.list_devices(request).await;
                    let list = match result {
                        Ok(response) => response.into_inner(),
                        Err(err) => {
                            error!("Error listing devices for {service_id}: {err}");
                            continue;
                        }
                    };
                    let devices = list
                        .devices
                        .into_iter()
                        .map(|device| (device.id.clone(), device))
                        .collect();
                    self.services.insert(
                        service_id.clone(),
                        DeviceServiceInfo {
                            id: service_id.clone(),
                            version,
                            config: service_config,
                            client: RefCell::new(client),
                            devices,
                        },
                    );
                }
                Err(err) => {
                    error!(
                        "Could not establish a connection to the plugin service: {service_id} \
                        Make sure it is running and the socket: {} is accessible - {err:?}",
                        service_config.uds.display()
                    );
                    let _ = self.service_manager.stop(&service_id).await;
                    info!("Plugin Service {} stopped.", service_id.to_service_name());
                    let _ = self.service_manager.remove(&service_id).await;
                }
            }
            info!(
                "Plugin Service {} successfully started and connected.",
                service_id.to_service_name()
            );
        }

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(uid.clone(), (device.borrow().clone(), hwmon_info.clone()));
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized Service Plugin Devices: {init_devices:?}");
        } else {
            let device_map: HashMap<_, _> = init_devices
                .iter()
                .map(|d| {
                    (
                        d.1 .0.name.clone(),
                        HashMap::from([
                            (
                                "driver name",
                                vec![d.1 .0.info.driver_info.name.clone().unwrap_or_default()],
                            ),
                            (
                                "driver version",
                                vec![d.1 .0.info.driver_info.version.clone().unwrap_or_default()],
                            ),
                            ("locations", d.1 .0.info.driver_info.locations.clone()),
                        ]),
                    )
                })
                .collect();
            info!(
                "Initialized Service Plugin Devices: {}",
                serde_json::to_string(&device_map).unwrap_or_default()
            );
        }
        trace!(
            "Time taken to initialize Service Plugin devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("Service Plugin Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        vec![]
    }

    async fn preload_statuses(self: Rc<Self>) {
    }

    async fn update_statuses(&self) -> Result<()> {
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for service in self.services.values() {
            let request = tonic::Request::new(ShutdownRequest {});
            if let Err(status) = service.client.borrow_mut().shutdown(request).await {
                error!(
                    "Error shutting down plugin service: {} - {status}",
                    service.id
                );
            }
            debug!("Plugin Service {} internal shutdown complete", service.id);
            let _ = self.service_manager.remove(&service.id).await;
            info!("Plugin Service {} stopped.", service.id);
        }
        info!("Service Plugins Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_manual_control(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_lighting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lighting: &LightingSettings,
    ) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_lcd(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd: &LcdSettings,
    ) -> Result<()> {
        Ok(())
    }
    async fn apply_setting_pwm_mode(
        &self,
        device_uid: &UID,
        channel_name: &str,
        pwm_mode: u8,
    ) -> Result<()> {
        Ok(())
    }

    async fn reinitialize_devices(&self) {
    }
}
