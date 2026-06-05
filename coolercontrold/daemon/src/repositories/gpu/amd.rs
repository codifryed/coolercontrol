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

use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::{Not, RangeInclusive};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::cc_fs;
use crate::config::Config;
use crate::device::{
    ChannelExtensionNames, ChannelInfo, ChannelKind, ChannelStatus, Device, DeviceInfo, DeviceType,
    DriverInfo, DriverType, Duty, SpeedOptions, Status, Temp, TempInfo, TempStatus, TypeIndex, UID,
};
use crate::repositories::gpu::gpu_repo::{
    GPU_FREQ_NAME, GPU_LOAD_NAME, GPU_POWER_NAME, GPU_TEMP_NAME, READ_PERMIT_RATIO,
};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::{devices, fans, freqs, power, temps};
use crate::repositories::repository::DeviceLock;
use anyhow::{anyhow, Context, Result};
use heck::ToTitleCase;
use libdrm_amdgpu_sys::LibDrmAmdgpu;
use libdrm_amdgpu_sys::AMDGPU::SENSOR_INFO::SENSOR_TYPE;
use libdrm_amdgpu_sys::AMDGPU::{AmdgpuDeviceHandle, GPU_INFO};
use log::{debug, error, info, trace, warn};
use regex::Regex;

pub const TEMP_FOR_FAN_CURVE: &str = "temp1";
const AMD_HWMON_NAME: &str = "amdgpu";
const PATTERN_FAN_CURVE_POINT: &str = r"(?P<index>\d+):\s+(?P<temp>\d+)C\s+(?P<duty>\d+)%";
const PATTERN_FAN_CURVE_LIMITS_TEMP: &str =
    r"FAN_CURVE\(hotspot temp\):\s+(?P<temp_min>\d+)C\s+(?P<temp_max>\d+)C";
const PATTERN_FAN_CURVE_LIMITS_DUTY: &str =
    r"FAN_CURVE\(fan speed\):\s+(?P<duty_min>\d+)%\s+(?P<duty_max>\d+)%";
const PATTERN_ZERO_RPM_STOP_LIMITS_TEMP: &str =
    r"ZERO_RPM_STOP_TEMPERATURE:\s+(?P<temp_min>\d+)\s+(?P<temp_max>\d+)";
type CurveTemp = u8;

/// Wall-clock budget for one libdrm `sensor_info` ioctl, expressed as a ratio
/// of the poll rate like `gpu_repo`'s other read timeouts. `gpu_repo` lacks the
/// per-channel device-delay resilience hwmon has, so the budget stays near a
/// single poll interval rather than hwmon's multi-interval ioctl budget. Reuses
/// `gpu_repo::READ_PERMIT_RATIO` so both share one poll-rate budgeting source.
/// The ioctl is offloaded and only nears this ceiling when the GPU is wedged
/// (powering down/up or saturated); a healthy read returns in microseconds.
fn drm_ioctl_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * READ_PERMIT_RATIO)
}

pub struct GpuAMD {
    config: Rc<Config>,
    /// Per-ioctl timeout for libdrm sensor reads. Frozen at construction since
    /// `poll_rate` only changes on restart.
    drm_ioctl_timeout: Duration,
    pub amd_devices: HashMap<UID, DeviceLock>,
    pub amd_driver_infos: HashMap<UID, Rc<AMDDriverInfo>>,
    pub amd_preloaded_statuses: RefCell<HashMap<TypeIndex, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
}

impl GpuAMD {
    pub fn new(config: Rc<Config>) -> Self {
        let poll_rate = config
            .get_settings()
            .map_or(1.0, |settings| settings.poll_rate);
        Self {
            config,
            drm_ioctl_timeout: drm_ioctl_timeout_for(poll_rate),
            amd_devices: HashMap::new(),
            amd_driver_infos: HashMap::new(),
            amd_preloaded_statuses: RefCell::new(HashMap::new()),
        }
    }

    #[allow(clippy::too_many_lines)]
    pub async fn init_devices(&self) -> Vec<AMDDriverInfo> {
        let base_paths = devices::find_all_hwmon_device_paths();
        let mut amd_infos = vec![];
        for path in base_paths {
            let device_name = devices::get_device_name(&path).await;
            if device_name != AMD_HWMON_NAME {
                continue;
            }
            let u_id = devices::get_device_unique_id(&path, &device_name).await;
            let device_uid = Device::create_uid_from(&device_name, DeviceType::GPU, 0, Some(&u_id));
            let cc_device_setting = self
                .config
                .get_cc_settings_for_device(&device_uid)
                .unwrap_or(None);
            if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
                info!("Skipping disabled device: {device_name} with UID: {device_uid}");
                continue;
            }
            let disabled_channels =
                cc_device_setting.map_or_else(Vec::new, |setting| setting.get_disabled_channels());
            let mut channels = vec![];
            match fans::init_fans(&path, &device_name).await {
                Ok(fans) => channels.extend(
                    fans.into_iter()
                        .filter(|fan| disabled_channels.contains(&fan.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing AMD Hwmon Fans: {err}"),
            }
            match temps::init_temps(&path, &device_name).await {
                Ok(temps) => channels.extend(
                    temps
                        .into_iter()
                        .filter(|temp| disabled_channels.contains(&temp.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing AMD Hwmon Temps: {err}"),
            }
            let device_path = path
                .join("device")
                .canonicalize()
                .unwrap_or_else(|_| path.join("device"));
            if let Some(load_channel) = Self::init_load(&device_path).await {
                if disabled_channels.contains(&load_channel.name).not() {
                    channels.push(load_channel);
                }
            }
            match freqs::init_freqs(&path).await {
                Ok(freqs) => channels.extend(
                    freqs
                        .into_iter()
                        .filter(|freq| disabled_channels.contains(&freq.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing AMD Hwmon Freqs: {err}"),
            }
            match power::init_power(&path).await {
                Ok(power) => channels.extend(
                    power
                        .into_iter()
                        .filter(|power| disabled_channels.contains(&power.name).not())
                        .collect::<Vec<HwmonChannelInfo>>(),
                ),
                Err(err) => error!("Error initializing AMD Hwmon Power: {err}"),
            }
            let fan_curve_info = Self::get_fan_curve_info(&device_path)
                .await
                .inspect_err(|err| {
                    debug!("Could not get RDNA3/4 fan curve info: {err}");
                })
                .ok();
            let has_rdna_fan_ctrl = device_path.join("gpu_od/fan_ctrl").is_dir();
            let overdrive_enabled = super::amd_overdrive::is_overdrive_enabled().await;
            let drm_device_name = Self::get_drm_device_name(&path).await;
            let pci_device_names = devices::get_device_pci_names(&path).await;
            let model = devices::get_device_model_name(&path)
                .await
                .or(drm_device_name)
                .or_else(|| pci_device_names.and_then(|names| names.device_name));
            let drm = Self::init_drm_metrics(
                &path,
                &channels,
                &disabled_channels,
                self.drm_ioctl_timeout,
            )
            .await;
            let amd_driver_info = AMDDriverInfo {
                hwmon: HwmonDriverInfo {
                    name: device_name,
                    path,
                    model,
                    u_id,
                    channels,
                    ..Default::default()
                },
                device_path,
                fan_curve_info,
                has_rdna_fan_ctrl,
                overdrive_enabled,
                drm,
            };
            amd_infos.push(amd_driver_info);
        }
        amd_infos
    }

    async fn init_load(device_path: &Path) -> Option<HwmonChannelInfo> {
        let load_path = device_path.join("gpu_busy_percent");
        match cc_fs::read_sysfs(&load_path).await {
            Ok(load) => match fans::check_parsing_8(load) {
                Ok(_) => Some(HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Load,
                    name: GPU_LOAD_NAME.to_string(),
                    label: Some(GPU_LOAD_NAME.to_string()),
                    ..Default::default()
                }),
                Err(err) => {
                    warn!(
                        "Error parsing AMD GPU load from {}: {err}",
                        load_path.display()
                    );
                    None
                }
            },
            Err(err) => {
                // The file may be absent or present-but-unreadable on older ASICs.
                // The libdrm fallback covers this when available, so this is expected.
                debug!(
                    "AMD GPU load unavailable from hwmon at {}. Will try with libdrm. {err}",
                    load_path.display()
                );
                None
            }
        }
    }

    /// Opens the amdgpu DRM render node for the device and returns an owned
    /// handle. The handle keeps its file descriptor alive for its lifetime, so
    /// it is safe to retain for repeated sensor queries. Returns None when
    /// libdrm is not present at runtime or the device has no render node.
    async fn open_drm_handle(base_path: &Path) -> Option<AmdgpuDeviceHandle> {
        let drm_amdgpu = LibDrmAmdgpu::new().ok()?;
        let slot_name = devices::get_pci_slot_name(base_path).await?;
        let path = format!("/dev/dri/by-path/pci-{slot_name}-render");
        let drm_file = cc_fs::open_options()
            .read(true)
            .write(true)
            .open(&path)
            .ok()?;
        let (handle, _, _) = drm_amdgpu.init_amdgpu_device_handle(drm_file).ok()?;
        Some(handle)
    }

    async fn get_drm_device_name(base_path: &Path) -> Option<String> {
        let handle = Self::open_drm_handle(base_path).await?;
        Some(handle.device_info().ok()?.find_device_name_or_default())
    }

    /// Gets the PMFW (power management firmware) fan curve information.
    /// Note: if the device is in auto mode or no custom curve is used,
    /// all the curve points may be set to 0.
    /// See: <https://docs.kernel.org/gpu/amdgpu/thermal.html#fan-curve>
    ///
    /// Only available on Navi3x (RDNA 3) or newer devices.
    async fn get_fan_curve_info(device_path: &Path) -> Result<FanCurveInfo> {
        let (path, fan_curve, temperature_range, speed_range) =
            Self::get_fan_curve_with_ranges(device_path).await?;
        let changeable = super::amd_overdrive::is_overdrive_enabled().await;
        if changeable.not() {
            let fan_control_boot_option = super::amd_overdrive::get_fan_control_boot_option().await;
            warn!(
                "AMD RDNA3/4 fan curve detected but overdrive is not enabled. \
                Enable it in the device's Advanced Settings \
                or add kernel boot parameter: {fan_control_boot_option}"
            );
        }

        let zero_rpm_enable_path = device_path.join("gpu_od/fan_ctrl/fan_zero_rpm_enable");
        let zero_rpm = cc_fs::read_txt(&zero_rpm_enable_path)
            .await
            .ok()
            .map(|_| zero_rpm_enable_path);
        if zero_rpm.is_none() {
            info!(
                "AMD GPU RDNA 3/4 Fan Control limitations: Fan will use Zero RPM Mode until 50/60C"
            );
        }

        let (zero_rpm_stop_temp, zero_rpm_stop_temp_range) =
            Self::get_zero_rpm_stop_temp_with_range(device_path).await?;
        Ok(FanCurveInfo {
            fan_curve,
            changeable,
            temperature_range,
            speed_range,
            path,
            zero_rpm,
            zero_rpm_stop_temp,
            zero_rpm_stop_temp_range,
        })
    }

    async fn get_fan_curve_with_ranges(
        device_path: &Path,
    ) -> Result<(
        PathBuf,
        FanCurve,
        RangeInclusive<CurveTemp>,
        RangeInclusive<Duty>,
    )> {
        let path = device_path.join("gpu_od/fan_ctrl/fan_curve");
        let fan_curve_file = cc_fs::read_txt(&path).await?;
        let mut points = Vec::new();
        let mut temp_min: CurveTemp = 0;
        let mut temp_max: CurveTemp = 0;
        let mut duty_min: Duty = 0;
        let mut duty_max: Duty = 0;
        let regex_fan_point = Regex::new(PATTERN_FAN_CURVE_POINT)?;
        let regex_fan_limits_temp = Regex::new(PATTERN_FAN_CURVE_LIMITS_TEMP)?;
        let regex_fan_limits_duty = Regex::new(PATTERN_FAN_CURVE_LIMITS_DUTY)?;
        for line in fan_curve_file.lines() {
            if let Some(fan_point_cap) = regex_fan_point.captures(line) {
                // let index: u8 = fan_point_cap.name("index").unwrap().as_str().parse().ok()?;
                let temp: CurveTemp = fan_point_cap.name("temp").unwrap().as_str().parse()?;
                let duty: Duty = fan_point_cap.name("duty").unwrap().as_str().parse()?;
                points.push((temp, duty));
            } else if let Some(fan_limits_temp_cap) = regex_fan_limits_temp.captures(line) {
                temp_min = fan_limits_temp_cap
                    .name("temp_min")
                    .unwrap()
                    .as_str()
                    .parse()?;
                temp_max = fan_limits_temp_cap
                    .name("temp_max")
                    .unwrap()
                    .as_str()
                    .parse()?;
            } else if let Some(fan_limits_duty_cap) = regex_fan_limits_duty.captures(line) {
                duty_min = fan_limits_duty_cap
                    .name("duty_min")
                    .unwrap()
                    .as_str()
                    .parse()?;
                duty_max = fan_limits_duty_cap
                    .name("duty_max")
                    .unwrap()
                    .as_str()
                    .parse()?;
            }
        }
        let fan_curve = FanCurve { points };
        let temperature_range = temp_min..=temp_max;
        let speed_range = duty_min..=duty_max;
        Ok((path, fan_curve, temperature_range, speed_range))
    }

    async fn get_zero_rpm_stop_temp_with_range(
        device_path: &Path,
    ) -> Result<(Option<PathBuf>, RangeInclusive<CurveTemp>)> {
        let mut zero_rpm_stop_temp_min: CurveTemp = 0;
        let mut zero_rpm_stop_temp_max: CurveTemp = 0;
        let zero_rpm_stop_temp_path =
            device_path.join("gpu_od/fan_ctrl/fan_zero_rpm_stop_temperature");
        let zero_rpm_stop_temp = if let Ok(zero_rpm_stop_temp_content) =
            cc_fs::read_txt(&zero_rpm_stop_temp_path).await
        {
            let regex_zero_rpm_stop_limits_temp = Regex::new(PATTERN_ZERO_RPM_STOP_LIMITS_TEMP)?;
            for line in zero_rpm_stop_temp_content.lines() {
                if let Some(stop_limits_temp_cap) = regex_zero_rpm_stop_limits_temp.captures(line) {
                    zero_rpm_stop_temp_min = stop_limits_temp_cap
                        .name("temp_min")
                        .unwrap()
                        .as_str()
                        .parse()?;
                    zero_rpm_stop_temp_max = stop_limits_temp_cap
                        .name("temp_max")
                        .unwrap()
                        .as_str()
                        .parse()?;
                }
            }
            Some(zero_rpm_stop_temp_path)
        } else {
            None
        };
        let zero_rpm_stop_temp_range = zero_rpm_stop_temp_min..=zero_rpm_stop_temp_max;
        Ok((zero_rpm_stop_temp, zero_rpm_stop_temp_range))
    }

    #[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
    pub async fn initialize_amd_devices(&mut self) -> Result<HashMap<UID, DeviceLock>> {
        let mut devices = HashMap::new();
        let poll_rate = self.config.get_settings()?.poll_rate;
        for (index, amd_driver) in self.init_devices().await.into_iter().enumerate() {
            let id = index as u8 + 1;
            let mut channels = HashMap::new();
            let (min_duty, max_duty) = Self::get_min_max_duty(amd_driver.fan_curve_info.as_ref());
            let fan_is_controllable = Self::get_fan_is_controllable(&amd_driver);
            let supports_internal_profiles = amd_driver
                .fan_curve_info
                .as_ref()
                .is_some_and(|fc| fc.changeable);
            for channel in amd_driver.all_channel_infos() {
                match channel.hwmon_type {
                    HwmonChannelType::Fan => {
                        let extension = if supports_internal_profiles {
                            Some(ChannelExtensionNames::AmdRdnaGpu)
                        } else {
                            None
                        };
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            kind: ChannelKind::Speed(SpeedOptions {
                                extension,
                                fixed_enabled: fan_is_controllable,
                                min_duty,
                                max_duty,
                            }),
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Load => {
                        let channel_info = ChannelInfo {
                            label: channel.label.clone(),
                            kind: ChannelKind::InfoOnly,
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Freq => {
                        let label_base = channel
                            .label
                            .as_ref()
                            .map_or_else(|| channel.name.to_title_case(), |l| l.to_title_case());
                        let channel_info = ChannelInfo {
                            label: Some(format!("{GPU_FREQ_NAME} {label_base}")),
                            kind: ChannelKind::InfoOnly,
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Power => {
                        let label_ext = channel
                            .label
                            .as_ref()
                            .map(|l| format!(" {l}"))
                            .unwrap_or_default();
                        let channel_info = ChannelInfo {
                            label: Some(format!("{GPU_POWER_NAME}{label_ext}")),
                            kind: ChannelKind::InfoOnly,
                        };
                        channels.insert(channel.name.clone(), channel_info);
                    }
                    HwmonChannelType::Temp | HwmonChannelType::PowerCap => (),
                }
            }
            let amd_status = self.get_amd_status(&amd_driver).await;
            self.amd_preloaded_statuses
                .borrow_mut()
                .insert(id, amd_status.clone());
            let temps = amd_driver
                .all_channel_infos()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    let label_base = channel
                        .label
                        .as_ref()
                        .map_or_else(|| channel.name.to_title_case(), |l| l.to_title_case());
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: format!("{GPU_TEMP_NAME} {label_base}"),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let (temp_min, temp_max) = Self::get_min_max_temps(amd_driver.fan_curve_info.as_ref());
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
                    profile_max_length: amd_driver
                        .fan_curve_info
                        .as_ref()
                        .map_or(17, |fc| fc.fan_curve.points.len() as u8),
                    model: amd_driver.hwmon.model.clone(),
                    amd_gpu_overdrive: if amd_driver.has_rdna_fan_ctrl {
                        Some(amd_driver.overdrive_enabled)
                    } else {
                        None
                    },
                    driver_info: DriverInfo {
                        drv_type: DriverType::Kernel,
                        name: devices::get_device_driver_name(&amd_driver.hwmon.path).await,
                        version: sysinfo::System::kernel_version(),
                        locations: Self::get_driver_locations(&amd_driver.hwmon.path).await,
                    },
                    ..Default::default()
                },
                Some(amd_driver.hwmon.u_id.clone()),
                poll_rate,
            );
            let status = Status {
                channels: amd_status.0,
                temps: amd_status.1,
                ..Default::default()
            };
            device.initialize_status_history_with(status, poll_rate);
            self.amd_driver_infos
                .insert(device.uid.clone(), Rc::new(amd_driver.clone()));
            devices.insert(device.uid.clone(), Rc::new(RefCell::new(device)));
        }
        if log::max_level() >= log::LevelFilter::Debug {
            info!("Initialized AMD HwmonInfos: {:?}", self.amd_driver_infos);
        }
        self.amd_devices.clone_from(&devices);
        Ok(devices)
    }

    fn get_min_max_duty(fan_curve_info: Option<&FanCurveInfo>) -> (Duty, Duty) {
        if let Some(fan_curve_info) = fan_curve_info {
            if fan_curve_info.zero_rpm.is_none() {
                // otherwise we have full range and can use the standard defaults
                return (
                    fan_curve_info.speed_range.start().to_owned(),
                    fan_curve_info.speed_range.end().to_owned(),
                );
            }
        }
        (0, 100) // Standard Defaults
    }

    fn get_min_max_temps(fan_curve_info: Option<&FanCurveInfo>) -> (CurveTemp, CurveTemp) {
        if let Some(fan_curve_info) = fan_curve_info {
            (
                fan_curve_info.temperature_range.start().to_owned(),
                fan_curve_info.temperature_range.end().to_owned(),
            )
        } else {
            (0, 150) // Standard Defaults
        }
    }

    /// If `FanCurve` is present, we check if fan control is enabled, otherwise it must use
    /// the standard pwm sysfs interface (pre-RDNA3).
    fn get_fan_is_controllable(amd_driver: &AMDDriverInfo) -> bool {
        if let Some(fan_curve_info) = &amd_driver.fan_curve_info {
            // RDNA3/4 with parsed fan curve: controllable only if overdrive enabled
            fan_curve_info.changeable
        } else if amd_driver.has_rdna_fan_ctrl {
            // RDNA3/4 but fan_curve_info failed to parse: check overdrive directly
            amd_driver.overdrive_enabled
        } else {
            // Pre-RDNA3: standard hwmon controls, always controllable
            true
        }
    }

    async fn get_driver_locations(base_path: &Path) -> Vec<String> {
        let hwmon_path = base_path.to_str().unwrap_or_default().to_owned();
        let device_path = devices::get_static_device_path_str(base_path);
        let mut locations = vec![hwmon_path, device_path.unwrap_or_default()];
        if let Some(mod_alias) = devices::get_device_mod_alias(base_path).await {
            locations.push(mod_alias);
        }
        locations
    }

    pub async fn get_amd_status(
        &self,
        amd_driver: &AMDDriverInfo,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let (mut status_channels, _) = fans::extract_fan_statuses(&amd_driver.hwmon).await;
        status_channels.extend(Self::extract_load_status(amd_driver).await);
        status_channels.extend(freqs::extract_freq_statuses(&amd_driver.hwmon).await);
        let (power_statuses, _) = power::extract_power_status(&amd_driver.hwmon).await;
        status_channels.extend(power_statuses);
        let (extracted_temps, _) = temps::extract_temp_statuses(&amd_driver.hwmon).await;
        let mut temps: Vec<TempStatus> = extracted_temps
            .iter()
            .map(|temp| TempStatus {
                name: temp.name.clone(),
                temp: temp.temp,
            })
            .collect();
        if let Some(drm) = &amd_driver.drm {
            let (drm_channels, drm_temps) =
                Self::extract_drm_statuses(drm, self.drm_ioctl_timeout).await;
            status_channels.extend(drm_channels);
            temps.extend(drm_temps);
        }
        (status_channels, temps)
    }

    async fn extract_load_status(driver: &AMDDriverInfo) -> Vec<ChannelStatus> {
        let load_channel_count = driver
            .hwmon
            .channels
            .iter()
            .filter(|c| c.hwmon_type == HwmonChannelType::Load)
            .count();
        let mut channels = Vec::with_capacity(load_channel_count);
        for channel in &driver.hwmon.channels {
            if channel.hwmon_type != HwmonChannelType::Load {
                continue;
            }
            let result = cc_fs::read_sysfs(driver.device_path.join("gpu_busy_percent"))
                .await
                .and_then(fans::check_parsing_8);
            if let Ok(load) = result {
                channels.push(ChannelStatus {
                    name: channel.name.clone(),
                    duty: Some(f64::from(load)),
                    ..Default::default()
                });
            }
        }
        channels
    }

    /// Probes libdrm for the metric types the hwmon driver does not expose and
    /// returns a `DrmMetrics` fallback for them. Returns None when libdrm is
    /// unavailable at runtime, every type is already covered by hwmon, or no
    /// candidate sensor reads successfully on this device.
    async fn init_drm_metrics(
        base_path: &Path,
        hwmon_channels: &[HwmonChannelInfo],
        disabled_channels: &[String],
        ioctl_timeout: Duration,
    ) -> Option<DrmMetrics> {
        let candidates = Self::drm_fallback_candidates(hwmon_channels);
        if candidates.is_empty() {
            return None;
        }
        let handle = Arc::new(Self::open_drm_handle(base_path).await?);
        let mut channels = Vec::with_capacity(candidates.len());
        for (info, sensor) in candidates {
            if disabled_channels.contains(&info.name) {
                continue;
            }
            // Only keep metrics the device actually answers for.
            if Self::read_drm_sensor(Arc::clone(&handle), sensor, ioctl_timeout)
                .await
                .is_none()
            {
                continue;
            }
            channels.push(DrmChannel { info, sensor });
        }
        if channels.is_empty() {
            return None;
        }
        let names: Vec<&str> = channels.iter().map(|c| c.info.name.as_str()).collect();
        info!(
            "AMD GPU libdrm fallback active for hwmon-missing metrics: {}",
            names.join(", ")
        );
        Some(DrmMetrics { handle, channels })
    }

    /// Builds the candidate libdrm channels for each metric type absent from
    /// the hwmon channel set. Pure (no I/O) so the missing-type logic and the
    /// channel identities are unit-testable.
    fn drm_fallback_candidates(
        hwmon_channels: &[HwmonChannelInfo],
    ) -> Vec<(HwmonChannelInfo, SENSOR_TYPE)> {
        let has_type = |hwmon_type: HwmonChannelType| {
            hwmon_channels.iter().any(|c| c.hwmon_type == hwmon_type)
        };
        let mut candidates = Vec::new();
        if has_type(HwmonChannelType::Load).not() {
            candidates.push((
                Self::drm_channel_info(
                    HwmonChannelType::Load,
                    GPU_LOAD_NAME,
                    Some(GPU_LOAD_NAME),
                    0,
                ),
                SENSOR_TYPE::GPU_LOAD,
            ));
        }
        if has_type(HwmonChannelType::Temp).not() {
            // Named `temp1` so it slots in like a hwmon edge temp, including
            // eligibility as the fan-curve source (`TEMP_FOR_FAN_CURVE`).
            candidates.push((
                Self::drm_channel_info(HwmonChannelType::Temp, TEMP_FOR_FAN_CURVE, Some("edge"), 1),
                SENSOR_TYPE::GPU_TEMP,
            ));
        }
        if has_type(HwmonChannelType::Freq).not() {
            candidates.push((
                Self::drm_channel_info(HwmonChannelType::Freq, "freq1", Some("sclk"), 1),
                SENSOR_TYPE::GFX_SCLK,
            ));
            candidates.push((
                Self::drm_channel_info(HwmonChannelType::Freq, "freq2", Some("mclk"), 2),
                SENSOR_TYPE::GFX_MCLK,
            ));
        }
        if has_type(HwmonChannelType::Power).not() {
            candidates.push((
                Self::drm_channel_info(HwmonChannelType::Power, "power1", Some("PPT"), 1),
                SENSOR_TYPE::GPU_AVG_POWER,
            ));
        }
        candidates
    }

    fn drm_channel_info(
        hwmon_type: HwmonChannelType,
        name: &str,
        label: Option<&str>,
        number: u8,
    ) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type,
            number,
            name: name.to_string(),
            label: label.map(ToString::to_string),
            ..Default::default()
        }
    }

    /// Reads every libdrm fallback channel and returns the converted statuses.
    /// Failed or timed-out reads are omitted (no sentinel), matching the hwmon
    /// read paths; the status cache and failsafe layer cover the gap.
    async fn extract_drm_statuses(
        drm: &DrmMetrics,
        ioctl_timeout: Duration,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut channels = Vec::with_capacity(drm.channels.len());
        let mut temps = Vec::with_capacity(1); // max 1 temp from drm
        for channel in &drm.channels {
            let Some(value) =
                Self::read_drm_sensor(Arc::clone(&drm.handle), channel.sensor, ioctl_timeout).await
            else {
                continue;
            };
            match Self::drm_status_from(channel, value) {
                DrmStatus::Channel(status) => channels.push(status),
                DrmStatus::Temp(status) => temps.push(status),
            }
        }
        (channels, temps)
    }

    /// Converts a raw libdrm sensor value to a status entry for its channel.
    /// Pure so the unit conversions are testable without hardware.
    fn drm_status_from(channel: &DrmChannel, value: u32) -> DrmStatus {
        let name = channel.info.name.clone();
        match channel.info.hwmon_type {
            // libdrm reports temperature in millidegrees Celsius.
            HwmonChannelType::Temp => DrmStatus::Temp(TempStatus {
                name,
                temp: f64::from(value) / 1000.0,
            }),
            // GPU load is already a percentage.
            HwmonChannelType::Load => DrmStatus::Channel(ChannelStatus {
                name,
                duty: Some(f64::from(value)),
                ..Default::default()
            }),
            // Clocks are reported in MHz.
            HwmonChannelType::Freq => DrmStatus::Channel(ChannelStatus {
                name,
                freq: Some(value),
                ..Default::default()
            }),
            // GPU_AVG_POWER is reported in whole watts.
            _ => DrmStatus::Channel(ChannelStatus {
                name,
                watts: Some(f64::from(value)),
                ..Default::default()
            }),
        }
    }

    /// Reads one libdrm sensor, offloaded to the blocking pool and bounded by
    /// `ioctl_timeout`. libdrm ioctls are not cancellable and can block while
    /// the GPU powers down/up or is saturated, so they must never run inline on
    /// the single-threaded runtime. Returns None on timeout, join panic, or a
    /// sensor errno.
    async fn read_drm_sensor(
        handle: Arc<AmdgpuDeviceHandle>,
        sensor: SENSOR_TYPE,
        ioctl_timeout: Duration,
    ) -> Option<u32> {
        match Self::run_drm_blocking(ioctl_timeout, move || handle.sensor_info(sensor)).await {
            Ok(value) => Some(value),
            Err(DrmReadError::Sensor(errno)) => {
                debug!("libdrm sensor {sensor:?} read error: {errno}");
                None
            }
            Err(DrmReadError::Join(err)) => {
                warn!("libdrm sensor {sensor:?} task panicked: {err}");
                None
            }
            Err(DrmReadError::TimedOut(elapsed)) => {
                warn!("libdrm sensor {sensor:?} ioctl exceeded {elapsed:?}; omitting this tick");
                None
            }
        }
    }

    /// Offloads a blocking libdrm call to the Tokio blocking pool and bounds the
    /// await with `ioctl_timeout`. Generic over the closure so the timeout and
    /// error branches are unit-testable with fake closures (no hardware).
    async fn run_drm_blocking<F, T>(
        ioctl_timeout: Duration,
        blocking_fn: F,
    ) -> Result<T, DrmReadError>
    where
        F: FnOnce() -> Result<T, i32> + Send + 'static,
        T: Send + 'static,
    {
        debug_assert!(ioctl_timeout > Duration::ZERO);
        let handle = tokio::task::spawn_blocking(blocking_fn);
        match tokio::time::timeout(ioctl_timeout, handle).await {
            Ok(Ok(Ok(value))) => Ok(value),
            Ok(Ok(Err(errno))) => Err(DrmReadError::Sensor(errno)),
            Ok(Err(join_err)) => Err(DrmReadError::Join(join_err)),
            Err(_elapsed) => Err(DrmReadError::TimedOut(ioctl_timeout)),
        }
    }

    pub fn update_all_statuses(&self) {
        for (uid, amd_driver) in &self.amd_driver_infos {
            if let Some(device_lock) = self.amd_devices.get(uid) {
                let device_index = device_lock.borrow().type_index;
                let preloaded_statuses_map = self.amd_preloaded_statuses.borrow();
                let preloaded_statuses = preloaded_statuses_map.get(&device_index);
                if preloaded_statuses.is_none() {
                    error!("There is no status preloaded for this AMD device: {device_index}");
                    continue;
                }
                let (channels, temps) = preloaded_statuses.unwrap().clone();
                let status = Status {
                    temps,
                    channels,
                    ..Default::default()
                };
                trace!(
                    "Device: {} status updated: {status:?}",
                    amd_driver.hwmon.name
                );
                device_lock.borrow_mut().set_status(status);
            }
        }
    }

    pub async fn reset_amd_to_default(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        let amd_hwmon_info = self
            .amd_driver_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        if let Some(fan_curve_info) = &amd_hwmon_info.fan_curve_info {
            if fan_curve_info.changeable {
                Self::reset_fan_curve_and_zero_rpm(fan_curve_info).await
            } else {
                Err(anyhow!(
                    "PMFW Fan Curve control is present for this device, but not enabled"
                ))
            }
        } else {
            let channel_info = amd_hwmon_info
                .hwmon
                .channels
                .iter()
                .find(|channel| {
                    channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
                })
                .with_context(|| format!("Searching for channel name: {channel_name}"))?;
            fans::set_pwm_enable_to_default_or_auto(&amd_hwmon_info.hwmon.path, channel_info).await
        }
    }

    async fn reset_fan_curve_and_zero_rpm(fan_curve_info: &FanCurveInfo) -> Result<()> {
        if let Some(zero_rpm_path) = &fan_curve_info.zero_rpm {
            let _ = cc_fs::write(zero_rpm_path, b"r\n".to_vec())
                .await
                .with_context(|| "Resetting Zero RPM Enable");
        }
        if let Some(zero_rpm_stop_temp_path) = &fan_curve_info.zero_rpm_stop_temp {
            let _ = cc_fs::write(zero_rpm_stop_temp_path, b"r\n".to_vec())
                .await
                .with_context(|| "Resetting Zero RPM Stop Temperature");
        }
        cc_fs::write(&fan_curve_info.path, b"r\n".to_vec())
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
        // RDNA3/4 Fan Curve logic:
        if let Some(fan_curve_info) = &amd_driver_info.fan_curve_info {
            if fan_curve_info.changeable.not() {
                return Err(anyhow!(
                    "PMFW Fan Curve control is present for this device, but not enabled. Please see documentation."
                ));
            }
            if fixed_speed == 0 && fan_curve_info.zero_rpm.is_some() {
                if fan_curve_info.zero_rpm_stop_temp.is_some() {
                    Self::set_zero_rpm(fan_curve_info, true).await?;
                    Self::set_zero_rpm_stop_temp_highest(fan_curve_info).await
                } else {
                    Self::set_zero_rpm(fan_curve_info, true).await?;
                    let lowest_fan_curve_speed = fan_curve_info.speed_range.start();
                    Self::set_fan_curve_duty(fan_curve_info, *lowest_fan_curve_speed).await
                }
            } else {
                if let Err(err) = Self::set_zero_rpm(fan_curve_info, false).await {
                    error!(
                        "Failed to disable Zero RPM Mode for {}: {err}",
                        amd_driver_info.hwmon.name
                    );
                }
                Self::set_fan_curve_duty(fan_curve_info, fixed_speed)
                    .await
                    .map_err(|err| {
                        anyhow!(
                            "Error settings PMFW fan duty of {fixed_speed} on {} - {err}",
                            amd_driver_info.hwmon.name
                        )
                    })
            }
        } else {
            // Standard HWMon Fan controls:
            let channel_info = amd_driver_info
                .hwmon
                .channels
                .iter()
                .find(|channel| {
                    channel.hwmon_type == HwmonChannelType::Fan && channel.name == channel_name
                })
                .with_context(|| "Searching for channel name")?;
            fans::set_pwm_enable_if_not_already(
                fans::PWM_ENABLE_MANUAL_VALUE,
                &amd_driver_info.hwmon.path,
                channel_info,
            )
            .await?;
            fans::set_pwm_duty(&amd_driver_info.hwmon.path, channel_info, fixed_speed)
                .await
                .map_err(|err| {
                    anyhow!(
                        "Error on {}:{channel_name} for duty {fixed_speed} - {err}",
                        amd_driver_info.hwmon.name
                    )
                })
        }
    }

    async fn set_zero_rpm(fan_curve_info: &FanCurveInfo, enable: bool) -> Result<()> {
        let Some(zero_rpm_path) = &fan_curve_info.zero_rpm else {
            return Ok(());
        };
        let binary_bool = u8::from(enable);
        cc_fs::write_string(zero_rpm_path, format!("{binary_bool}\n"))
            .await
            .map_err(|err| anyhow!("Error applying {binary_bool} to Zero RPM Enable: {err}"))?;
        cc_fs::write(&zero_rpm_path, b"c\n".to_vec())
            .await
            .map_err(|err| anyhow!("Error Committing Zero RPM Enable: {err}"))
    }

    async fn set_zero_rpm_stop_temp_highest(fan_curve_info: &FanCurveInfo) -> Result<()> {
        let highest_temp = fan_curve_info.zero_rpm_stop_temp_range.end();
        Self::set_zero_rpm_stop_temp(fan_curve_info, highest_temp).await
    }

    async fn set_zero_rpm_stop_temp(fan_curve_info: &FanCurveInfo, temp: &CurveTemp) -> Result<()> {
        let Some(zero_rpm_stop_temp_path) = &fan_curve_info.zero_rpm_stop_temp else {
            return Ok(());
        };
        cc_fs::write_string(&zero_rpm_stop_temp_path, format!("{temp}\n"))
            .await
            .map_err(|err| anyhow!("Error applying {temp} to Zero RPM Stop Temperature: {err}"))?;
        cc_fs::write(&zero_rpm_stop_temp_path, b"c\n".to_vec())
            .await
            .map_err(|err| anyhow!("Error Committing Zero RPM Stop Temperature: {err}"))
    }

    async fn set_fan_curve_duty(fan_curve_info: &FanCurveInfo, duty: Duty) -> Result<()> {
        let flat_curve = Self::create_flat_curve(fan_curve_info, duty);
        Self::set_fan_curve(flat_curve, &fan_curve_info.path).await
    }

    async fn set_fan_curve(fan_curve: FanCurve, fan_curve_path: &Path) -> Result<()> {
        for (i, (temp, duty)) in fan_curve.points.into_iter().enumerate() {
            cc_fs::write_string(&fan_curve_path, format!("{i} {temp} {duty}\n"))
                .await
                .map_err(|err| anyhow!("Error applying '{i} {temp} {duty}' to Fan Curve: {err}"))?;
        }
        cc_fs::write(&fan_curve_path, b"c\n".to_vec())
            .await
            .map_err(|err| anyhow!("Error committing Fan Curve changes: {err}"))
    }

    /// Creates a "flat" fan curve by setting the duty with the `temp_min` and all the rest of
    /// the points set to `temp_max`. This allows `CoolerControl` to handle Profiles and Functions
    /// natively, which the firmware cannot do.
    fn create_flat_curve(fan_curve_info: &FanCurveInfo, duty: Duty) -> FanCurve {
        let clamped_duty = if fan_curve_info.speed_range.contains(&duty) {
            duty
        } else {
            debug!(
                "AMD GPU RDNA 3 - Only fan duties within range of {}% to {}% are allowed. \
                Clamping passed duty of {duty}% to nearest limit",
                fan_curve_info.speed_range.start(),
                fan_curve_info.speed_range.end(),
            );
            *fan_curve_info
                .speed_range
                .end()
                .min(fan_curve_info.speed_range.start().max(&duty))
        };
        let mut new_fan_curve = FanCurve::default();
        new_fan_curve
            .points
            .push((*fan_curve_info.temperature_range.start(), clamped_duty));
        for _ in 1..fan_curve_info.fan_curve.points.len() {
            new_fan_curve
                .points
                .push((*fan_curve_info.temperature_range.end(), clamped_duty));
        }
        new_fan_curve
    }

    /// Applies a speed profile to the GPU's fan curve.
    /// This is only supported on Navi3x (RDNA 3/7000 series) or newer devices.
    pub async fn set_amd_fan_curve(
        &self,
        device_uid: &UID,
        speed_profile: &[(Temp, Duty)],
    ) -> Result<()> {
        let amd_driver_info = self
            .amd_driver_infos
            .get(device_uid)
            .with_context(|| "Hwmon Info should exist")?;
        let Some(fan_curve_info) = &amd_driver_info.fan_curve_info else {
            return Err(anyhow!(
                "Applying Internal Profiler Error: device_uid: {device_uid}. \
                Only AMD GPU's with fan curves are supported."
            ));
        };
        if fan_curve_info.changeable.not() {
            return Err(anyhow!(
                "Applying Internal Profiler Error: PMFW Fan Curve control is present for this device, \
                but not enabled. Please see the documentation about enabling this in the kernel."
            ));
        }
        // if present and fan curve hits 0, enable Zero RPM with interpolated temp
        // Otherwise we let the firmware handle it like stock.
        let mut set_zero_rpm = false;
        if fan_curve_info.zero_rpm.is_some() && fan_curve_info.zero_rpm_stop_temp.is_some() {
            if let Some(stop_temp) = Self::find_zero_rpm_stop_temp(fan_curve_info, speed_profile) {
                Self::set_zero_rpm(fan_curve_info, true).await?;
                Self::set_zero_rpm_stop_temp(fan_curve_info, &stop_temp).await?;
                set_zero_rpm = true;
            }
        }
        if set_zero_rpm.not() {
            if let Err(err) = Self::reset_fan_curve_and_zero_rpm(fan_curve_info).await {
                warn!("Failed to reset fan curve and zero rpm: {err}");
            }
        }
        let fan_curve = Self::create_fan_curve(fan_curve_info, speed_profile, set_zero_rpm);
        Self::set_fan_curve(fan_curve, &fan_curve_info.path)
            .await
            .map_err(|err| {
                anyhow!(
                    "Error settings PMFW fan curve of {speed_profile:?} on {} - {err}",
                    amd_driver_info.hwmon.name
                )
            })
    }

    /// Returns the temperature at which the fan curve hits 0.
    /// If the fan curve never hits 0, returns None.
    /// We use this to auto-set the `zero_rpm_stop_temp`.
    fn find_zero_rpm_stop_temp(
        fan_curve_info: &FanCurveInfo,
        speed_profile: &[(Temp, Duty)],
    ) -> Option<CurveTemp> {
        // We don't need to interpolate here really, we can just reverse check the points to find
        // the first point that hits 0.
        // (as the curve can never go below 0, and it therefore has to be a point)
        speed_profile.iter().rev().find_map(|(temp, duty)| {
            if duty < &1 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let curve_temp = temp.round() as CurveTemp;
                let clamped_temp = if fan_curve_info.zero_rpm_stop_temp_range.contains(&curve_temp) {
                    curve_temp
                } else {
                    warn!(
                    "AMD GPU RDNA - Only zero_rpm stop temps within range of {}C to {}C are allowed. \
                Clamping passed temp of {curve_temp}% to nearest limit",
                    fan_curve_info.zero_rpm_stop_temp_range.start(),
                    fan_curve_info.zero_rpm_stop_temp_range.end(),
                );
                    *fan_curve_info
                        .zero_rpm_stop_temp_range
                        .end()
                        .min(fan_curve_info.zero_rpm_stop_temp_range.start().max(&curve_temp))
                };
                return Some(clamped_temp);
            }
            None
        })
    }

    /// Returns a sanitized fan curve with clamped values.
    fn create_fan_curve(
        fan_curve_info: &FanCurveInfo,
        speed_profile: &[(Temp, Duty)],
        set_zero_rpm: bool,
    ) -> FanCurve {
        let mut fan_curve = FanCurve::default();
        let fan_curve_length = fan_curve_info.fan_curve.points.len();
        let capped_profile = Self::cap_speed_profile(speed_profile, fan_curve_length);
        let capped_profile_length = capped_profile.len();
        for (temp, duty) in capped_profile {
            let clamped_duty = if fan_curve_info.speed_range.contains(&duty) {
                duty
            } else {
                if set_zero_rpm.not() && duty != 0 {
                    warn!(
                        "AMD GPU RDNA - Only fan duties within range of {}% to {}% are allowed. \
                    Clamping passed duty of {duty}% to nearest limit",
                        fan_curve_info.speed_range.start(),
                        fan_curve_info.speed_range.end(),
                    );
                }
                *fan_curve_info
                    .speed_range
                    .end()
                    .min(fan_curve_info.speed_range.start().max(&duty))
            };
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let temp_integer = temp.round() as CurveTemp;
            let clamped_temp = if fan_curve_info.temperature_range.contains(&temp_integer) {
                temp_integer
            } else {
                warn!(
                    "AMD GPU RDNA - Only fan curve temps within range of {}C to {}C are allowed. \
                Clamping passed temp of {temp_integer}% to nearest limit",
                    fan_curve_info.temperature_range.start(),
                    fan_curve_info.temperature_range.end(),
                );
                *fan_curve_info
                    .temperature_range
                    .end()
                    .min(fan_curve_info.temperature_range.start().max(&temp_integer))
            };
            fan_curve.points.push((clamped_temp, clamped_duty));
        }
        let last_point = *fan_curve
            .points
            .last()
            .expect("Should be at least one point");
        // add any missing points:
        for _ in capped_profile_length..fan_curve_length {
            fan_curve.points.push((last_point.0, last_point.1));
        }
        fan_curve
    }

    /// Caps the speed profile to the max number of points allowed by the fan curve.
    ///
    /// If the speed profile is longer than the fan curve, we truncate the speed profile to the
    /// max number of points allowed by the fan curve. We keep the last point as reference for
    /// the fan curve, safety-wise, but allow setting it truncated.
    fn cap_speed_profile(
        speed_profile: &[(Temp, Duty)],
        fan_curve_length: usize,
    ) -> Vec<(Temp, Duty)> {
        let mut capped_profile = speed_profile.to_vec();
        if capped_profile.len() > fan_curve_length {
            warn!(
                "AMD GPU RDNA - Max {fan_curve_length} fan curve points are allowed. \
                Truncating speed profile with {} points. Please adjust the \
                Graph Profile to match the number of points allowed by the device fan curve.",
                capped_profile.len()
            );
            capped_profile.truncate(fan_curve_length - 1); // remove all but the last point
            capped_profile.push(speed_profile.last().copied().unwrap_or((100., 100_u8)));
        }
        capped_profile
    }
}

#[derive(Debug, Clone)]
pub struct AMDDriverInfo {
    pub hwmon: HwmonDriverInfo,
    device_path: PathBuf,
    pub fan_curve_info: Option<FanCurveInfo>,
    /// Whether the `gpu_od/fan_ctrl/` directory exists (RDNA3/4 indicator).
    has_rdna_fan_ctrl: bool,
    /// Whether the ppfeaturemask has the overdrive bit enabled.
    overdrive_enabled: bool,
    /// libdrm-sourced metrics that the hwmon driver does not expose, or None
    /// when hwmon covers everything or libdrm is unavailable.
    drm: Option<DrmMetrics>,
}

impl AMDDriverInfo {
    /// Iterates the hwmon channels followed by any libdrm fallback channels.
    /// Device building treats both uniformly; only the status source differs.
    fn all_channel_infos(&self) -> impl Iterator<Item = &HwmonChannelInfo> {
        self.hwmon.channels.iter().chain(
            self.drm
                .iter()
                .flat_map(|drm| drm.channels.iter().map(|channel| &channel.info)),
        )
    }
}

/// libdrm-sourced GPU metrics, used as a fallback when the amdgpu hwmon driver
/// does not expose a metric type. Only present when libdrm loaded at runtime
/// and at least one sensor read succeeded.
#[derive(Clone)]
struct DrmMetrics {
    /// Owns the render-node fd for the daemon's life. `Arc` (not `Rc`) because
    /// the handle is moved to the blocking pool for each sensor ioctl, which
    /// requires `Send`. `AmdgpuDeviceHandle` is `Send`: the crate unsafe-impls
    /// `Send` for the inner handle and `OwnedFd` is `Send`.
    handle: Arc<AmdgpuDeviceHandle>,
    channels: Vec<DrmChannel>,
}

impl std::fmt::Debug for DrmMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The libdrm handle is not Debug; surface only the backed channels.
        f.debug_struct("DrmMetrics")
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
struct DrmChannel {
    info: HwmonChannelInfo,
    sensor: SENSOR_TYPE,
}

/// One converted libdrm reading, routed to the channel or temp status vec.
enum DrmStatus {
    Channel(ChannelStatus),
    Temp(TempStatus),
}

/// Failure modes of a single libdrm sensor read. Mirrors
/// `drivetemp::BlockingTimeoutError` so timeouts (an expected transient while
/// the GPU powers down/up) are distinguishable from genuine errors.
#[derive(Debug)]
enum DrmReadError {
    /// The libdrm `sensor_info` ioctl returned a negative errno.
    Sensor(i32),
    /// The blocking task failed to join (typically a panic in the closure).
    Join(tokio::task::JoinError),
    /// The ioctl exceeded its wall-clock budget; the blocking thread is leaked
    /// until the kernel returns, but the caller is freed.
    TimedOut(Duration),
}

/// The PMFW (power management firmware) fan curve information.
/// Only available on Navi3x (RDNA 3/7000 series) or newer devices.
#[derive(Debug, Clone)]
pub struct FanCurveInfo {
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

    /// The optionally supported (Kernel 6.13+) ability to disable the Zero RPM feature.
    /// The Path to the sysfs file if exists.
    zero_rpm: Option<PathBuf>,

    /// The optionally supported (Kernel 6.13+) ability to disable the Zero RPM Stop Temperature
    /// feature. Note: Not likely supported for RDNA4 devices (9000 series)
    /// The Path to the sysfs file if exists.
    zero_rpm_stop_temp: Option<PathBuf>,
    zero_rpm_stop_temp_range: RangeInclusive<CurveTemp>,
}

impl Default for FanCurveInfo {
    fn default() -> Self {
        Self {
            fan_curve: FanCurve::default(),
            changeable: false,
            temperature_range: RangeInclusive::new(0, 0),
            speed_range: RangeInclusive::new(0, 0),
            path: PathBuf::default(),
            zero_rpm: None,
            zero_rpm_stop_temp: None,
            zero_rpm_stop_temp_range: RangeInclusive::new(0, 0),
        }
    }
}

#[derive(Debug, Default, Clone)]
struct FanCurve {
    /// Fan curve points in the (temperature, speed) format
    points: Vec<(CurveTemp, Duty)>,
}

#[cfg(test)]
mod tests {
    use crate::repositories::gpu::amd::{AMDDriverInfo, FanCurve, FanCurveInfo, GpuAMD};
    use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;
    use std::ops::Not;
    use std::ops::RangeInclusive;
    use std::path::PathBuf;

    fn basic_test_fan_curve_info() -> FanCurveInfo {
        FanCurveInfo {
            changeable: true,
            temperature_range: RangeInclusive::new(0, 100),
            speed_range: RangeInclusive::new(0, 100),
            path: PathBuf::default(),
            zero_rpm: None,
            zero_rpm_stop_temp: None,
            zero_rpm_stop_temp_range: RangeInclusive::new(0, 100),
            fan_curve: FanCurve {
                // default curve from auto mode: (5 points)
                points: vec![(0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
            },
        }
    }

    #[test]
    fn create_fan_curve_valid_length() {
        // given
        let fan_curve_info = basic_test_fan_curve_info();
        let speed_profile = vec![(25.0, 29), (35.2, 30), (62.5, 50), (81.3, 75), (100.0, 100)];

        // when
        let resulting_fan_curve = GpuAMD::create_fan_curve(&fan_curve_info, &speed_profile, false);

        // then
        assert_eq!(resulting_fan_curve.points.len(), 5);
    }

    #[test]
    fn create_fan_curve_caps_to_length() {
        // given
        let fan_curve_info = basic_test_fan_curve_info();
        let speed_profile = vec![
            (25.0, 29),
            (35.2, 30),
            (35.2, 30),
            (62.5, 50),
            (81.3, 75),
            (100.0, 100),
        ];

        // when
        let resulting_fan_curve = GpuAMD::create_fan_curve(&fan_curve_info, &speed_profile, false);

        // then
        assert_eq!(resulting_fan_curve.points.len(), 5);
        let (curve_temps, curve_duties): (Vec<u8>, Vec<u8>) =
            resulting_fan_curve.points.into_iter().unzip();
        let (expected_curve_temps, expected_curve_duties): (Vec<u8>, Vec<u8>) =
            vec![(25, 29), (35, 30), (35, 30), (63, 50), (100, 100)]
                .into_iter()
                .unzip();
        assert_eq!(curve_temps, expected_curve_temps);
        assert_eq!(curve_duties, expected_curve_duties);
    }

    #[test]
    fn create_fan_curve_expands_to_length() {
        // given
        let fan_curve_info = basic_test_fan_curve_info();
        let speed_profile = vec![(25.0, 29), (62.5, 50), (81.3, 75), (100.0, 100)];

        // when
        let resulting_fan_curve = GpuAMD::create_fan_curve(&fan_curve_info, &speed_profile, false);

        // then
        assert_eq!(resulting_fan_curve.points.len(), 5);
        let (curve_temps, curve_duties): (Vec<u8>, Vec<u8>) =
            resulting_fan_curve.points.into_iter().unzip();
        let (expected_curve_temps, expected_curve_duties): (Vec<u8>, Vec<u8>) =
            vec![(25, 29), (63, 50), (81, 75), (100, 100), (100, 100)]
                .into_iter()
                .unzip();
        assert_eq!(curve_temps, expected_curve_temps);
        assert_eq!(curve_duties, expected_curve_duties);
    }

    #[test]
    fn create_fan_curve_rounds_temps() {
        // given
        let fan_curve_info = basic_test_fan_curve_info();
        let speed_profile = vec![(25.0, 29), (35.2, 30), (62.5, 50), (81.7, 75), (100.0, 100)];

        // when
        let resulting_fan_curve = GpuAMD::create_fan_curve(&fan_curve_info, &speed_profile, false);

        // then
        let (curve_temps, _): (Vec<u8>, Vec<u8>) = resulting_fan_curve.points.into_iter().unzip();
        let expected_curve_temps = vec![25, 35, 63, 82, 100];
        assert_eq!(curve_temps, expected_curve_temps);
    }

    #[test]
    fn create_fan_curve_clamps_temps_and_duties() {
        // given
        let fan_curve_info = FanCurveInfo {
            changeable: true,
            temperature_range: RangeInclusive::new(50, 90),
            speed_range: RangeInclusive::new(50, 90),
            path: PathBuf::default(),
            zero_rpm: None,
            zero_rpm_stop_temp: None,
            zero_rpm_stop_temp_range: RangeInclusive::new(0, 100),
            fan_curve: FanCurve {
                // default curve from auto mode: (5 points)
                points: vec![(0, 0), (0, 0), (0, 0), (0, 0), (0, 0)],
            },
        };
        let speed_profile = vec![(25.0, 29), (35.2, 30), (62.5, 50), (81.7, 75), (100.0, 100)];

        // when
        let resulting_fan_curve = GpuAMD::create_fan_curve(&fan_curve_info, &speed_profile, false);

        // then
        let (curve_temps, curve_duties): (Vec<u8>, Vec<u8>) =
            resulting_fan_curve.points.into_iter().unzip();
        let (expected_curve_temps, expected_curve_duties): (Vec<u8>, Vec<u8>) =
            vec![(50, 50), (50, 50), (63, 50), (82, 75), (90, 90)]
                .into_iter()
                .unzip();
        assert_eq!(curve_temps, expected_curve_temps);
        assert_eq!(curve_duties, expected_curve_duties);
    }

    #[test]
    fn find_zero_rpm_stop_temp() {
        // given
        let fan_curve_info = basic_test_fan_curve_info();
        let speed_profile = vec![(25.0, 0), (35.2, 30), (62.5, 50), (81.3, 75), (100.0, 100)];

        // when
        let stop_temp = GpuAMD::find_zero_rpm_stop_temp(&fan_curve_info, &speed_profile);

        // then
        assert!(stop_temp.is_some(), "Expected a stop temp");
        assert_eq!(stop_temp.unwrap(), 25);
    }

    #[test]
    fn find_zero_rpm_stop_temp_highest() {
        // given
        let fan_curve_info = basic_test_fan_curve_info();
        let speed_profile = vec![(25.0, 0), (35.2, 0), (62.5, 0), (81.3, 75), (100.0, 100)];

        // when
        let stop_temp = GpuAMD::find_zero_rpm_stop_temp(&fan_curve_info, &speed_profile);

        // then
        assert!(stop_temp.is_some(), "Expected a stop temp");
        assert_eq!(stop_temp.unwrap(), 63);
    }

    fn basic_test_amd_driver(
        fan_curve_info: Option<FanCurveInfo>,
        has_rdna_fan_ctrl: bool,
        overdrive_enabled: bool,
    ) -> AMDDriverInfo {
        AMDDriverInfo {
            hwmon: HwmonDriverInfo::default(),
            device_path: PathBuf::default(),
            fan_curve_info,
            has_rdna_fan_ctrl,
            overdrive_enabled,
            drm: None,
        }
    }

    #[test]
    fn fan_controllable_pre_rdna3() {
        // Pre-RDNA3 GPUs have no fan curve info and no gpu_od directory
        let driver = basic_test_amd_driver(None, false, false);
        assert!(GpuAMD::get_fan_is_controllable(&driver));
    }

    // Verify RDNA3/4 with fan curve and overdrive enabled is controllable.
    #[test]
    fn fan_controllable_with_overdrive_enabled() {
        let driver = basic_test_amd_driver(Some(basic_test_fan_curve_info()), true, true);
        assert!(GpuAMD::get_fan_is_controllable(&driver));
    }

    // Verify RDNA3/4 with fan curve but overdrive disabled is not controllable.
    #[test]
    fn fan_not_controllable_with_overdrive_disabled() {
        let mut info = basic_test_fan_curve_info();
        info.changeable = false;
        let driver = basic_test_amd_driver(Some(info), true, false);
        assert!(GpuAMD::get_fan_is_controllable(&driver).not());
    }

    #[test]
    fn fan_not_controllable_rdna_without_fan_curve_and_no_overdrive() {
        // RDNA3/4 GPU where fan_curve_info failed to parse, overdrive not enabled
        let driver = basic_test_amd_driver(None, true, false);
        assert!(GpuAMD::get_fan_is_controllable(&driver).not());
    }

    #[test]
    fn fan_controllable_rdna_without_fan_curve_but_overdrive_enabled() {
        // RDNA3/4 GPU where fan_curve_info failed to parse, but overdrive is enabled
        let driver = basic_test_amd_driver(None, true, true);
        assert!(GpuAMD::get_fan_is_controllable(&driver));
    }

    use crate::cc_fs;
    use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};
    use serial_test::serial;
    use std::path::Path;
    use uuid::Uuid;

    const LOAD_TEST_BASE_PATH_STR: &str = "/tmp/coolercontrol-tests-";

    struct LoadFileContext {
        test_base_path: PathBuf,
    }

    async fn load_setup() -> LoadFileContext {
        let test_base_path =
            Path::new(&(LOAD_TEST_BASE_PATH_STR.to_string() + &Uuid::new_v4().to_string()))
                .to_path_buf();
        cc_fs::create_dir_all(&test_base_path).await.unwrap();
        LoadFileContext { test_base_path }
    }

    async fn load_teardown(ctx: &LoadFileContext) {
        cc_fs::remove_dir_all(&ctx.test_base_path).await.unwrap();
    }

    #[test]
    #[serial]
    fn extract_load_status_returns_value_on_success() {
        // Verifies a readable gpu_busy_percent results in a channel status
        // with the expected duty.
        cc_fs::test_runtime(async {
            let ctx = load_setup().await;
            // given: gpu_busy_percent sysfs file is present and parseable.
            cc_fs::write(ctx.test_base_path.join("gpu_busy_percent"), b"42".to_vec())
                .await
                .unwrap();
            let driver = AMDDriverInfo {
                hwmon: HwmonDriverInfo {
                    channels: vec![HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Load,
                        name: "load".to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                device_path: ctx.test_base_path.clone(),
                fan_curve_info: None,
                has_rdna_fan_ctrl: false,
                overdrive_enabled: false,
                drm: None,
            };

            // when:
            let result = GpuAMD::extract_load_status(&driver).await;

            // then:
            load_teardown(&ctx).await;
            assert_eq!(result.len(), 1);
            assert_eq!(result[0].name, "load");
            assert_eq!(result[0].duty, Some(42.0));
        });
    }

    #[test]
    #[serial]
    fn extract_load_status_skips_on_failure() {
        // Verifies that when gpu_busy_percent is missing, no channel
        // status is emitted at all. Fabricating a 0% duty would lie to
        // downstream GPU fan curves.
        cc_fs::test_runtime(async {
            let ctx = load_setup().await;
            // given: load channel configured but sysfs file absent.
            let driver = AMDDriverInfo {
                hwmon: HwmonDriverInfo {
                    channels: vec![HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Load,
                        name: "load".to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                device_path: ctx.test_base_path.clone(),
                fan_curve_info: None,
                has_rdna_fan_ctrl: false,
                overdrive_enabled: false,
                drm: None,
            };

            // when:
            let result = GpuAMD::extract_load_status(&driver).await;

            // then:
            load_teardown(&ctx).await;
            assert_eq!(result.len(), 0);
        });
    }

    #[test]
    #[serial]
    fn extract_load_status_ignores_non_load_channels() {
        // Verifies that non-Load channel types never emit load entries.
        cc_fs::test_runtime(async {
            let ctx = load_setup().await;
            // given: only a Temp channel exists; no Load channels.
            cc_fs::write(ctx.test_base_path.join("gpu_busy_percent"), b"50".to_vec())
                .await
                .unwrap();
            let driver = AMDDriverInfo {
                hwmon: HwmonDriverInfo {
                    channels: vec![HwmonChannelInfo {
                        hwmon_type: HwmonChannelType::Temp,
                        name: "edge".to_string(),
                        ..Default::default()
                    }],
                    ..Default::default()
                },
                device_path: ctx.test_base_path.clone(),
                fan_curve_info: None,
                has_rdna_fan_ctrl: false,
                overdrive_enabled: false,
                drm: None,
            };

            // when:
            let result = GpuAMD::extract_load_status(&driver).await;

            // then:
            load_teardown(&ctx).await;
            assert_eq!(result.len(), 0);
        });
    }

    use super::{drm_ioctl_timeout_for, DrmChannel, DrmReadError, DrmStatus};
    use crate::repositories::gpu::gpu_repo::GPU_LOAD_NAME;
    use libdrm_amdgpu_sys::AMDGPU::SENSOR_INFO::SENSOR_TYPE;
    use std::time::Duration;

    #[test]
    fn drm_ioctl_timeout_scales_with_poll_rate() {
        // Goal: the libdrm ioctl budget is a ratio of the poll rate
        // (poll_rate * READ_PERMIT_RATIO, 0.7) and grows with poll_rate, so it
        // stays near one poll interval rather than hwmon's larger ioctl budget.
        assert_eq!(drm_ioctl_timeout_for(1.0), Duration::from_secs_f64(0.7));
        assert_eq!(drm_ioctl_timeout_for(0.5), Duration::from_secs_f64(0.35));
        assert!(drm_ioctl_timeout_for(0.5) < drm_ioctl_timeout_for(5.0));
    }

    #[test]
    fn drm_fallback_candidates_when_hwmon_empty() {
        // Goal: with no hwmon channels, libdrm backs all four metric types,
        // splitting freq into sclk + mclk, using hwmon-convention names.
        let candidates = GpuAMD::drm_fallback_candidates(&[]);
        let pairs: Vec<(&str, SENSOR_TYPE)> = candidates
            .iter()
            .map(|(info, sensor)| (info.name.as_str(), *sensor))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (GPU_LOAD_NAME, SENSOR_TYPE::GPU_LOAD),
                ("temp1", SENSOR_TYPE::GPU_TEMP),
                ("freq1", SENSOR_TYPE::GFX_SCLK),
                ("freq2", SENSOR_TYPE::GFX_MCLK),
                ("power1", SENSOR_TYPE::GPU_AVG_POWER),
            ]
        );
        // Labels feed the GPU_* base device-build arms: load carries its full
        // label directly (InfoOnly copies it), temp/freq carry the hwmon-style
        // descriptor the arm prefixes, and power has none (arm yields "GPU Power").
        let labels: Vec<Option<&str>> = candidates
            .iter()
            .map(|(info, _)| info.label.as_deref())
            .collect();
        assert_eq!(
            labels,
            vec![
                Some(GPU_LOAD_NAME),
                Some("edge"),
                Some("sclk"),
                Some("mclk"),
                None,
            ]
        );
    }

    #[test]
    fn drm_fallback_candidates_none_when_hwmon_covers_all() {
        // Goal: when hwmon already exposes every metric type, libdrm adds nothing.
        let hwmon = vec![
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Load,
                ..Default::default()
            },
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Temp,
                ..Default::default()
            },
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Freq,
                ..Default::default()
            },
            HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Power,
                ..Default::default()
            },
        ];
        assert!(GpuAMD::drm_fallback_candidates(&hwmon).is_empty());
    }

    #[test]
    fn drm_fallback_candidates_only_missing_types() {
        // Goal: a present hwmon temp means libdrm backs only load, freq, power.
        let hwmon = vec![HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            ..Default::default()
        }];
        let types: Vec<HwmonChannelType> = GpuAMD::drm_fallback_candidates(&hwmon)
            .iter()
            .map(|(info, _)| info.hwmon_type.clone())
            .collect();
        assert_eq!(
            types,
            vec![
                HwmonChannelType::Load,
                HwmonChannelType::Freq,
                HwmonChannelType::Freq,
                HwmonChannelType::Power,
            ]
        );
    }

    fn drm_channel(hwmon_type: HwmonChannelType, sensor: SENSOR_TYPE) -> DrmChannel {
        DrmChannel {
            info: GpuAMD::drm_channel_info(hwmon_type, "x", None, 0),
            sensor,
        }
    }

    #[test]
    #[allow(clippy::float_cmp)] // exact conversions, no rounding
    fn drm_status_from_converts_units() {
        // Goal: each sensor's raw value maps to the right status field and unit:
        // temp is millidegrees C, load is %, sclk is MHz, power is W.
        let temp = drm_channel(HwmonChannelType::Temp, SENSOR_TYPE::GPU_TEMP);
        match GpuAMD::drm_status_from(&temp, 45000) {
            DrmStatus::Temp(status) => assert_eq!(status.temp, 45.0),
            DrmStatus::Channel(_) => panic!("temp should map to TempStatus"),
        }
        let load = drm_channel(HwmonChannelType::Load, SENSOR_TYPE::GPU_LOAD);
        match GpuAMD::drm_status_from(&load, 42) {
            DrmStatus::Channel(status) => assert_eq!(status.duty, Some(42.0)),
            DrmStatus::Temp(_) => panic!("load should map to ChannelStatus"),
        }
        let freq = drm_channel(HwmonChannelType::Freq, SENSOR_TYPE::GFX_SCLK);
        match GpuAMD::drm_status_from(&freq, 1500) {
            DrmStatus::Channel(status) => assert_eq!(status.freq, Some(1500)),
            DrmStatus::Temp(_) => panic!("freq should map to ChannelStatus"),
        }
        let power = drm_channel(HwmonChannelType::Power, SENSOR_TYPE::GPU_AVG_POWER);
        match GpuAMD::drm_status_from(&power, 120) {
            DrmStatus::Channel(status) => assert_eq!(status.watts, Some(120.0)),
            DrmStatus::Temp(_) => panic!("power should map to ChannelStatus"),
        }
    }

    #[test]
    #[serial]
    fn run_drm_blocking_returns_value_when_fast() {
        // Goal: a prompt closure yields its value unchanged.
        cc_fs::test_runtime(async {
            let result = GpuAMD::run_drm_blocking(Duration::from_secs(1), || Ok(42u32)).await;
            assert!(matches!(result, Ok(42)), "expected Ok(42), got {result:?}");
        });
    }

    #[test]
    #[serial]
    fn run_drm_blocking_times_out_on_slow_closure() {
        // Goal: a closure that blocks past the timeout yields TimedOut without
        // stalling the caller (the libdrm ioctl analog of a wedged GPU).
        cc_fs::test_runtime(async {
            let start = std::time::Instant::now();
            let result = GpuAMD::run_drm_blocking(Duration::from_millis(100), || {
                std::thread::sleep(Duration::from_secs(5));
                Ok(0u32)
            })
            .await;
            assert!(
                matches!(result, Err(DrmReadError::TimedOut(_))),
                "expected TimedOut, got {result:?}"
            );
            assert!(
                start.elapsed() < Duration::from_secs(2),
                "caller was stalled: {:?}",
                start.elapsed()
            );
        });
    }

    #[test]
    #[serial]
    fn run_drm_blocking_surfaces_sensor_errno() {
        // Goal: a negative errno from the ioctl surfaces as DrmReadError::Sensor.
        cc_fs::test_runtime(async {
            let result: Result<u32, DrmReadError> =
                GpuAMD::run_drm_blocking(Duration::from_secs(1), || Err(-22)).await;
            assert!(
                matches!(result, Err(DrmReadError::Sensor(-22))),
                "expected Sensor(-22), got {result:?}"
            );
        });
    }
}
