/*
 * CoolerControl - monitor and control your cooling and other devices
 * Copyright (c) 2021-2024  Guy Boldon, Eren Simsek and contributors
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

use std::collections::HashMap;
use std::ops::{Add, Not};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, trace, warn};
use nu_glob::glob;
use regex::Regex;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::RwLock;
use tokio::time::{sleep, Instant};

use ShellCommandResult::{Error, Success};

use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, SpeedOptions, Status, TempInfo,
    TempStatus, TypeIndex, UID,
};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::{devices, fans, freqs, temps};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::repositories::utils::{ShellCommand, ShellCommandResult};
use crate::setting::{LcdSettings, LightingSettings, TempSource};

pub const GPU_TEMP_NAME: &str = "GPU Temp";
const GPU_FREQ_NAME: &str = "GPU Freq";
const GPU_LOAD_NAME: &str = "GPU Load";
// synonymous with amd hwmon fan names:
const NVIDIA_FAN_NAME: &str = "fan1";
const AMD_HWMON_NAME: &str = "amdgpu";
const GLOB_XAUTHORITY_PATH_GDM: &str = "/run/user/*/gdm/Xauthority";
const GLOB_XAUTHORITY_PATH_USER: &str = "/home/*/.Xauthority";
const GLOB_XAUTHORITY_PATH_SDDM: &str = "/run/sddm/xauth_*";
const GLOB_XAUTHORITY_PATH_SDDM_USER: &str = "/run/user/*/xauth_*";
const GLOB_XAUTHORITY_PATH_MUTTER_XWAYLAND_USER: &str = "/run/user/*/.*Xwaylandauth*";
const GLOB_XAUTHORITY_PATH_ROOT: &str = "/root/.Xauthority";
const PATTERN_GPU_INDEX: &str = r"\[gpu:(?P<index>\d+)\]";
const PATTERN_FAN_INDEX: &str = r"\[fan:(?P<index>\d+)\]";
const COMMAND_TIMEOUT_DEFAULT: Duration = Duration::from_millis(800);
const COMMAND_TIMEOUT_FIRST_TRY: Duration = Duration::from_secs(5);
const XAUTHORITY_SEARCH_TIMEOUT: Duration = Duration::from_secs(10);

type DisplayId = u8;
type GpuIndex = u8;
type FanIndex = u8;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    config: Arc<Config>,
    devices: HashMap<UID, DeviceLock>,
    nvidia_devices: HashMap<TypeIndex, DeviceLock>,
    nvidia_device_infos: HashMap<UID, NvidiaDeviceInfo>,
    nvidia_preloaded_statuses: RwLock<HashMap<TypeIndex, StatusNvidiaDevice>>,
    amd_device_infos: HashMap<UID, Arc<HwmonDriverInfo>>,
    amd_preloaded_statuses: RwLock<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    gpu_type_count: RwLock<HashMap<GpuType, u8>>,
    has_multiple_gpus: RwLock<bool>,
    xauthority_path: RwLock<Option<String>>,
}

impl GpuRepo {
    pub async fn new(config: Arc<Config>) -> Result<Self> {
        Ok(Self {
            config,
            devices: HashMap::new(),
            nvidia_devices: HashMap::new(),
            nvidia_device_infos: HashMap::new(),
            nvidia_preloaded_statuses: RwLock::new(HashMap::new()),
            amd_device_infos: HashMap::new(),
            amd_preloaded_statuses: RwLock::new(HashMap::new()),
            gpu_type_count: RwLock::new(HashMap::new()),
            has_multiple_gpus: RwLock::new(false),
            xauthority_path: RwLock::new(None),
        })
    }

    async fn detect_gpu_types(&self) {
        {
            let mut type_count = self.gpu_type_count.write().await;
            type_count.insert(
                GpuType::Nvidia,
                self.get_nvidia_status(COMMAND_TIMEOUT_FIRST_TRY)
                    .await
                    .len() as u8,
            );
            type_count.insert(GpuType::AMD, Self::init_amd_devices().await.len() as u8);
        }
        let number_of_gpus = self.gpu_type_count.read().await.values().sum::<u8>();
        let mut has_multiple_gpus = self.has_multiple_gpus.write().await;
        *has_multiple_gpus = number_of_gpus > 1;
        if number_of_gpus == 0 {
            warn!("No GPU Devices detected");
        }
    }

    /// Only attempts to retrieve Nvidia sensor values if Nvidia device was detected.
    async fn try_request_nv_statuses(&self) -> Vec<StatusNvidiaDevice> {
        let mut statuses = vec![];
        if self
            .gpu_type_count
            .read()
            .await
            .get(&GpuType::Nvidia)
            .unwrap()
            > &0
        {
            statuses.extend(self.request_nvidia_statuses().await);
        }
        statuses
    }

    /// Requests sensor data for Nvidia devices and maps the data to our internal model.
    async fn request_nvidia_statuses(&self) -> Vec<StatusNvidiaDevice> {
        let mut statuses = vec![];
        let nvidia_statuses = self.get_nvidia_status(COMMAND_TIMEOUT_DEFAULT).await;
        for nvidia_status in &nvidia_statuses {
            let mut temps = vec![];
            let mut channels = vec![];
            if let Some(temp) = nvidia_status.temp {
                let standard_temp_name = GPU_TEMP_NAME.to_string();
                temps.push(TempStatus {
                    name: standard_temp_name.clone(),
                    temp,
                });
            }
            if let Some(load) = nvidia_status.load {
                channels.push(ChannelStatus {
                    name: GPU_LOAD_NAME.to_string(),
                    duty: Some(f64::from(load)),
                    ..Default::default()
                });
            }
            if let Some(fan_duty) = nvidia_status.fan_duty {
                channels.push(ChannelStatus {
                    name: NVIDIA_FAN_NAME.to_string(),
                    duty: Some(f64::from(fan_duty)),
                    ..Default::default()
                });
            }
            statuses.push(StatusNvidiaDevice {
                index: nvidia_status.index,
                name: nvidia_status.name.clone(),
                temps,
                channels,
            });
        }
        statuses
    }

    /// Retrieve sensor data for all `NVidia` cards using `nvidia-smi`.
    /// Calling `nvidia-smi` is a relatively safe operation and will let us know if there is a
    /// `NVidia` device with the official `NVidia` drivers on the system.
    /// (Nouveau comes as a hwmon device)
    async fn get_nvidia_status(&self, command_timeout: Duration) -> Vec<StatusNvidia> {
        let mut nvidia_statuses = Vec::new();
        let command = "nvidia-smi \
        --query-gpu=index,gpu_name,temperature.gpu,utilization.gpu,fan.speed \
        --format=csv,noheader,nounits";
        let command_result = ShellCommand::new(command, command_timeout).run().await;
        match command_result {
            Error(stderr) => warn!(
                "Error communicating with nvidia-smi. \
                If you have a Nvidia card with proprietary drivers, \
                nvidia-smi and nvidia-settings are required. {}",
                stderr
            ),
            Success { stdout, stderr: _ } => {
                debug!("Nvidia raw status output: {}", stdout);
                for line in stdout.lines() {
                    if line.trim().is_empty() {
                        continue; // skip any empty lines
                    }
                    let values = line.split(", ").collect::<Vec<&str>>();
                    if values.len() >= 5 {
                        match values[0].parse::<u8>() {
                            Err(err) => {
                                error!("Something unexpected in nvidia status output: {}", err);
                            }
                            Ok(index) => {
                                nvidia_statuses.push(StatusNvidia {
                                    index,
                                    name: values[1].to_string(),
                                    temp: values[2].parse::<f64>().ok(),
                                    load: values[3].parse::<u8>().ok(),
                                    // on laptops for ex., this can be None as their is no fan control
                                    fan_duty: values[4].parse::<u8>().ok(),
                                });
                            }
                        }
                    }
                }
            }
        }
        nvidia_statuses
    }

    /// Get the various GPU and Fan information from `nvidia-settings`.
    /// For most cases it seems that the display id doesn't really matter, as each id will
    /// give the same output. But that is not true for all systems. Some systems are sensitive
    /// to the display id, and will only give the proper output when using the correct one.
    /// See: https://gitlab.com/coolercontrol/coolercontrol/-/issues/104
    /// Note: This implementation doesn't yet support multiple display servers with multiple display IDs.
    async fn get_nvidia_device_infos(
        &self,
    ) -> Result<HashMap<GpuIndex, (DisplayId, Vec<FanIndex>)>> {
        if self.xauthority_path.read().await.is_none() {
            error!(
                "Nvidia device detected but no xauthority cookie found which is needed \
            for proper communication with nvidia-settings. Nvidia Fan Control disabled."
            );
            return Ok(HashMap::new());
        }
        for display_id in 0..=3_u8 {
            let command = format!("nvidia-settings -c :{display_id} -q gpus --verbose");
            let command_result = ShellCommand::new(&command, COMMAND_TIMEOUT_FIRST_TRY)
                .env(
                    "XAUTHORITY",
                    &self
                        .xauthority_path
                        .read()
                        .await
                        .clone()
                        .unwrap_or_default(),
                )
                .run()
                .await;
            match command_result {
                Success { stdout, stderr } => {
                    debug!(
                        "Nvidia gpu info output from display :{} - {}",
                        display_id, stdout
                    );
                    if stdout.is_empty() {
                        warn!(
                            "nvidia-settings returned no data for display :{} - \
                            will retry on next display. Error output: {}",
                            display_id, stderr
                        );
                        continue;
                    } else {
                        return Ok(Self::process_nv_setting_output(display_id, &stdout));
                    }
                }
                Error(err) => {
                    return Err(anyhow!(
                        "Could not communicate with nvidia-settings. \
                        If you have a Nvidia card nvidia-settings needs to be installed for fan control. {}",
                        err));
                }
            }
        }
        Err(anyhow!(
            "Could not find applicable Display ID for nvidia-settings."
        ))
    }

    fn process_nv_setting_output(
        display_id: DisplayId,
        output: &str,
    ) -> HashMap<GpuIndex, (DisplayId, Vec<FanIndex>)> {
        let mut infos = HashMap::new();
        let mut gpu_index_current = 0_u8;
        let regex_gpu_index = Regex::new(PATTERN_GPU_INDEX).expect("This regex should be valid");
        let regex_fan_index = Regex::new(PATTERN_FAN_INDEX).expect("This regex should be valid");
        output
            .lines()
            .map(str::trim)
            .filter(|l| l.is_empty().not())
            .for_each(|line| {
                if regex_gpu_index.is_match(line) {
                    // happens first in the output
                    let gpu_index_found: u8 = regex_gpu_index
                        .captures(line)
                        .expect("GPU index should exist")
                        .name("index")
                        .expect("Index Regex Group should exist")
                        .as_str()
                        .parse()
                        .expect("GPU index should be a valid u8 integer");
                    gpu_index_current = gpu_index_found;
                    infos.insert(gpu_index_current, (display_id, Vec::new()));
                } else if regex_fan_index.is_match(line) {
                    let fan_index: u8 = regex_fan_index
                        .captures(line)
                        .expect("Fan index should exist")
                        .name("index")
                        .expect("Index Regex Group should exist")
                        .as_str()
                        .parse()
                        .expect("Fan index should be a valid u8 integer");
                    infos
                        .get_mut(&gpu_index_current)
                        .expect("GPU index should already be present")
                        .1
                        .push(fan_index);
                }
            });
        infos
    }

    /// Searches for the Xauthority magic cookie on the system. This is needed for
    /// `nvidia-settings` to work correctly. If it is not found, fan control is disabled.
    /// Often the cookie is not immediately available at boot time and extra time is needed to let
    /// the display-manager and Xorg to fully come up.
    /// See https://gitlab.com/coolercontrol/coolercontrol/-/issues/156
    async fn search_for_xauthority_path() -> Option<String> {
        let search_timeout_time = Instant::now().add(XAUTHORITY_SEARCH_TIMEOUT);
        while Instant::now() < search_timeout_time {
            sleep(Duration::from_millis(500)).await;
            if let Ok(environment_xauthority) = std::env::var("XAUTHORITY") {
                info!(
                    "Found existing Xauthority in the environment: {}",
                    environment_xauthority
                );
                return Some(environment_xauthority);
            } else {
                let xauthority_path_opt = glob(GLOB_XAUTHORITY_PATH_GDM)
                    .unwrap()
                    .chain(glob(GLOB_XAUTHORITY_PATH_USER).unwrap())
                    .chain(glob(GLOB_XAUTHORITY_PATH_SDDM).unwrap())
                    .chain(glob(GLOB_XAUTHORITY_PATH_SDDM_USER).unwrap())
                    .chain(glob(GLOB_XAUTHORITY_PATH_MUTTER_XWAYLAND_USER).unwrap())
                    .chain(glob(GLOB_XAUTHORITY_PATH_ROOT).unwrap())
                    .filter_map(Result::ok)
                    .find(|path| path.is_absolute());
                if let Some(xauthority_path) = xauthority_path_opt {
                    if let Some(xauthority_str) = xauthority_path.to_str() {
                        info!("Xauthority found in file path: {}", xauthority_str);
                        return Some(xauthority_str.to_owned());
                    }
                }
            }
        }
        error!(
            "Xauthority not found within {:?}.",
            XAUTHORITY_SEARCH_TIMEOUT
        );
        None
    }

    /// Sets the nvidia fan duty
    async fn set_nvidia_duty(&self, nvidia_info: &NvidiaDeviceInfo, fixed_speed: u8) -> Result<()> {
        if self.xauthority_path.read().await.is_none() {
            return Ok(()); // nvidia-settings won't work
        }
        let mut command = format!(
            "nvidia-settings -c :{0} -a \"[gpu:{1}]/GPUFanControlState=1\"",
            nvidia_info.display_id, nvidia_info.gpu_index
        );
        for fan_index in nvidia_info.fan_indices.iter().take(6) {
            // defensive take
            command.push_str(&format!(
                " -a \"[fan:{fan_index}]/GPUTargetFanSpeed={fixed_speed}\""
            ));
        }
        self.send_command_to_nvidia_settings(&command).await
    }

    /// resets the nvidia fan control back to automatic
    async fn reset_nvidia_to_default(&self, nvidia_info: &NvidiaDeviceInfo) -> Result<()> {
        if self.xauthority_path.read().await.is_none() {
            return Ok(()); // nvidia-settings won't work
        }
        let command = format!(
            "nvidia-settings -c :{0} -a \"[gpu:{1}]/GPUFanControlState=0\"",
            nvidia_info.display_id, nvidia_info.gpu_index
        );
        self.send_command_to_nvidia_settings(&command).await
    }

    async fn send_command_to_nvidia_settings(&self, command: &str) -> Result<()> {
        let command_result = ShellCommand::new(command, COMMAND_TIMEOUT_DEFAULT)
            .env(
                "XAUTHORITY",
                &self
                    .xauthority_path
                    .read()
                    .await
                    .clone()
                    .unwrap_or_default(),
            )
            .run()
            .await;
        match command_result {
            Error(stderr) => {
                if stderr.contains("Authorization required")
                    || stderr.contains("Error resolving target specification")
                {
                    error!(
                        "Error communicating with nvidia-settings and appears to be an issue with \
                        the Xauthority file. Xauthority file reset in progress."
                    );
                    let mut xauth = self.xauthority_path.write().await;
                    *xauth = Self::search_for_xauthority_path().await;
                }
                Err(anyhow!(
                    "Error communicating with nvidia-settings: {}",
                    stderr
                ))
            }
            Success { stdout, stderr } => {
                debug!("Nvidia-settings output: {} - {}", stdout, stderr);
                if stderr.is_empty() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Error output received when trying to set nvidia fan speed settings. \
                    Some errors don't affect setting the fan speed. YMMV: {}",
                        stderr
                    ))
                }
            }
        }
    }

    async fn init_amd_devices() -> Vec<HwmonDriverInfo> {
        let base_paths = devices::find_all_hwmon_device_paths();
        let mut amd_devices = vec![];
        for path in base_paths {
            let device_name = devices::get_device_name(&path).await;
            if device_name != AMD_HWMON_NAME {
                continue;
            }
            let mut channels = vec![];
            match fans::init_fans(&path, &device_name).await {
                Ok(fans) => channels.extend(fans),
                Err(err) => error!("Error initializing AMD Hwmon Fans: {}", err),
            };
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(temps),
                Err(err) => error!("Error initializing AMD Hwmon Temps: {}", err),
            };
            if let Some(load_channel) = Self::init_amd_load(&path).await {
                channels.push(load_channel);
            }
            match freqs::init_freqs(&path).await {
                Ok(freqs) => channels.extend(freqs),
                Err(err) => error!("Error initializing AMD Hwmon Freqs: {}", err),
            };
            let model = devices::get_device_model_name(&path).await;
            let u_id = devices::get_device_unique_id(&path).await;
            let hwmon_driver_info = HwmonDriverInfo {
                name: device_name,
                path,
                model,
                u_id,
                channels,
            };
            amd_devices.push(hwmon_driver_info);
        }
        amd_devices
    }

    async fn init_amd_load(base_path: &PathBuf) -> Option<HwmonChannelInfo> {
        match tokio::fs::read_to_string(base_path.join("device").join("gpu_busy_percent")).await {
            Ok(load) => match fans::check_parsing_8(load) {
                Ok(_) => Some(HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Load,
                    name: GPU_LOAD_NAME.to_string(),
                    label: Some(GPU_LOAD_NAME.to_string()),
                    ..Default::default()
                }),
                Err(err) => {
                    warn!("Error reading AMD busy percent value: {}", err);
                    None
                }
            },
            Err(_) => {
                warn!(
                    "No AMDGPU load found: {:?}/device/gpu_busy_percent",
                    base_path
                );
                None
            }
        }
    }

    async fn get_amd_status(
        &self,
        amd_driver: &HwmonDriverInfo,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = fans::extract_fan_statuses(amd_driver).await;
        status_channels.extend(Self::extract_load_status(amd_driver).await);
        status_channels.extend(freqs::extract_freq_statuses(amd_driver).await);
        let temps = temps::extract_temp_statuses(amd_driver)
            .await
            .iter()
            .map(|temp| TempStatus {
                name: temp.name.clone(),
                temp: temp.temp,
            })
            .collect();
        (status_channels, temps)
    }

    async fn extract_load_status(driver: &HwmonDriverInfo) -> Vec<ChannelStatus> {
        let mut channels = vec![];
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Load {
                continue;
            }
            let load =
                tokio::fs::read_to_string(driver.path.join("device").join("gpu_busy_percent"))
                    .await
                    .and_then(fans::check_parsing_8)
                    .unwrap_or(0);
            channels.push(ChannelStatus {
                name: channel.name.clone(),
                duty: Some(f64::from(load)),
                ..Default::default()
            });
        }
        channels
    }

    async fn reset_amd_to_default(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let amd_hwmon_info = self
            .amd_device_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let channel_info = amd_hwmon_info
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| format!("Searching for channel name: {channel_name}"))?;
        fans::set_pwm_enable_to_default(&amd_hwmon_info.path, channel_info).await
    }

    async fn set_amd_duty(
        &self,
        device_uid: &UID,
        channel_name: &str,
        fixed_speed: u8,
    ) -> Result<()> {
        let amd_hwmon_info = self
            .amd_device_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let channel_info = amd_hwmon_info
            .channels
            .iter()
            .find(|channel| {
                channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
            })
            .with_context(|| "Searching for channel name")?;
        fans::set_pwm_duty(&amd_hwmon_info.path, channel_info, fixed_speed).await
    }

    async fn initialize_amd_devices(&mut self) -> Result<()> {
        for (index, amd_driver) in Self::init_amd_devices().await.into_iter().enumerate() {
            let id = index as u8 + 1;
            let mut channels = HashMap::new();
            for channel in &amd_driver.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Fan => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            speed_options: Some(SpeedOptions {
                                profiles_enabled: false,
                                fixed_enabled: true,
                                manual_profiles_enabled: true,
                                ..Default::default()
                            }),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Load => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Freq => {
                        let label_base = channel
                            .label
                            .as_ref()
                            .map(|l| l.to_title_case())
                            .unwrap_or_else(|| channel.name.to_title_case());
                        let channel_info = ChannelInfo {
                            label: Some(format!("{GPU_FREQ_NAME} {label_base}")),
                            ..Default::default()
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    _ => continue,
                }
            }
            let amd_status = self.get_amd_status(&amd_driver).await;
            self.amd_preloaded_statuses
                .write()
                .await
                .insert(id, amd_status.clone());
            let temps = amd_driver
                .channels
                .iter()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    let label_base = channel
                        .label
                        .as_ref()
                        .map(|l| l.to_title_case())
                        .unwrap_or_else(|| channel.name.to_title_case());
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: format!("{GPU_TEMP_NAME} {label_base}"),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut device = Device::new(
                amd_driver.name.clone(),
                DeviceType::GPU,
                id,
                None,
                DeviceInfo {
                    temps,
                    channels,
                    temp_max: 100,
                    model: amd_driver.model.clone(),
                    ..Default::default()
                },
                Some(amd_driver.u_id.clone()),
            );
            let status = Status {
                channels: amd_status.0,
                temps: amd_status.1,
                ..Default::default()
            };
            device.initialize_status_history_with(status);
            let cc_device_setting = self.config.get_cc_settings_for_device(&device.uid).await?;
            if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                info!(
                    "Skipping disabled device: {} with UID: {}",
                    device.name, device.uid
                );
                continue; // skip loading this device into the device list
            }
            self.amd_device_infos
                .insert(device.uid.clone(), Arc::new(amd_driver.clone()));
            self.devices
                .insert(device.uid.clone(), Arc::new(RwLock::new(device)));
        }
        Ok(())
    }

    async fn initialize_nvidia_devices(&mut self) -> Result<()> {
        if self
            .gpu_type_count
            .read()
            .await
            .get(&GpuType::Nvidia)
            .unwrap_or(&0)
            == &0
        {
            return Ok(()); // skip if no Nvidia devices detected
        }
        let starting_nvidia_index = if *self.has_multiple_gpus.read().await {
            self.gpu_type_count
                .read()
                .await
                .get(&GpuType::AMD)
                .unwrap_or(&0)
                + 1
        } else {
            1
        };
        {
            let mut xauth = self.xauthority_path.write().await;
            *xauth = Self::search_for_xauthority_path().await;
        }
        match self.get_nvidia_device_infos().await {
            Err(err) => {
                error!("{}", err);
                Ok(()) // skip nvidia devices if something has unexpectedly gone wrong
            }
            Ok(mut nvidia_infos) => {
                for nv_status in self.request_nvidia_statuses().await {
                    if self.xauthority_path.read().await.is_none() {
                        nvidia_infos.insert(nv_status.index, (0, vec![0])); // set defaults
                    }
                    let type_index = nv_status.index + starting_nvidia_index;
                    self.nvidia_preloaded_statuses
                        .write()
                        .await
                        .insert(type_index, nv_status.clone());
                    let status = Status {
                        channels: nv_status.channels,
                        temps: nv_status.temps,
                        ..Default::default()
                    };
                    let temps = status
                        .temps
                        .iter()
                        .enumerate()
                        .map(|(index, temp_status)| {
                            (
                                temp_status.name.clone(),
                                TempInfo {
                                    label: temp_status.name.clone(),
                                    number: index as u8,
                                },
                            )
                        })
                        .collect();
                    let mut channels = HashMap::new();
                    if status
                        .channels
                        .iter()
                        .any(|channel| channel.name == NVIDIA_FAN_NAME)
                    {
                        channels.insert(
                            NVIDIA_FAN_NAME.to_string(),
                            ChannelInfo {
                                speed_options: Some(SpeedOptions {
                                    profiles_enabled: false,
                                    fixed_enabled: self.xauthority_path.read().await.is_some(), // disable if xauth not found
                                    manual_profiles_enabled: self
                                        .xauthority_path
                                        .read()
                                        .await
                                        .is_some(),
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        );
                    }
                    let mut device_raw = Device::new(
                        nv_status.name,
                        DeviceType::GPU,
                        type_index,
                        None,
                        DeviceInfo {
                            temps,
                            temp_max: 100,
                            channels,
                            ..Default::default()
                        },
                        None,
                    );
                    device_raw.initialize_status_history_with(status);
                    let uid = device_raw.uid.clone();
                    let cc_device_setting = self.config.get_cc_settings_for_device(&uid).await?;
                    if cc_device_setting.is_some() && cc_device_setting.unwrap().disable {
                        info!(
                            "Skipping disabled device: {} with UID: {}",
                            device_raw.name, uid
                        );
                        continue; // skip loading this device into the device list
                    }
                    let device = Arc::new(RwLock::new(device_raw));
                    self.nvidia_devices.insert(type_index, Arc::clone(&device));
                    let (display_id, fan_indices) = nvidia_infos
                        .get(&nv_status.index)
                        .with_context(|| {
                            format!(
                                "Nvidia GPU index not found! {}, index: {}",
                                uid, nv_status.index
                            )
                        })?
                        .to_owned();
                    self.nvidia_device_infos.insert(
                        uid.clone(),
                        NvidiaDeviceInfo {
                            gpu_index: nv_status.index,
                            display_id,
                            fan_indices,
                        },
                    );
                    self.devices.insert(uid, device);
                }
                Ok(())
            }
        }
    }
}

#[async_trait]
impl Repository for GpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::GPU
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.detect_gpu_types().await;
        self.initialize_amd_devices().await?;
        self.initialize_nvidia_devices().await?;
        let mut init_devices = HashMap::new();
        for (uid, device) in &self.devices {
            init_devices.insert(uid.clone(), device.read().await.clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized GPU Devices: {:?}", init_devices);
            info!("Initialized AMD HwmonInfos: {:?}", self.amd_device_infos);
        } else {
            info!(
                "Initialized GPU Devices: {:?}",
                init_devices
                    .iter()
                    .map(|d| d.1.name.clone())
                    .collect::<Vec<String>>()
            );
        }
        trace!(
            "Time taken to initialize all GPU devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("GPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn preload_statuses(self: Arc<Self>) {
        let start_update = Instant::now();

        let mut tasks = Vec::new();
        for (uid, amd_driver) in &self.amd_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let self = Arc::clone(&self);
                let device_lock = Arc::clone(device_lock);
                let amd_driver = Arc::clone(amd_driver);
                let join_handle = tokio::task::spawn(async move {
                    let type_index = device_lock.read().await.type_index;
                    let statuses = self.get_amd_status(&amd_driver).await;
                    self.amd_preloaded_statuses
                        .write()
                        .await
                        .insert(type_index, statuses);
                });
                tasks.push(join_handle);
            }
        }
        let self = Arc::clone(&self);
        let join_handle = tokio::task::spawn(async move {
            let mut nv_status_map = HashMap::new();
            for nv_status in self.try_request_nv_statuses().await {
                nv_status_map.insert(nv_status.index, nv_status);
            }
            for (uid, nv_info) in &self.nvidia_device_infos {
                if let Some(device_lock) = self.devices.get(uid) {
                    let type_index = device_lock.read().await.type_index;
                    if let Some(nv_status) = nv_status_map.remove(&nv_info.gpu_index) {
                        self.nvidia_preloaded_statuses
                            .write()
                            .await
                            .insert(type_index, nv_status);
                    } else {
                        error!("GPU Index not found in Nvidia status response");
                    }
                }
            }
        });
        tasks.push(join_handle);
        for task in tasks {
            if let Err(err) = task.await {
                error!("{}", err);
            }
        }
        trace!(
            "STATUS PRELOAD Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        let start_update = Instant::now();
        for (uid, amd_driver) in &self.amd_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let preloaded_statuses_map = self.amd_preloaded_statuses.read().await;
                let preloaded_statuses =
                    preloaded_statuses_map.get(&device_lock.read().await.type_index);
                if preloaded_statuses.is_none() {
                    error!(
                        "There is no status preloaded for this AMD device: {}",
                        device_lock.read().await.type_index
                    );
                    continue;
                }
                let (channels, temps) = preloaded_statuses.unwrap().clone();
                let status = Status {
                    temps,
                    channels,
                    ..Default::default()
                };
                trace!("Device: {} status updated: {:?}", amd_driver.name, status);
                device_lock.write().await.set_status(status);
            }
        }
        for (type_index, nv_device_lock) in &self.nvidia_devices {
            let preloaded_statuses_map = self.nvidia_preloaded_statuses.read().await;
            let preloaded_statuses = preloaded_statuses_map.get(type_index);
            if preloaded_statuses.is_none() {
                error!(
                    "There is no status preloaded for this Nvidia device: {}",
                    type_index
                );
                continue;
            }
            let nv_status = preloaded_statuses.unwrap().clone();
            let status = Status {
                channels: nv_status.channels.clone(),
                temps: nv_status.temps.clone(),
                ..Default::default()
            };
            trace!("Device: {} status updated: {:?}", nv_status.name, status);
            nv_device_lock.write().await.set_status(status);
        }
        trace!(
            "STATUS SNAPSHOT Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (uid, device_lock) in &self.devices {
            let is_amd = self.amd_device_infos.contains_key(uid);
            if is_amd {
                for channel_name in device_lock.read().await.info.channels.keys() {
                    self.reset_amd_to_default(uid, channel_name).await.ok();
                }
            } else if let Some(nvidia_info) = self.nvidia_device_infos.get(uid) {
                self.reset_nvidia_to_default(nvidia_info).await.ok();
            };
        }
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        debug!(
            "Applying GPU device: {} channel: {}; Resetting to Automatic fan control",
            device_uid, channel_name
        );
        let is_amd = self.amd_device_infos.contains_key(device_uid);
        if is_amd {
            self.reset_amd_to_default(device_uid, channel_name).await
        } else {
            let nvidia_gpu_index = self
                .nvidia_device_infos
                .get(device_uid)
                .with_context(|| format!("Nvidia Device Info by UID not found! {device_uid}"))?;
            self.reset_nvidia_to_default(nvidia_gpu_index).await
        }
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        debug!(
            "Applying GPU device: {} channel: {}; Fixed Speed: {}",
            device_uid, channel_name, speed_fixed
        );
        let is_amd = self.amd_device_infos.contains_key(device_uid);
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {}", speed_fixed));
        }
        if is_amd {
            self.set_amd_duty(device_uid, channel_name, speed_fixed)
                .await
        } else {
            let nvidia_gpu_info = self
                .nvidia_device_infos
                .get(device_uid)
                .with_context(|| format!("Device UID not found! {device_uid}"))?;
            self.set_nvidia_duty(nvidia_gpu_info, speed_fixed).await
        }
    }

    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Speed Profiles are not supported for GPU devices"
        ))
    }

    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying Speed Profiles are not supported for GPU devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying LCD settings are not supported for GPU devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying pwm modes are not supported for GPU devices"
        ))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StatusNvidia {
    index: u8,
    name: String,
    temp: Option<f64>,
    load: Option<u8>,
    fan_duty: Option<u8>,
}

#[derive(Debug, Clone)]
struct StatusNvidiaDevice {
    index: u8,
    name: String,
    channels: Vec<ChannelStatus>,
    temps: Vec<TempStatus>,
}

#[derive(Debug)]
struct NvidiaDeviceInfo {
    gpu_index: GpuIndex,
    display_id: DisplayId,
    fan_indices: Vec<FanIndex>,
}
