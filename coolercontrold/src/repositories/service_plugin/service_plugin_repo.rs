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
    ChannelExtensionNames, ChannelName, ChannelStatus, DeviceType, DeviceUID, Mhz, Status, Temp,
    TempStatus, Watts, RPM, UID,
};
use crate::grpc_api::device_service::v1::health_response;
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::repositories::service_plugin::client::DeviceServiceClient;
use crate::repositories::service_plugin::plugin_controller::{
    secure_config_file, PLUGIN_CONFIG_FILE_NAME,
};
use crate::repositories::service_plugin::service_management::manager::{
    Manager, ServiceDefinition, ServiceManager, ServiceStatus,
};
use crate::repositories::service_plugin::service_management::systemd::SystemdManager;
use crate::repositories::service_plugin::service_management::{ServiceId, ServiceIdExt};
use crate::repositories::service_plugin::service_manifest::{ServiceManifest, ServiceType};
use crate::setting::{CCDeviceSettings, LcdSettings, LightingSettings, TempSource};
use crate::{cc_fs, ENV_CC_LOG};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use const_format::concatcp;
use log::{debug, error, info, trace, warn, LevelFilter};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;
use toml_edit::DocumentMut;

pub type ServiceDeviceID = String;

pub const DEFAULT_PLUGINS_PATH: &str = concatcp!(DEFAULT_CONFIG_DIR, "/plugins");
const SERVICE_MANIFEST_FILE_NAME: &str = "manifest.toml";
pub const CC_PLUGIN_USER: &str = "cc-plugin-user";
const TIMEOUT_SERVICE_START_SECONDS: usize = 5;
const TIMEOUT_SERVICE_CONNECTION_SECONDS: usize = 10;
const TIMEOUT_API_UP_SECONDS: u64 = 60; // We have a 30-second max startup delay
const MISSING_STATUS_THRESHOLD: usize = 8;
const MISSING_TEMP_FAILSAFE: Temp = 100.;
const MISSING_DUTY_FAILSAFE: f64 = 0.;
const MISSING_RPM_FAILSAFE: RPM = 0;
const MISSING_WATTS_FAILSAFE: Watts = 0.;
const MISSING_FREQ_FAILSAFE: Mhz = 0;

#[derive(Debug)]
struct DeviceServiceConnection {
    id: ServiceId,
    version: String,
    client: DeviceServiceClient,
}

pub struct ServicePluginRepo {
    config: Rc<Config>,
    service_manager: Manager,
    api_up_token: CancellationToken,
    services: HashMap<ServiceId, (Option<Rc<DeviceServiceConnection>>, ServiceManifest)>,
    devices: HashMap<DeviceUID, (DeviceLock, Rc<DeviceServiceConnection>)>,
    preloaded_statuses: RefCell<HashMap<DeviceUID, PreloadData>>,
    failsafe_statuses: RefCell<HashMap<DeviceUID, FailsafeStatusData>>,
    disabled_channels: HashMap<DeviceUID, HashSet<String>>,
}

#[derive(Debug)]
struct PreloadData {
    channels: HashMap<ChannelName, ChannelStatus>,
    temps: HashMap<ChannelName, TempStatus>,
}

struct FailsafeStatusData {
    count: usize,
    logged: bool,

    /// These are failsafes metrics for each channel, should it's status be missing.
    channel_failsafes: HashMap<ChannelName, ChannelStatus>,
    temp_failsafes: HashMap<ChannelName, TempStatus>,
}

impl ServicePluginRepo {
    pub fn new(config: Rc<Config>, api_up_token: CancellationToken) -> Result<Self> {
        let service_manager = Manager::detect()?;
        Ok(Self {
            config,
            service_manager,
            api_up_token,
            services: HashMap::new(),
            devices: HashMap::new(),
            preloaded_statuses: RefCell::new(HashMap::new()),
            failsafe_statuses: RefCell::new(HashMap::new()),
            disabled_channels: HashMap::new(),
        })
    }

    async fn find_service_manifests() -> HashMap<ServiceId, ServiceManifest> {
        let plugins_dir = Path::new(DEFAULT_PLUGINS_PATH);
        let mut services = HashMap::new();
        let Ok(dir_entries) = cc_fs::read_dir(plugins_dir) else {
            debug!("Error reading plugins directory: {DEFAULT_PLUGINS_PATH}");
            if let Err(err) = cc_fs::create_dir_all(plugins_dir).await {
                error!("Error creating plugins directory: {DEFAULT_PLUGINS_PATH} Reason: {err}");
            }
            return services;
        };
        // cycle through subdirectories looking for a manifest.toml file
        for entry in dir_entries {
            let Ok(dir_entry) = entry else {
                continue;
            };
            let path = dir_entry.path();
            if path.is_dir().not() {
                continue;
            }
            let service_manifest_file = path.join(SERVICE_MANIFEST_FILE_NAME);
            if service_manifest_file.exists() {
                let Ok(manifest_content) = cc_fs::read_txt(&service_manifest_file).await else {
                    error!(
                        "Error reading plugin manifest: {}",
                        service_manifest_file.display()
                    );
                    continue;
                };
                let Ok(document) = manifest_content.parse::<DocumentMut>() else {
                    error!(
                        "Error Parsing TOML manifest file, check the syntax: {}",
                        service_manifest_file.display()
                    );
                    continue;
                };
                match ServiceManifest::from_document(&document, path) {
                    Ok(manifest) => {
                        if services.contains_key(&manifest.id) {
                            error!(
                                "Service Name {} already registered. Skipping {}",
                                manifest.id,
                                service_manifest_file.display()
                            );
                            continue;
                        }
                        services.insert(manifest.id.clone(), manifest);
                    }
                    Err(err) => {
                        error!(
                            "Error parsing service manifest file: {} Reason: {err}",
                            service_manifest_file.display()
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

    #[allow(clippy::too_many_lines)]
    async fn initialize_service(
        service_id: ServiceId,
        service_manifest: ServiceManifest,
        service_manager: Rc<Manager>,
        services: Rc<
            RefCell<HashMap<ServiceId, (Option<Rc<DeviceServiceConnection>>, ServiceManifest)>>,
        >,
        devices: Rc<RefCell<HashMap<DeviceUID, (DeviceLock, Rc<DeviceServiceConnection>)>>>,
        poll_rate: f64,
        api_up_token: CancellationToken,
    ) {
        let username = service_manifest
            .privileged
            .not()
            .then_some(CC_PLUGIN_USER.to_string());
        let mut envs = Self::env_log_level();
        envs.append(&mut service_manifest.envs.clone());
        // This will also reload this daemon unit if already installed:
        let _ = service_manager.remove(&service_id).await;
        if let Some(exe) = &service_manifest.executable {
            if let Err(e) = service_manager
                .add(ServiceDefinition {
                    service_id: service_id.clone(),
                    executable: exe.clone(),
                    args: service_manifest.args.clone(),
                    username,
                    wrk_dir: None,
                    envs: Some(envs),
                    disable_restart_on_failure: false,
                })
                .await
            {
                error!(
                    "Error adding plugin service. This service {service_id} will be skipped: {e}"
                );
                return;
            }
        }
        let config_path = service_manifest.path.join(PLUGIN_CONFIG_FILE_NAME);
        if config_path.exists() {
            let owner = service_manager.is_systemd().then(|| {
                if service_manifest.privileged {
                    "root"
                } else {
                    CC_PLUGIN_USER
                }
            });
            if let Err(err) = secure_config_file(&config_path, owner).await {
                warn!(
                    "Failed to secure plugin config file {}: {err}",
                    config_path.display()
                );
            }
        }
        match service_manifest.service_type {
            ServiceType::Integration => {
                if service_manifest.is_managed() {
                    Self::start_integration_service(&service_id, &service_manager, api_up_token);
                }
                // Integration services may not have client connection, but are possibly still managed
                services
                    .borrow_mut()
                    .insert(service_id, (None, service_manifest));
                return; // Integration service startup queued, no other action required
            }
            ServiceType::Device => {
                if service_manifest.is_managed() {
                    if let Err(err) = service_manager.start(&service_id).await {
                        error!(
                            "Error starting plugin service. This service {service_id} will be skipped: {err}"
                        );
                        let _ = service_manager.remove(&service_id).await;
                        return;
                    }
                }
            }
        }
        if service_manifest.is_managed() {
            let mut wait_secs = 0;
            while wait_secs < TIMEOUT_SERVICE_START_SECONDS {
                // It takes a moment for the status to come up, also for service crashing.
                sleep(Duration::from_secs(1)).await;
                let status = service_manager.status(&service_id).await;
                if status.is_ok_and(|status| status == ServiceStatus::Running) {
                    break;
                }
                wait_secs += 1;
            }
            if wait_secs == TIMEOUT_SERVICE_START_SECONDS {
                error!(
                    "Service {service_id} did not start within {TIMEOUT_SERVICE_START_SECONDS} seconds. This service will be skipped."
                );
                let _ = service_manager.remove(&service_id).await;
                return;
            }
        }
        let mut connect_wait_secs = 0;
        'connection: loop {
            match DeviceServiceClient::connect(&service_manifest, poll_rate).await {
                Ok(mut client) => {
                    let mut version = String::new();
                    let mut retries = 0;
                    'health: while retries < TIMEOUT_SERVICE_START_SECONDS {
                        match client.health().await {
                            Ok(response) => {
                                match response.status() {
                                    health_response::Status::Ok => {
                                        debug!("Service {} is healthy", response.name);
                                    }
                                    health_response::Status::Warning => {
                                        warn!("Service {} has warnings", response.name);
                                    }
                                    health_response::Status::Error => {
                                        error!("Service {} has errors", response.name);
                                    }
                                    _ => {
                                        error!(
                                            "Service {service_id} has unknown status. Shutting Service down."
                                        );
                                        if let Err(status) = client.shutdown().await {
                                            error!(
                                                "Error shutting down plugin service: {service_id} - {status}"
                                            );
                                        }
                                        let _ = service_manager.remove(&service_id).await;
                                        return;
                                    }
                                }
                                version = response.version;
                                break 'health;
                            }
                            Err(status) => {
                                debug!("Health request returned status: {status}, retrying...");
                                retries += 1;
                            }
                        }
                    }
                    if retries == TIMEOUT_SERVICE_START_SECONDS {
                        error!("Service {service_id} did not start within {TIMEOUT_SERVICE_START_SECONDS} seconds");
                        if service_manifest.is_managed() {
                            let _ = service_manager.remove(&service_id).await;
                        }
                        return;
                    }
                    let devices_response = match client.list_devices().await {
                        Ok(devices_response) => devices_response,
                        Err(err) => {
                            error!("Error listing devices for {service_id}: {err}");
                            return;
                        }
                    };
                    let device_ids = devices_response
                        .iter()
                        .map(|(service_device_id, device)| {
                            (device.uid.clone(), service_device_id.clone())
                        })
                        .collect();
                    client.with_device_ids(device_ids).await;
                    let device_service_conn = Rc::new(DeviceServiceConnection {
                        id: service_id.clone(),
                        version,
                        client,
                    });
                    for (_, device) in devices_response {
                        devices.borrow_mut().insert(
                            device.uid.clone(),
                            (
                                Rc::new(RefCell::new(device)),
                                Rc::clone(&device_service_conn),
                            ),
                        );
                    }
                    info!(
                        "Plugin Service {} v{} successfully started and connected.",
                        service_id.to_service_name(),
                        device_service_conn.version
                    );
                    services
                        .borrow_mut()
                        .insert(service_id, (Some(device_service_conn), service_manifest));
                    break 'connection;
                }
                Err(err) => {
                    connect_wait_secs += 1;
                    if connect_wait_secs < TIMEOUT_SERVICE_CONNECTION_SECONDS {
                        info!("Could not establish a connection to the plugin service: {service_id}. Retrying...");
                        sleep(Duration::from_secs(1)).await;
                    } else {
                        error!(
                            "Could not establish a connection to the plugin service: {service_id} \
                             Make sure it is running and the socket: {:?} is accessible - {err:?}",
                            service_manifest.address
                        );
                        if service_manifest.is_managed() {
                            let _ = service_manager.remove(&service_id).await;
                        }
                        info!("Plugin Service {} stopped.", service_id.to_service_name());
                        break 'connection;
                    }
                }
            }
        }
    }

    /// Starts an integration service in a detached task.
    /// This is used to start integration services after the daemon's API is up.
    fn start_integration_service(
        service_id: &ServiceId,
        service_manager: &Manager,
        api_up_token: CancellationToken,
    ) {
        let service_id = service_id.clone();
        let service_manager = service_manager.clone();
        tokio::task::spawn_local(async move {
            tokio::select! {
                // The api_up_token will be canceld once the daemon's API is up, making sure
                // that integration services connect at the proper time.
                () = sleep(Duration::from_secs(TIMEOUT_API_UP_SECONDS)) => warn!("Timeout waiting for the daemon's API to come up. Will start integration services anyway."),
                () = api_up_token.cancelled() => debug!("API startup complete, starting integration service: {service_id}"),
            }
            if let Err(err) = service_manager.start(&service_id).await {
                error!("Error starting plugin service: {err}");
                return;
            }
            let mut wait_secs = 0;
            while wait_secs < TIMEOUT_SERVICE_START_SECONDS {
                sleep(Duration::from_secs(1)).await;
                let status = service_manager.status(&service_id).await;
                if status.is_ok_and(|status| status == ServiceStatus::Running) {
                    break;
                }
                wait_secs += 1;
            }
            if wait_secs == TIMEOUT_SERVICE_START_SECONDS {
                error!(
                    "Service {service_id} did not start within {TIMEOUT_SERVICE_START_SECONDS} seconds"
                );
            }
        });
    }

    #[allow(clippy::await_holding_refcell_ref)]
    async fn init_devices_concurrently(
        &self,
        devices: &Rc<RefCell<HashMap<DeviceUID, (DeviceLock, Rc<DeviceServiceConnection>)>>>,
        devices_to_remove: &Rc<RefCell<Vec<DeviceUID>>>,
        preloaded_statuses: &Rc<RefCell<HashMap<DeviceUID, PreloadData>>>,
        failsafe_statuses: &Rc<RefCell<HashMap<DeviceUID, FailsafeStatusData>>>,
        poll_rate: f64,
    ) {
        // This is ok for initialization:
        let devices_lock = devices.borrow();
        moro_local::async_scope!(|device_init_scope| {
            for (device_uid, (device, service)) in devices_lock.iter() {
                let devices_to_remove = Rc::clone(devices_to_remove);
                let preloaded_statuses = Rc::clone(preloaded_statuses);
                let failsafe_statuses = Rc::clone(failsafe_statuses);
                let cc_device_setting = self
                    .config
                    .get_cc_settings_for_device(device_uid)
                    .ok()
                    .flatten();
                if cc_device_setting.as_ref().is_some_and(|s| s.disable) {
                    info!(
                        "Skipping disabled device: {} with UID: {device_uid}",
                        device.borrow().name
                    );
                    devices_to_remove.borrow_mut().push(device_uid.clone());
                    continue;
                }
                device_init_scope.spawn(Self::init_single_device(
                    device_uid,
                    device,
                    service,
                    devices_to_remove,
                    preloaded_statuses,
                    failsafe_statuses,
                    cc_device_setting,
                    poll_rate,
                ));
            }
        })
        .await;
    }

    /// Used to concurrently initialize device data for a single device
    async fn init_single_device(
        device_uid: &DeviceUID,
        device: &DeviceLock,
        service: &Rc<DeviceServiceConnection>,
        devices_to_remove: Rc<RefCell<Vec<DeviceUID>>>,
        preloaded_statuses: Rc<RefCell<HashMap<DeviceUID, PreloadData>>>,
        failsafe_statuses: Rc<RefCell<HashMap<DeviceUID, FailsafeStatusData>>>,
        cc_device_setting: Option<CCDeviceSettings>,
        poll_rate: f64,
    ) {
        if let Err(err) = service.client.initialize_device(device_uid).await {
            error!(
                "Error initializing device {device_uid} from Service {}. Skipping. - {err}",
                service.id
            );
            devices_to_remove.borrow_mut().push(device_uid.clone());
            return;
        }
        let Ok((mut channel_statuses, mut temp_statuses)) = service.client.status(device_uid).await
        else {
            error!(
                "Error getting device status for {device_uid} from Service {}. Skipping.",
                service.id
            );
            devices_to_remove.borrow_mut().push(device_uid.clone());
            return;
        };
        if device.borrow().info.channels.is_empty()
            && channel_statuses.is_empty()
            && temp_statuses.is_empty()
        {
            // If no fan, temp, lighting, or lcd channels, then no need to bother connecting and polling
            debug!(
                "Device {device_uid} from Service {} has no channels or temps. Skipping.",
                service.id
            );
            devices_to_remove.borrow_mut().push(device_uid.clone());
            return;
        }
        let disabled_channels =
            cc_device_setting.map_or_else(Vec::new, |setting| setting.get_disabled_channels());
        channel_statuses.retain(|s| disabled_channels.contains(&s.name).not());
        temp_statuses.retain(|s| disabled_channels.contains(&s.name).not());
        {
            let mut device_lock = device.borrow_mut();
            device_lock
                .info
                .channels
                .retain(|name, _info| disabled_channels.contains(name).not());
            device_lock
                .info
                .temps
                .retain(|name, _info| disabled_channels.contains(name).not());
        }

        let (channel_failsafes, temp_failsafes) =
            Self::create_failsafe_data(&channel_statuses, &temp_statuses);
        failsafe_statuses.borrow_mut().insert(
            device_uid.clone(),
            FailsafeStatusData {
                count: 0,
                logged: false,
                channel_failsafes,
                temp_failsafes,
            },
        );
        let preload_data = PreloadData {
            channels: channel_statuses
                .iter()
                .cloned()
                .map(|s| (s.name.clone(), s))
                .collect(),
            temps: temp_statuses
                .iter()
                .cloned()
                .map(|t| (t.name.clone(), t))
                .collect(),
        };
        preloaded_statuses
            .borrow_mut()
            .insert(device_uid.clone(), preload_data);
        let status = Status {
            channels: channel_statuses,
            temps: temp_statuses,
            ..Default::default()
        };
        device
            .borrow_mut()
            .initialize_status_history_with(status, poll_rate);
    }

    /// Creates failsafe data for all channels with initial status output.
    fn create_failsafe_data(
        channel_statuses: &[ChannelStatus],
        temp_statuses: &[TempStatus],
    ) -> (HashMap<String, ChannelStatus>, HashMap<String, TempStatus>) {
        let channel_failsafes = channel_statuses
            .iter()
            .map(|s| {
                let status = ChannelStatus {
                    name: s.name.clone(),
                    rpm: s.rpm.and(Some(MISSING_RPM_FAILSAFE)),
                    duty: s.duty.and(Some(MISSING_DUTY_FAILSAFE)),
                    freq: s.freq.and(Some(MISSING_FREQ_FAILSAFE)),
                    watts: s.watts.and(Some(MISSING_WATTS_FAILSAFE)),
                    pwm_mode: s.pwm_mode,
                };
                (s.name.clone(), status)
            })
            .collect();
        let temp_failsafes = temp_statuses
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    TempStatus {
                        name: t.name.clone(),
                        temp: MISSING_TEMP_FAILSAFE,
                    },
                )
            })
            .collect();
        (channel_failsafes, temp_failsafes)
    }

    async fn preload_device_status(
        self: Rc<Self>,
        device_uid: DeviceUID,
        service: &Rc<DeviceServiceConnection>,
    ) {
        let Ok((mut channel_statuses, mut temp_statuses)) =
            service.client.status(&device_uid).await
        else {
            let mut missing_lock = self.failsafe_statuses.borrow_mut();
            let msd = missing_lock
                .get_mut(&device_uid)
                .expect("Missing Status data should exist for existing Devices");
            msd.count += 1;
            if msd.count > MISSING_STATUS_THRESHOLD {
                if msd.logged.not() {
                    error!(
                        "There is a significant issue with retrieving status data for \
                                    device: {device_uid}, from service: {}. Setting critical values \
                                    for this device.",
                        service.id
                    );
                    msd.logged = true;
                }
                // insert ALL failsafe channels
                let preload_data = PreloadData {
                    channels: msd.channel_failsafes.clone(),
                    temps: msd.temp_failsafes.clone(),
                };
                self.preloaded_statuses
                    .borrow_mut()
                    .insert(device_uid, preload_data);
            }
            return;
        };
        let disabled_channels_for_device = self.disabled_channels.get(&device_uid);
        channel_statuses
            .retain(|s| Self::retain_enabled_channels(&s.name, disabled_channels_for_device));
        temp_statuses
            .retain(|s| Self::retain_enabled_channels(&s.name, disabled_channels_for_device));
        {
            let mut missing_lock = self.failsafe_statuses.borrow_mut();
            let msd = missing_lock
                .get_mut(&device_uid)
                .expect("Missing Status data should exist for existing Devices");
            let mut has_missing_statuses = false;
            for (f_name, f_status) in &msd.channel_failsafes {
                if channel_statuses.iter().all(|status| &status.name != f_name) {
                    if has_missing_statuses.not() {
                        has_missing_statuses = true;
                        msd.count += 1;
                    }
                    if msd.count > MISSING_STATUS_THRESHOLD {
                        channel_statuses.push(f_status.clone());
                    }
                }
            }
            for (f_name, f_status) in &msd.temp_failsafes {
                if temp_statuses.iter().all(|status| &status.name != f_name) {
                    if has_missing_statuses.not() {
                        has_missing_statuses = true;
                        msd.count += 1;
                    }
                    if msd.count > MISSING_STATUS_THRESHOLD {
                        temp_statuses.push(f_status.clone());
                    }
                }
            }
            if has_missing_statuses {
                if msd.count > MISSING_STATUS_THRESHOLD && msd.logged.not() {
                    error!(
                        "There is a significant issue with retrieving status data for \
                                    device: {device_uid}, from service: {}. Setting critical values \
                                    for this device.",
                        service.id
                    );
                    msd.logged = true;
                }
            } else if msd.count > 0 {
                // reset count on expected response
                msd.count = 0;
            }
        }
        let preload_data = PreloadData {
            channels: channel_statuses
                .into_iter()
                .map(|s| (s.name.clone(), s))
                .collect(),
            temps: temp_statuses
                .into_iter()
                .map(|t| (t.name.clone(), t))
                .collect(),
        };
        self.preloaded_statuses
            .borrow_mut()
            .insert(device_uid, preload_data);
    }

    fn retain_enabled_channels(
        channel_name: &str,
        disabled_channels_for_device: Option<&HashSet<String>>,
    ) -> bool {
        disabled_channels_for_device.is_none()
            || disabled_channels_for_device
                .is_some_and(|disabled_channels| disabled_channels.contains(channel_name).not())
    }

    pub fn is_systemd(&self) -> bool {
        self.service_manager.is_systemd()
    }

    /// Returns a copy of the plugins information, used by the plugin controller.
    pub fn get_plugins(&self) -> HashMap<ServiceId, ServiceManifest> {
        let mut plugins = HashMap::new();
        for (service_id, (_, service_manifest)) in &self.services {
            plugins.insert(service_id.clone(), service_manifest.clone());
        }
        plugins
    }
}

#[async_trait(?Send)]
impl Repository for ServicePluginRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::ServicePlugin
    }

    #[allow(clippy::too_many_lines)]
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Service Plugins Initialization");
        let start_initialization = Instant::now();
        // We use Rc to load things concurrently, then move them into the main struct
        let service_manager = Rc::new(self.service_manager.clone());
        let services = Rc::new(RefCell::new(HashMap::new()));
        let devices = Rc::new(RefCell::new(HashMap::new()));
        let preloaded_statuses = Rc::new(RefCell::new(HashMap::new()));
        let failsafe_statuses = Rc::new(RefCell::new(HashMap::new()));
        let poll_rate = self.config.get_settings()?.poll_rate;
        remove_plugin_user().await;
        moro_local::async_scope!(|service_init_scope| {
            for (service_id, service_manifest) in Self::find_service_manifests().await {
                let service_manager = Rc::clone(&service_manager);
                let services = Rc::clone(&services);
                let devices = Rc::clone(&devices);
                let api_up_token = self.api_up_token.clone();
                service_init_scope.spawn(async move {
                    Self::initialize_service(
                        service_id,
                        service_manifest,
                        service_manager,
                        services,
                        devices,
                        poll_rate,
                        api_up_token,
                    )
                    .await;
                });
            }
        })
        .await;

        let devices_to_remove = Rc::new(RefCell::new(Vec::new()));
        self.init_devices_concurrently(
            &devices,
            &devices_to_remove,
            &preloaded_statuses,
            &failsafe_statuses,
            poll_rate,
        )
        .await;
        for device_uid in devices_to_remove.borrow().iter() {
            devices.borrow_mut().remove(device_uid);
        }

        self.services = Rc::into_inner(services)
            .expect("All services references should be gone")
            .into_inner();
        self.devices = Rc::into_inner(devices)
            .expect("All devices references should be gone")
            .into_inner();
        self.preloaded_statuses =
            Rc::into_inner(preloaded_statuses).expect("All status references should be gone");
        self.failsafe_statuses = Rc::into_inner(failsafe_statuses)
            .expect("All failsafe_statuses references should be gone");
        if let Ok(cc_device_settings) = self.config.get_all_cc_devices_settings() {
            // get_all_cc_device_settings only returns devices that have settings
            for (device_uid, cc_device_settings) in cc_device_settings {
                self.disabled_channels.insert(
                    device_uid,
                    cc_device_settings
                        .get_disabled_channels()
                        .into_iter()
                        .collect(),
                );
            }
        }

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(uid.clone(), (device.borrow().clone(), hwmon_info.clone()));
        }
        if log::max_level() == LevelFilter::Debug {
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
        self.devices
            .values()
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        moro_local::async_scope!(|status_scope| {
            for (device_uid, (_device_lock, service)) in &self.devices {
                let self = Rc::clone(&self);
                status_scope.spawn(self.preload_device_status(device_uid.clone(), service));
            }
        })
        .await;
        trace!(
            "STATUS PRELOAD Time taken for all Device Service devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        for (device_uid, (device_lock, _)) in &self.devices {
            let (temps, channels) = self
                .preloaded_statuses
                .borrow()
                .get(device_uid)
                .map(|data| {
                    let temps = data.temps.values().cloned().collect();
                    let channels = data.channels.values().cloned().collect();
                    (temps, channels)
                })
                .expect("Preloaded Data should always be present");
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            trace!(
                "Hwmon device: {} status was updated with: {status:?}",
                device_lock.borrow().name
            );
            device_lock.borrow_mut().set_status(status);
        }
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        moro_local::async_scope!(|scope| {
            for (service_id, (optional_service_connection, service_manifest)) in &self.services {
                scope.spawn(async move {
                    if let Some(service) = optional_service_connection {
                        if let Err(status) = service.client.shutdown().await {
                            error!("Error shutting down plugin service: {service_id} - {status}");
                        }
                        debug!("Plugin Service {service_id} internal shutdown complete");
                    }
                    if service_manifest.is_managed() {
                        let _ = self.service_manager.remove(service_id).await;
                        info!("Plugin Service {service_id} stopped.");
                    }
                });
            }
        })
        .await;
        info!("Service Plugins Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let (_, device_service) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        debug!(
            "Applying Device Service Plugin: {} device: {device_uid} channel: {channel_name}; \
            Resetting to Original fan control mode",
            device_service.id,
        );
        device_service
            .client
            .reset_channel(device_uid, channel_name)
            .await
            .map_err(|status| anyhow!("Error resetting device channel: {status}"))
    }

    async fn apply_setting_manual_control(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<()> {
        let (device_lock, device_service) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        {
            let device = device_lock.borrow();
            let channel_info = device
                .info
                .channels
                .get(channel_name)
                .with_context(|| format!("Searching for channel name: {channel_name}"))?;
            if channel_info
                .speed_options
                .as_ref()
                .is_some_and(|opt| opt.fixed_enabled)
                .not()
            {
                return Err(anyhow!(
                    "Channel: {channel_name} does not support manual control"
                ));
            }
        }
        device_service
            .client
            .enable_manual_fan_control(device_uid, channel_name)
            .await
            .map_err(|status| anyhow!("Error enabling manual control for device channel: {status}"))
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {speed_fixed}"));
        }
        let (device_lock, device_service) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        {
            let device = device_lock.borrow();
            let channel_info = device
                .info
                .channels
                .get(channel_name)
                .with_context(|| format!("Searching for channel name: {channel_name}"))?;
            if channel_info
                .speed_options
                .as_ref()
                .is_some_and(|opt| opt.fixed_enabled)
                .not()
            {
                return Err(anyhow!(
                    "Channel: {channel_name} does not support setting fixed speeds"
                ));
            }
        }
        debug!(
            "Applying Service Plugin device: {device_uid} channel: {channel_name}; Fixed Speed: {speed_fixed}"
        );
        device_service
            .client
            .fixed_duty(device_uid, channel_name, speed_fixed)
            .await
            .map_err(|status| anyhow!("Error applying fixed speed for device channel: {status}"))
    }

    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        let (device_lock, device_service) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        {
            let device = device_lock.borrow();
            let channel_info = device
                .info
                .channels
                .get(channel_name)
                .with_context(|| format!("Searching for channel name: {channel_name}"))?;
            if channel_info
                .speed_options
                .as_ref()
                .is_some_and(|opt| {
                    opt.extension
                        .as_ref()
                        .is_some_and(|ext| ext == &ChannelExtensionNames::AutoHWCurve)
                })
                .not()
            {
                return Err(anyhow!(
                    "Channel: {channel_name} does not support firmware profiles"
                ));
            }
            if &temp_source.device_uid != device_uid {
                return Err(anyhow!(
                    "Applying Internal Profile Error: temp_source device_uid: {} does not match this device. \
                    Auto curves temperature sources must be internal to the device.",
                    temp_source.device_uid
                ));
            }
            if self
                .failsafe_statuses
                .borrow()
                .get(device_uid)
                .is_some_and(|d| d.temp_failsafes.contains_key(&temp_source.temp_name))
                .not()
            {
                return Err(anyhow!(
                    "Applying Internal Profile Error: temp_source channel: {} does not match a \
                    channel on this device. Auto curves temperature sources must be internal \
                    to the device.",
                    temp_source.temp_name
                ));
            }
        }
        debug!(
            "Applying Service Plugin device: {device_uid} channel: {channel_name}; Speed Profile: {speed_profile:?}"
        );
        device_service
            .client
            .speed_profile(device_uid, channel_name, temp_source, speed_profile)
            .await
            .map_err(|status| anyhow!("Error applying speed profile for device channel: {status}"))
    }

    async fn apply_setting_lighting(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lighting: &LightingSettings,
    ) -> Result<()> {
        let (device_lock, device_service) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        {
            let device = device_lock.borrow();
            let channel_info = device
                .info
                .channels
                .get(channel_name)
                .with_context(|| format!("Searching for channel name: {channel_name}"))?;
            if channel_info.lighting_modes.is_empty() {
                return Err(anyhow!(
                    "Channel: {channel_name} does not support lighting modes"
                ));
            }
        }
        debug!(
            "Applying Service Plugin device: {device_uid} channel: {channel_name}; Lighting: {lighting:?}"
        );
        device_service
            .client
            .lighting(device_uid, channel_name, lighting)
            .await
            .map_err(|status| {
                anyhow!("Error applying lighting settings for device channel: {status}")
            })
    }

    async fn apply_setting_lcd(
        &self,
        device_uid: &UID,
        channel_name: &str,
        lcd: &LcdSettings,
    ) -> Result<()> {
        let (device_lock, device_service) = self
            .devices
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        {
            let device = device_lock.borrow();
            let channel_info = device
                .info
                .channels
                .get(channel_name)
                .with_context(|| format!("Searching for channel name: {channel_name}"))?;
            if channel_info.lcd_modes.is_empty() {
                return Err(anyhow!(
                    "Channel: {channel_name} does not support LCD modes"
                ));
            }
        }
        debug!(
            "Applying Service Plugin device: {device_uid} channel: {channel_name}; LCD: {lcd:?}"
        );
        device_service
            .client
            .lcd(device_uid, channel_name, lcd)
            .await
            .map_err(|status| {
                anyhow!("Error applying lighting settings for device channel: {status}")
            })
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying pwm_mode setting is not supported for Service Plugin devices"
        ))
    }

    async fn reinitialize_devices(&self) {
        moro_local::async_scope!(|device_init_scope| {
            for (device_uid, (_device_lock, service)) in &self.devices {
                device_init_scope.spawn(async move {
                    if let Err(err) = service.client.initialize_device(device_uid).await {
                        warn!(
                            "Error initializing device {device_uid} from Service {} - {err}",
                            service.id
                        );
                    }
                });
            }
        })
        .await;
    }
}

/// This will delete the plugin user if it exists.
/// This is used to clean up the original plugin user which was a non-system user.
/// We can remove this after v3.1.1 is released.
async fn remove_plugin_user() {
    let _ = SystemdManager::delete_plugin_user(CC_PLUGIN_USER).await;
}
