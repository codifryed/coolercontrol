// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later
use chrono::{DateTime, Local};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::default::Default;
use std::ops::{Add, Not, RangeInclusive};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use crate::repositories::utils::find_xauthority_path;
use crate::rt;
use anyhow::{anyhow, Context, Result};
use futures_util::future::join_all;
use log::{debug, error, info, trace, warn};
use nvml_wrapper::enum_wrappers::device::{Clock, TemperatureSensor};
use nvml_wrapper::enums::device::{DeviceArchitecture, SampleValue};
use nvml_wrapper::error::NvmlError;
use nvml_wrapper::structs::device::FieldId;
use nvml_wrapper::sys_exports::field_id;
use nvml_wrapper::Nvml;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use tokio::sync::OnceCell;

use crate::config::Config;
use crate::device::health::FailsafeRef;
use crate::device::{
    ChannelInfo, ChannelKind, ChannelName, ChannelSample, ChannelStatus, Device, DeviceInfo,
    DeviceType, DriverInfo, DriverType, Duty, SpeedOptions, Status, StatusRefill, Temp, TempInfo,
    TempStatus, TypeIndex, UID,
};
use crate::repositories::failsafe::{self, FailsafeStatusData, FailureLogAction};
use crate::repositories::gpu::gpu_repo::{
    COMMAND_TIMEOUT_DEFAULT, COMMAND_TIMEOUT_FIRST_TRY, GPU_LOAD_NAME, GPU_POWER_NAME,
    GPU_TEMP_NAME,
};
use crate::repositories::gpu::nvapi_io::{self, NvApiIo};
use crate::repositories::gpu::nvml_io::{self, NvmlIo};
use crate::repositories::hwmon::device_io;
use crate::repositories::repository::DeviceLock;
use crate::repositories::utils::ShellCommand;
use crate::repositories::utils::ShellCommandResult::{Error, Success};

// Only way this works for our current implementation.
// See: https://github.com/Cldfire/nvml-wrapper/issues/21
static NVML: OnceCell<Nvml> = OnceCell::const_new();
const GPU_TEMP_MEMORY_NAME: &str = "GPU Temp Memory";
const GPU_TEMP_HOTSPOT_NAME: &str = "GPU Temp Hotspot";

// synonymous with amd hwmon fan names:
const NVIDIA_FAN_NAME: &str = "fan1";
const NVIDIA_FAN_PREFIX: &str = "fan";
const NVIDIA_FREQ_PREFIX: &str = "GPU Freq";
const NVIDIA_CLOCK_GRAPHICS: &str = "freq_graphics";
const NVIDIA_CLOCK_SM: &str = "freq_sm";
const NVIDIA_CLOCK_MEMORY: &str = "freq_memory";
const NVIDIA_CLOCK_VIDEO: &str = "freq_video";
const PATTERN_GPU_INDEX: &str = r"\[gpu:(?P<index>\d+)\]";
const PATTERN_FAN_INDEX: &str = r"\[fan:(?P<index>\d+)\]";
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
    /// Per NVML GPU, the values written once its reads go unanswered past
    /// `MISSING_STATUS_THRESHOLD`. Seeded from the status read at init.
    nvml_failsafe_statuses: RefCell<HashMap<TypeIndex, FailsafeStatusData>>,
    /// Owned handles, held only until `start_nvml_workers` moves each onto its own thread.
    ///
    /// **Empty once detection finishes.** Never test this to decide whether NVML is in play: it
    /// says "workers not started yet", not "NVML unavailable". `nvml_io` is that answer.
    nvml_handles_pending_workers: HashMap<GpuIndex, PendingNvmlGpu>,
    /// One worker per GPU. Owns the device, so NVML needs no lock and a hung call parks only
    /// that GPU's thread.
    nvml_io: HashMap<GpuIndex, NvmlIo>,
    xauthority_path: RefCell<Option<String>>,
    nvidia_smi_disabled_channels: RefCell<HashMap<GpuIndex, Vec<ChannelName>>>,
    nvidia_nvml_load_enabled: Cell<bool>,
    /// Reads every GPU's hotspot on one thread. None when nvapi is unavailable.
    nvapi_io: Option<NvApiIo>,
}

impl GpuNVidia {
    pub fn new(config: Rc<Config>) -> Self {
        Self {
            config,
            nvidia_devices: HashMap::new(),
            nvidia_device_infos: HashMap::new(),
            nvidia_preloaded_statuses: RefCell::new(HashMap::new()),
            nvml_failsafe_statuses: RefCell::new(HashMap::new()),
            nvml_handles_pending_workers: HashMap::new(),
            nvml_io: HashMap::new(),
            xauthority_path: RefCell::new(None),
            nvidia_smi_disabled_channels: RefCell::new(HashMap::new()),
            nvidia_nvml_load_enabled: Cell::new(false),
            nvapi_io: None,
        }
    }

    pub async fn initialize_nvidia_devices(
        &mut self,
        starting_nvidia_index: GpuIndex,
    ) -> Result<HashMap<UID, DeviceLock>> {
        if self.nvml_handles_pending_workers.is_empty() {
            return self.init_nvidia_smi_devices(starting_nvidia_index).await;
        }
        // Workers first, so every read of a GPU runs on its own thread from here on.
        let poll_rate = self.config.get_settings()?.poll_rate;
        self.start_nvml_workers(starting_nvidia_index, poll_rate);
        let init_readings = self.read_init_readings(NVML_INIT_READ_TIMEOUT).await;
        self.start_nvapi(&init_readings).await;
        let init_hotspots = self.read_init_hotspots(&init_readings).await;
        self.retrieve_nvml_devices(starting_nvidia_index, &init_readings, &init_hotspots)
    }

    /// Each GPU's init readings, asked of its worker, all at once. A GPU that does not answer
    /// within `budget` loses its worker and is unmanaged until the next start.
    async fn read_init_readings(
        &mut self,
        budget: Duration,
    ) -> HashMap<GpuIndex, NvmlInitReadings> {
        let reads = join_all(self.nvml_io.iter().map(|(gpu_index, io)| async move {
            let read = io
                .call_within("init readings", budget, NvmlInitReadings::read)
                .await;
            (*gpu_index, read)
        }))
        .await;
        let mut init_readings = HashMap::with_capacity(reads.len());
        for (gpu_index, read) in reads {
            match read {
                Ok(readings) => {
                    init_readings.insert(gpu_index, readings);
                }
                Err(err) => {
                    let Some(io) = self.nvml_io.remove(&gpu_index) else {
                        continue;
                    };
                    warn!(
                        "NVIDIA GPU {} did not answer its first NVML reads: {err}. It is not \
                         managed until the next start.",
                        io.device_name()
                    );
                }
            }
        }
        debug_assert!(init_readings.len() <= self.nvml_io.len());
        init_readings
    }

    /// Starts nvapi on its own thread, for the hotspot of each NVML GPU.
    async fn start_nvapi(&mut self, init_readings: &HashMap<GpuIndex, NvmlInitReadings>) {
        if init_readings.is_empty() {
            return;
        }
        // The GPUs whose architecture keeps hotspot out of the nvapi thermals array, so nvapi
        // reads it from the aggregated hotspot register instead.
        let register_hotspot_buses = init_readings
            .values()
            .filter(|readings| readings.needs_hotspot_register)
            .filter_map(|readings| readings.pci_bus)
            .collect();
        let reply_timeout = device_io::reply_timeout_from(&self.config);
        self.nvapi_io = NvApiIo::start(register_hotspot_buses, reply_timeout).await;
    }

    /// Each GPU's hotspot at init, by GPU index. Only a GPU found here gets a hotspot channel.
    async fn read_init_hotspots(
        &self,
        init_readings: &HashMap<GpuIndex, NvmlInitReadings>,
    ) -> HashMap<GpuIndex, f64> {
        let Some(nvapi_io) = self.nvapi_io.as_ref() else {
            return HashMap::new();
        };
        let buses: Vec<(GpuIndex, u32)> = init_readings
            .iter()
            .filter_map(|(gpu_index, readings)| Some((*gpu_index, readings.pci_bus?)))
            .collect();
        read_hotspots_within(nvapi_io, &buses, nvapi_io::INIT_TIMEOUT).await
    }

    pub fn update_all_statuses(&self, timestamp: DateTime<Local>) {
        let preloaded_statuses_map = self.nvidia_preloaded_statuses.borrow();
        let failsafes = self.nvml_failsafe_statuses.borrow();
        for (type_index, nv_device_lock) in &self.nvidia_devices {
            let failsafe = failsafes
                .get(type_index)
                .filter(|fsd| fsd.threshold_exceeded());
            if let Some(fsd) = failsafe {
                let mut device = nv_device_lock.borrow_mut();
                trace!("Device: {} using failsafe status", device.name);
                fsd.write_failsafe_status(&mut device.write_status(timestamp));
                continue;
            }
            let Some(nv_status) = preloaded_statuses_map.get(type_index) else {
                error!("There is no status preloaded for this Nvidia device: {type_index}");
                continue;
            };
            trace!(
                "Device: {} status updated: {:?} {:?}",
                nv_status.name,
                nv_status.temps,
                nv_status.channels
            );
            let mut device = nv_device_lock.borrow_mut();
            device
                .write_status(timestamp)
                .write_all(&nv_status.temps, &nv_status.channels);
        }
    }

    /// Records that NVML GPU `type_index` answered its read, ending any failsafe.
    pub fn record_nvml_answer(&self, type_index: TypeIndex) {
        let mut failsafes = self.nvml_failsafe_statuses.borrow_mut();
        let Some(fsd) = failsafes.get_mut(&type_index) else {
            return;
        };
        if fsd.record_success() {
            info!(
                "Recovered from failsafe for NVIDIA GPU: {}. Resuming normal status reads.",
                self.device_name(type_index)
            );
        }
    }

    /// Records that NVML GPU `type_index` did not answer its read. Its last values hold until
    /// more than `MISSING_STATUS_THRESHOLD` reads in a row go unanswered, then its failsafe
    /// values apply.
    pub fn record_nvml_no_answer(&self, type_index: TypeIndex) {
        let mut failsafes = self.nvml_failsafe_statuses.borrow_mut();
        let Some(fsd) = failsafes.get_mut(&type_index) else {
            return;
        };
        if fsd.record_failure_log_action() == FailureLogAction::FailsafeLatch {
            error!(
                "NVIDIA GPU: {} has not answered status reads. Setting failsafe values.",
                self.device_name(type_index)
            );
        }
    }

    pub async fn reset_devices(&self) {
        for device_lock in self.nvidia_devices.values() {
            let device_uid = device_lock.borrow().uid.clone();
            if let Some(nv_info) = self.nvidia_device_infos.get(&device_uid) {
                if self.is_nvml_controlled(nv_info).not() {
                    self.reset_nvidia_settings_to_default(nv_info).await.ok();
                } else {
                    let channel_names: Vec<String> =
                        device_lock.borrow().info.channels.keys().cloned().collect();
                    for channel_name in channel_names {
                        self.reset_nvml_device_to_default(nv_info, &channel_name)
                            .await
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
        if self.is_nvml_controlled(nv_info).not() {
            self.reset_nvidia_settings_to_default(nv_info).await?;
        } else {
            self.reset_nvml_device_to_default(nv_info, channel_name)
                .await?;
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
        if self.is_nvml_controlled(nvidia_gpu_info).not() {
            self.set_nvidia_settings_fan_duty(nvidia_gpu_info, speed_fixed)
                .await
        } else {
            self.set_nvml_fan_duty(nvidia_gpu_info, channel_name, speed_fixed)
                .await
                .map_err(|err| {
                    anyhow!(
                        "Error setting device duty of {speed_fixed}% on Nvidia GPU \
                         #{}:{channel_name} - {err}",
                        nvidia_gpu_info.gpu_index
                    )
                })
        }
    }

    // --------------------------------------------------------------------------------------------
    // NVML
    // --------------------------------------------------------------------------------------------

    /// Loads NVML and takes a handle for each GPU, all on the blocking pool: the library load and
    /// `nvmlInit` can take seconds without persistence mode, and hang on a GPU off the bus.
    #[allow(clippy::cast_possible_truncation)]
    pub async fn init_nvml_devices(&mut self, starting_nvidia_index: GpuIndex) -> NvmlInitResult {
        let Some(probe) = probe_nvml(starting_nvidia_index).await else {
            return NvmlInitResult::Unavailable;
        };
        let NvmlProbe {
            nvml,
            device_count,
            device_uids,
        } = probe;
        // Inspect device identities on the owned handle before committing NVML to the
        // process-lifetime static. If CoolerControl manages no NVIDIA GPU (none detected, or
        // every one disabled), drop the handle so the driver detaches instead of holding the
        // GPU active for the daemon's lifetime. NVML is process-global, hence all-or-nothing.
        if self.should_release_nvml(&device_uids, device_count) {
            if device_count == 0 {
                info!(
                    "No NVIDIA GPUs detected; shutting down NVML so the driver can detach \
                     instead of staying attached for the daemon's lifetime."
                );
            } else {
                info!(
                    "All NVIDIA GPUs are disabled in CoolerControl; shutting down NVML so the \
                     driver releases the GPU instead of holding it active."
                );
            }
            release_nvml(nvml).await;
            return NvmlInitResult::Released;
        }
        if let Err(err) = NVML.set(*nvml) {
            error!("Error setting NVML lib: {err}");
            return NvmlInitResult::Unavailable;
        }
        self.populate_nvml_device_handles(device_count).await;
        if self.nvml_handles_pending_workers.is_empty() {
            warn!("No NVML accessible devices found, falling back to CLI tools");
            return NvmlInitResult::Unavailable;
        }
        NvmlInitResult::Active(self.nvml_handles_pending_workers.len() as u8)
    }

    /// Takes a handle and the name of each GPU, on the blocking pool.
    #[allow(clippy::cast_possible_truncation)]
    async fn populate_nvml_device_handles(&mut self, device_count: u32) {
        debug_assert!(
            NVML.get().is_some(),
            "NVML must be set before populating device handles"
        );
        let enumerate = rt::spawn_blocking(move || {
            let Some(nvml) = NVML.get() else {
                error!("NVML was not set before populating device handles");
                return Vec::new();
            };
            let mut gpus = Vec::with_capacity(device_count as usize);
            for device_index in 0..device_count {
                let Ok(device) = nvml
                    .device_by_index(device_index)
                    // unexpected/not allowed:
                    .inspect_err(|err| error!("Error getting NVML device by index: {err}"))
                else {
                    continue;
                };
                let name = device.name().ok();
                gpus.push((device_index as GpuIndex, PendingNvmlGpu { device, name }));
            }
            gpus
        });
        match rt::timeout(NVML_INIT_TIMEOUT, enumerate).await {
            Ok(Ok(gpus)) => self.nvml_handles_pending_workers.extend(gpus),
            Ok(Err(err)) => error!("Error getting the NVML devices: {err}"),
            Err(_) => warn!(
                "NVML did not hand out its GPUs within {NVML_INIT_TIMEOUT:?}. NVIDIA GPUs fall \
                 back to the CLI tools."
            ),
        }
    }

    /// True when the device with this UID is disabled in config. A config read error is
    /// logged and treated as NOT disabled (keeps the GPU managed) so a transient read
    /// failure never silently releases a device. Unlike `retrieve_nvml_devices`, which is
    /// authoritative and propagates the error, this release pre-check is best-effort.
    fn is_nvml_device_disabled(&self, device_uid: &UID) -> bool {
        match self.config.get_cc_settings_for_device(device_uid) {
            Ok(setting) => setting.is_some_and(|cc_setting| cc_setting.disable),
            Err(err) => {
                warn!("Error reading settings for {device_uid}, treating as enabled: {err}");
                false
            }
        }
    }

    /// True when no NVIDIA GPU will be managed through NVML, so NVML can be shut down to let
    /// the driver detach: either no device was detected, or every enumerated device is
    /// disabled in config. A partial enumeration (a device failed to read) keeps NVML, since
    /// we cannot be sure it is unmanaged. NVML init/shutdown is process-global.
    fn should_release_nvml(&self, device_uids: &[UID], device_count: u32) -> bool {
        debug_assert!(
            device_uids.len() <= device_count as usize,
            "collected more device UIDs than NVML reported devices"
        );
        if device_count == 0 {
            return true;
        }
        if device_uids.len() != device_count as usize {
            return false;
        }
        device_uids
            .iter()
            .all(|device_uid| self.is_nvml_device_disabled(device_uid))
    }

    /// Builds a device for each GPU from its init readings. `init_hotspots` holds the hotspot
    /// nvapi read for each GPU that has one.
    #[allow(clippy::too_many_lines)]
    pub fn retrieve_nvml_devices(
        &mut self,
        starting_nvidia_index: u8,
        init_readings: &HashMap<GpuIndex, NvmlInitReadings>,
        init_hotspots: &HashMap<GpuIndex, f64>,
    ) -> Result<HashMap<UID, DeviceLock>> {
        let mut devices = HashMap::new();
        let poll_rate = self.config.get_settings()?.poll_rate;
        for (gpu_index, readings) in init_readings {
            let type_index = gpu_index + starting_nvidia_index;
            let (name, device_uid) = nvml_name_and_uid(readings.name.clone(), type_index);
            let cc_device_setting = self.config.get_cc_settings_for_device(&device_uid)?;
            if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
                info!("Skipping disabled device: {name} with UID: {device_uid}");
                continue;
            }
            let disabled_channels =
                cc_device_setting.map_or_else(Vec::new, |setting| setting.get_disabled_channels());
            self.nvidia_nvml_load_enabled
                .set(disabled_channels.contains(&GPU_LOAD_NAME.to_string()).not());
            let mut nvidia_temp_infos = Vec::new();
            let mut nvidia_freq_infos = Vec::new();
            let mut nvidia_power_supported = false;
            let mut temp_infos = HashMap::new();
            let mut temp_status = Vec::new();

            let temp_name = GPU_TEMP_NAME.to_string();
            if disabled_channels.contains(&temp_name).not() {
                if let Some(temp) = readings.temperature_celsius {
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
                if let Some(mem_temp) = readings.memory_temperature_celsius {
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
            let hotspot_temp_name = GPU_TEMP_HOTSPOT_NAME.to_string();
            let mut hotspot_pci_bus = None;
            if disabled_channels.contains(&hotspot_temp_name).not() {
                if let (Some(hotspot_temp), Some(pci_bus)) =
                    (init_hotspots.get(gpu_index).copied(), readings.pci_bus)
                {
                    hotspot_pci_bus = Some(pci_bus);
                    temp_infos.insert(
                        hotspot_temp_name.clone(),
                        TempInfo {
                            label: hotspot_temp_name.clone(),
                            number: 3,
                        },
                    );
                    temp_status.push(TempStatus {
                        name: hotspot_temp_name.clone(),
                        temp: hotspot_temp,
                    });
                    nvidia_temp_infos.push(hotspot_temp_name);
                }
            }
            let mut channel_infos = HashMap::new();
            let mut channel_status = Vec::new();
            let mut fan_indices = Vec::new();
            let num_fans = u8::try_from(readings.fan_count)?;
            for fan in &readings.fans {
                debug_assert!(fan.index < num_fans);
                let fan_name = format!("{NVIDIA_FAN_PREFIX}{}", fan.index + 1);
                if disabled_channels.contains(&fan_name).not() {
                    fan_indices.push(fan.index);
                    channel_infos.insert(
                        fan_name.clone(),
                        ChannelInfo {
                            label: Some(fan_name.clone()),
                            kind: ChannelKind::Speed(SpeedOptions {
                                fixed_enabled: true,
                                ..Default::default()
                            }),
                        },
                    );
                    channel_status.push(ChannelStatus {
                        name: fan_name,
                        duty: Some(f64::from(fan.duty_percent)),
                        rpm: fan.rpm,
                        ..Default::default()
                    });
                }
            }
            if self.nvidia_nvml_load_enabled.get() {
                let load_name = GPU_LOAD_NAME.to_string();
                if let Some(gpu_load) = readings.load_percent {
                    channel_infos.insert(
                        load_name.clone(),
                        ChannelInfo {
                            label: Some(load_name.clone()),
                            kind: ChannelKind::InfoOnly,
                        },
                    );
                    channel_status.push(ChannelStatus {
                        name: load_name,
                        duty: Some(f64::from(gpu_load)),
                        ..Default::default()
                    });
                }
            }
            for (clock_type, clock_name, label_suffix) in NVML_CLOCKS {
                if disabled_channels.contains(&clock_name.to_string()) {
                    continue;
                }
                let Some(freq) = readings.clock(clock_type) else {
                    continue;
                };
                channel_infos.insert(
                    clock_name.to_string(),
                    ChannelInfo {
                        label: Some(format!("{NVIDIA_FREQ_PREFIX} {label_suffix}")),
                        kind: ChannelKind::InfoOnly,
                    },
                );
                nvidia_freq_infos.push(clock_type);
                channel_status.push(ChannelStatus {
                    name: clock_name.to_string(),
                    freq: Some(freq),
                    ..Default::default()
                });
            }
            if disabled_channels
                .contains(&GPU_POWER_NAME.to_string())
                .not()
            {
                let power_name = GPU_POWER_NAME.to_string();
                if let Some(milli_watts) = readings.power_milliwatts {
                    channel_infos.insert(
                        power_name.clone(),
                        ChannelInfo {
                            label: Some(power_name.clone()),
                            kind: ChannelKind::InfoOnly,
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

            let driver_name = format!("nvml:{}", readings.nvml_version.clone().unwrap_or_default());
            let driver_locations = readings.pci_bus_id.iter().cloned().collect();
            let mut device_raw = Device::new(
                name,
                DeviceType::GPU,
                type_index,
                None,
                DeviceInfo {
                    temps: temp_infos,
                    temp_max: 150,
                    channels: channel_infos,
                    driver_info: DriverInfo {
                        drv_type: DriverType::NVML,
                        name: Some(driver_name),
                        version: readings.driver_version.clone(),
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
            self.seed_nvml_failsafe(type_index, &status);
            device_raw.initialize_status_history_with(status, poll_rate);
            let uid = device_raw.uid.clone();

            let device = Rc::new(RefCell::new(device_raw));
            self.nvidia_devices.insert(type_index, Rc::clone(&device));
            let fan_range = match readings.fan_duty_percent_range {
                Some((min, max)) => {
                    debug!("Nvidia NVML returned Fan range: {min} - {max}");
                    NvidiaFanRange::new(Duty::try_from(min)?, Duty::try_from(max)?)
                }
                None => NvidiaFanRange::default(),
            };
            let fan_ranges = fan_indices
                .iter()
                .copied()
                .map(|fan_index| (fan_index, fan_range.clone()))
                .collect();
            self.nvidia_device_infos.insert(
                uid.clone(),
                Rc::new(NvidiaDeviceInfo {
                    gpu_index: *gpu_index,
                    display_id: 0,
                    fan_indices,
                    temps: nvidia_temp_infos,
                    freqs: nvidia_freq_infos,
                    power: nvidia_power_supported,
                    fan_ranges,
                    hotspot_pci_bus,
                    read_plan_parts: std::cell::OnceCell::new(),
                }),
            );
            devices.insert(uid, device);
        }
        Ok(devices)
    }

    /// Builds the failsafe values of NVML GPU `type_index` from its init status.
    pub fn seed_nvml_failsafe(&self, type_index: TypeIndex, status: &Status) {
        let (channel_failsafes, temp_failsafes) =
            failsafe::create_failsafe_data(&status.channels, &status.temps);
        if let Some(fsd) = FailsafeStatusData::new(channel_failsafes, temp_failsafes) {
            self.nvml_failsafe_statuses
                .borrow_mut()
                .insert(type_index, fsd);
        }
    }

    /// Whether this GPU is driven through NVML rather than the `nvidia-settings` CLI.
    ///
    /// Per GPU, and read from the worker table: a card whose worker failed to start correctly
    /// falls back on its own rather than dragging every other card with it.
    fn is_nvml_controlled(&self, nv_info: &NvidiaDeviceInfo) -> bool {
        self.nvml_io.contains_key(&nv_info.gpu_index)
    }

    /// This GPU's NVML worker, when it has one.
    #[must_use]
    pub fn nvml_worker(&self, gpu_index: GpuIndex) -> Option<&NvmlIo> {
        self.nvml_io.get(&gpu_index)
    }

    /// Lets every GPU have one more attempt even if it is currently unreachable. Shutdown uses
    /// this: handing fan control back to the firmware is worth a retry on a device that may
    /// well answer now.
    pub fn allow_nvml_probe_now(&self) {
        for io in self.nvml_io.values() {
            io.allow_probe_now();
        }
    }

    /// Hands every NVML device to a thread of its own, before any of its init reads.
    ///
    /// From here on the device is only reachable through its worker, init readings included, so a
    /// GPU that hangs in a read parks only its own thread.
    fn start_nvml_workers(&mut self, starting_nvidia_index: GpuIndex, poll_rate: f64) {
        let reply_timeout = device_io::reply_timeout_for(poll_rate);
        for (gpu_index, pending) in std::mem::take(&mut self.nvml_handles_pending_workers) {
            let PendingNvmlGpu { device, name } = pending;
            let type_index = gpu_index + starting_nvidia_index;
            let (name, _) = nvml_name_and_uid(name, type_index);
            match NvmlIo::spawn(name.clone(), device, reply_timeout) {
                Ok(io) => {
                    self.nvml_io.insert(gpu_index, io);
                }
                Err(err) => nvml_io::log_spawn_failure(&name, &err),
            }
        }
    }

    fn convert_milliwatts_to_watts(milli_watts: u32) -> f64 {
        f64::from(milli_watts / 1_000)
    }

    #[allow(clippy::cast_precision_loss)]
    fn get_memory_temp(nvml_device: &nvml_wrapper::Device) -> Option<Temp> {
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

    /// One GPU's per-tick NVML reads, gathered on a blocking thread in a single hop.
    ///
    /// Batched deliberately: a hop per call would cost a thread round trip per sensor, where one
    /// hop per GPU per tick costs one. Every field is plain data so the whole pass crosses the
    /// thread boundary by value.
    fn read_nvml_readings(
        device: &nvml_wrapper::Device<'static>,
        plan: &NvmlReadPlan,
    ) -> NvmlReadings {
        let mut readings = NvmlReadings::default();
        for temp in plan.temps.iter().copied() {
            match temp {
                NvmlTemp::Gpu => {
                    readings.gpu_temp = device.temperature(TemperatureSensor::Gpu).ok();
                }
                NvmlTemp::Memory => readings.memory_temp = Self::get_memory_temp(device),
            }
        }
        for fan_index in plan.fan_indices.iter().copied() {
            let fan_index_u32 = u32::from(fan_index);
            let Ok(duty) = device.fan_speed(fan_index_u32) else {
                continue;
            };
            readings
                .fans
                .push((fan_index, duty, device.fan_speed_rpm(fan_index_u32).ok()));
        }
        if plan.read_load {
            readings.load = device.utilization_rates().ok().map(|rates| rates.gpu);
        }
        if plan.read_power {
            readings.power_milliwatts = device.power_usage().ok();
        }
        for clock in plan.freqs.iter().copied() {
            if let Ok(frequency) = device.clock_info(clock) {
                readings.freqs.push((clock, frequency));
            }
        }
        readings
    }

    /// Runs one GPU's read pass on its own thread, bounded.
    ///
    /// NVML can block indefinitely when the GPU falls off the bus or hits an Xid error. The call
    /// owns the device on that thread, so a hang parks it alone and the timeout can elapse.
    async fn read_nvml_bounded(
        &self,
        gpu_index: GpuIndex,
        plan: NvmlReadPlan,
    ) -> Option<NvmlReadings> {
        let io = self.nvml_io.get(&gpu_index)?;
        io.call("read", move |device| {
            Self::read_nvml_readings(device, &plan)
        })
        .await
        // Unactionable and recurring while the GPU is wedged, so not a warning. Its readings
        // go stale until the driver answers again.
        .inspect_err(|err| debug!("NVML read for GPU {gpu_index} failed: {err}"))
        .ok()
    }

    /// Runs one NVML fan-control write on the GPU's own thread, bounded.
    async fn write_nvml_bounded<F>(&self, gpu_index: GpuIndex, what: &str, call: F) -> Result<()>
    where
        F: FnOnce(&mut nvml_wrapper::Device<'static>) -> Result<(), NvmlError> + Send + 'static,
    {
        self.nvml_io
            .get(&gpu_index)
            .with_context(|| format!("NVML device {gpu_index} should exist"))?
            .call(what, move |device| call(device))
            .await?
            .map_err(anyhow::Error::from)
    }

    /// One GPU's NVML readings. `None` when the GPU did not answer: the caller keeps its
    /// last-known-good values.
    pub async fn request_nvml_status(
        &self,
        nv_info: &Rc<NvidiaDeviceInfo>,
    ) -> Option<NvmlReadings> {
        let plan = self.read_plan(nv_info);
        self.read_nvml_bounded(nv_info.gpu_index, plan).await
    }

    /// Stores one GPU's answered NVML status, then patches in its hotspot if nvapi answers.
    /// Stored first so a stalled nvapi never holds back this tick's NVML temps and fans. The
    /// last hotspot is carried over until this tick's read settles, so one that answers after
    /// the snapshot still shows a tick late. A failed read removes it.
    pub async fn store_nvml_status(
        &self,
        type_index: TypeIndex,
        nv_info: &NvidiaDeviceInfo,
        readings: &NvmlReadings,
    ) {
        debug_assert!(
            nv_info.hotspot_pci_bus.is_none()
                || nv_info
                    .temps
                    .iter()
                    .any(|name| name == GPU_TEMP_HOTSPOT_NAME),
            "a hotspot bus implies a hotspot channel"
        );
        self.refill_nvml_status(type_index, nv_info, readings);
        self.record_nvml_answer(type_index);
        let hotspot_temp = self.read_hotspot(nv_info).await;
        let mut statuses = self.nvidia_preloaded_statuses.borrow_mut();
        let Some(status) = statuses.get_mut(&type_index) else {
            return;
        };
        match hotspot_temp {
            Some(hotspot_temp) => Self::insert_hotspot(nv_info, &mut status.temps, hotspot_temp),
            None => status
                .temps
                .retain(|temp| temp.name != GPU_TEMP_HOTSPOT_NAME),
        }
    }

    /// Refills the GPU's stored status from its NVML readings, in place, carrying over last
    /// tick's hotspot. A tick reading what the last one did keeps every name.
    fn refill_nvml_status(
        &self,
        type_index: TypeIndex,
        nv_info: &NvidiaDeviceInfo,
        readings: &NvmlReadings,
    ) {
        let mut statuses = self.nvidia_preloaded_statuses.borrow_mut();
        let status = statuses.entry(type_index).or_default();
        let carried_hotspot = status
            .temps
            .iter()
            .find(|temp| temp.name == GPU_TEMP_HOTSPOT_NAME)
            .map(|temp| temp.temp);
        let mut refill = StatusRefill::new(&mut status.channels, &mut status.temps);
        Self::refill_temps(nv_info, readings, carried_hotspot, &mut refill);
        Self::refill_channels(readings, &mut refill);
        drop(refill);
        debug_assert!(status.temps.len() <= nv_info.temps.len());
    }

    /// This tick's hotspot, read through nvapi once NVML has answered. None when the GPU has no
    /// hotspot channel or the read failed or timed out, which omits it for this tick.
    async fn read_hotspot(&self, nv_info: &NvidiaDeviceInfo) -> Option<f64> {
        self.nvapi_io
            .as_ref()?
            .hotspot_temp(nv_info.hotspot_pci_bus?)
            .await
    }

    /// What this tick's read pass gathers. The per-device parts are shared, not copied.
    fn read_plan(&self, nv_info: &NvidiaDeviceInfo) -> NvmlReadPlan {
        let parts = nv_info.read_plan_parts();
        NvmlReadPlan {
            fan_indices: Arc::clone(&parts.fan_indices),
            temps: Arc::clone(&parts.temps),
            read_load: self.nvidia_nvml_load_enabled.get(),
            read_power: nv_info.power,
            freqs: Arc::clone(&parts.freqs),
        }
    }

    /// Refills the temps in the device's own order. Hotspot comes from nvapi rather than NVML,
    /// so it is `hotspot`, the last one read, until `store_nvml_status` settles this tick's.
    fn refill_temps(
        nv_info: &NvidiaDeviceInfo,
        readings: &NvmlReadings,
        hotspot: Option<Temp>,
        refill: &mut StatusRefill,
    ) {
        for name in &nv_info.temps {
            let temp = match name.as_str() {
                GPU_TEMP_NAME => readings.gpu_temp.map(f64::from),
                GPU_TEMP_MEMORY_NAME => readings.memory_temp,
                GPU_TEMP_HOTSPOT_NAME => hotspot,
                other => {
                    error!("Unexpected Nvidia temp name: {other}");
                    None
                }
            };
            if let Some(temp) = temp {
                refill.temp(name, temp);
            }
        }
    }

    /// Puts the hotspot into `temps` at its place in the device's own order, as if every temp
    /// came from one source. A hotspot already there, from a late read, is overwritten.
    fn insert_hotspot(nv_info: &NvidiaDeviceInfo, temps: &mut Vec<TempStatus>, hotspot_temp: f64) {
        debug_assert!(hotspot_temp.is_finite());
        Self::insert_hotspot_unchecked(nv_info, temps, hotspot_temp);
        debug_assert!(
            temps
                .iter()
                .filter(|temp| temp.name == GPU_TEMP_HOTSPOT_NAME)
                .count()
                <= 1
        );
        debug_assert!(temps.len() <= nv_info.temps.len());
    }

    fn insert_hotspot_unchecked(
        nv_info: &NvidiaDeviceInfo,
        temps: &mut Vec<TempStatus>,
        hotspot_temp: f64,
    ) {
        if let Some(existing) = temps
            .iter_mut()
            .find(|temp| temp.name == GPU_TEMP_HOTSPOT_NAME)
        {
            existing.temp = hotspot_temp;
            return;
        }
        let rank = |name: &str| nv_info.temps.iter().position(|known| known == name);
        let Some(hotspot_rank) = rank(GPU_TEMP_HOTSPOT_NAME) else {
            return;
        };
        let at = temps
            .iter()
            .position(|temp| rank(&temp.name).is_some_and(|r| r > hotspot_rank))
            .unwrap_or(temps.len());
        temps.insert(
            at,
            TempStatus {
                name: GPU_TEMP_HOTSPOT_NAME.to_string(),
                temp: hotspot_temp,
            },
        );
    }

    fn refill_channels(readings: &NvmlReadings, refill: &mut StatusRefill) {
        for (fan_index, duty, rpm) in &readings.fans {
            let sample = ChannelSample {
                duty: Some(f64::from(*duty)),
                rpm: *rpm,
                ..Default::default()
            };
            refill.channel_fmt(format_args!("{NVIDIA_FAN_PREFIX}{}", fan_index + 1), sample);
        }
        if let Some(load) = readings.load {
            let sample = ChannelSample {
                duty: Some(f64::from(load)),
                ..Default::default()
            };
            refill.channel(GPU_LOAD_NAME, sample);
        }
        if let Some(milli_watts) = readings.power_milliwatts {
            let sample = ChannelSample {
                watts: Some(Self::convert_milliwatts_to_watts(milli_watts)),
                ..Default::default()
            };
            refill.channel(GPU_POWER_NAME, sample);
        }
        for (clock, frequency) in &readings.freqs {
            let name = match clock {
                Clock::Graphics => NVIDIA_CLOCK_GRAPHICS,
                Clock::SM => NVIDIA_CLOCK_SM,
                Clock::Memory => NVIDIA_CLOCK_MEMORY,
                Clock::Video => NVIDIA_CLOCK_VIDEO,
            };
            let sample = ChannelSample {
                freq: Some(*frequency),
                ..Default::default()
            };
            refill.channel(name, sample);
        }
    }

    /// resets the nvidia fan control back to automatic
    async fn reset_nvml_device_to_default(
        &self,
        nv_info: &Rc<NvidiaDeviceInfo>,
        channel_name: &str,
    ) -> Result<()> {
        let fan_index = Self::parse_fan_index(channel_name)?;
        Self::verify_fan_index(nv_info, fan_index)?;
        self.write_nvml_bounded(nv_info.gpu_index, "fan reset", move |device| {
            device.set_default_fan_speed(u32::from(fan_index))
        })
        .await
    }

    async fn set_nvml_fan_duty(
        &self,
        nv_info: &Rc<NvidiaDeviceInfo>,
        channel_name: &str,
        fan_duty: Duty,
    ) -> Result<()> {
        // Every check stays on this thread: only the driver call itself is worth isolating, and
        // the range's log-once guards are not shareable across threads.
        let fan_index = Self::parse_fan_index(channel_name)?;
        Self::verify_fan_index(nv_info, fan_index)?;
        let fan_range = nv_info.get_fan_range(fan_index)?;
        if fan_range.is_outside(fan_duty) {
            if fan_duty == 0 {
                fan_range.log_zero_info_once();
                return self
                    .write_nvml_bounded(nv_info.gpu_index, "fan reset", move |device| {
                        device.set_default_fan_speed(u32::from(fan_index))
                    })
                    .await;
            }
            // we set the fan speed anyway, as it doesn't appear to produce an error, and will
            // set the fan control policy to manual automatically.
            fan_range.log_outside_warning_once(fan_duty);
        }
        self.nvml_io
            .get(&nv_info.gpu_index)
            .with_context(|| format!("NVML device {} should exist", nv_info.gpu_index))?
            .set_fan_speed(u32::from(fan_index), u32::from(fan_duty))
            .await?
            .map_err(anyhow::Error::from)
    }

    /// The 0-based fan index of a `fanN` channel. Parsed by hand: it runs on every duty write,
    /// and a regex compiled here allocated dozens of times per write.
    fn parse_fan_index(channel_name: &str) -> Result<FanIndex> {
        let fan_number = channel_name
            .strip_prefix(NVIDIA_FAN_PREFIX)
            .and_then(|digits| digits.parse::<FanIndex>().ok())
            .with_context(|| format!("Nvidia fan channel name should be fanN: {channel_name}"))?;
        let fan_index = fan_number
            .checked_sub(1)
            .with_context(|| format!("Nvidia fan channels count from 1: {channel_name}"))?;
        debug_assert!(fan_index < FanIndex::MAX);
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
                    let device_uid =
                        Device::create_uid_from(&nv_status.name, DeviceType::GPU, type_index, None);
                    let cc_device_setting = self.config.get_cc_settings_for_device(&device_uid)?;
                    if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
                        info!(
                            "Skipping disabled device: {} with UID: {device_uid}",
                            nv_status.name
                        );
                        continue;
                    }
                    let disabled_channels = cc_device_setting
                        .map_or_else(Vec::new, |setting| setting.get_disabled_channels());
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
                                label: None,
                                kind: ChannelKind::Speed(SpeedOptions {
                                    fixed_enabled: has_xauth, // disable if xauth not found
                                    ..Default::default()
                                }),
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
                                kind: ChannelKind::InfoOnly,
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
                            temp_max: 150,
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
                    let fan_ranges = fan_indices
                        .iter()
                        .copied()
                        .map(|fan_index| (fan_index, NvidiaFanRange::default()))
                        .collect();
                    self.nvidia_device_infos.insert(
                        uid.clone(),
                        Rc::new(NvidiaDeviceInfo {
                            gpu_index: nv_status.index,
                            display_id,
                            fan_indices,
                            temps: Vec::new(),
                            freqs: Vec::new(),
                            power: false,
                            fan_ranges,
                            hotspot_pci_bus: None,
                            read_plan_parts: std::cell::OnceCell::new(),
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
            rt::sleep(Duration::from_millis(500)).await;
            // The glob walks home directories, which may be automounted, so it runs on the
            // blocking pool.
            if let Ok(Some(path)) = rt::spawn_blocking(find_xauthority_path).await {
                return Some(path);
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
                        If you have a Nvidia card nvidia-settings needs to be installed for fan control. {err}"
                    ));
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
                    "Error communicating with nvidia-settings: {stderr}"
                ))
            }
            Success { stdout, stderr } => {
                debug!("Nvidia-settings output: {stdout} - {stderr}");
                if stderr.is_empty() {
                    Ok(())
                } else {
                    Err(anyhow!(
                        "Error output received when trying to set nvidia fan speed settings. \
                    Some errors don't affect setting the fan speed. YMMV: {stderr}"
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

    /// The channels and temps currently held at their failsafe values.
    pub fn failsafing(&self) -> Vec<FailsafeRef> {
        let failsafes = self.nvml_failsafe_statuses.borrow();
        let mut refs = Vec::new();
        for (type_index, device_lock) in &self.nvidia_devices {
            if let Some(fsd) = failsafes.get(type_index) {
                refs.extend(fsd.device_failsafe_refs(&device_lock.borrow().uid));
            }
        }
        refs
    }

    fn device_name(&self, type_index: TypeIndex) -> String {
        self.nvidia_devices.get(&type_index).map_or_else(
            || format!("#{type_index}"),
            |device| device.borrow().name.clone(),
        )
    }

    /// Adds `device` as the NVIDIA GPU `type_index`, for tests.
    #[cfg(test)]
    pub fn insert_device_for_test(&mut self, type_index: TypeIndex, device: DeviceLock) {
        self.nvidia_devices.insert(type_index, device);
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
/// What one GPU's read pass should gather, in a form that crosses to a blocking thread.
struct NvmlReadPlan {
    fan_indices: Arc<[FanIndex]>,
    /// Which NVML-backed temps to read. Hotspot is not here: it comes from nvapi, not NVML.
    temps: Arc<[NvmlTemp]>,
    read_load: bool,
    read_power: bool,
    freqs: Arc<[Clock]>,
}

/// The parts of a GPU's read plan that never change, built on its first pass.
#[derive(Debug)]
struct NvmlReadPlanParts {
    fan_indices: Arc<[FanIndex]>,
    temps: Arc<[NvmlTemp]>,
    freqs: Arc<[Clock]>,
}

/// The temps NVML itself can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NvmlTemp {
    Gpu,
    Memory,
}

/// What it gathered. Plain data, so it crosses back by value.
#[derive(Debug, Default)]
pub struct NvmlReadings {
    gpu_temp: Option<u32>,
    memory_temp: Option<Temp>,
    /// Fan index, duty percent, rpm when the device reports one.
    fans: Vec<(FanIndex, u32, Option<u32>)>,
    load: Option<u32>,
    power_milliwatts: Option<u32>,
    freqs: Vec<(Clock, u32)>,
}

#[derive(Debug)]
pub struct NvidiaDeviceInfo {
    pub gpu_index: GpuIndex,
    pub display_id: DisplayId,
    pub fan_indices: Vec<FanIndex>,
    fan_ranges: HashMap<FanIndex, NvidiaFanRange>,
    /// The names of the temperature sensors that have been successfully found (NVML)
    pub temps: Vec<String>,
    /// The Clock type sensors that have been successfully found (NVML)
    pub freqs: Vec<Clock>,
    /// Whether power is supported
    pub power: bool,
    /// PCI bus for the per-tick nvapi hotspot read. Some only when the GPU has a hotspot channel.
    pub hotspot_pci_bus: Option<u32>,
    read_plan_parts: std::cell::OnceCell<NvmlReadPlanParts>,
}

impl NvidiaDeviceInfo {
    fn read_plan_parts(&self) -> &NvmlReadPlanParts {
        self.read_plan_parts.get_or_init(|| NvmlReadPlanParts {
            fan_indices: self.fan_indices.as_slice().into(),
            temps: self
                .temps
                .iter()
                .filter_map(|name| match name.as_str() {
                    GPU_TEMP_NAME => Some(NvmlTemp::Gpu),
                    GPU_TEMP_MEMORY_NAME => Some(NvmlTemp::Memory),
                    // Hotspot comes from nvapi, read on its own thread.
                    _ => None,
                })
                .collect(),
            freqs: self.freqs.as_slice().into(),
        })
    }

    /// A GPU with no fans or sensors found, for tests.
    #[cfg(test)]
    pub fn for_test(gpu_index: GpuIndex) -> Self {
        Self {
            gpu_index,
            display_id: 0,
            fan_indices: Vec::new(),
            fan_ranges: HashMap::new(),
            temps: Vec::new(),
            freqs: Vec::new(),
            power: false,
            hotspot_pci_bus: None,
            read_plan_parts: std::cell::OnceCell::new(),
        }
    }

    pub fn get_fan_range(&self, fan_index: FanIndex) -> Result<&NvidiaFanRange> {
        self.fan_ranges
            .get(&fan_index)
            .ok_or_else(|| anyhow!("Fan speed range not found with Index: {fan_index}"))
    }
}

#[derive(Debug, Clone)]
/// A range of allowed fan speeds for a specific fan
pub struct NvidiaFanRange {
    range: RangeInclusive<Duty>,

    /// Whether a log has been output if the user has attempted to set a fan speed
    /// outside the allowed rage. This is to prevent log spam.
    outside_logged: Cell<bool>,

    /// Whether a log has been output if the user has attempted to set a fan speed
    /// to 0, which is not in the allowed range, and we have attempted to use auto
    /// mode to simulate 0 rpm. This is to prevent log spam.
    zero_logged: Cell<bool>,
}

impl NvidiaFanRange {
    pub fn new(start_duty: Duty, end_duty: Duty) -> Self {
        Self {
            range: RangeInclusive::new(start_duty, end_duty),
            ..Default::default()
        }
    }

    pub fn is_outside(&self, fan_duty: Duty) -> bool {
        self.range.contains(&fan_duty).not()
    }

    pub fn log_zero_info_once(&self) {
        if self.zero_logged.get() {
            return;
        }
        info!(
            "Your Nvidia GPU does not allow setting a manual fan speed of 0% ({} - {}). \
            Attempting to use auto-mode to enable zero rpm.",
            self.range.start(),
            self.range.end()
        );
        self.zero_logged.set(true);
    }

    pub fn log_outside_warning_once(&self, fan_duty: Duty) {
        if self.outside_logged.get() {
            return;
        }
        warn!(
            "The fan speed {fan_duty}% is not in the allowed Nvidia speed range: {} - {}. \
                Adjust your Profile to only use a duty within this range.",
            self.range.start(),
            self.range.end()
        );
        self.outside_logged.set(true);
    }
}

impl Default for NvidiaFanRange {
    fn default() -> Self {
        Self {
            range: RangeInclusive::new(0, 100),
            outside_logged: Cell::new(false),
            zero_logged: Cell::new(false),
        }
    }
}

/// Outcome of attempting to initialize NVML during GPU detection.
pub enum NvmlInitResult {
    /// NVML is active and managing this many NVIDIA GPUs.
    Active(u8),
    /// NVML was available but no NVIDIA GPU is managed (none detected, or every one
    /// disabled), so NVML was shut down to let the driver detach. No nvidia-smi fallback
    /// should run: it shares the same driver and would just re-attach it.
    Released,
    /// NVML is unavailable; the caller should fall back to the nvidia-smi CLI.
    Unavailable,
}

/// How long NVML gets to load, initialize and enumerate, and to shut down again. Without
/// persistence mode the driver initializes each GPU on the first call, about a second each; a GPU
/// that fell off the bus never answers.
const NVML_INIT_TIMEOUT: Duration = Duration::from_secs(30);

/// How long one GPU gets for its init readings, which can wake it from D3cold first.
const NVML_INIT_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// The NVML clocks offered as frequency channels, in the order their statuses are listed: each
/// clock, its channel name, and its label after `NVIDIA_FREQ_PREFIX`.
const NVML_CLOCKS: [(Clock, &str, &str); 4] = [
    (Clock::Graphics, NVIDIA_CLOCK_GRAPHICS, "Graphics"),
    (Clock::SM, NVIDIA_CLOCK_SM, "SM"),
    (Clock::Memory, NVIDIA_CLOCK_MEMORY, "Memory"),
    (Clock::Video, NVIDIA_CLOCK_VIDEO, "Video"),
];

/// One NVML GPU before its worker starts: the handle, and the name it enumerated with.
struct PendingNvmlGpu {
    device: nvml_wrapper::Device<'static>,
    name: Option<String>,
}

/// NVML once loaded, before it is committed to the process-lifetime static.
struct NvmlProbe {
    /// Boxed: the library's function table is about 20 KB, which every future holding the probe
    /// across an await would otherwise carry.
    nvml: Box<Nvml>,
    device_count: u32,
    device_uids: Vec<UID>,
}

/// Loads NVML and enumerates its GPUs on the blocking pool, within `NVML_INIT_TIMEOUT`. `None`
/// when NVML is unavailable, which falls back to the CLI tools.
async fn probe_nvml(starting_nvidia_index: GpuIndex) -> Option<NvmlProbe> {
    let probe = rt::spawn_blocking(move || {
        let nvml = match Nvml::init() {
            Ok(nvml) => nvml,
            Err(err) => {
                debug!("NVML not found or failed to initialize, falling back to CLI tools");
                debug!("NVML initialize error: {err}");
                return None;
            }
        };
        info!("NVML found and initialized.");
        let Ok(device_count) = nvml
            .device_count()
            .inspect_err(|err| error!("Error getting NVML device count: {err}"))
        else {
            return None;
        };
        debug!("Found {device_count} NVML devices");
        let device_uids = collect_nvml_device_uids(&nvml, device_count, starting_nvidia_index);
        Some(NvmlProbe {
            nvml: Box::new(nvml),
            device_count,
            device_uids,
        })
    });
    match rt::timeout(NVML_INIT_TIMEOUT, probe).await {
        Ok(Ok(probe)) => probe,
        Ok(Err(err)) => {
            error!("NVML initialization failed: {err}");
            None
        }
        Err(_) => {
            warn!(
                "NVML did not initialize within {NVML_INIT_TIMEOUT:?}. NVIDIA GPUs fall back to \
                 the CLI tools."
            );
            None
        }
    }
}

/// Shuts NVML down on the blocking pool, so the driver can detach. It can block like init.
async fn release_nvml(nvml: Box<Nvml>) {
    let shutdown = rt::spawn_blocking(move || nvml.shutdown());
    match rt::timeout(NVML_INIT_TIMEOUT, shutdown).await {
        Ok(Ok(Ok(()))) => {}
        Ok(Ok(Err(err))) => {
            error!("Error shutting down NVML while releasing unmanaged NVIDIA GPUs: {err}");
        }
        Ok(Err(err)) => error!("NVML shutdown failed: {err}"),
        Err(_) => warn!("NVML did not shut down within {NVML_INIT_TIMEOUT:?}"),
    }
}

/// One fan an init read the duty of.
#[derive(Debug)]
struct NvmlFanReading {
    index: FanIndex,
    duty_percent: u32,
    /// When the device reports one.
    rpm: Option<u32>,
}

/// Every NVML read a GPU's init makes, taken in one call on its worker. Plain data, so it crosses
/// the thread boundary by value.
#[derive(Debug, Default)]
pub struct NvmlInitReadings {
    name: Option<String>,
    temperature_celsius: Option<u32>,
    memory_temperature_celsius: Option<Temp>,
    pci_bus: Option<u32>,
    pci_bus_id: Option<String>,
    needs_hotspot_register: bool,
    fan_count: u32,
    /// Each fan whose duty reads.
    fans: Vec<NvmlFanReading>,
    load_percent: Option<u32>,
    /// Each clock that answers, with its current value.
    clocks: Vec<(Clock, u32)>,
    power_milliwatts: Option<u32>,
    driver_version: Option<String>,
    nvml_version: Option<String>,
    fan_duty_percent_range: Option<(u32, u32)>,
}

impl NvmlInitReadings {
    /// Runs on the GPU's worker.
    fn read(device: &mut nvml_wrapper::Device<'static>) -> Self {
        let pci_info = device.pci_info().ok();
        let fan_count = device.num_fans().unwrap_or_default();
        Self {
            name: device.name().ok(),
            temperature_celsius: device.temperature(TemperatureSensor::Gpu).ok(),
            memory_temperature_celsius: GpuNVidia::get_memory_temp(device),
            pci_bus: pci_info.as_ref().map(|info| info.bus),
            pci_bus_id: pci_info.map(|info| info.bus_id),
            needs_hotspot_register: needs_hotspot_register(device.architecture()),
            fan_count,
            fans: Self::read_fans(device, fan_count),
            load_percent: device.utilization_rates().ok().map(|rates| rates.gpu),
            clocks: NVML_CLOCKS
                .iter()
                .filter_map(|(clock, _, _)| Some((*clock, device.clock_info(*clock).ok()?)))
                .collect(),
            power_milliwatts: device.power_usage().ok(),
            driver_version: device.nvml().sys_driver_version().ok(),
            nvml_version: device.nvml().sys_nvml_version().ok(),
            fan_duty_percent_range: device.min_max_fan_speed().ok(),
        }
    }

    /// Each fan whose duty reads. Past `FanIndex` there are none: the count is not believable,
    /// and the build fails on it as it always has.
    fn read_fans(device: &nvml_wrapper::Device<'static>, fan_count: u32) -> Vec<NvmlFanReading> {
        let Ok(fan_count) = FanIndex::try_from(fan_count) else {
            return Vec::new();
        };
        let mut fans = Vec::with_capacity(usize::from(fan_count));
        for fan_index in 0..fan_count {
            let Ok(duty_percent) = device.fan_speed(u32::from(fan_index)) else {
                continue;
            };
            let rpm = device.fan_speed_rpm(u32::from(fan_index)).ok();
            fans.push(NvmlFanReading {
                index: fan_index,
                duty_percent,
                rpm,
            });
        }
        fans
    }

    fn clock(&self, clock_type: Clock) -> Option<u32> {
        self.clocks
            .iter()
            .find(|(clock, _)| *clock == clock_type)
            .map(|(_, freq)| *freq)
    }
}

/// Resolves an NVML GPU's display name (with the standard `Nvidia GPU #{index}` fallback)
/// and the UID config keys it under. Shared by the release check and device registration
/// so the two always key the same UID for a given GPU.
fn nvml_name_and_uid(name: Option<String>, type_index: TypeIndex) -> (String, UID) {
    let name = name.unwrap_or_else(|| format!("Nvidia GPU #{type_index}"));
    let device_uid = Device::create_uid_from(&name, DeviceType::GPU, type_index, None);
    debug_assert!(
        device_uid.is_empty().not(),
        "device UID hash must be non-empty"
    );
    (name, device_uid)
}

/// Enumerates the owned NVML handle and returns each accessible device's config UID, before
/// NVML is committed to the process-lifetime static so the handle can still be dropped to
/// release the GPU. A device that fails to enumerate is skipped, leaving fewer UIDs than
/// `device_count`, which `should_release_nvml` treats as "do not release".
#[allow(clippy::cast_possible_truncation)]
fn collect_nvml_device_uids(
    nvml: &Nvml,
    device_count: u32,
    starting_nvidia_index: GpuIndex,
) -> Vec<UID> {
    let mut device_uids = Vec::with_capacity(device_count as usize);
    for device_index in 0..device_count {
        let Ok(device) = nvml.device_by_index(device_index) else {
            continue;
        };
        let type_index = device_index as TypeIndex + starting_nvidia_index;
        let (_name, device_uid) = nvml_name_and_uid(device.name().ok(), type_index);
        device_uids.push(device_uid);
    }
    debug_assert!(
        device_uids.len() <= device_count as usize,
        "enumerated more devices than NVML reported"
    );
    device_uids
}

/// Each GPU's hotspot by GPU index, read in turn within one shared `budget`, so a wedged nvapi
/// cannot hold startup for a full reply timeout per GPU. A GPU not reached in time gets none.
async fn read_hotspots_within(
    nvapi_io: &NvApiIo,
    buses: &[(GpuIndex, u32)],
    budget: Duration,
) -> HashMap<GpuIndex, f64> {
    debug_assert!(budget > Duration::ZERO);
    let deadline = Instant::now() + budget;
    let mut hotspots = HashMap::with_capacity(buses.len());
    for &(gpu_index, pci_bus) in buses {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            warn!(
                "nvapi did not read every GPU hotspot within {budget:?}. The rest have no \
                 hotspot temperature."
            );
            break;
        }
        if let Ok(Some(hotspot_temp)) = rt::timeout(remaining, nvapi_io.hotspot_temp(pci_bus)).await
        {
            hotspots.insert(gpu_index, hotspot_temp);
        }
    }
    debug_assert!(hotspots.len() <= buses.len());
    hotspots
}

/// True for Blackwell and newer, where nvapi thermals no longer exposes hotspot.
/// `Unknown` and unmapped raw variants sort above Blackwell too; guessing wrong there
/// only costs one register probe, since nvapi falls back to thermals on failure.
fn needs_hotspot_register(architecture: Result<DeviceArchitecture, NvmlError>) -> bool {
    let raw_architecture = match architecture {
        Ok(architecture) => architecture.as_c(),
        Err(NvmlError::UnexpectedVariant(raw_architecture)) => raw_architecture,
        // Any other error means the architecture is unreadable, not that it is newer.
        Err(_) => return false,
    };
    raw_architecture >= DeviceArchitecture::Blackwell.as_c()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: a `fanN` channel names fan N-1, and anything else is refused rather than read as
    /// some other fan. Method: parse valid names at both widths and malformed ones.
    #[test]
    fn a_fan_channel_names_its_zero_based_index() {
        assert_eq!(GpuNVidia::parse_fan_index("fan1").unwrap(), 0);
        assert_eq!(GpuNVidia::parse_fan_index("fan12").unwrap(), 11);
        for malformed in ["fan0", "fan", "pump", "fan1x", "fan256", "1fan"] {
            assert!(
                GpuNVidia::parse_fan_index(malformed).is_err(),
                "{malformed} must not name a fan"
            );
        }
    }

    /// Goal: parsing the fan index on every duty write costs nothing. Method: count one parse.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn parsing_a_fan_index_allocates_nothing() {
        let (fan_index, allocations) =
            crate::alloc_count::count_allocations(|| GpuNVidia::parse_fan_index("fan3"));

        assert_eq!(fan_index.unwrap(), 2);
        assert_eq!(allocations, 0);
    }
    use crate::repositories::gpu::nvapi_io::HotspotReader;
    use crate::setting::CCDeviceSettings;

    #[test]
    #[cfg(feature = "gated-tests")]
    fn the_read_plan_is_built_once_per_gpu() {
        // Goal: a tick's NVML read plan shares the GPU's fans, temps and clocks instead of
        // copying them, and still asks for exactly what the GPU has. Method: count the
        // allocations of a second plan for the same GPU.
        let gpus = GpuNVidia::new(Rc::new(Config::init_default_config().unwrap()));
        let mut nv_info = NvidiaDeviceInfo::for_test(0);
        nv_info.fan_indices = vec![0, 1];
        nv_info.temps = vec![
            GPU_TEMP_NAME.to_string(),
            GPU_TEMP_MEMORY_NAME.to_string(),
            "GPU Hotspot".to_string(),
        ];
        nv_info.freqs = vec![Clock::Graphics];
        let _ = gpus.read_plan(&nv_info);

        let (plan, allocations) =
            crate::alloc_count::count_allocations(|| gpus.read_plan(&nv_info));

        assert_eq!(allocations, 0);
        assert_eq!(&*plan.fan_indices, &[0, 1]);
        assert_eq!(&*plan.temps, &[NvmlTemp::Gpu, NvmlTemp::Memory]);
        assert_eq!(plan.freqs.len(), 1);
    }

    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_warm_update_stores_the_preloaded_status_without_allocating() {
        // Goal: the update pass writes the preloaded values into the history in place,
        // stamped with the tick's timestamp. Method: one tick warms the stats, then count
        // the allocations of the next.
        let mut gpus = GpuNVidia::new(Rc::new(Config::init_default_config().unwrap()));
        let status = StatusNvidiaDeviceSMI {
            temps: vec![TempStatus {
                name: GPU_TEMP_NAME.to_string(),
                temp: 60.0,
            }],
            channels: vec![ChannelStatus {
                name: "fan1".to_string(),
                rpm: Some(1500),
                duty: Some(40.0),
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut device = Device::new(
            "Test GPU".to_string(),
            DeviceType::GPU,
            1,
            None,
            DeviceInfo::default(),
            None,
            1.0,
        );
        device.initialize_status_history_with(
            Status {
                timestamp: Local::now(),
                temps: status.temps.clone(),
                channels: status.channels.clone(),
            },
            1.0,
        );
        let device = Rc::new(RefCell::new(device));
        gpus.nvidia_devices.insert(1, Rc::clone(&device));
        gpus.nvidia_preloaded_statuses
            .borrow_mut()
            .insert(1, status);
        gpus.update_all_statuses(Local::now());
        let tick = Local::now();

        let ((), allocations) =
            crate::alloc_count::count_allocations(|| gpus.update_all_statuses(tick));

        assert_eq!(allocations, 0);
        let device = device.borrow();
        assert_eq!(device.latest_timestamp(), Some(tick));
        assert_eq!(device.latest_temp(GPU_TEMP_NAME), Some(60.0));
        assert_eq!(device.latest_channel("fan1").unwrap().rpm, Some(1500));
    }

    /// One GPU's status refilled from `readings` into empty lists, with no hotspot carried.
    fn refilled(
        nv_info: &NvidiaDeviceInfo,
        readings: &NvmlReadings,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut channels = Vec::new();
        let mut temps = Vec::new();
        let mut refill = StatusRefill::new(&mut channels, &mut temps);
        GpuNVidia::refill_temps(nv_info, readings, None, &mut refill);
        GpuNVidia::refill_channels(readings, &mut refill);
        drop(refill);
        (channels, temps)
    }

    /// Goal: a tick refills the stored NVML status in place, so one reading what the last did
    /// allocates nothing, and the result is what a fresh build gives. Method: one GPU's readings,
    /// fans included, refilled twice into the stored status; the second is counted.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn a_warm_nvml_refill_allocates_nothing() {
        let gpus = GpuNVidia::new(Rc::new(Config::init_default_config().unwrap()));
        let nv_info = NvidiaDeviceInfo {
            temps: vec![GPU_TEMP_NAME.to_string(), GPU_TEMP_MEMORY_NAME.to_string()],
            ..nv_info_for(0)
        };
        let readings = NvmlReadings {
            gpu_temp: Some(55),
            memory_temp: Some(70.0),
            fans: vec![(0, 42, Some(1200)), (1, 55, None)],
            load: Some(37),
            power_milliwatts: Some(150_000),
            freqs: vec![(Clock::Graphics, 2100)],
        };
        gpus.refill_nvml_status(1, &nv_info, &readings);

        let ((), allocations) = crate::alloc_count::count_allocations(|| {
            gpus.refill_nvml_status(1, &nv_info, &readings);
        });

        assert_eq!(allocations, 0);
        let statuses = gpus.nvidia_preloaded_statuses.borrow();
        let stored = (statuses[&1].channels.clone(), statuses[&1].temps.clone());
        assert_eq!(stored, refilled(&nv_info, &readings));
    }

    fn nv_info_for(gpu_index: GpuIndex) -> NvidiaDeviceInfo {
        NvidiaDeviceInfo {
            gpu_index,
            display_id: 0,
            fan_indices: vec![0],
            fan_ranges: HashMap::new(),
            temps: Vec::new(),
            freqs: Vec::new(),
            power: false,
            hotspot_pci_bus: None,
            read_plan_parts: std::cell::OnceCell::new(),
        }
    }

    /// Goal: the handle map is drained into the workers at init, so it is empty for the whole life
    /// of a running daemon. Deciding the control path from it therefore sends every NVML card down
    /// the `nvidia-settings` branch, which returns `Ok(())` without doing anything when there is no
    /// xauthority: fan speeds are silently never applied, with nothing in the log to say so.
    ///
    /// Method: a GPU with a worker and an empty handle map, which is exactly the running state.
    #[test]
    fn a_gpu_with_a_worker_is_nvml_controlled_though_the_handle_map_is_empty() {
        let mut repo = repo_with_disabled(&[]);
        let nv_info = nv_info_for(0);
        assert!(
            repo.is_nvml_controlled(&nv_info).not(),
            "no worker yet, so the CLI is correct here"
        );

        let (io, _rx) = NvmlIo::for_test("test-gpu");
        repo.nvml_io.insert(0, io);

        assert!(
            repo.nvml_handles_pending_workers.is_empty(),
            "the running state: handles were moved into the workers"
        );
        assert!(
            repo.is_nvml_controlled(&nv_info),
            "a GPU with a worker must stay on NVML, or its fan writes go nowhere"
        );
    }

    /// Goal: a card whose worker never started falls back on its own, rather than one bad card
    /// deciding the path for every other.
    #[test]
    fn a_gpu_without_a_worker_falls_back_alone() {
        let mut repo = repo_with_disabled(&[]);
        let (io, _rx) = NvmlIo::for_test("test-gpu");
        repo.nvml_io.insert(0, io);

        assert!(repo.is_nvml_controlled(&nv_info_for(0)));
        assert!(repo.is_nvml_controlled(&nv_info_for(1)).not());
    }

    /// Goal: moving the NVML reads to a blocking thread must not reorder what the device reports.
    /// Temps are assembled on this thread from two sources now (NVML for gpu and memory, nvapi for
    /// hotspot), so the device's own `temps` order is what has to be preserved.
    ///
    /// Method: ask for memory before gpu, supply both readings, and assert the output follows the
    /// requested order rather than the order the worker happened to gather them in.
    #[test]
    fn assembled_temps_follow_the_devices_own_order() {
        let nv_info = Rc::new(NvidiaDeviceInfo {
            gpu_index: 0,
            display_id: 0,
            fan_indices: vec![],
            fan_ranges: HashMap::new(),
            temps: vec![GPU_TEMP_MEMORY_NAME.to_string(), GPU_TEMP_NAME.to_string()],
            freqs: vec![],
            power: false,
            hotspot_pci_bus: None,
            read_plan_parts: std::cell::OnceCell::new(),
        });
        let readings = NvmlReadings {
            gpu_temp: Some(61),
            memory_temp: Some(74.0),
            ..Default::default()
        };

        let temps = refilled(&nv_info, &readings).1;

        assert_eq!(temps.len(), 2);
        assert_eq!(temps[0].name, GPU_TEMP_MEMORY_NAME);
        assert_eq!(temps[0].temp, 74.0);
        assert_eq!(temps[1].name, GPU_TEMP_NAME);
        assert_eq!(temps[1].temp, 61.0);
    }

    /// Goal: the hotspot nvapi read takes its place in the device's order, a late second read
    /// overwrites rather than duplicates it, and a tick without one omits it rather than reporting
    /// zero. Method: a device with hotspot between gpu and memory, assembled, then patched twice.
    #[test]
    fn the_hotspot_keeps_its_place_or_is_omitted() {
        let nv_info = NvidiaDeviceInfo {
            temps: vec![
                GPU_TEMP_NAME.to_string(),
                GPU_TEMP_HOTSPOT_NAME.to_string(),
                GPU_TEMP_MEMORY_NAME.to_string(),
            ],
            ..nv_info_for(0)
        };
        let readings = NvmlReadings {
            gpu_temp: Some(61),
            memory_temp: Some(74.0),
            ..Default::default()
        };

        let mut temps = refilled(&nv_info, &readings).1;
        assert_eq!(temps.len(), 2);
        assert!(temps.iter().all(|temp| temp.name != GPU_TEMP_HOTSPOT_NAME));
        GpuNVidia::insert_hotspot(&nv_info, &mut temps, 80.0);
        GpuNVidia::insert_hotspot(&nv_info, &mut temps, 81.0);

        let names: Vec<&str> = temps.iter().map(|temp| temp.name.as_str()).collect();
        assert_eq!(
            names,
            [GPU_TEMP_NAME, GPU_TEMP_HOTSPOT_NAME, GPU_TEMP_MEMORY_NAME]
        );
        assert_eq!(temps[1].temp, 81.0);
    }

    /// Blocks every hotspot read until the test lets it go, like a wedged nvapi.
    struct StalledHotspot {
        release: std::sync::mpsc::Receiver<()>,
    }

    impl HotspotReader for StalledHotspot {
        fn hotspot_temp(&self, _pci_bus: u32) -> Option<f64> {
            let _ = self.release.recv();
            Some(80.0)
        }
    }

    /// Answers each hotspot read with the next scripted reply, blocking until it is sent.
    struct ScriptedHotspot {
        replies: std::sync::mpsc::Receiver<Option<f64>>,
    }

    impl HotspotReader for ScriptedHotspot {
        fn hotspot_temp(&self, _pci_bus: u32) -> Option<f64> {
            self.replies.recv().ok().flatten()
        }
    }

    /// Answers every hotspot at once, at 70 plus the PCI bus.
    struct FixedHotspot;

    impl HotspotReader for FixedHotspot {
        fn hotspot_temp(&self, pci_bus: u32) -> Option<f64> {
            Some(70.0 + f64::from(pci_bus))
        }
    }

    /// Goal: an answering nvapi gives every GPU its init hotspot, keyed by GPU index. Method: a
    /// fake that answers at once, asked for three GPUs.
    #[test]
    fn init_hotspots_are_read_for_every_gpu() {
        crate::rt::test_runtime(async {
            let nvapi_io = NvApiIo::start_for_test(FixedHotspot, Duration::from_secs(30))
                .await
                .expect("the fake initializes");

            let hotspots = read_hotspots_within(
                &nvapi_io,
                &[(0, 1), (1, 2), (2, 3)],
                Duration::from_secs(30),
            )
            .await;

            assert_eq!(hotspots.len(), 3);
            assert_eq!(hotspots[&0], 71.0);
            assert_eq!(hotspots[&2], 73.0);
        });
    }

    /// Goal: a wedged nvapi holds startup for one shared budget, not a reply timeout per GPU.
    /// Method: a fake that never answers, a long reply timeout, three GPUs and a short budget.
    /// Gated: it bounds a real-time wait from above.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn init_hotspot_reads_share_one_budget() {
        crate::rt::test_runtime(async {
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let nvapi_io = NvApiIo::start_for_test(
                StalledHotspot {
                    release: release_rx,
                },
                Duration::from_secs(30),
            )
            .await
            .expect("the fake initializes");
            let budget = Duration::from_millis(100);

            let started = Instant::now();
            let hotspots = read_hotspots_within(&nvapi_io, &[(0, 1), (1, 2), (2, 3)], budget).await;

            assert!(started.elapsed() < Duration::from_secs(5));
            assert!(hotspots.is_empty());
            drop(release_tx);
        });
    }

    /// Goal: a stalled nvapi never holds back a GPU's NVML status, and a hotspot that does answer
    /// is patched into it. Method: store a status while the hotspot read is blocked, check the
    /// status is already there, then release the read and check the hotspot landed.
    #[test]
    fn the_nvml_status_lands_before_a_stalled_hotspot() {
        crate::rt::test_runtime(async {
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let mut gpus = GpuNVidia::new(Rc::new(Config::init_default_config().unwrap()));
            gpus.nvapi_io = NvApiIo::start_for_test(
                StalledHotspot {
                    release: release_rx,
                },
                Duration::from_secs(30),
            )
            .await;
            assert!(gpus.nvapi_io.is_some());
            let nv_info = NvidiaDeviceInfo {
                temps: vec![GPU_TEMP_NAME.to_string(), GPU_TEMP_HOTSPOT_NAME.to_string()],
                hotspot_pci_bus: Some(1),
                ..nv_info_for(0)
            };
            let readings = NvmlReadings {
                gpu_temp: Some(61),
                ..Default::default()
            };

            let check_then_release = async {
                let statuses = gpus.nvidia_preloaded_statuses.borrow();
                let stored = statuses.get(&1).expect("stored before the hotspot read");
                assert_eq!(stored.temps.len(), 1);
                drop(statuses);
                release_tx.send(()).unwrap();
            };
            tokio::join!(
                gpus.store_nvml_status(1, &nv_info, &readings),
                check_then_release
            );

            let statuses = gpus.nvidia_preloaded_statuses.borrow();
            let names: Vec<&str> = statuses[&1]
                .temps
                .iter()
                .map(|temp| temp.name.as_str())
                .collect();
            assert_eq!(names, [GPU_TEMP_NAME, GPU_TEMP_HOTSPOT_NAME]);
        });
    }

    /// Goal: a hotspot that answers after the snapshot still shows a tick late, and a failed
    /// read removes it rather than keeping a stale value. Method: store with a hotspot answer,
    /// store again while that tick's read is blocked and check the hotspot is carried over, then
    /// fail the read and check it is gone.
    #[test]
    fn the_last_hotspot_is_carried_until_a_read_fails() {
        crate::rt::test_runtime(async {
            let (reply_tx, reply_rx) = std::sync::mpsc::channel();
            let mut gpus = GpuNVidia::new(Rc::new(Config::init_default_config().unwrap()));
            gpus.nvapi_io = NvApiIo::start_for_test(
                ScriptedHotspot { replies: reply_rx },
                Duration::from_secs(30),
            )
            .await;
            assert!(gpus.nvapi_io.is_some());
            let nv_info = NvidiaDeviceInfo {
                temps: vec![
                    GPU_TEMP_NAME.to_string(),
                    GPU_TEMP_HOTSPOT_NAME.to_string(),
                    GPU_TEMP_MEMORY_NAME.to_string(),
                ],
                hotspot_pci_bus: Some(1),
                ..nv_info_for(0)
            };
            let readings = NvmlReadings {
                gpu_temp: Some(61),
                memory_temp: Some(70.0),
                ..Default::default()
            };
            let temp_names = |gpus: &GpuNVidia| -> Vec<String> {
                gpus.nvidia_preloaded_statuses.borrow()[&1]
                    .temps
                    .iter()
                    .map(|temp| temp.name.clone())
                    .collect()
            };

            reply_tx.send(Some(80.0)).unwrap();
            gpus.store_nvml_status(1, &nv_info, &readings).await;
            assert_eq!(
                temp_names(&gpus),
                [GPU_TEMP_NAME, GPU_TEMP_HOTSPOT_NAME, GPU_TEMP_MEMORY_NAME]
            );

            let check_then_fail = async {
                assert_eq!(
                    temp_names(&gpus),
                    [GPU_TEMP_NAME, GPU_TEMP_HOTSPOT_NAME, GPU_TEMP_MEMORY_NAME]
                );
                reply_tx.send(None).unwrap();
            };
            tokio::join!(
                gpus.store_nvml_status(1, &nv_info, &readings),
                check_then_fail
            );

            assert_eq!(temp_names(&gpus), [GPU_TEMP_NAME, GPU_TEMP_MEMORY_NAME]);
        });
    }

    /// Goal: a temp the device asked for but the driver did not answer must be omitted, not
    /// reported as zero. A fabricated reading would drive a fan curve from a value the hardware
    /// never produced.
    #[test]
    fn an_unanswered_temp_is_omitted_rather_than_zeroed() {
        let nv_info = Rc::new(NvidiaDeviceInfo {
            gpu_index: 0,
            display_id: 0,
            fan_indices: vec![],
            fan_ranges: HashMap::new(),
            temps: vec![GPU_TEMP_NAME.to_string(), GPU_TEMP_MEMORY_NAME.to_string()],
            freqs: vec![],
            power: false,
            hotspot_pci_bus: None,
            read_plan_parts: std::cell::OnceCell::new(),
        });
        // Memory temp unsupported on this card, so the worker reports nothing for it.
        let readings = NvmlReadings {
            gpu_temp: Some(55),
            memory_temp: None,
            ..Default::default()
        };

        let temps = refilled(&nv_info, &readings).1;

        assert_eq!(temps.len(), 1, "only the answered temp is reported");
        assert_eq!(temps[0].name, GPU_TEMP_NAME);
        assert!(temps.iter().all(|temp| temp.name != GPU_TEMP_MEMORY_NAME));
    }

    /// Goal: the channel list must carry every reading the worker gathered, with fans first so the
    /// order matches what the previous in-line implementation produced.
    #[test]
    fn assembled_channels_carry_every_reading() {
        let readings = NvmlReadings {
            fans: vec![(0, 42, Some(1200)), (1, 55, None)],
            load: Some(37),
            power_milliwatts: Some(150_000),
            freqs: vec![(Clock::Graphics, 2100)],
            ..Default::default()
        };

        let (channels, _) = refilled(&nv_info_for(0), &readings);

        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            names,
            vec![
                "fan1",
                "fan2",
                GPU_LOAD_NAME,
                GPU_POWER_NAME,
                NVIDIA_CLOCK_GRAPHICS
            ]
        );
        assert_eq!(channels[0].duty, Some(42.0));
        assert_eq!(channels[0].rpm, Some(1200));
        assert_eq!(channels[1].rpm, None, "a fan with no rpm reports none");
        assert_eq!(channels[3].watts, Some(150.0));
        assert_eq!(channels[4].freq, Some(2100));
    }

    /// Goal: a GPU that answers nothing must produce an empty status, so the caller's cache keeps
    /// its last-known-good values and the failsafe decides what to do. Regression guard on the
    /// wedged-driver path, which is the reason these calls moved off the runtime thread.
    #[test]
    fn no_readings_produce_an_empty_status() {
        let (channels, temps) = refilled(&nv_info_for(0), &NvmlReadings::default());
        assert!(channels.is_empty());
        assert!(temps.is_empty());
    }

    // Builds a GpuNVidia backed by a default config in which each given UID is marked
    // disabled, so the tests exercise the real get_cc_settings_for_device predicate (not a
    // stub) through should_release_nvml / is_nvml_device_disabled.
    fn repo_with_disabled(disabled_uids: &[UID]) -> GpuNVidia {
        let config = Config::init_default_config().expect("default config parses");
        for device_uid in disabled_uids {
            config.set_cc_settings_for_device(
                device_uid,
                &CCDeviceSettings {
                    name: "test gpu".to_string(),
                    disable: true,
                    ..Default::default()
                },
            );
        }
        GpuNVidia::new(Rc::new(config))
    }

    #[test]
    fn nvml_name_and_uid_matches_create_uid_from() {
        // The shared helper must derive the exact UID retrieve_nvml_devices keys on, and
        // apply the documented "Nvidia GPU #{type_index}" fallback when the name is absent.
        let (name, uid) = nvml_name_and_uid(Some("GPU A".to_string()), 2);
        assert_eq!(name, "GPU A");
        assert_eq!(
            uid,
            Device::create_uid_from("GPU A", DeviceType::GPU, 2, None)
        );

        let (fallback_name, fallback_uid) = nvml_name_and_uid(None, 5);
        assert_eq!(fallback_name, "Nvidia GPU #5");
        assert_eq!(
            fallback_uid,
            Device::create_uid_from("Nvidia GPU #5", DeviceType::GPU, 5, None)
        );
    }

    #[test]
    fn release_when_every_device_disabled() {
        // Every enumerated GPU disabled in config plus full enumeration -> release NVML.
        let uid_a = nvml_name_and_uid(Some("GPU A".to_string()), 2).1;
        let uid_b = nvml_name_and_uid(Some("GPU B".to_string()), 3).1;
        let repo = repo_with_disabled(&[uid_a.clone(), uid_b.clone()]);
        assert!(repo.should_release_nvml(&[uid_a, uid_b], 2));
    }

    #[test]
    fn keep_when_one_device_enabled() {
        // A single still-enabled GPU keeps NVML attached; init/shutdown is process-global,
        // so a disabled sibling cannot be released independently.
        let uid_a = nvml_name_and_uid(Some("GPU A".to_string()), 2).1;
        let uid_b = nvml_name_and_uid(Some("GPU B".to_string()), 3).1;
        let repo = repo_with_disabled(std::slice::from_ref(&uid_a)); // GPU B left enabled
        assert!(repo.should_release_nvml(&[uid_a, uid_b], 2).not());
    }

    #[test]
    fn keep_on_partial_enumeration() {
        // The len == device_count guard: a device failed to enumerate (one UID for a count
        // of two), so NVML must stay attached even though the collected device is disabled.
        let uid_a = nvml_name_and_uid(Some("GPU A".to_string()), 2).1;
        let repo = repo_with_disabled(std::slice::from_ref(&uid_a));
        assert!(repo.should_release_nvml(&[uid_a], 2).not());
    }

    #[test]
    fn release_when_no_devices() {
        // Zero detected GPUs means CoolerControl manages no NVIDIA GPU, so NVML is released.
        // This is the drivers-present-but-no-GPU case from the original report.
        let repo = repo_with_disabled(&[]);
        assert!(repo.should_release_nvml(&[], 0));
    }

    #[test]
    fn unknown_uid_reads_as_enabled() {
        // A UID absent from config (Ok(None)) must read as enabled so a missing entry never
        // causes a release. Fail-safe direction.
        let uid_a = nvml_name_and_uid(Some("GPU A".to_string()), 2).1;
        let repo = repo_with_disabled(&[]);
        assert!(repo.is_nvml_device_disabled(&uid_a).not());
    }

    #[test]
    fn blackwell_and_newer_take_the_hotspot_register() {
        // Blackwell is the cutoff where hotspot left the thermals array. Unknown is NVML
        // saying "newer than I know", and UnexpectedVariant is a raw constant this
        // nvml-wrapper release cannot name yet; both sort above Blackwell, so both take
        // the register path rather than silently losing the sensor on a new card.
        assert!(needs_hotspot_register(Ok(DeviceArchitecture::Blackwell)));
        assert!(needs_hotspot_register(Ok(DeviceArchitecture::Unknown)));
        assert!(needs_hotspot_register(Err(NvmlError::UnexpectedVariant(
            DeviceArchitecture::Blackwell.as_c() + 1
        ))));
    }

    #[test]
    fn older_architectures_keep_the_thermals_array() {
        // Every architecture below Blackwell reports hotspot through the thermals array,
        // as does a raw constant that sorts below it. An architecture that simply could
        // not be read is not evidence of a new card, so it stays on the old path too.
        assert!(needs_hotspot_register(Ok(DeviceArchitecture::Ada)).not());
        assert!(needs_hotspot_register(Ok(DeviceArchitecture::Hopper)).not());
        assert!(needs_hotspot_register(Ok(DeviceArchitecture::Kepler)).not());
        assert!(needs_hotspot_register(Err(NvmlError::UnexpectedVariant(
            DeviceArchitecture::Blackwell.as_c() - 1
        )))
        .not());
        assert!(needs_hotspot_register(Err(NvmlError::NotSupported)).not());
    }

    #[test]
    fn a_device_is_built_from_its_init_readings_alone() {
        // Goal: init builds an NVML GPU's device from what its worker read, with no NVML call of
        // its own, and keeps the channels, temps and status order it had when it read the handle
        // directly. Method: build from canned readings whose clocks arrive out of order.
        let mut repo = repo_with_disabled(&[]);
        let readings = NvmlInitReadings {
            name: Some("RTX 5090".to_owned()),
            temperature_celsius: Some(61),
            memory_temperature_celsius: Some(70.0),
            pci_bus: Some(1),
            pci_bus_id: Some("00000000:01:00.0".to_owned()),
            fan_count: 2,
            fans: vec![
                NvmlFanReading {
                    index: 0,
                    duty_percent: 40,
                    rpm: Some(1200),
                },
                NvmlFanReading {
                    index: 1,
                    duty_percent: 45,
                    rpm: None,
                },
            ],
            load_percent: Some(12),
            clocks: vec![(Clock::Memory, 10_501), (Clock::Graphics, 2407)],
            power_milliwatts: Some(85_000),
            driver_version: Some("610.57.04".to_owned()),
            nvml_version: Some("13.610.57".to_owned()),
            fan_duty_percent_range: Some((30, 100)),
            ..Default::default()
        };
        let init_readings = HashMap::from([(0, readings)]);
        let init_hotspots = HashMap::from([(0, 72.5)]);

        let devices = repo
            .retrieve_nvml_devices(1, &init_readings, &init_hotspots)
            .unwrap();

        assert_eq!(devices.len(), 1);
        let device = devices.values().next().unwrap().borrow();
        assert_eq!(device.name, "RTX 5090");
        let mut temps: Vec<&str> = device.info.temps.keys().map(String::as_str).collect();
        temps.sort_unstable();
        assert_eq!(
            temps,
            [GPU_TEMP_NAME, GPU_TEMP_HOTSPOT_NAME, GPU_TEMP_MEMORY_NAME]
        );
        let status = device.latest_status().unwrap();
        let channels: Vec<&str> = status.channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(
            channels,
            [
                "fan1",
                "fan2",
                GPU_LOAD_NAME,
                NVIDIA_CLOCK_GRAPHICS,
                NVIDIA_CLOCK_MEMORY,
                GPU_POWER_NAME
            ]
        );
        assert_eq!(status.channels[0].rpm, Some(1200));
        assert_eq!(status.channels[5].watts, Some(85.0));
        let driver = &device.info.driver_info;
        assert_eq!(driver.name.as_deref(), Some("nvml:13.610.57"));
        assert_eq!(driver.locations, ["00000000:01:00.0"]);
        let nv_info = repo.nvidia_device_infos.values().next().unwrap();
        assert_eq!(nv_info.fan_indices, [0, 1]);
        assert_eq!(nv_info.hotspot_pci_bus, Some(1));
    }

    #[test]
    fn a_gpu_that_does_not_answer_its_init_reads_loses_its_worker() {
        // Goal: a GPU that hangs in its first reads costs only itself: it builds no device and
        // loses its worker, so nothing later queues behind the hang. Method: a worker that never
        // answers, and a short budget.
        crate::rt::test_runtime(async {
            let mut repo = repo_with_disabled(&[]);
            let (io, _rx) = NvmlIo::for_test("test-gpu");
            repo.nvml_io.insert(0, io);

            let readings = repo.read_init_readings(Duration::from_millis(20)).await;

            assert!(readings.is_empty());
            assert!(repo.nvml_io.is_empty());
        });
    }
}
