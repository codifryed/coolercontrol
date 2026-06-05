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

use std::collections::HashMap;
use std::env;
use std::ops::Not;
use std::rc::Rc;
use std::time::Duration;

use crate::rt;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use log::{debug, error, info, trace, warn};
use moro_local::Scope;
use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};
use tokio::sync::{Semaphore, SemaphorePermit};
use tokio::time::Instant;

use crate::config::Config;
use crate::device::{DeviceType, UID};
use crate::repositories::failsafe::MISSING_STATUS_THRESHOLD;
use crate::repositories::gpu::amd::{GpuAMD, TEMP_FOR_FAN_CURVE};
use crate::repositories::gpu::nvidia::{GpuNVidia, StatusNvidiaDeviceSMI};
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
/// the failsafe layer. Also reused by `amd::drm_ioctl_timeout_for` so the
/// libdrm sensor budget tracks the same poll-rate ratio.
pub const READ_PERMIT_RATIO: f64 = 0.7;

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
        let poll_rate = config.get_settings().map_or(1.0, |s| s.poll_rate);
        let device_read_permit_timeout = device_read_permit_timeout_for(poll_rate);
        let device_write_permit_timeout = device_write_permit_timeout_for(poll_rate);
        Self {
            gpus_nvidia: GpuNVidia::new(Rc::clone(&config)),
            gpus_amd: GpuAMD::new(Rc::clone(&config)),
            config,
            devices: HashMap::new(),
            gpu_type_count: HashMap::new(),
            nvml_active: false,
            force_nvidia_cli: nvidia_cli,
            device_delays: HashMap::new(),
            device_permits: HashMap::new(),
            device_read_permit_timeout,
            device_write_permit_timeout,
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

    #[allow(clippy::cast_possible_truncation)]
    async fn detect_gpu_types(&mut self) {
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
        let nvidia_dev_count = if self.force_nvidia_cli || nvml_enabled.not() {
            self.gpus_nvidia
                .get_nvidia_smi_status(COMMAND_TIMEOUT_FIRST_TRY)
                .await
                .len() as u8
        } else if let Some(num_nvml_devices) = self.gpus_nvidia.init_nvml_devices() {
            self.nvml_active = true;
            num_nvml_devices
        } else {
            self.gpus_nvidia
                .get_nvidia_smi_status(COMMAND_TIMEOUT_FIRST_TRY)
                .await
                .len() as u8
        };
        self.gpu_type_count
            .insert(GpuType::Nvidia, nvidia_dev_count);
        self.gpu_type_count
            .insert(GpuType::AMD, self.gpus_amd.init_devices().await.len() as u8);
    }

    pub fn load_amd_statuses<'s>(self: &'s Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        let read_permit_timeout = self.device_read_permit_timeout;
        for (uid, amd_driver) in &self.gpus_amd.amd_driver_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.borrow().type_index;
                let delay = self.device_delay(uid);
                scope.spawn(async move {
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
                            let statuses = self.gpus_amd.get_amd_status(amd_driver).await;
                            self.gpus_amd
                                .amd_preloaded_statuses
                                .borrow_mut()
                                .insert(type_index, statuses);
                            apply_device_command_delay(delay).await;
                            drop(device_permit);
                        },
                    }
                });
            }
        }
    }

    fn load_nvml_status<'s>(self: &'s Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        for (uid, nv_info) in &self.gpus_nvidia.nvidia_device_infos {
            if let Some(device_lock) = self.devices.get(uid) {
                let type_index = device_lock.borrow().type_index;
                let delay = self.device_delay(uid);
                scope.spawn(async move {
                    let nvml_status = self.gpus_nvidia.request_nvml_status(nv_info);
                    self.gpus_nvidia
                        .nvidia_preloaded_statuses
                        .borrow_mut()
                        .insert(
                            type_index,
                            StatusNvidiaDeviceSMI {
                                temps: nvml_status.temps,
                                channels: nvml_status.channels,
                                ..Default::default()
                            },
                        );
                    apply_device_command_delay(delay).await;
                });
            }
        }
    }

    fn load_nvidia_smi_status<'s>(self: Rc<Self>, scope: &'s Scope<'s, 's, ()>) {
        scope.spawn(async move {
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
        });
    }
}

#[async_trait(?Send)]
impl Repository for GpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::GPU
    }

    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.detect_gpu_types().await;
        let amd_devices = self.gpus_amd.initialize_amd_devices().await?;
        for uid in amd_devices.keys() {
            self.device_permits
                .insert(uid.clone(), Semaphore::const_new(1));
        }
        self.devices.extend(amd_devices);
        let has_nvidia_devices = self.gpu_type_count.get(&GpuType::Nvidia).unwrap_or(&0) > &0;
        if has_nvidia_devices {
            let starting_nvidia_index = self.gpu_type_count.get(&GpuType::AMD).unwrap_or(&0) + 1;
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
            let device_map: HashMap<_, _> = init_devices
                .iter()
                .map(|d| {
                    (
                        d.1.name.clone(),
                        HashMap::from([
                            (
                                "driver name",
                                vec![d.1.info.driver_info.name.clone().unwrap_or_default()],
                            ),
                            (
                                "driver version",
                                vec![d.1.info.driver_info.version.clone().unwrap_or_default()],
                            ),
                            ("locations", d.1.info.driver_info.locations.clone()),
                            ("channels", {
                                let mut ch: Vec<_> = d.1.info.channels.keys().cloned().collect();
                                ch.sort();
                                ch
                            }),
                            ("temps", {
                                let mut t: Vec<_> = d.1.info.temps.keys().cloned().collect();
                                t.sort();
                                t
                            }),
                        ]),
                    )
                })
                .collect();
            info!(
                "Initialized GPU Devices: {}",
                serde_json::to_string(&device_map).unwrap_or_default()
            );
        }
        trace!(
            "Time taken to initialize all GPU devices: {:?}",
            start_initialization.elapsed()
        );
        self.load_device_delays();
        debug!("GPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices.values().cloned().collect()
    }

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        let self_c = Rc::clone(&self);
        moro_local::async_scope!(|scope| {
            if self.devices.is_empty().not() {
                self_c.load_amd_statuses(scope);
                if self.nvml_active {
                    self_c.load_nvml_status(scope);
                } else {
                    self.load_nvidia_smi_status(scope);
                }
            }
        })
        .await;
        trace!(
            "STATUS PRELOAD Time taken for all GPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        self.gpus_amd.update_all_statuses();
        self.gpus_nvidia.update_all_statuses();
        Ok(())
    }

    async fn shutdown(&self) -> Result<()> {
        for (uid, device_lock) in &self.gpus_amd.amd_devices {
            let channel_names: Vec<String> =
                device_lock.borrow().info.channels.keys().cloned().collect();
            for channel_name in &channel_names {
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
        self.gpus_nvidia.reset_devices().await;
        info!("GPU Repository shutdown");
        Ok(())
    }

    async fn apply_setting_reset(&self, device_uid: &UID, channel_name: &str) -> Result<()> {
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
            "Applying GPU device: {device_uid} channel: {channel_name}; Fixed Speed: {speed_fixed}"
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
            "Applying GPU device: {device_uid} channel: {channel_name}; Speed Profile: {speed_profile:?}"
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
