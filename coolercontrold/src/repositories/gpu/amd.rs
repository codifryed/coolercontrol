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
use std::ops::{Not, RangeInclusive};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use heck::ToTitleCase;
use libdrm_amdgpu_sys::AMDGPU::{DeviceHandle, GPU_INFO};
use log::{error, info, trace, warn};
use regex::Regex;
use tokio::fs::OpenOptions;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelStatus, Device, DeviceInfo, DeviceType, Duty, SpeedOptions, Status,
    TempInfo, TempStatus, TypeIndex, UID,
};
use crate::repositories::gpu::gpu_repo::{GPU_FREQ_NAME, GPU_LOAD_NAME, GPU_TEMP_NAME};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::{devices, fans, freqs, temps};
use crate::repositories::repository::DeviceLock;

const AMD_HWMON_NAME: &str = "amdgpu";
const PATTERN_FAN_CURVE_POINT: &str = r"(?P<index>\d+):\s+(?P<temp>\d+)C\s+(?P<duty>\d+)%";
const PATTERN_FAN_CURVE_LIMITS_TEMP: &str =
    r"FAN_CURVE\(hotspot temp\):\s+(?P<temp_min>\d+)C\s+(?P<temp_max>\d+)C";
const PATTERN_FAN_CURVE_LIMITS_DUTY: &str =
    r"FAN_CURVE\(fan speed\):\s+(?P<duty_min>\d+)%\s+(?P<duty_max>\d+)%";
type CurveTemp = u8;

pub struct GpuAMD {
    config: Arc<Config>,
    amd_devices: HashMap<UID, DeviceLock>,
    pub amd_driver_infos: HashMap<UID, Arc<AMDDriverInfo>>,
    pub amd_preloaded_statuses: RwLock<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
}

impl GpuAMD {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            amd_devices: HashMap::new(),
            amd_driver_infos: HashMap::new(),
            amd_preloaded_statuses: RwLock::new(HashMap::new()),
        }
    }

    pub async fn init_devices(&self) -> Vec<AMDDriverInfo> {
        let base_paths = devices::find_all_hwmon_device_paths();
        let mut amd_infos = vec![];
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
            let device_path = path
                .join("device")
                .canonicalize()
                .unwrap_or_else(|_| path.join("device"));
            if let Some(load_channel) = Self::init_load(&device_path).await {
                channels.push(load_channel);
            }
            match freqs::init_freqs(&path).await {
                Ok(freqs) => channels.extend(freqs),
                Err(err) => error!("Error initializing AMD Hwmon Freqs: {}", err),
            };
            let fan_curve_info = Self::get_fan_curve_info(&device_path).await;
            let drm_device_name = Self::get_drm_device_name(&path).await;
            let pci_device_names = devices::get_device_pci_names(&path).await;
            let model = devices::get_device_model_name(&path)
                .await
                .or(drm_device_name)
                .or_else(|| pci_device_names.and_then(|names| names.device_name));
            let u_id = devices::get_device_unique_id(&path).await;
            let amd_driver_info = AMDDriverInfo {
                hwmon: HwmonDriverInfo {
                    name: device_name,
                    path,
                    model,
                    u_id,
                    channels,
                },
                device_path,
                fan_curve_info,
            };
            amd_infos.push(amd_driver_info);
        }
        amd_infos
    }

    async fn init_load(device_path: &PathBuf) -> Option<HwmonChannelInfo> {
        match tokio::fs::read_to_string(device_path.join("gpu_busy_percent")).await {
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
                    device_path
                );
                None
            }
        }
    }

    async fn get_drm_device_name(base_path: &Path) -> Option<String> {
        let slot_name = devices::get_pci_slot_name(base_path).await?;
        let path = format!("/dev/dri/by-path/pci-{slot_name}-render");
        let drm_file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
            .await
            .ok()?;
        let (handle, _, _) = DeviceHandle::init(drm_file.as_raw_fd()).ok()?;
        Some(handle.device_info().ok()?.find_device_name_or_default())
    }

    /// Gets the PMFW (power management firmware) fan curve information.
    /// Note: if the device is in auto mode or no custom curve is used,
    /// all the curve points may be set to 0.
    ///
    /// Only available on Navi3x (RDNA 3) or newer devices.
    async fn get_fan_curve_info(device_path: &Path) -> Option<FanCurveInfo> {
        let path = device_path.join("gpu_od/fan_ctrl/fan_curve");
        let fan_curve_file = tokio::fs::read_to_string(&path).await.ok()?;
        let mut points = Vec::new();
        let mut temp_min: CurveTemp = 0;
        let mut temp_max: CurveTemp = 0;
        let mut duty_min: Duty = 0;
        let mut duty_max: Duty = 0;
        let regex_fan_point = Regex::new(PATTERN_FAN_CURVE_POINT).unwrap();
        let regex_fan_limits_temp = Regex::new(PATTERN_FAN_CURVE_LIMITS_TEMP).unwrap();
        let regex_fan_limits_duty = Regex::new(PATTERN_FAN_CURVE_LIMITS_DUTY).unwrap();
        for line in fan_curve_file.lines() {
            if let Some(fan_point_cap) = regex_fan_point.captures(line) {
                // let index: u8 = fan_point_cap.name("index").unwrap().as_str().parse().ok()?;
                let temp: CurveTemp = fan_point_cap.name("temp").unwrap().as_str().parse().ok()?;
                let duty: Duty = fan_point_cap.name("duty").unwrap().as_str().parse().ok()?;
                points.push((temp, duty));
            } else if let Some(fan_limits_temp_cap) = regex_fan_limits_temp.captures(line) {
                temp_min = fan_limits_temp_cap
                    .name("temp_min")
                    .unwrap()
                    .as_str()
                    .parse()
                    .ok()?;
                temp_max = fan_limits_temp_cap
                    .name("temp_max")
                    .unwrap()
                    .as_str()
                    .parse()
                    .ok()?;
            } else if let Some(fan_limits_duty_cap) = regex_fan_limits_duty.captures(line) {
                duty_min = fan_limits_duty_cap
                    .name("duty_min")
                    .unwrap()
                    .as_str()
                    .parse()
                    .ok()?;
                duty_max = fan_limits_duty_cap
                    .name("duty_max")
                    .unwrap()
                    .as_str()
                    .parse()
                    .ok()?;
            }
        }
        let changeable = temp_max > 0 && duty_max > 0;
        if changeable.not() {
            warn!(
                "AMD Fan Curve found but not controllable. \
                        You may need to enable this feature with the kernel boot option: \
                        amdgpu.ppfeaturemask=0xffffffff"
            )
        }
        info!("AMD GPU RDNA 3 Fan Control limitations - Fan in 0rpm mode until 50/60C and Min Fan Duty: {duty_min}%");
        Some(FanCurveInfo {
            fan_curve: FanCurve { points },
            changeable,
            temperature_range: temp_min..=temp_max,
            speed_range: duty_min..=duty_max,
            path,
        })
    }

    pub async fn initialize_amd_devices(&mut self) -> Result<HashMap<UID, DeviceLock>> {
        let mut devices = HashMap::new();
        for (index, amd_driver) in self.init_devices().await.into_iter().enumerate() {
            let id = index as u8 + 1;
            let mut channels = HashMap::new();
            let (min_duty, max_duty) = Self::get_min_max_duty(&amd_driver.fan_curve_info);
            for channel in &amd_driver.hwmon.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Fan => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            speed_options: Some(SpeedOptions {
                                profiles_enabled: false,
                                fixed_enabled: true,
                                manual_profiles_enabled: true,
                                min_duty,
                                max_duty,
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
                .hwmon
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
            let (temp_min, temp_max) = Self::get_min_max_temps(&amd_driver.fan_curve_info);
            let mut device = Device::new(
                amd_driver.hwmon.name.clone(),
                DeviceType::GPU,
                id,
                None,
                DeviceInfo {
                    temps,
                    channels,
                    temp_min,
                    temp_max,
                    model: amd_driver.hwmon.model.clone(),
                    ..Default::default()
                },
                Some(amd_driver.hwmon.u_id.clone()),
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
            self.amd_driver_infos
                .insert(device.uid.clone(), Arc::new(amd_driver.clone()));
            devices.insert(device.uid.clone(), Arc::new(RwLock::new(device)));
        }
        if log::max_level() >= log::LevelFilter::Debug {
            info!("Initialized AMD HwmonInfos: {:?}", self.amd_driver_infos);
        }
        self.amd_devices.clone_from(&devices);
        Ok(devices)
    }

    fn get_min_max_duty(fan_curve_info: &Option<FanCurveInfo>) -> (Duty, Duty) {
        if let Some(fan_curve_info) = fan_curve_info {
            (
                fan_curve_info.speed_range.start().to_owned(),
                fan_curve_info.speed_range.end().to_owned(),
            )
        } else {
            (0, 100) // Standard Defaults
        }
    }

    fn get_min_max_temps(fan_curve_info: &Option<FanCurveInfo>) -> (CurveTemp, CurveTemp) {
        if let Some(fan_curve_info) = fan_curve_info {
            (
                fan_curve_info.temperature_range.start().to_owned(),
                fan_curve_info.temperature_range.end().to_owned(),
            )
        } else {
            (0, 100) // Standard Defaults
        }
    }

    pub async fn get_amd_status(
        &self,
        amd_driver: &AMDDriverInfo,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = fans::extract_fan_statuses(&amd_driver.hwmon).await;
        status_channels.extend(Self::extract_load_status(amd_driver).await);
        status_channels.extend(freqs::extract_freq_statuses(&amd_driver.hwmon).await);
        let temps = temps::extract_temp_statuses(&amd_driver.hwmon)
            .await
            .iter()
            .map(|temp| TempStatus {
                name: temp.name.clone(),
                temp: temp.temp,
            })
            .collect();
        (status_channels, temps)
    }

    async fn extract_load_status(driver: &AMDDriverInfo) -> Vec<ChannelStatus> {
        let mut channels = vec![];
        for channel in &driver.hwmon.channels {
            if channel.hwmon_type != HwmonChannelType::Load {
                continue;
            }
            let load = tokio::fs::read_to_string(driver.device_path.join("gpu_busy_percent"))
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

    pub async fn update_all_statuses(&self) {
        for (uid, amd_driver) in &self.amd_driver_infos {
            if let Some(device_lock) = self.amd_devices.get(uid) {
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
                trace!(
                    "Device: {} status updated: {:?}",
                    amd_driver.hwmon.name,
                    status
                );
                device_lock.write().await.set_status(status);
            }
        }
    }

    pub async fn reset_devices(&self) {
        for (uid, device_lock) in &self.amd_devices {
            for channel_name in device_lock.read().await.info.channels.keys() {
                self.reset_amd_to_default(uid, channel_name).await.ok();
            }
        }
    }

    pub async fn reset_amd_to_default(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let amd_hwmon_info = self
            .amd_driver_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        match &amd_hwmon_info.fan_curve_info {
            Some(fan_curve_info) => {
                if fan_curve_info.changeable {
                    Self::reset_fan_curve(fan_curve_info).await
                } else {
                    Err(anyhow!(
                        "PMFW Fan Curve control is present for this device, but not enabled"
                    ))
                }
            }
            None => {
                let channel_info = amd_hwmon_info
                    .hwmon
                    .channels
                    .iter()
                    .find(|channel| {
                        channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
                    })
                    .with_context(|| format!("Searching for channel name: {channel_name}"))?;
                fans::set_pwm_enable_to_default(&amd_hwmon_info.hwmon.path, channel_info).await
            }
        }
    }

    async fn reset_fan_curve(fan_curve_info: &FanCurveInfo) -> Result<()> {
        tokio::fs::write(&fan_curve_info.path, b"r\n")
            .await
            .with_context(|| "Resetting Fan Curve file to automatic mode")
    }

    pub async fn set_amd_duty(
        &self,
        device_uid: &UID,
        channel_name: &str,
        fixed_speed: Duty,
    ) -> Result<()> {
        let amd_driver_info = self
            .amd_driver_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        match &amd_driver_info.fan_curve_info {
            Some(fan_curve_info) => {
                if fan_curve_info.changeable {
                    Self::set_fan_curve_duty(fan_curve_info, fixed_speed).await
                } else {
                    Err(anyhow!(
                        "PMFW Fan Curve control is present for this device, but not enabled"
                    ))
                }
            }
            None => {
                let channel_info = amd_driver_info
                    .hwmon
                    .channels
                    .iter()
                    .find(|channel| {
                        channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
                    })
                    .with_context(|| "Searching for channel name")?;
                fans::set_pwm_duty(&amd_driver_info.hwmon.path, channel_info, fixed_speed).await
            }
        }
    }

    async fn set_fan_curve_duty(fan_curve_info: &FanCurveInfo, duty: Duty) -> Result<()> {
        let flat_curve = Self::create_flat_curve(fan_curve_info, duty);
        for (i, (temp, duty)) in flat_curve.points.into_iter().enumerate() {
            tokio::fs::write(&fan_curve_info.path, format!("{i} {temp} {duty}\n")).await?;
        }
        tokio::fs::write(&fan_curve_info.path, b"c\n")
            .await
            .with_context(|| "Committing Fan Curve changes")
    }

    /// Creates a "flat" fan curve by setting the duty with the temp_min and all the rest of
    /// the points set to temp_max. This allows CoolerControl to handle Profiles and Functions
    /// natively, which the firmware cannot do.
    fn create_flat_curve(fan_curve_info: &FanCurveInfo, duty: Duty) -> FanCurve {
        let clamped_duty = if fan_curve_info.speed_range.contains(&duty) {
            duty
        } else {
            warn!(
                "AMD GPU RDNA 3 - Only fan duties within range of {}% to {}% are allowed. \
                Clamping passed duty of {duty}% to nearest limit",
                fan_curve_info.speed_range.start(),
                fan_curve_info.speed_range.end(),
            );
            fan_curve_info
                .speed_range
                .end()
                .min(fan_curve_info.speed_range.start().max(&duty))
                .to_owned()
        };
        let mut new_fan_curve = FanCurve::default();
        let mut temp_steps = vec![fan_curve_info.temperature_range.start().to_owned()];
        for _ in 1..fan_curve_info.fan_curve.points.len() {
            temp_steps.push(fan_curve_info.temperature_range.end().to_owned())
        }
        for temp_step in temp_steps {
            new_fan_curve.points.push((temp_step, clamped_duty));
        }
        new_fan_curve
    }
}

#[derive(Debug, Clone)]
pub struct AMDDriverInfo {
    pub hwmon: HwmonDriverInfo,
    device_path: PathBuf,
    fan_curve_info: Option<FanCurveInfo>,
}

/// The PMFW (power management firmware) fan curve information.
/// Only available on Navi3x (RDNA 3) or newer devices.
#[derive(Debug, Clone)]
struct FanCurveInfo {
    /// Fan curve points
    fan_curve: FanCurve,
    /// Whether the fan curve is changeable or not. Determined by the present of the ranges below.
    changeable: bool,
    /// Temperature range allowed in curve points
    temperature_range: RangeInclusive<CurveTemp>,
    /// Fan speed range allowed in curve points
    speed_range: RangeInclusive<Duty>,
    /// The path to the gpu fan curve file
    path: PathBuf,
}

impl Default for FanCurveInfo {
    fn default() -> Self {
        Self {
            fan_curve: FanCurve::default(),
            changeable: false,
            temperature_range: RangeInclusive::new(0, 0),
            speed_range: RangeInclusive::new(0, 0),
            path: PathBuf::default(),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct FanCurve {
    /// Fan curve points in the (temperature, speed) format
    points: Vec<(CurveTemp, Duty)>,
}
