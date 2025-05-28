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
use std::borrow::Cow;
use std::cell::{Cell, Ref, RefCell};
use std::collections::HashMap;
use std::ops::{Add, Not, Sub};
use std::rc::Rc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use log::{debug, error, info, trace, warn};
use nu_glob::{glob, Uninterruptible};
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::enums::device::SampleValue;
use nvml_wrapper::structs::device::FieldId;
use nvml_wrapper::sys_exports::field_id;
use nvml_wrapper::Nvml;
use regex::Regex;
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;
use tokio::time::{sleep, Instant};

use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelName, ChannelStatus, Device, DeviceInfo, DeviceType, DriverInfo,
    DriverType, Duty, SpeedOptions, Status, Temp, TempInfo, TempStatus, TypeIndex, UID,
};
use crate::repositories::gpu::gpu_repo::{
    COMMAND_TIMEOUT_DEFAULT, COMMAND_TIMEOUT_FIRST_TRY, GPU_LOAD_NAME, GPU_POWER_NAME,
    GPU_TEMP_NAME,
};
use crate::repositories::repository::DeviceLock;
use crate::repositories::utils::ShellCommand;
use crate::repositories::utils::ShellCommandResult::{Error, Success};

// Only way this works for our current implementation.
// See: https://github.com/Cldfire/nvml-wrapper/issues/21
static NVML: OnceCell<Nvml> = OnceCell::const_new();
const GPU_TEMP_MEMORY_NAME: &str = "GPU Temp Memory";

// synonymous with amd hwmon fan names:
const NVIDIA_FAN_NAME: &str = "fan1";
const NVIDIA_FAN_PREFIX: &str = "fan";
const NVIDIA_FREQ_PREFIX: &str = "GPU Freq";
const NVIDIA_CLOCK_GRAPHICS: &str = "freq_graphics";
const NVIDIA_CLOCK_SM: &str = "freq_sm";
const NVIDIA_CLOCK_MEMORY: &str = "freq_memory";
const NVIDIA_CLOCK_VIDEO: &str = "freq_video";
const GLOB_XAUTHORITY_PATH_GDM: &str = "/run/user/*/gdm/Xauthority";
const GLOB_XAUTHORITY_PATH_USER: &str = "/home/*/.Xauthority";
const GLOB_XAUTHORITY_PATH_SDDM: &str = "/run/sddm/xauth_*";
const GLOB_XAUTHORITY_PATH_SDDM_USER: &str = "/run/user/*/xauth_*";
const GLOB_XAUTHORITY_PATH_MUTTER_XWAYLAND_USER: &str = "/run/user/*/.*Xwaylandauth*";
const GLOB_XAUTHORITY_PATH_ROOT: &str = "/root/.Xauthority";
const PATTERN_GPU_INDEX: &str = r"\[gpu:(?P<index>\d+)\]";
const PATTERN_FAN_INDEX: &str = r"\[fan:(?P<index>\d+)\]";
const PATTERN_FAN_CHANNEL_INDEX: &str = r"^fan(?P<index>\d+)";
const XAUTHORITY_SEARCH_TIMEOUT: Duration = Duration::from_secs(10);

type DisplayId = u8;
/// The index used by the drivers:
type GpuIndex = u8;
type FanIndex = u8;

pub struct GpuNVidia {
    config: Rc<Config>,
    nvidia_devices: HashMap<TypeIndex, DeviceLock>,
    pub nvidia_device_infos: HashMap<UID, Rc<NvidiaDeviceInfo>>,
    pub nvidia_preloaded_statuses: RefCell<HashMap<TypeIndex, StatusNvidiaDeviceSMI>>,
    nvidia_nvml_devices: HashMap<GpuIndex, RefCell<nvml_wrapper::Device<'static>>>,
    xauthority_path: RefCell<Option<String>>,
    nvidia_smi_disabled_channels: RefCell<HashMap<GpuIndex, Vec<ChannelName>>>,
    nvidia_nvml_load_enabled: Cell<bool>,
}

impl GpuNVidia {
    pub fn new(config: Rc<Config>) -> Self {
        Self {
            config,
            nvidia_devices: HashMap::new(),
            nvidia_device_infos: HashMap::new(),
            nvidia_preloaded_statuses: RefCell::new(HashMap::new()),
            nvidia_nvml_devices: HashMap::new(),
            xauthority_path: RefCell::new(None),
            nvidia_smi_disabled_channels: RefCell::new(HashMap::new()),
            nvidia_nvml_load_enabled: Cell::new(false),
        }
    }

    pub async fn initialize_nvidia_devices(
        &mut self,
        starting_nvidia_index: GpuIndex,
    ) -> Result<HashMap<UID, DeviceLock>> {
        let nvidia_devices = if self.nvidia_nvml_devices.is_empty() {
            self.init_nvidia_smi_devices(starting_nvidia_index).await?
        } else {
            self.retrieve_nvml_devices(starting_nvidia_index)?
        };
        Ok(nvidia_devices)
    }

    pub fn update_all_statuses(&self) {
        for (type_index, nv_device_lock) in &self.nvidia_devices {
            let preloaded_statuses_map = self.nvidia_preloaded_statuses.borrow();
            let preloaded_statuses = preloaded_statuses_map.get(type_index);
            if preloaded_statuses.is_none() {
                error!("There is no status preloaded for this Nvidia device: {type_index}");
                continue;
            }
            let nv_status = preloaded_statuses.unwrap().clone();
            let status = Status {
                channels: nv_status.channels.clone(),
                temps: nv_status.temps.clone(),
                ..Default::default()
            };
            trace!("Device: {} status updated: {:?}", nv_status.name, status);
            nv_device_lock.borrow_mut().set_status(status);
        }
    }

    pub async fn reset_devices(&self) {
        for device_lock in self.nvidia_devices.values() {
            let device_uid = device_lock.borrow().uid.clone();
            if let Some(nv_info) = self.nvidia_device_infos.get(&device_uid) {
                if self.nvidia_nvml_devices.is_empty() {
                    self.reset_nvidia_settings_to_default(nv_info).await.ok();
                } else {
                    for channel_name in device_lock.borrow().info.channels.keys() {
                        self.reset_nvml_device_to_default(nv_info, channel_name)
                            .ok();
                    }
                }
            }
        }
    }

    pub async fn reset_device(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let Some(nv_info) = self.nvidia_device_infos.get(device_uid) else {
            return Err(anyhow!("Device UID not found! {device_uid}"));
        };
        if self.nvidia_nvml_devices.is_empty() {
            self.reset_nvidia_settings_to_default(nv_info).await?;
        } else {
            self.reset_nvml_device_to_default(nv_info, channel_name)?;
        }
        Ok(())
    }

    pub async fn set_fan_duty(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        let nvidia_gpu_info = self
            .nvidia_device_infos
            .get(device_uid)
            .with_context(|| format!("Device UID not found! {device_uid}"))?;
        if self.nvidia_nvml_devices.is_empty() {
            self.set_nvidia_settings_fan_duty(nvidia_gpu_info, speed_fixed)
                .await
        } else {
            self.set_nvml_fan_duty(nvidia_gpu_info, channel_name, speed_fixed)
                .map_err(|err| {
                    anyhow!(
                        "Error settings fan duty of {speed_fixed} on Nvidia GPU #{}:{channel_name} - {err}",
                        nvidia_gpu_info.gpu_index
                    )
                })
        }
    }

    // --------------------------------------------------------------------------------------------
    // NVML
    // --------------------------------------------------------------------------------------------

    #[allow(clippy::cast_possible_truncation)]
    pub fn init_nvml_devices(&mut self) -> Option<u8> {
        let nvml = Nvml::init()
            .inspect_err(|err| {
                debug!("NVML not found or failed to initialize, falling back to CLI tools");
                debug!("NVML initialize error: {err}");
            })
            .ok()?;
        info!("NVML found and initialized.");
        NVML.set(nvml)
            .inspect_err(|err| error!("Error setting NVML lib: {err}"))
            .ok()?;
        let device_count = NVML
            .get()
            .unwrap()
            .device_count()
            .inspect_err(|err| error!("Error getting NVML device count: {err}"))
            .ok()?;
        debug!("Found {device_count} NVML devices");
        for device_index in 0..device_count {
            let Ok(accessible_device) = NVML
                .get()?
                .device_by_index(device_index)
                // unexpected/not allowed:
                .inspect_err(|err| error!("Error getting NVML device by index: {err}"))
            else {
                continue;
            };
            self.nvidia_nvml_devices
                .insert(device_index as GpuIndex, RefCell::new(accessible_device));
        }
        if self.nvidia_nvml_devices.is_empty() {
            warn!("No NVML accessible devices found, falling back to CLI tools");
            None
        } else {
            Some(self.nvidia_nvml_devices.len() as u8)
        }
    }

    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub fn retrieve_nvml_devices(
        &mut self,
        starting_nvidia_index: u8,
    ) -> Result<HashMap<UID, DeviceLock>> {
        let mut devices = HashMap::new();
        let poll_rate = self.config.get_settings()?.poll_rate;
        for (gpu_index, device) in &self.nvidia_nvml_devices {
            let device_lock = device.borrow();
            let type_index = gpu_index + starting_nvidia_index;
            let name = device_lock
                .name()
                .unwrap_or_else(|_| format!("Nvidia GPU #{type_index}"));
            let device_uid = Device::create_uid_from(&name, &DeviceType::GPU, type_index, None);
            let cc_device_setting = self.config.get_cc_settings_for_device(&device_uid)?;
            if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
                info!("Skipping disabled device: {name} with UID: {device_uid}");
                continue;
            }
            let disabled_channels =
                cc_device_setting.map_or_else(Vec::new, |setting| setting.disable_channels);
            self.nvidia_nvml_load_enabled
                .set(disabled_channels.contains(&GPU_LOAD_NAME.to_string()).not());
            let mut nvidia_temp_infos = Vec::new();
            let mut nvidia_freq_infos = Vec::new();
            let mut nvidia_power_supported = false;
            let mut temp_infos = HashMap::new();
            let mut temp_status = Vec::new();

            let temp_name = GPU_TEMP_NAME.to_string();
            if disabled_channels.contains(&temp_name).not() {
                if let Ok(temp) = device_lock.temperature(TemperatureSensor::Gpu) {
                    temp_infos.insert(
                        temp_name.clone(),
                        TempInfo {
                            label: temp_name.clone(),
                            number: 1,
                        },
                    );
                    temp_status.push(TempStatus {
                        name: temp_name.clone(),
                        temp: f64::from(temp),
                    });
                    nvidia_temp_infos.push(temp_name);
                }
            }
            let mem_temp_name = GPU_TEMP_MEMORY_NAME.to_string();
            if disabled_channels.contains(&mem_temp_name).not() {
                if let Some(mem_temp) = Self::get_memory_temp(&device_lock) {
                    temp_infos.insert(
                        mem_temp_name.clone(),
                        TempInfo {
                            label: mem_temp_name.clone(),
                            number: 2,
                        },
                    );
                    temp_status.push(TempStatus {
                        name: mem_temp_name.clone(),
                        temp: mem_temp,
                    });
                    nvidia_temp_infos.push(mem_temp_name);
                }
            }
            let mut channel_infos = HashMap::new();
            let mut channel_status = Vec::new();
            let mut fan_indices = Vec::new();
            let num_fans = device_lock.num_fans().unwrap_or_default() as u8;
            for fan_index in 0..num_fans {
                let fan_index_u32 = u32::from(fan_index);
                let Ok(fan_speed) = device_lock.fan_speed(fan_index_u32) else {
                    continue;
                };
                let fan_rpm = device_lock.fan_speed_rpm(fan_index_u32).ok();
                let fan_name = format!("{NVIDIA_FAN_PREFIX}{}", fan_index + 1);
                if disabled_channels.contains(&fan_name).not() {
                    fan_indices.push(fan_index);
                    channel_infos.insert(
                        fan_name.clone(),
                        ChannelInfo {
                            label: Some(fan_name.clone()),
                            speed_options: Some(SpeedOptions {
                                profiles_enabled: false,
                                fixed_enabled: true,
                                manual_profiles_enabled: true,
                                ..Default::default()
                            }),
                            ..Default::default()
                        },
                    );
                    channel_status.push(ChannelStatus {
                        name: fan_name,
                        duty: Some(f64::from(fan_speed)),
                        rpm: fan_rpm,
                        ..Default::default()
                    });
                }
            }
            if self.nvidia_nvml_load_enabled.get() {
                let load_name = GPU_LOAD_NAME.to_string();
                if let Ok(util_rates) = device_lock.utilization_rates() {
                    channel_infos.insert(
                        load_name.clone(),
                        ChannelInfo {
                            label: Some(load_name.clone()),
                            ..Default::default()
                        },
                    );
                    channel_status.push(ChannelStatus {
                        name: load_name,
                        duty: Some(f64::from(util_rates.gpu)),
                        ..Default::default()
                    });
                }
            }
            if disabled_channels
                .contains(&NVIDIA_CLOCK_GRAPHICS.to_string())
                .not()
            {
                Self::add_nvml_clock_label(
                    &device_lock,
                    Clock::Graphics,
                    NVIDIA_CLOCK_GRAPHICS,
                    format!("{NVIDIA_FREQ_PREFIX} Graphics"),
                    &mut channel_infos,
                    &mut nvidia_freq_infos,
                );
                Self::add_nvml_clock_status(
                    &device_lock,
                    Clock::Graphics,
                    NVIDIA_CLOCK_GRAPHICS,
                    &mut channel_status,
                );
            }
            if disabled_channels
                .contains(&NVIDIA_CLOCK_SM.to_string())
                .not()
            {
                Self::add_nvml_clock_label(
                    &device_lock,
                    Clock::SM,
                    NVIDIA_CLOCK_SM,
                    format!("{NVIDIA_FREQ_PREFIX} SM"),
                    &mut channel_infos,
                    &mut nvidia_freq_infos,
                );
                Self::add_nvml_clock_status(
                    &device_lock,
                    Clock::SM,
                    NVIDIA_CLOCK_SM,
                    &mut channel_status,
                );
            }
            if disabled_channels
                .contains(&NVIDIA_CLOCK_MEMORY.to_string())
                .not()
            {
                Self::add_nvml_clock_label(
                    &device_lock,
                    Clock::Memory,
                    NVIDIA_CLOCK_MEMORY,
                    format!("{NVIDIA_FREQ_PREFIX} Memory"),
                    &mut channel_infos,
                    &mut nvidia_freq_infos,
                );
                Self::add_nvml_clock_status(
                    &device_lock,
                    Clock::Memory,
                    NVIDIA_CLOCK_MEMORY,
                    &mut channel_status,
                );
            }
            if disabled_channels
                .contains(&NVIDIA_CLOCK_VIDEO.to_string())
                .not()
            {
                Self::add_nvml_clock_label(
                    &device_lock,
                    Clock::Video,
                    NVIDIA_CLOCK_VIDEO,
                    format!("{NVIDIA_FREQ_PREFIX} Video"),
                    &mut channel_infos,
                    &mut nvidia_freq_infos,
                );
                Self::add_nvml_clock_status(
                    &device_lock,
                    Clock::Video,
                    NVIDIA_CLOCK_VIDEO,
                    &mut channel_status,
                );
            }
            if disabled_channels
                .contains(&GPU_POWER_NAME.to_string())
                .not()
            {
                let power_name = GPU_POWER_NAME.to_string();
                if let Ok(milli_watts) = device_lock.power_usage() {
                    channel_infos.insert(
                        power_name.clone(),
                        ChannelInfo {
                            label: Some(power_name.clone()),
                            ..Default::default()
                        },
                    );
                    channel_status.push(ChannelStatus {
                        name: power_name,
                        watts: Some(Self::convert_milliwatts_to_watts(milli_watts)),
                        ..Default::default()
                    });
                    nvidia_power_supported = true;
                }
            }

            let driver_version = device_lock.nvml().sys_driver_version().ok();
            let driver_name = format!(
                "nvml:{}",
                device_lock.nvml().sys_nvml_version().unwrap_or_default()
            );
            let mut driver_locations = Vec::new();
            if let Ok(pci_info) = device_lock.pci_info() {
                driver_locations.push(pci_info.bus_id);
            }
            let mut device_raw = Device::new(
                name,
                DeviceType::GPU,
                type_index,
                None,
                DeviceInfo {
                    temps: temp_infos,
                    temp_max: 100,
                    channels: channel_infos,
                    driver_info: DriverInfo {
                        drv_type: DriverType::NVML,
                        name: Some(driver_name),
                        version: driver_version,
                        locations: driver_locations,
                    },
                    ..Default::default()
                },
                None,
                poll_rate,
            );
            let status = Status {
                channels: channel_status,
                temps: temp_status,
                ..Default::default()
            };
            device_raw.initialize_status_history_with(status, poll_rate);
            let uid = device_raw.uid.clone();

            let device = Rc::new(RefCell::new(device_raw));
            self.nvidia_devices.insert(type_index, Rc::clone(&device));
            self.nvidia_device_infos.insert(
                uid.clone(),
                Rc::new(NvidiaDeviceInfo {
                    gpu_index: *gpu_index,
                    display_id: 0,
                    fan_indices,
                    temps: nvidia_temp_infos,
                    freqs: nvidia_freq_infos,
                    power: nvidia_power_supported,
                }),
            );
            devices.insert(uid, device);
        }
        Ok(devices)
    }

    fn add_nvml_clock_label(
        nvml_device: &Ref<nvml_wrapper::Device>,
        clock_type: Clock,
        clock_name: &str,
        label: String,
        channel_infos: &mut HashMap<String, ChannelInfo>,
        nvidia_freq_infos: &mut Vec<Clock>,
    ) {
        if nvml_device.clock_info(clock_type.clone()).is_ok() {
            channel_infos.insert(
                clock_name.to_string(),
                ChannelInfo {
                    label: Some(label),
                    ..Default::default()
                },
            );
            nvidia_freq_infos.push(clock_type);
        }
    }

    fn add_nvml_clock_status(
        nvml_device: &Ref<nvml_wrapper::Device>,
        clock_type: Clock,
        clock_name: &str,
        channel_status: &mut Vec<ChannelStatus>,
    ) {
        if let Ok(freq) = nvml_device.clock_info(clock_type) {
            channel_status.push(ChannelStatus {
                name: clock_name.to_string(),
                freq: Some(freq),
                ..Default::default()
            });
        }
    }

    fn convert_milliwatts_to_watts(milli_watts: u32) -> f64 {
        f64::from(milli_watts / 1_000)
    }

    #[allow(clippy::cast_precision_loss)]
    fn get_memory_temp(nvml_device: &Ref<nvml_wrapper::Device>) -> Option<Temp> {
        let field_values = nvml_device
            .field_values_for(&[FieldId(field_id::NVML_FI_DEV_MEMORY_TEMP)])
            .ok()?; // If not supported, will return here
        field_values
            .into_iter()
            .find_map(Result::ok)
            .filter(|field_sample| field_sample.field == FieldId(field_id::NVML_FI_DEV_MEMORY_TEMP))
            .and_then(|field_sample| field_sample.value.ok())
            .map(|sample_value| match sample_value {
                SampleValue::F64(value) => value,
                SampleValue::U32(value) => Temp::from(value),
                SampleValue::U64(value) => value as Temp,
                SampleValue::I64(value) => value as Temp,
            })
            // Do not return negative or zero values (also means not supported)
            .filter(|temp| *temp > 0.)
    }

    pub fn request_nvml_status(&self, nv_info: &Rc<NvidiaDeviceInfo>) -> StatusNvidiaDeviceNvml {
        let nvml_device_lock = self
            .nvidia_nvml_devices
            .get(&nv_info.gpu_index)
            .expect("Device should exist")
            .borrow();
        let temp_status = Self::get_nvml_temp_status(&nvml_device_lock, nv_info);
        let mut channel_status = Vec::new();
        for fan_index in &nv_info.fan_indices {
            let fan_index_u32 = u32::from(*fan_index);
            let Ok(fan_speed) = nvml_device_lock.fan_speed(fan_index_u32) else {
                continue;
            };
            let fan_rpm = nvml_device_lock.fan_speed_rpm(fan_index_u32).ok();
            channel_status.push(ChannelStatus {
                name: format!("{NVIDIA_FAN_PREFIX}{}", fan_index + 1),
                duty: Some(f64::from(fan_speed)),
                rpm: fan_rpm,
                ..Default::default()
            });
        }
        if self.nvidia_nvml_load_enabled.get() {
            if let Ok(util_rates) = nvml_device_lock.utilization_rates() {
                channel_status.push(ChannelStatus {
                    name: GPU_LOAD_NAME.to_string(),
                    duty: Some(f64::from(util_rates.gpu)),
                    ..Default::default()
                });
            }
        }
        if nv_info.power {
            if let Ok(milli_watts) = nvml_device_lock.power_usage() {
                channel_status.push(ChannelStatus {
                    name: GPU_POWER_NAME.to_string(),
                    watts: Some(Self::convert_milliwatts_to_watts(milli_watts)),
                    ..Default::default()
                });
            }
        }
        Self::get_nvml_freq_status(&nvml_device_lock, nv_info, &mut channel_status);
        StatusNvidiaDeviceNvml {
            temps: temp_status,
            channels: channel_status,
        }
    }

    fn get_nvml_temp_status(
        nvml_device: &Ref<nvml_wrapper::Device>,
        nv_info: &Rc<NvidiaDeviceInfo>,
    ) -> Vec<TempStatus> {
        let mut temp_status = Vec::new();
        for nvidia_temp_name in &nv_info.temps {
            match nvidia_temp_name.as_str() {
                GPU_TEMP_NAME => {
                    if let Ok(temp) = nvml_device.temperature(TemperatureSensor::Gpu) {
                        temp_status.push(TempStatus {
                            name: GPU_TEMP_NAME.to_string(),
                            temp: f64::from(temp),
                        });
                    }
                }
                GPU_TEMP_MEMORY_NAME => {
                    if let Some(mem_temp) = Self::get_memory_temp(nvml_device) {
                        temp_status.push(TempStatus {
                            name: GPU_TEMP_MEMORY_NAME.to_string(),
                            temp: mem_temp,
                        });
                    }
                }
                _ => {
                    error!("Unexpected Nvidia temp name: {nvidia_temp_name}");
                }
            }
        }
        temp_status
    }

    fn get_nvml_freq_status(
        nvml_device: &Ref<nvml_wrapper::Device>,
        nv_info: &Rc<NvidiaDeviceInfo>,
        channel_status: &mut Vec<ChannelStatus>,
    ) {
        for nvml_clock_type in &nv_info.freqs {
            match nvml_clock_type {
                Clock::Graphics => Self::add_nvml_clock_status(
                    nvml_device,
                    Clock::Graphics,
                    NVIDIA_CLOCK_GRAPHICS,
                    channel_status,
                ),
                Clock::SM => Self::add_nvml_clock_status(
                    nvml_device,
                    Clock::SM,
                    NVIDIA_CLOCK_SM,
                    channel_status,
                ),
                Clock::Memory => Self::add_nvml_clock_status(
                    nvml_device,
                    Clock::Memory,
                    NVIDIA_CLOCK_MEMORY,
                    channel_status,
                ),
                Clock::Video => Self::add_nvml_clock_status(
                    nvml_device,
                    Clock::Video,
                    NVIDIA_CLOCK_VIDEO,
                    channel_status,
                ),
            }
        }
    }

    /// resets the nvidia fan control back to automatic
    fn reset_nvml_device_to_default(
        &self,
        nv_info: &Rc<NvidiaDeviceInfo>,
        channel_name: &str,
    ) -> Result<()> {
        let nvml_device = self
            .nvidia_nvml_devices
            .get(&nv_info.gpu_index)
            .expect("Device should exist");
        let fan_index = Self::parse_fan_index(channel_name)?;
        Self::verify_fan_index(nv_info, fan_index)?;
        nvml_device
            .borrow_mut()
            .set_default_fan_speed(u32::from(fan_index))?;
        Ok(())
    }

    fn set_nvml_fan_duty(
        &self,
        nv_info: &Rc<NvidiaDeviceInfo>,
        channel_name: &str,
        fan_duty: Duty,
    ) -> Result<()> {
        let nvml_device = self
            .nvidia_nvml_devices
            .get(&nv_info.gpu_index)
            .expect("Device should exist");
        let fan_index = Self::parse_fan_index(channel_name)?;
        Self::verify_fan_index(nv_info, fan_index)?;
        nvml_device
            .borrow_mut()
            .set_fan_speed(u32::from(fan_index), u32::from(fan_duty))?;
        Ok(())
    }

    fn parse_fan_index(channel_name: &str) -> Result<FanIndex> {
        let regex_fan_index =
            Regex::new(PATTERN_FAN_CHANNEL_INDEX).expect("This regex should be valid");
        let fan_index = regex_fan_index
            .captures(channel_name)
            .with_context(|| "Nvidia fan channel name Should always match")?
            .name("index")
            .with_context(|| "This should be a valid capture name")?
            .as_str()
            .parse::<FanIndex>()?
            .sub(1);
        Ok(fan_index)
    }

    fn verify_fan_index(nv_info: &Rc<NvidiaDeviceInfo>, fan_index: u8) -> Result<()> {
        if nv_info.fan_indices.contains(&fan_index) {
            Ok(())
        } else {
            Err(anyhow!(
                "Fan index: {fan_index} not found for this device index: {}",
                nv_info.gpu_index
            ))
        }
    }

    // --------------------------------------------------------------------------------------------
    // NVidia-SMI
    // --------------------------------------------------------------------------------------------

    /// Retrieve sensor data for all `NVidia` cards using `nvidia-smi`.
    /// Calling `nvidia-smi` is a relatively safe operation and will let us know if there is a
    /// `NVidia` device with the official `NVidia` drivers on the system.
    /// (Nouveau comes as a hwmon device)
    pub async fn get_nvidia_smi_status(&self, command_timeout: Duration) -> Vec<StatusNvidia> {
        let mut nvidia_statuses = Vec::new();
        let command = "nvidia-smi \
        --query-gpu=index,gpu_name,temperature.gpu,utilization.gpu,fan.speed \
        --format=csv,noheader,nounits";
        let command_result = ShellCommand::new(command, command_timeout).run().await;
        match command_result {
            Error(stderr) => {
                debug!(
                    "Nvidia proprietary driver not found. Assuming not present. If you have a \
                    Nvidia card with the proprietary drivers installed, allow access to either \
                    NVML or the CLI tools: nvidia-smi and nvidia-settings."
                );
                debug!("Error trying to communicate with nvidia-smi: {stderr}");
            }
            Success { stdout, stderr: _ } => {
                debug!("Nvidia raw status output: {stdout}");
                for line in stdout.lines() {
                    if line.trim().is_empty() {
                        continue; // skip any empty lines
                    }
                    let values = line.split(", ").collect::<Vec<&str>>();
                    if values.len() >= 5 {
                        match values[0].parse::<u8>() {
                            Err(err) => {
                                error!("Something unexpected in nvidia status output: {err}");
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

    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub async fn init_nvidia_smi_devices(
        &mut self,
        starting_nvidia_index: u8,
    ) -> Result<HashMap<UID, DeviceLock>> {
        let mut devices = HashMap::new();
        let poll_rate = self.config.get_settings()?.poll_rate;
        {
            let xauthority_path = Self::search_for_xauthority_path().await;
            self.xauthority_path.replace(xauthority_path);
        }
        match self.get_nvidia_device_infos().await {
            Err(err) => {
                error!("{err}");
                Ok(devices) // skip nvidia devices if something has unexpectedly gone wrong
            }
            Ok(mut nvidia_infos) => {
                for nv_status in self.request_nvidia_smi_statuses().await {
                    if self.xauthority_path.borrow().is_none() {
                        nvidia_infos.insert(nv_status.index, (0, vec![0])); // set defaults
                    }
                    let type_index = nv_status.index + starting_nvidia_index;
                    let device_uid = Device::create_uid_from(
                        &nv_status.name,
                        &DeviceType::GPU,
                        type_index,
                        None,
                    );
                    let cc_device_setting = self.config.get_cc_settings_for_device(&device_uid)?;
                    if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
                        info!(
                            "Skipping disabled device: {} with UID: {device_uid}",
                            nv_status.name
                        );
                        continue;
                    }
                    let disabled_channels =
                        cc_device_setting.map_or_else(Vec::new, |setting| setting.disable_channels);
                    self.nvidia_smi_disabled_channels
                        .borrow_mut()
                        .insert(nv_status.index, disabled_channels.clone());
                    self.nvidia_preloaded_statuses
                        .borrow_mut()
                        .insert(type_index, nv_status.clone());
                    let status = Status {
                        channels: nv_status
                            .channels
                            .into_iter()
                            .filter(|channel| disabled_channels.contains(&channel.name).not())
                            .collect::<Vec<ChannelStatus>>(),
                        temps: nv_status
                            .temps
                            .into_iter()
                            .filter(|temp| disabled_channels.contains(&temp.name).not())
                            .collect::<Vec<TempStatus>>(),
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
                        let has_xauth = self.xauthority_path.borrow().is_some();
                        channels.insert(
                            NVIDIA_FAN_NAME.to_string(),
                            ChannelInfo {
                                speed_options: Some(SpeedOptions {
                                    profiles_enabled: false,
                                    fixed_enabled: has_xauth, // disable if xauth not found
                                    manual_profiles_enabled: has_xauth,
                                    ..Default::default()
                                }),
                                ..Default::default()
                            },
                        );
                    }
                    if status
                        .channels
                        .iter()
                        .any(|channel| channel.name == GPU_LOAD_NAME)
                    {
                        channels.insert(
                            GPU_LOAD_NAME.to_string(),
                            ChannelInfo {
                                label: Some(GPU_LOAD_NAME.to_string()),
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
                            driver_info: DriverInfo {
                                drv_type: DriverType::NvidiaCLI,
                                name: Some("nvidia-smi;nvidia-settings".to_string()),
                                version: None,
                                locations: Vec::new(),
                            },
                            ..Default::default()
                        },
                        None,
                        poll_rate,
                    );
                    device_raw.initialize_status_history_with(status, poll_rate);
                    let uid = device_raw.uid.clone();
                    let device = Rc::new(RefCell::new(device_raw));
                    self.nvidia_devices.insert(type_index, Rc::clone(&device));
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
                        Rc::new(NvidiaDeviceInfo {
                            gpu_index: nv_status.index,
                            display_id,
                            fan_indices,
                            temps: Vec::new(),
                            freqs: Vec::new(),
                            power: false,
                        }),
                    );
                    devices.insert(uid, device);
                }
                Ok(devices)
            }
        }
    }

    /// Searches for the Xauthority magic cookie on the system. This is needed for
    /// `nvidia-settings` to work correctly. If it is not found, fan control is disabled.
    /// Often the cookie is not immediately available at boot time and extra time is needed to let
    /// the display-manager and Xorg to fully come up.
    /// See [issue](https://gitlab.com/coolercontrol/coolercontrol/-/issues/156)
    async fn search_for_xauthority_path() -> Option<String> {
        let search_timeout_time = Instant::now().add(XAUTHORITY_SEARCH_TIMEOUT);
        while Instant::now() < search_timeout_time {
            sleep(Duration::from_millis(500)).await;
            if let Ok(environment_xauthority) = std::env::var("XAUTHORITY") {
                info!("Found existing Xauthority in the environment: {environment_xauthority}");
                return Some(environment_xauthority);
            }
            let xauthority_path_opt = glob(GLOB_XAUTHORITY_PATH_GDM, Uninterruptible)
                .unwrap()
                .chain(glob(GLOB_XAUTHORITY_PATH_USER, Uninterruptible).unwrap())
                .chain(glob(GLOB_XAUTHORITY_PATH_SDDM, Uninterruptible).unwrap())
                .chain(glob(GLOB_XAUTHORITY_PATH_SDDM_USER, Uninterruptible).unwrap())
                .chain(glob(GLOB_XAUTHORITY_PATH_MUTTER_XWAYLAND_USER, Uninterruptible).unwrap())
                .chain(glob(GLOB_XAUTHORITY_PATH_ROOT, Uninterruptible).unwrap())
                .filter_map(Result::ok)
                .find(|path| path.is_absolute());
            if let Some(xauthority_path) = xauthority_path_opt {
                if let Some(xauthority_str) = xauthority_path.to_str() {
                    info!("Xauthority found in file path: {xauthority_str}");
                    return Some(xauthority_str.to_owned());
                }
            }
        }
        error!("Xauthority not found within {XAUTHORITY_SEARCH_TIMEOUT:?}.");
        None
    }

    /// Get the various GPU and Fan information from `nvidia-settings`.
    /// For most cases it seems that the display id doesn't really matter, as each id will
    /// give the same output. But that is not true for all systems. Some systems are sensitive
    /// to the display id, and will only give the proper output when using the correct one.
    /// See: [issue](https://gitlab.com/coolercontrol/coolercontrol/-/issues/104)
    /// Note: This implementation doesn't yet support multiple display servers with multiple display IDs.
    async fn get_nvidia_device_infos(
        &self,
    ) -> Result<HashMap<GpuIndex, (DisplayId, Vec<FanIndex>)>> {
        if self.xauthority_path.borrow().is_none() {
            error!(
                "Nvidia device detected but no xauthority cookie found which is needed \
            for proper communication with nvidia-settings. Nvidia Fan Control disabled."
            );
            return Ok(HashMap::new());
        }
        for display_id in 0..=3_u8 {
            let command = format!("nvidia-settings -c :{display_id} -q gpus --verbose");
            let xauthority_path = self.xauthority_path.borrow().clone().unwrap_or_default();
            let command_result = ShellCommand::new(&command, COMMAND_TIMEOUT_FIRST_TRY)
                .env("XAUTHORITY", &xauthority_path)
                .run()
                .await;
            match command_result {
                Success { stdout, stderr } => {
                    debug!("Nvidia gpu info output from display :{display_id} - {stdout}");
                    if stdout.is_empty() {
                        warn!(
                            "nvidia-settings returned no data for display :{display_id} - \
                            will retry on next display. Error output: {stderr}"
                        );
                        continue;
                    }
                    return Ok(Self::process_nv_setting_output(display_id, &stdout));
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

    /// Requests sensor data for Nvidia devices and maps the data to our internal model.
    async fn request_nvidia_smi_statuses(&self) -> Vec<StatusNvidiaDeviceSMI> {
        let mut statuses = vec![];
        let nvidia_statuses = self.get_nvidia_smi_status(COMMAND_TIMEOUT_DEFAULT).await;
        for nvidia_status in &nvidia_statuses {
            let mut temps = vec![];
            let mut channels = vec![];
            {
                let disabled_channels_lock = self.nvidia_smi_disabled_channels.borrow();
                // on first run this is empty:
                let disabled_channels = disabled_channels_lock
                    .get(&nvidia_status.index)
                    .map_or_else(|| Cow::Owned(Vec::new()), Cow::Borrowed);
                let temp_name = GPU_TEMP_NAME.to_string();
                if disabled_channels.contains(&temp_name).not() {
                    if let Some(temp) = nvidia_status.temp {
                        temps.push(TempStatus {
                            name: temp_name,
                            temp,
                        });
                    }
                }
                let load_name = GPU_LOAD_NAME.to_string();
                if disabled_channels.contains(&load_name).not() {
                    if let Some(load) = nvidia_status.load {
                        channels.push(ChannelStatus {
                            name: load_name,
                            duty: Some(f64::from(load)),
                            ..Default::default()
                        });
                    }
                }
                let fan_name = NVIDIA_FAN_NAME.to_string();
                if disabled_channels.contains(&fan_name).not() {
                    if let Some(fan_duty) = nvidia_status.fan_duty {
                        channels.push(ChannelStatus {
                            name: fan_name,
                            duty: Some(f64::from(fan_duty)),
                            ..Default::default()
                        });
                    }
                }
            }
            statuses.push(StatusNvidiaDeviceSMI {
                index: nvidia_status.index,
                name: nvidia_status.name.clone(),
                temps,
                channels,
            });
        }
        statuses
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

    /// Only attempts to retrieve Nvidia sensor values if Nvidia device was detected.
    pub async fn try_request_nv_smi_statuses(&self) -> Vec<StatusNvidiaDeviceSMI> {
        let mut statuses = vec![];
        if self.nvidia_devices.is_empty().not() {
            statuses.extend(self.request_nvidia_smi_statuses().await);
        }
        statuses
    }

    /// resets the nvidia fan control back to automatic
    async fn reset_nvidia_settings_to_default(&self, nvidia_info: &NvidiaDeviceInfo) -> Result<()> {
        if self.xauthority_path.borrow().is_none() {
            return Ok(()); // nvidia-settings won't work
        }
        let command = format!(
            "nvidia-settings -c :{0} -a \"[gpu:{1}]/GPUFanControlState=0\"",
            nvidia_info.display_id, nvidia_info.gpu_index
        );
        self.send_command_to_nvidia_settings(&command).await
    }

    async fn send_command_to_nvidia_settings(&self, command: &str) -> Result<()> {
        let xauthority_path = self.xauthority_path.borrow().clone().unwrap_or_default();
        let command_result = ShellCommand::new(command, COMMAND_TIMEOUT_DEFAULT)
            .env("XAUTHORITY", &xauthority_path)
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
                    let xauthority_path = Self::search_for_xauthority_path().await;
                    self.xauthority_path.replace(xauthority_path);
                }
                Err(anyhow!(
                    "Error communicating with nvidia-settings: {}",
                    stderr
                ))
            }
            Success { stdout, stderr } => {
                debug!("Nvidia-settings output: {stdout} - {stderr}");
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

    /// Sets the nvidia fan duty
    #[allow(clippy::format_push_string)]
    async fn set_nvidia_settings_fan_duty(
        &self,
        nvidia_info: &NvidiaDeviceInfo,
        fixed_speed: Duty,
    ) -> Result<()> {
        if self.xauthority_path.borrow().is_none() {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusNvidia {
    pub index: GpuIndex,
    pub name: String,
    pub temp: Option<f64>,
    pub load: Option<u8>,
    pub fan_duty: Option<u8>,
}

#[derive(Debug, Clone, Default)]
pub struct StatusNvidiaDeviceSMI {
    pub index: GpuIndex,
    pub name: String,
    pub channels: Vec<ChannelStatus>,
    pub temps: Vec<TempStatus>,
}

#[derive(Debug, Clone)]
pub struct StatusNvidiaDeviceNvml {
    pub channels: Vec<ChannelStatus>,
    pub temps: Vec<TempStatus>,
}

#[derive(Debug)]
pub struct NvidiaDeviceInfo {
    pub gpu_index: GpuIndex,
    pub display_id: DisplayId,
    pub fan_indices: Vec<FanIndex>,
    /// The names of the temperature sensors that have been successfully found (NVML)
    pub temps: Vec<String>,
    /// The Clock type sensors that have been successfully found (NVML)
    pub freqs: Vec<Clock>,
    /// Whether power is supported
    pub power: bool,
}
