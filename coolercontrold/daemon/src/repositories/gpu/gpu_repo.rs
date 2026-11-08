// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::device::health::{FailsafeRef, UnreachableRef};
use crate::repositories::hwmon::device_io::DeviceHealth;
use chrono::{DateTime, Local};
use std::collections::HashMap;
use std::env;
use std::ops::Not;
use std::rc::Rc;
use std::time::Duration;

use crate::diagnostics::support::HardwareSupportController;
use crate::rt;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use futures_util::future::{join, join_all};
use log::{debug, error, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::time::Instant;
use strum::{Display, EnumString};
use tokio::sync::{Semaphore, SemaphorePermit};

use crate::config::Config;
use crate::device::{ChannelAttribute, DeviceType, Duty, MAX_CHANNEL_ATTRIBUTES, UID};
use crate::repositories::device_summary;
use crate::repositories::failsafe::MISSING_STATUS_THRESHOLD;
use crate::repositories::gpu::amd::{AMDDriverInfo, GpuAMD, TEMP_FOR_FAN_CURVE};
use crate::repositories::gpu::nvidia::{self, GpuNVidia, NvidiaPresence, NvmlInitResult};
use crate::repositories::hwmon::attributes;
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::repositories::utils::apply_device_command_delay;
use crate::setting::{LcdSettings, LightingSettings, TempSource};
use crate::ENV_NVML;

pub const GPU_TEMP_NAME: &str = "GPU Temp";
pub const GPU_FREQ_NAME: &str = "GPU Freq";
pub const GPU_LOAD_NAME: &str = "GPU Load";
pub const GPU_POWER_NAME: &str = "GPU Power";
pub const COMMAND_TIMEOUT_DEFAULT: Duration = Duration::from_millis(800);
pub const COMMAND_TIMEOUT_FIRST_TRY: Duration = Duration::from_secs(5);

/// Fraction of `poll_rate` a device preload is allowed before the
/// slow-device arm fires. Mirrors the hwmon value so AMD GPUs
/// (which use hwmon/sysfs under the hood) share the same budgeting.
/// See `hwmon_repo::READ_PERMIT_RATIO` and the `main_loop` module
/// doc for how this layer interacts with the snapshot timeout and
/// the failsafe layer.
const READ_PERMIT_RATIO: f64 = 0.7;

/// Derives the read permit timeout from `poll_rate`. Pure helper so
/// the ratio is testable without constructing a full `GpuRepo`.
fn device_read_permit_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * READ_PERMIT_RATIO)
}

/// Derives the write permit timeout from `poll_rate`. Pure helper so
/// the formula is testable without constructing a full `GpuRepo`.
/// `MISSING_STATUS_THRESHOLD` is a small `usize` (8) that fits within
/// `u8::MAX`, so the cast to `f64` is lossless.
#[allow(clippy::cast_precision_loss)]
fn device_write_permit_timeout_for(poll_rate: f64) -> Duration {
    debug_assert!(poll_rate >= 0.5);
    debug_assert!(poll_rate <= 5.0);
    Duration::from_secs_f64(poll_rate * MISSING_STATUS_THRESHOLD as f64)
}

/// The NVIDIA detection path, decided before anything is probed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NvidiaPath {
    /// No NVIDIA GPU or driver.
    Skip,
    /// musl build: NVML is a glibc library and segfaults under musl. The CLI tools need glibc,
    /// and fan control through them also needs X.
    MuslUnsupported,
    /// `nvidia-smi` only: forced, or NVML disabled.
    Cli,
    /// NVML, with `nvidia-smi` as the fallback.
    Nvml,
}

/// Pure helper so the order of the checks is testable without constructing a full `GpuRepo`.
fn nvidia_path(presence: NvidiaPresence, force_cli: bool, nvml_enabled: bool) -> NvidiaPath {
    if presence == NvidiaPresence::Absent {
        NvidiaPath::Skip
    } else if cfg!(target_env = "musl") {
        NvidiaPath::MuslUnsupported
    } else if force_cli || nvml_enabled.not() {
        NvidiaPath::Cli
    } else {
        NvidiaPath::Nvml
    }
}

/// nvidia-smi output is external input, so the count saturates instead of asserting.
fn nvidia_smi_gpu_count_u8(len: usize) -> u8 {
    u8::try_from(len).unwrap_or(u8::MAX)
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, PartialEq, Eq, Hash, Display, EnumString, Serialize, Deserialize)]
pub enum GpuType {
    Nvidia,
    AMD,
}

/// A Repository for GPU devices
pub struct GpuRepo {
    config: Rc<Config>,
    devices: HashMap<UID, DeviceLock>,
    /// Registry the channel verdicts are published to.
    hardware_support: Option<Rc<HardwareSupportController>>,
    gpu_type_count: HashMap<GpuType, u8>,
    gpus_nvidia: GpuNVidia,
    nvml_active: bool,
    gpus_amd: GpuAMD,
    force_nvidia_cli: bool,
    /// Cached per-device command delay in milliseconds. Loaded at startup from config.
    device_delays: HashMap<UID, u16>,
    /// Permits for each AMD GPU device. AMD GPUs use hwmon/sysfs under the hood,
    /// so they need the same serialization as hwmon devices to prevent concurrent
    /// reads/writes to the same sysfs files.
    device_permits: HashMap<UID, Semaphore>,

    /// Snapshot of the read-permit timeout. `poll_rate` only changes on
    /// daemon restart, so this value is constant for the repo's lifetime
    /// and is computed once in `new` to avoid per-poll f64 math and a
    /// `RefCell` borrow on the config hot path.
    device_read_permit_timeout: Duration,

    /// Snapshot of the write-permit timeout. Constant for the repo's
    /// lifetime; see `device_read_permit_timeout`.
    device_write_permit_timeout: Duration,
}

impl GpuRepo {
    pub fn new(config: Rc<Config>, nvidia_cli: bool) -> Self {
        // `poll_rate` is captured at daemon startup and cannot change
        // without a restart, so the derived permit timeouts are frozen
        // here for the repo's lifetime.
        let poll_rate = config.poll_rate_or_default();
        let device_read_permit_timeout = device_read_permit_timeout_for(poll_rate);
        let device_write_permit_timeout = device_write_permit_timeout_for(poll_rate);
        Self {
            gpus_nvidia: GpuNVidia::new(Rc::clone(&config)),
            gpus_amd: GpuAMD::new(Rc::clone(&config)),
            config,
            devices: HashMap::new(),
            hardware_support: None,
            gpu_type_count: HashMap::new(),
            nvml_active: false,
            force_nvidia_cli: nvidia_cli,
            device_delays: HashMap::new(),
            device_permits: HashMap::new(),
            device_read_permit_timeout,
            device_write_permit_timeout,
        }
    }

    /// Attaches the hardware-support registry. `None` in tests, which simply
    /// skips publishing.
    #[must_use]
    pub fn with_hardware_support(
        mut self,
        hardware_support: Rc<HardwareSupportController>,
    ) -> Self {
        self.hardware_support = Some(hardware_support);
        self
    }

    /// Publishes whether each channel can be driven. This repository has no
    /// sysfs-level evidence, so the verdict carries none: an unexplained
    /// "not controllable" is honest, invented measurements are not.
    fn publish_channel_drivability(&self) {
        let Some(hardware_support) = self.hardware_support.as_ref() else {
            return;
        };
        for (device_uid, device_lock) in &self.devices {
            let device = device_lock.borrow();
            hardware_support.record_device_channels(device_uid, &device.info);
            // Enabled by construction: a disabled device never reaches this map.
            hardware_support.record_device_summary(
                device_uid,
                crate::diagnostics::report::DeviceSummary::from_device(&device, true),
            );
        }
    }

    fn load_device_delays(&mut self) {
        for uid in self.devices.keys() {
            let delay_millis = self
                .config
                .get_cc_settings_for_device(uid)
                .ok()
                .flatten()
                .map_or(0, |s| s.extensions.delay_millis);
            if delay_millis > 0 {
                self.device_delays.insert(uid.clone(), delay_millis);
            }
        }
    }

    fn device_delay(&self, device_uid: &UID) -> u16 {
        self.device_delays.get(device_uid).copied().unwrap_or(0)
    }

    fn is_amd_device(&self, device_uid: &UID) -> bool {
        self.gpus_amd.amd_driver_infos.contains_key(device_uid)
    }

    /// Whether the daemon has driven this channel, which is what a reset needs before it writes.
    fn is_channel_taken(&self, device_uid: &UID, channel_name: &str) -> bool {
        if self.is_amd_device(device_uid) {
            self.gpus_amd.is_channel_taken(device_uid, channel_name)
        } else {
            self.gpus_nvidia.is_channel_taken(device_uid, channel_name)
        }
    }

    async fn get_amd_permit_with_write_timeout(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<SemaphorePermit<'_>> {
        let Some(semaphore) = self.device_permits.get(device_uid) else {
            return Err(anyhow!("No device permit found for AMD GPU: {device_uid}"));
        };
        tokio::select! {
            () = rt::sleep(self.device_write_permit_timeout) => Err(anyhow!(
                "TIMEOUT AMD GPU device: {device_uid} channel: {channel_name}; waiting to apply \
                setting. There will be significant issues handling this device due to extreme lag."
            )),
            device_permit = semaphore.acquire() =>
                device_permit.map_err(|e| anyhow!(e)),
        }
    }

    /// The type-index offset for NVIDIA GPUs: they are numbered after the AMD GPUs, so this
    /// must only be read after the AMD count has been stored in `gpu_type_count`.
    fn starting_nvidia_index(&self) -> u8 {
        self.gpu_type_count.get(&GpuType::AMD).unwrap_or(&0) + 1
    }

    /// Returns the detected AMD GPUs, which initialization builds its devices from.
    #[allow(clippy::cast_possible_truncation)]
    async fn detect_gpu_types(&mut self) -> Vec<AMDDriverInfo> {
        // Count AMD GPUs first so the NVIDIA type-index offset is known: init_nvml_devices
        // derives each device UID from gpu_index plus this offset to check the disabled
        // flag, and a wrong offset would silently miss it and keep NVML attached.
        let amd_drivers = self.gpus_amd.init_devices().await;
        self.gpu_type_count
            .insert(GpuType::AMD, amd_drivers.len() as u8);
        let starting_nvidia_index = self.starting_nvidia_index();
        let nvml_enabled = env::var(ENV_NVML)
            .ok()
            .and_then(|env_nvml| {
                env_nvml
                    .parse::<u8>()
                    .ok()
                    .map(|bb| bb != 0)
                    .or_else(|| Some(env_nvml.trim().to_lowercase() != "off"))
            })
            .unwrap_or(true);
        let nvidia_presence = nvidia::nvidia_gpu_may_be_present().await;
        let path = nvidia_path(nvidia_presence, self.force_nvidia_cli, nvml_enabled);
        let nvidia_dev_count = match path {
            NvidiaPath::Skip => {
                info!("No NVIDIA GPU or driver found, skipping NVML and nvidia-smi.");
                0
            }
            NvidiaPath::MuslUnsupported => {
                if nvidia_presence == NvidiaPresence::DriverLoaded {
                    warn!(
                        "NVIDIA GPUs are not supported in musl builds, such as the Alpine Docker \
                         image. Use a glibc build, such as the default Docker image."
                    );
                } else {
                    info!(
                        "NVIDIA GPUs on the proprietary driver are not supported in musl builds. \
                         GPUs on nouveau show as hwmon devices instead."
                    );
                }
                0
            }
            NvidiaPath::Cli => self.nvidia_smi_gpu_count().await,
            NvidiaPath::Nvml => match self
                .gpus_nvidia
                .init_nvml_devices(starting_nvidia_index)
                .await
            {
                NvmlInitResult::Active(num_nvml_devices) => {
                    self.nvml_active = true;
                    num_nvml_devices
                }
                NvmlInitResult::Released => 0,
                NvmlInitResult::Unavailable => self.nvidia_smi_gpu_count().await,
            },
        };
        self.gpu_type_count
            .insert(GpuType::Nvidia, nvidia_dev_count);
        amd_drivers
    }

    /// Counts the GPUs `nvidia-smi` reports. Only called once NVML is unavailable or disabled, so
    /// zero means no NVIDIA path works.
    async fn nvidia_smi_gpu_count(&self) -> u8 {
        let gpu_count = self
            .gpus_nvidia
            .get_nvidia_smi_status(COMMAND_TIMEOUT_FIRST_TRY)
            .await
            .len();
        if gpu_count == 0 {
            info!(
                "An NVIDIA GPU or driver was found, but neither NVML nor nvidia-smi can reach it, \
                 so it is not shown. With the proprietary driver, check that NVML and nvidia-smi \
                 are installed and accessible. GPUs on nouveau show as hwmon devices instead."
            );
        }
        nvidia_smi_gpu_count_u8(gpu_count)
    }

    pub async fn load_amd_statuses(&self) {
        let read_permit_timeout = self.device_read_permit_timeout;
        let reads = self
            .gpus_amd
            .amd_driver_infos
            .iter()
            .filter_map(|(uid, amd_driver)| {
                let type_index = self.devices.get(uid)?.borrow().type_index;
                let delay = self.device_delay(uid);
                Some(async move {
                    let Some(device_semaphore) = self.device_permits.get(uid) else {
                        return;
                    };
                    tokio::select! {
                        () = rt::sleep(read_permit_timeout) => {
                            warn!(
                                "TIMEOUT waiting for AMD GPU device permit: {uid}. \
                                Skipping status preload for this cycle."
                            );
                        },
                        Ok(device_permit) = device_semaphore.acquire() => {
                            self.gpus_amd
                                .preload_amd_status(type_index, amd_driver)
                                .await;
                            apply_device_command_delay(delay).await;
                            drop(device_permit);
                        },
                    }
                })
            });
        join_all(reads).await;
    }

    async fn load_nvml_status(&self) {
        let reads = self
            .gpus_nvidia
            .nvidia_device_infos
            .iter()
            .filter_map(|(uid, nv_info)| {
                let type_index = self.devices.get(uid)?.borrow().type_index;
                let delay = self.device_delay(uid);
                Some(async move {
                    // A GPU that did not answer keeps its last-known-good values until its
                    // failsafe takes over.
                    match self.gpus_nvidia.request_nvml_status(nv_info).await {
                        Some(readings) => {
                            self.gpus_nvidia
                                .store_nvml_status(type_index, nv_info, &readings)
                                .await;
                        }
                        None => self.gpus_nvidia.record_nvml_no_answer(type_index),
                    }
                    apply_device_command_delay(delay).await;
                })
            });
        join_all(reads).await;
    }

    async fn load_nvidia_smi_status(&self) {
        let mut nv_status_map = HashMap::new();
        for nv_status in self.gpus_nvidia.try_request_nv_smi_statuses().await {
            nv_status_map.insert(nv_status.index, nv_status);
        }
        for (uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.borrow().type_index;
                if let Some(nv_status) = nv_status_map.remove(&nv_info.gpu_index) {
                    self.gpus_nvidia
                        .nvidia_preloaded_statuses
                        .borrow_mut()
                        .insert(type_index, nv_status);
                } else {
                    error!("GPU Index not found in Nvidia status response");
                }
            }
            apply_device_command_delay(self.device_delay(uid)).await;
        }
    }
}

#[async_trait(?Send)]
impl Repository for GpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::GPU
    }

    fn failsafing(&self) -> Vec<FailsafeRef> {
        self.gpus_nvidia.failsafing()
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        let amd_drivers = self.detect_gpu_types().await;
        let amd_devices = self.gpus_amd.initialize_amd_devices(amd_drivers).await?;
        for uid in amd_devices.keys() {
            self.device_permits
                .insert(uid.clone(), Semaphore::const_new(1));
        }
        self.devices.extend(amd_devices);
        let has_nvidia_devices = self.gpu_type_count.get(&GpuType::Nvidia).unwrap_or(&0) > &0;
        if has_nvidia_devices {
            let starting_nvidia_index = self.starting_nvidia_index();
            let nvidia_devices = self
                .gpus_nvidia
                .initialize_nvidia_devices(starting_nvidia_index)
                .await?;
            self.devices.extend(nvidia_devices);
        }
        let mut init_devices = HashMap::new();
        for (uid, device) in &self.devices {
            init_devices.insert(uid.clone(), device.borrow().clone());
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized GPU Devices: {init_devices:?}");
        } else {
            info!(
                "Initialized GPU Devices: {}",
                device_summary::summarize_devices(init_devices.values())
            );
        }
        trace!(
            "Time taken to initialize all GPU devices: {:?}",
            start_initialization.elapsed()
        );
        self.load_device_delays();
        self.publish_channel_drivability();
        debug!("GPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        if self.devices.is_empty().not() {
            if self.nvml_active {
                join(self.load_amd_statuses(), self.load_nvml_status()).await;
            } else {
                join(self.load_amd_statuses(), self.load_nvidia_smi_status()).await;
            }
        }
        trace!(
            "STATUS PRELOAD Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
    }

    fn update_statuses(&self, timestamp: DateTime<Local>) -> Result<()> {
        self.gpus_amd.update_all_statuses(timestamp);
        self.gpus_nvidia.update_all_statuses(timestamp);
        Ok(())
    }

    fn unreachable_devices(&self) -> Vec<UnreachableRef> {
        let mut out = Vec::new();
        for (device_uid, amd_driver) in &self.gpus_amd.amd_driver_infos {
            let DeviceHealth::Unreachable {
                consecutive_timeouts,
            } = amd_driver.hwmon.io.health()
            else {
                continue;
            };
            out.push(UnreachableRef {
                device_uid: device_uid.clone(),
                device_name: amd_driver.hwmon.name.clone(),
                consecutive_timeouts,
            });
        }
        for (device_uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
            let Some(io) = self.gpus_nvidia.nvml_worker(nv_info.gpu_index) else {
                continue;
            };
            let DeviceHealth::Unreachable {
                consecutive_timeouts,
            } = io.health()
            else {
                continue;
            };
            out.push(UnreachableRef {
                device_uid: device_uid.clone(),
                device_name: io.device_name().to_owned(),
                consecutive_timeouts,
            });
        }
        out
    }

    async fn shutdown(&self) -> Result<()> {
        for (uid, device_lock) in &self.gpus_amd.amd_devices {
            let channel_names: Vec<String> =
                device_lock.borrow().info.channels.keys().cloned().collect();
            for channel_name in &channel_names {
                if self.gpus_amd.is_channel_taken(uid, channel_name).not() {
                    continue;
                }
                if let Ok(_device_permit) = self
                    .get_amd_permit_with_write_timeout(uid, channel_name)
                    .await
                {
                    self.gpus_amd
                        .reset_amd_to_default(uid, channel_name)
                        .await
                        .ok();
                }
            }
        }
        // A GPU that stopped answering during the session may answer now, and handing fan
        // control back is the most safety-relevant write here, so it earns one attempt.
        self.gpus_nvidia.allow_nvml_probe_now();
        self.gpus_nvidia.reset_devices().await;
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
        if self.is_channel_taken(device_uid, channel_name).not() {
            debug!("Leaving GPU device: {device_uid} channel: {channel_name} as it is: not taken");
            return Ok(());
        }
        debug!(
            "Applying GPU device: {device_uid} channel: {channel_name}; Resetting to Automatic fan control"
        );
        let result = if self.is_amd_device(device_uid) {
            let _device_permit = self
                .get_amd_permit_with_write_timeout(device_uid, channel_name)
                .await?;
            self.gpus_amd
                .reset_amd_to_default(device_uid, channel_name)
                .await
        } else {
            self.gpus_nvidia
                .reset_device(device_uid, channel_name)
                .await
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    /// Applying manual control is handled internally for GPU devices.
    async fn apply_setting_manual_control(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
    ) -> Result<()> {
        Ok(())
    }

    async fn apply_setting_speed_fixed(
        &self,
        device_uid: &UID,
        channel_name: &str,
        speed_fixed: u8,
    ) -> Result<()> {
        debug!(
            "Applying GPU device: {device_uid} channel: {channel_name}; \
             Device Duty: {speed_fixed}%"
        );
        if speed_fixed > 100 {
            return Err(anyhow!("Invalid fixed_speed: {speed_fixed}"));
        }
        let result = if self.is_amd_device(device_uid) {
            let _device_permit = self
                .get_amd_permit_with_write_timeout(device_uid, channel_name)
                .await?;
            self.gpus_amd
                .set_amd_duty(device_uid, channel_name, speed_fixed)
                .await
        } else {
            self.gpus_nvidia
                .set_fan_duty(device_uid, channel_name, speed_fixed)
                .await
        };
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
    }

    async fn apply_setting_speed_profile(
        &self,
        device_uid: &UID,
        channel_name: &str,
        temp_source: &TempSource,
        speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        let is_supported = self
            .gpus_amd
            .amd_driver_infos
            .get(device_uid)
            .is_some_and(|infos| infos.fan_curve_info.is_some());
        if is_supported.not() {
            return Err(anyhow!(
                "Applying Internal Profiler Error: device_uid: {device_uid}. Only AMD RNDA3/7000 series and newer GPU's support hardware fan curves."
            ));
        }
        if &temp_source.device_uid != device_uid {
            return Err(anyhow!(
                "Applying Internal Profiler Error: temp_source device_uid: {} does not match this device.",
                temp_source.device_uid
            ));
        }
        if temp_source.temp_name != TEMP_FOR_FAN_CURVE {
            return Err(anyhow!(
                "Applying Internal Profiler Error: temp_source temp_name: {}. \
                Only 'temp1' Edge temperature is supported for internal profiles.",
                temp_source.temp_name
            ));
        }
        debug!(
            "Applying GPU device: {device_uid} channel: {channel_name}; \
             Speed Profile (device duty): {speed_profile:?}"
        );
        let _device_permit = self
            .get_amd_permit_with_write_timeout(device_uid, channel_name)
            .await?;
        let result = self
            .gpus_amd
            .set_amd_fan_curve(device_uid, speed_profile)
            .await;
        apply_device_command_delay(self.device_delay(device_uid)).await;
        result
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

    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }

    /// RDNA3/4 cards clamp non-zero duty into the PMFW speed range, and
    /// with Zero RPM present `SpeedOptions::min_duty` reports 0, so the
    /// floor is invisible without this.
    fn duty_floor(&self, device_uid: &UID, _channel_name: &str) -> Duty {
        self.gpus_amd.pmfw_duty_floor(device_uid).unwrap_or(0)
    }

    /// AMD only: its channels are hwmon files. NVIDIA thresholds live in NVML and are not
    /// read yet, so NVIDIA devices report none.
    async fn channel_attributes(
        &self,
        device_uid: &UID,
        channel_name: &str,
    ) -> Result<Vec<ChannelAttribute>> {
        let Some(amd_driver) = self.gpus_amd.amd_driver_infos.get(device_uid) else {
            return Ok(Vec::new());
        };
        let driver = &amd_driver.hwmon;
        let Some(channel) = driver.channels.iter().find(|c| c.name == channel_name) else {
            return Ok(Vec::new());
        };
        debug_assert_eq!(channel.name, channel_name);
        if driver.io.is_unreachable() {
            return Err(anyhow!("AMD GPU device {} is not responding", driver.name));
        }
        let semaphore = self
            .device_permits
            .get(device_uid)
            .ok_or_else(|| anyhow!("No device permit found for AMD GPU: {device_uid}"))?;
        let device_label = format!("AMD GPU device: {device_uid} channel: {channel_name}");
        let _permit =
            attributes::acquire_permit(semaphore, self.device_read_permit_timeout, &device_label)
                .await?;
        let attributes = attributes::read_channel_attributes(driver, channel).await;
        debug_assert!(attributes.len() <= MAX_CHANNEL_ATTRIBUTES);
        Ok(attributes)
    }
}

#[cfg(test)]
mod nvidia_path_tests {
    use super::*;

    const PRESENT: [NvidiaPresence; 2] =
        [NvidiaPresence::MaybePresent, NvidiaPresence::DriverLoaded];
    const FLAGS: [(bool, bool); 4] = [(false, false), (false, true), (true, false), (true, true)];

    /// Goal: with no NVIDIA GPU or driver, nothing is probed, whatever the flags.
    /// Method: every flag combination with `Absent`.
    #[test]
    fn absent_skips_every_probe() {
        for (force_cli, nvml_enabled) in FLAGS {
            assert_eq!(
                nvidia_path(NvidiaPresence::Absent, force_cli, nvml_enabled),
                NvidiaPath::Skip
            );
        }
    }

    /// Goal: a musl build never reaches NVML, which segfaults there, or the CLI tools. Method:
    /// every present state and flag combination, including a forced CLI and NVML enabled.
    #[cfg(target_env = "musl")]
    #[test]
    fn musl_never_probes_nvidia() {
        for presence in PRESENT {
            for (force_cli, nvml_enabled) in FLAGS {
                assert_eq!(
                    nvidia_path(presence, force_cli, nvml_enabled),
                    NvidiaPath::MuslUnsupported
                );
            }
        }
    }

    /// Goal: a glibc build uses NVML unless the CLI is forced or NVML is disabled.
    /// Method: every present state and flag combination.
    #[cfg(not(target_env = "musl"))]
    #[test]
    fn glibc_picks_nvml_unless_cli_forced_or_nvml_disabled() {
        for presence in PRESENT {
            assert_eq!(nvidia_path(presence, false, true), NvidiaPath::Nvml);
            assert_eq!(nvidia_path(presence, true, true), NvidiaPath::Cli);
            assert_eq!(nvidia_path(presence, false, false), NvidiaPath::Cli);
            assert_eq!(nvidia_path(presence, true, false), NvidiaPath::Cli);
        }
    }

    /// Goal: an nvidia-smi GPU count past `u8::MAX` saturates instead of panicking or wrapping.
    /// Method: convert counts at zero, the `u8` limit, and one past it.
    #[test]
    fn nvidia_smi_gpu_count_saturates() {
        assert_eq!(nvidia_smi_gpu_count_u8(0), 0);
        assert_eq!(nvidia_smi_gpu_count_u8(255), 255);
        assert_eq!(nvidia_smi_gpu_count_u8(256), u8::MAX);
    }
}

#[cfg(test)]
mod permit_timeout_tests {
    use super::*;

    #[test]
    fn read_permit_timeout_matches_legacy_at_min_poll_rate() {
        // Regression: at poll_rate = 0.5 s the formula must reproduce
        // the previous hard-coded 350 ms value.
        assert_eq!(
            device_read_permit_timeout_for(0.5),
            Duration::from_millis(350)
        );
    }

    #[test]
    fn read_permit_timeout_scales_with_poll_rate() {
        // The budget must widen proportionally for slower polls.
        assert_eq!(
            device_read_permit_timeout_for(1.0),
            Duration::from_millis(700)
        );
        assert_eq!(
            device_read_permit_timeout_for(5.0),
            Duration::from_millis(3500)
        );
    }

    #[test]
    fn write_permit_timeout_matches_legacy_at_default_poll_rate() {
        // Regression: at the default poll_rate = 1.0 s the formula
        // must reproduce the previous hard-coded 8 s value.
        assert_eq!(device_write_permit_timeout_for(1.0), Duration::from_secs(8));
    }

    #[test]
    fn write_permit_timeout_scales_with_poll_rate() {
        // The write timeout must track the failsafe wall time
        // exactly, i.e. MISSING_STATUS_THRESHOLD * poll_rate.
        assert_eq!(device_write_permit_timeout_for(0.5), Duration::from_secs(4));
        assert_eq!(
            device_write_permit_timeout_for(5.0),
            Duration::from_secs(40)
        );
    }
}

#[cfg(all(test, feature = "gated-tests"))]
mod verdict_publish_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::{ChannelInfo, ChannelKind, Device, DeviceInfo, SpeedOptions};
    use crate::diagnostics::support::ChannelVerdict;
    use serial_test::serial;
    use std::cell::RefCell;

    /// Goal: prove this repository's publish path reaches the registry the
    /// health snapshot is served from. Its channels carry no sysfs evidence, so
    /// `fixed_enabled` is the only fact behind the verdict and a broken hand-off
    /// would silently show nothing rather than fail. Method: attach a
    /// controller, publish one undrivable channel, read the partition back.
    #[test]
    #[serial]
    fn publishes_drivability_for_gpu_channels() {
        cc_fs::test_runtime(async {
            let hardware_support = Rc::new(HardwareSupportController::init(None, true).await);
            let mut repo = GpuRepo::new(Rc::new(Config::init_default_config().unwrap()), false)
                .with_hardware_support(Rc::clone(&hardware_support));
            let mut info = DeviceInfo::default();
            info.channels.insert(
                "fan1".to_string(),
                ChannelInfo {
                    label: None,
                    kind: ChannelKind::Speed(SpeedOptions {
                        fixed_enabled: false,
                        ..Default::default()
                    }),
                },
            );
            let device = Device::new(
                "Test GPU".to_string(),
                DeviceType::GPU,
                1,
                None,
                info,
                None,
                1.0,
            );
            let uid = device.uid.clone();
            repo.devices.insert(uid, Rc::new(RefCell::new(device)));

            repo.publish_channel_drivability();

            let (permanent, current) = hardware_support.partitioned_verdicts();
            assert!(current.is_empty(), "unexpected current-state verdicts");
            assert_eq!(permanent.len(), 1, "expected one published verdict");
            assert_eq!(permanent[0].channel_name, "fan1");
            assert_eq!(permanent[0].verdict, ChannelVerdict::NotSupportedByDriver);
        });
    }
}

#[cfg(test)]
mod nvml_status_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::{ChannelStatus, Device, DeviceInfo, Status, TempStatus};
    use crate::repositories::failsafe::{
        MISSING_DUTY_FAILSAFE, MISSING_RPM_FAILSAFE, MISSING_TEMP_FAILSAFE,
    };
    use crate::repositories::gpu::nvidia::{NvidiaDeviceInfo, StatusNvidiaDeviceSMI};
    use serial_test::serial;
    use std::cell::RefCell;

    /// Goal: a GPU whose NVML read gets no answer keeps reporting its last-known-good
    /// values instead of an empty status. Method: no NVML worker serves the GPU, so its read
    /// finds no one to answer, as with a wedged driver.
    #[test]
    #[serial]
    fn an_unanswered_nvml_read_keeps_the_last_values() {
        cc_fs::test_runtime(async {
            let mut repo = GpuRepo::new(Rc::new(Config::init_default_config().unwrap()), false);
            let device = Device::new(
                "Test GPU".to_string(),
                DeviceType::GPU,
                1,
                None,
                DeviceInfo::default(),
                None,
                1.0,
            );
            let uid = device.uid.clone();
            repo.devices
                .insert(uid.clone(), Rc::new(RefCell::new(device)));
            repo.gpus_nvidia
                .nvidia_device_infos
                .insert(uid, Rc::new(NvidiaDeviceInfo::for_test(0)));
            let last_good = vec![TempStatus {
                name: "GPU Temp".to_string(),
                temp: 60.0,
            }];
            repo.gpus_nvidia
                .nvidia_preloaded_statuses
                .borrow_mut()
                .insert(
                    1,
                    StatusNvidiaDeviceSMI {
                        temps: last_good.clone(),
                        ..Default::default()
                    },
                );
            let repo = Rc::new(repo);

            repo.load_nvml_status().await;

            let cache = repo.gpus_nvidia.nvidia_preloaded_statuses.borrow();
            assert_eq!(cache.get(&1).unwrap().temps, last_good);
        });
    }

    /// Goal: a GPU whose NVML reads stay unanswered holds its last values only up to the
    /// threshold, then reports its failsafe values and is listed as failsafing. Method: no
    /// NVML worker serves the GPU; run threshold preloads, check the hold, then one more.
    #[test]
    #[serial]
    fn an_nvml_gpu_unanswered_past_the_threshold_fails_safe() {
        cc_fs::test_runtime(async {
            let mut repo = GpuRepo::new(Rc::new(Config::init_default_config().unwrap()), false);
            let device = Device::new(
                "Test GPU".to_string(),
                DeviceType::GPU,
                1,
                None,
                DeviceInfo::default(),
                None,
                1.0,
            );
            let uid = device.uid.clone();
            let device = Rc::new(RefCell::new(device));
            repo.devices.insert(uid.clone(), Rc::clone(&device));
            repo.gpus_nvidia
                .nvidia_device_infos
                .insert(uid, Rc::new(NvidiaDeviceInfo::for_test(0)));
            repo.gpus_nvidia
                .insert_device_for_test(1, Rc::clone(&device));
            let status = Status {
                temps: vec![TempStatus {
                    name: GPU_TEMP_NAME.to_string(),
                    temp: 60.0,
                }],
                channels: vec![ChannelStatus {
                    name: "fan1".to_string(),
                    rpm: Some(1200),
                    duty: Some(40.0),
                    ..Default::default()
                }],
                ..Default::default()
            };
            repo.gpus_nvidia.seed_nvml_failsafe(1, &status);
            repo.gpus_nvidia
                .nvidia_preloaded_statuses
                .borrow_mut()
                .insert(
                    1,
                    StatusNvidiaDeviceSMI {
                        temps: status.temps.clone(),
                        channels: status.channels.clone(),
                        ..Default::default()
                    },
                );
            let repo = Rc::new(repo);

            for _ in 0..MISSING_STATUS_THRESHOLD {
                repo.load_nvml_status().await;
            }
            repo.update_statuses(Local::now()).unwrap();
            assert_eq!(device.borrow().latest_temp(GPU_TEMP_NAME), Some(60.0));
            assert!(repo.failsafing().is_empty());

            repo.load_nvml_status().await;
            repo.update_statuses(Local::now()).unwrap();
            let device = device.borrow();
            assert_eq!(
                device.latest_temp(GPU_TEMP_NAME),
                Some(MISSING_TEMP_FAILSAFE)
            );
            let fan = device.latest_channel("fan1").unwrap();
            assert_eq!(fan.duty, Some(MISSING_DUTY_FAILSAFE));
            assert_eq!(fan.rpm, Some(MISSING_RPM_FAILSAFE));
            assert_eq!(repo.failsafing().len(), 2);
        });
    }
}

#[cfg(test)]
mod amd_taken_channel_tests {
    use super::*;
    use crate::cc_fs;
    use crate::device::{ChannelInfo, Device, DeviceInfo};
    use crate::repositories::gpu::amd::AMDDriverInfo;
    use crate::repositories::hwmon::device_io::DeviceIo;
    use crate::repositories::hwmon::hwmon_repo::{
        HwmonChannelCapabilities, HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
    };
    use serial_test::serial;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    const FAN: &str = "fan1";

    fn repo_with(driver: AMDDriverInfo) -> (GpuRepo, UID) {
        let mut repo = GpuRepo::new(Rc::new(Config::init_default_config().unwrap()), false);
        let mut info = DeviceInfo::default();
        info.channels
            .insert(FAN.to_string(), ChannelInfo::default());
        let device = Device::new(
            "Test AMD GPU".to_string(),
            DeviceType::GPU,
            1,
            None,
            info,
            None,
            1.0,
        );
        let uid = device.uid.clone();
        repo.gpus_amd
            .amd_devices
            .insert(uid.clone(), Rc::new(RefCell::new(device)));
        repo.gpus_amd
            .amd_driver_infos
            .insert(uid.clone(), Rc::new(driver));
        repo.device_permits
            .insert(uid.clone(), Semaphore::const_new(1));
        (repo, uid)
    }

    fn pmfw_card(io: DeviceIo, fan_curve_path: PathBuf, changeable: bool) -> AMDDriverInfo {
        let hwmon = HwmonDriverInfo {
            name: "amdgpu".to_string(),
            io,
            ..Default::default()
        };
        AMDDriverInfo::for_test_with_fan_curve(hwmon, fan_curve_path, changeable)
    }

    async fn pwm_enable_at(dir: &Path) -> String {
        let value = cc_fs::read_sysfs(dir.join("pwm1_enable")).await.unwrap();
        value.trim().to_owned()
    }

    /// A pre-RDNA3 card whose fan sits in `dir`, with an automatic startup default.
    async fn pre_rdna3_card(dir: &Path, pwm_enable: &[u8]) -> AMDDriverInfo {
        cc_fs::write(dir.join("pwm1_enable"), pwm_enable.to_vec())
            .await
            .unwrap();
        cc_fs::write(dir.join("pwm1"), b"128".to_vec())
            .await
            .unwrap();
        AMDDriverInfo::for_test(HwmonDriverInfo {
            name: "amdgpu".to_string(),
            path: dir.to_path_buf(),
            channels: vec![HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Fan,
                number: 1,
                name: FAN.to_string(),
                pwm_enable_default: Some(2),
                caps: HwmonChannelCapabilities::FAN_WRITABLE | HwmonChannelCapabilities::PWM,
                pwm_path: Some(dir.join("pwm1")),
                ..Default::default()
            }],
            ..Default::default()
        })
    }

    /// Goal: the issue's two cases on RDNA3/4. A restart and a saved Unmanaged setting must not
    /// write to a fan the daemon never drove, since `r` wipes what another tool set. Method: a
    /// recording device through a boot-time reset and a shutdown.
    #[test]
    fn a_pmfw_fan_it_never_took_gets_no_writes() {
        cc_fs::test_runtime(async {
            let (io, mut recorder) = DeviceIo::recording_for_test();
            let (repo, uid) = repo_with(pmfw_card(io, PathBuf::from("/fake/fan_curve"), true));

            let reset = recorder.serve(repo.apply_setting_reset(&uid, FAN)).await;
            let shutdown = recorder.serve(repo.shutdown()).await;

            assert!(reset.is_ok(), "{reset:?}");
            assert!(shutdown.is_ok(), "{shutdown:?}");
            assert_eq!(recorder.writes(), []);
        });
    }

    /// Goal: a Mode moving a driven fan to Unmanaged hands it back once, and neither reapplying
    /// Unmanaged nor shutdown writes again. Method: drive the fan on a recording device, reset
    /// twice, shut down, and count the writes after the first reset.
    #[test]
    fn a_pmfw_fan_is_reset_once_and_then_left_alone() {
        cc_fs::test_runtime(async {
            let (io, mut recorder) = DeviceIo::recording_for_test();
            let curve = PathBuf::from("/fake/fan_curve");
            let (repo, uid) = repo_with(pmfw_card(io, curve.clone(), true));
            recorder
                .serve(repo.apply_setting_speed_fixed(&uid, FAN, 50))
                .await
                .unwrap();
            let writes_driving = recorder.writes().len();

            recorder
                .serve(repo.apply_setting_reset(&uid, FAN))
                .await
                .unwrap();
            let writes_after_reset = recorder.writes().len();
            recorder
                .serve(repo.apply_setting_reset(&uid, FAN))
                .await
                .unwrap();
            recorder.serve(repo.shutdown()).await.unwrap();

            assert!(writes_driving > 0);
            assert_eq!(writes_after_reset, writes_driving + 1);
            assert_eq!(recorder.writes().len(), writes_after_reset);
            assert_eq!(recorder.writes().last(), Some(&(curve, "r\n".to_owned())));
        });
    }

    /// Goal: shutdown hands back a fan still under control. Method: drive it, then shut down.
    #[test]
    fn shutdown_returns_a_pmfw_fan_under_control() {
        cc_fs::test_runtime(async {
            let (io, mut recorder) = DeviceIo::recording_for_test();
            let curve = PathBuf::from("/fake/fan_curve");
            let (repo, uid) = repo_with(pmfw_card(io, curve.clone(), true));
            recorder
                .serve(repo.apply_setting_speed_fixed(&uid, FAN, 50))
                .await
                .unwrap();

            recorder.serve(repo.shutdown()).await.unwrap();

            assert_eq!(recorder.writes().last(), Some(&(curve, "r\n".to_owned())));
        });
    }

    /// Goal: a firmware curve takes the fan like a duty does. Method: apply one to a recording
    /// device, then reset it.
    #[test]
    fn a_pmfw_hardware_curve_takes_the_fan() {
        cc_fs::test_runtime(async {
            let (io, mut recorder) = DeviceIo::recording_for_test();
            let curve = PathBuf::from("/fake/fan_curve");
            let (repo, uid) = repo_with(pmfw_card(io, curve.clone(), true));
            let temp_source = TempSource {
                device_uid: uid.clone(),
                temp_name: TEMP_FOR_FAN_CURVE.to_string(),
            };
            let profile = [(30.0, 20), (50.0, 40), (60.0, 60), (70.0, 80), (80.0, 100)];
            recorder
                .serve(repo.apply_setting_speed_profile(&uid, FAN, &temp_source, &profile))
                .await
                .unwrap();

            recorder
                .serve(repo.apply_setting_reset(&uid, FAN))
                .await
                .unwrap();

            assert_eq!(recorder.writes().last(), Some(&(curve, "r\n".to_owned())));
        });
    }

    /// Goal: a write that fails part way still takes the fan, and a reset that fails leaves it
    /// taken, so shutdown tries the hand-back again. Method: a fan curve path that is a
    /// directory, which no write can land on, then a real file for shutdown.
    #[test]
    #[serial]
    fn a_failed_pmfw_reset_is_retried_at_shutdown() {
        cc_fs::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let curve = tmp.path().join("fan_curve");
            cc_fs::create_dir_all(&curve).await.unwrap();
            let card = pmfw_card(DeviceIo::default(), curve.clone(), true);
            let (repo, uid) = repo_with(card);

            let duty = repo.apply_setting_speed_fixed(&uid, FAN, 50).await;
            let reset = repo.apply_setting_reset(&uid, FAN).await;

            cc_fs::remove_dir_all(&curve).await.unwrap();
            cc_fs::write(&curve, b"OD_FAN_CURVE:\n".to_vec())
                .await
                .unwrap();
            let shutdown = repo.shutdown().await;

            assert!(duty.is_err());
            assert!(reset.is_err());
            assert!(shutdown.is_ok(), "{shutdown:?}");
            let handed_back = cc_fs::read_sysfs(&curve).await.unwrap();
            assert_eq!(handed_back.trim(), "r");
        });
    }

    /// Goal: a Zero RPM restore that fails keeps the card taken even when the curve restore lands,
    /// or firmware keeps the stop temperature the daemon set. Method: for each Zero RPM file, make
    /// it a directory, which no write can land on, reset, then make it a real file for shutdown.
    #[test]
    #[serial]
    fn a_failed_zero_rpm_reset_is_retried_at_shutdown() {
        cc_fs::test_runtime(async {
            for failing_file in ["zero_rpm", "zero_rpm_stop_temp"] {
                let tmp = tempfile::tempdir().unwrap();
                let curve = tmp.path().join("fan_curve");
                let zero_rpm = tmp.path().join("zero_rpm");
                let stop_temp = tmp.path().join("zero_rpm_stop_temp");
                let failing = tmp.path().join(failing_file);
                for path in [&curve, &zero_rpm, &stop_temp] {
                    cc_fs::write(path, Vec::new()).await.unwrap();
                }
                cc_fs::remove_file(&failing).await.unwrap();
                cc_fs::create_dir_all(&failing).await.unwrap();
                let hwmon = HwmonDriverInfo {
                    name: "amdgpu".to_string(),
                    ..Default::default()
                };
                let (repo, uid) = repo_with(AMDDriverInfo::for_test_with_zero_rpm(
                    hwmon,
                    curve.clone(),
                    zero_rpm,
                    stop_temp,
                ));
                repo.apply_setting_speed_fixed(&uid, FAN, 50).await.unwrap();

                let reset = repo.apply_setting_reset(&uid, FAN).await;

                assert!(reset.is_err(), "{failing_file}");
                assert!(repo.is_channel_taken(&uid, FAN), "{failing_file}");
                let curve_after_reset = cc_fs::read_sysfs(&curve).await.unwrap();
                assert_eq!(curve_after_reset.trim(), "r", "{failing_file}");

                cc_fs::remove_dir_all(&failing).await.unwrap();
                cc_fs::write(&failing, Vec::new()).await.unwrap();
                repo.shutdown().await.unwrap();
                let handed_back = cc_fs::read_sysfs(&failing).await.unwrap();
                assert_eq!(handed_back.trim(), "r", "{failing_file}");
            }
        });
    }

    /// Goal: a saved Unmanaged setting on a card with its fan control off stops failing at boot,
    /// where the error would clear the active Mode. Method: a card whose curve is not changeable,
    /// with no write allowed to reach the device.
    #[test]
    fn a_card_without_fan_control_resets_without_error() {
        cc_fs::test_runtime(async {
            let (io, mut recorder) = DeviceIo::recording_for_test();
            let (repo, uid) = repo_with(pmfw_card(io, PathBuf::from("/fake/fan_curve"), false));

            let refused = recorder
                .serve(repo.apply_setting_speed_fixed(&uid, FAN, 50))
                .await;
            let reset = recorder.serve(repo.apply_setting_reset(&uid, FAN)).await;

            assert!(refused.is_err());
            assert!(reset.is_ok(), "{reset:?}");
            assert_eq!(recorder.writes(), []);
        });
    }

    /// Goal: the hwmon path on older cards leaves an untaken fan as another tool set it. Method:
    /// a fan held in manual with an automatic startup default, through a reset and a shutdown.
    #[test]
    #[serial]
    fn a_pre_rdna3_fan_it_never_took_is_left_alone() {
        cc_fs::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let (repo, uid) = repo_with(pre_rdna3_card(tmp.path(), b"1").await);

            let reset = repo.apply_setting_reset(&uid, FAN).await;
            let shutdown = repo.shutdown().await;

            assert!(reset.is_ok(), "{reset:?}");
            assert!(shutdown.is_ok(), "{shutdown:?}");
            assert_eq!(pwm_enable_at(tmp.path()).await, "1");
        });
    }

    /// Goal: the hwmon path on older cards hands a driven fan back once. Method: drive it, reset
    /// it, play another tool, then reset again and shut down.
    #[test]
    #[serial]
    fn a_pre_rdna3_fan_is_reset_once_and_then_left_alone() {
        cc_fs::test_runtime(async {
            let tmp = tempfile::tempdir().unwrap();
            let (repo, uid) = repo_with(pre_rdna3_card(tmp.path(), b"2").await);
            repo.apply_setting_speed_fixed(&uid, FAN, 50).await.unwrap();
            let while_taken = pwm_enable_at(tmp.path()).await;

            repo.apply_setting_reset(&uid, FAN).await.unwrap();
            let after_reset = pwm_enable_at(tmp.path()).await;
            cc_fs::write(tmp.path().join("pwm1_enable"), b"1".to_vec())
                .await
                .unwrap();
            repo.apply_setting_reset(&uid, FAN).await.unwrap();
            repo.shutdown().await.unwrap();

            assert_eq!(while_taken, "1");
            assert_eq!(after_reset, "2");
            assert_eq!(pwm_enable_at(tmp.path()).await, "1");
        });
    }
}

#[cfg(test)]
mod channel_attributes_tests {
    use super::*;
    use crate::cc_fs;
    use crate::repositories::gpu::amd::AMDDriverInfo;
    use crate::repositories::hwmon::hwmon_repo::{
        HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
    };
    use serial_test::serial;
    use std::path::Path;

    /// amdgpu junction limits as reported on real hardware, hysteresis unset.
    async fn seeded_dir() -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().to_path_buf();
        for (name, contents) in [
            ("temp2_crit", "110000"),
            ("temp2_crit_hyst", "-273150"),
            ("temp2_emergency", "115000"),
        ] {
            cc_fs::write(base.join(name), contents.as_bytes().to_vec())
                .await
                .unwrap();
        }
        tmp
    }

    fn repo_with_amd_gpu(base: &Path, uid: &UID) -> GpuRepo {
        let mut repo = GpuRepo::new(Rc::new(Config::init_default_config().unwrap()), false);
        let hwmon = HwmonDriverInfo {
            name: "amdgpu".to_string(),
            path: base.to_path_buf(),
            channels: vec![HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Temp,
                number: 2,
                name: "temp2".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        repo.gpus_amd
            .amd_driver_infos
            .insert(uid.clone(), Rc::new(AMDDriverInfo::for_test(hwmon)));
        repo.device_permits
            .insert(uid.clone(), Semaphore::const_new(1));
        repo
    }

    #[test]
    #[serial]
    fn amd_limits_are_read_and_nvidia_reports_none() {
        // Goal: an AMD GPU temp reports its hwmon limits without the unset hysteresis, and a
        // device that is not an AMD GPU (NVIDIA) reports none. Method: one AMD card registered
        // by hand, then a query for an unregistered uid.
        cc_fs::test_runtime(async {
            let tmp = seeded_dir().await;
            let base = tmp.path().to_path_buf();
            let uid = "amd-gpu".to_string();
            let repo = repo_with_amd_gpu(&base, &uid);

            let temp2 = repo.channel_attributes(&uid, "temp2").await.unwrap();
            let other = repo
                .channel_attributes(&"nvidia-gpu".to_string(), "GPU Temp")
                .await
                .unwrap();

            let names: Vec<&str> = temp2.iter().map(|a| a.name.as_str()).collect();
            assert_eq!(names, ["temp2_crit", "temp2_emergency"]);
            assert_eq!(temp2[1].value, 115.0);
            assert!(other.is_empty());
        });
    }

    #[test]
    #[serial]
    fn held_amd_permit_times_out_with_an_error() {
        // Goal: a busy AMD GPU fails the request after the permit timeout instead of blocking
        // the API actor. Method: hold its permit with a shortened read permit timeout.
        cc_fs::test_runtime(async {
            let tmp = seeded_dir().await;
            let base = tmp.path().to_path_buf();
            let uid = "amd-gpu".to_string();
            let mut repo = repo_with_amd_gpu(&base, &uid);
            repo.device_read_permit_timeout = Duration::from_millis(100);
            let _holder = repo
                .device_permits
                .get(&uid)
                .unwrap()
                .try_acquire()
                .expect("permit must start free");

            let result = repo.channel_attributes(&uid, "temp2").await;

            let err = result.expect_err("held permit must time out");
            assert!(err.to_string().contains("TIMEOUT"), "{err}");
        });
    }
}
