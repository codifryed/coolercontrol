// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::device_health::UnreachableRef;
use crate::repositories::hwmon::device_io::{self, DeviceHealth, DeviceIo};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::cc_fs;
use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelKind, ChannelStatus, Device, DeviceInfo, DeviceType, DriverInfo,
    DriverType, Status, TempInfo, TempStatus, Watts, UID,
};
use crate::overrides::OverridesController;
use crate::repositories::cpu::association::{
    self, CpuAssociation, CpuDriver, DriverCensus, ZoneID,
};
use crate::repositories::cpu::percent::{CpuPercent, CpuPercentCollector};
use crate::repositories::cpu::topology::{self, CpuFreqs, CpuTopology, PhysicalID};
use crate::repositories::cpu::{CPU_DEVICE_NAMES_ORDERED, CPU_TEMP_NAME, INTEL_DEVICE_NAME};
use crate::repositories::hwmon::chip_name::{self, ChipName};
use crate::repositories::hwmon::hwmon_repo::{
    install_read_registry, HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
};
use crate::repositories::hwmon::{devices, power_cap, temps};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{CCDeviceSettings, LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, log, trace};
use std::time::Instant;

const SINGLE_CPU_LOAD_NAME: &str = "CPU Load";
const SINGLE_CPU_FREQ_AVG_NAME: &str = "CPU Freq Avg";
const SINGLE_CPU_FREQ_MAX_NAME: &str = "CPU Freq Max";
const SINGLE_CPU_FREQ_MIN_NAME: &str = "CPU Freq Min";
const CPUINFO_PATH: &str = "/proc/cpuinfo";

/// A CPU device and what it was built from. The association is kept rather than recovered from
/// `type_index`, so the `+ 1` that numbers a device lives in `device_type_index` alone.
struct CpuDevice {
    association: CpuAssociation,
    type_index: u8,
    device: DeviceLock,
    driver: Rc<HwmonDriverInfo>,
}

/// A CPU Repository for CPU status
pub struct CpuRepo {
    config: Rc<Config>,
    /// Owns the lm-sensors labels for the CPU chips, which the hwmon repo leaves to us.
    overrides: Rc<OverridesController>,
    devices: HashMap<UID, CpuDevice>,
    /// What cpuinfo says about this machine's processors. Empty until `initialize_devices` reads
    /// it, which every other path here runs after.
    topology: CpuTopology,
    cpu_percent_collector: RefCell<CpuPercentCollector>,
    /// The latest load sample for every processor, taken once per poll and shared by all packages.
    cpu_load_percents: RefCell<Vec<CpuPercent>>,
    /// Keyed by association, so a zone-keyed device cannot be confused with the socket whose
    /// physical id happens to be the same number.
    preloaded_statuses: RefCell<HashMap<CpuAssociation, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    energy_counters: HashMap<PhysicalID, Cell<f64>>,
    poll_rate: f64,
}

impl CpuRepo {
    pub fn new(config: Rc<Config>, overrides: Rc<OverridesController>) -> Result<Self> {
        Ok(Self {
            config,
            overrides,
            devices: HashMap::new(),
            topology: CpuTopology::default(),
            cpu_percent_collector: RefCell::new(CpuPercentCollector::new()?),
            cpu_load_percents: RefCell::new(Vec::new()),
            preloaded_statuses: RefCell::new(HashMap::new()),
            energy_counters: HashMap::new(),
            poll_rate: 0.,
        })
    }

    /// The temp label to show for a CPU sensor.
    ///
    /// A label from the user's lm-sensors configuration is used exactly as written, without the
    /// title-casing or the `CPU Temp` prefix we add to driver labels: the user already said what
    /// they want this sensor called, and it is shown under the CPU device either way.
    fn resolve_temp_label(&self, chip: Option<&ChipName>, channel: &HwmonChannelInfo) -> String {
        if let Some((label, source)) = self
            .overrides
            .sensors_conf_label_source(chip, &channel.name)
        {
            let chip_name = chip.map(ToString::to_string).unwrap_or_default();
            info!(
                "Labeling CPU channel {} of {chip_name} as \"{label}\": from {}",
                channel.name,
                source.display()
            );
            return label.to_owned();
        }
        let label_base = channel
            .label
            .as_ref()
            .map_or_else(|| channel.name.to_title_case(), |l| l.to_title_case());
        format!("{CPU_TEMP_NAME} {label_base}")
    }

    /// Drops every channel the user hides: those disabled in `CoolerControl`, and those the
    /// lm-sensors configuration ignores.
    async fn retain_visible_channels(
        &self,
        channels: Vec<HwmonChannelInfo>,
        cc_device_setting: Option<&CCDeviceSettings>,
        path: &Path,
    ) -> Vec<HwmonChannelInfo> {
        let disabled_channels =
            cc_device_setting.map_or_else(Vec::new, CCDeviceSettings::get_disabled_channels);
        let mut channels = channels
            .into_iter()
            .filter(|channel| disabled_channels.contains(&channel.name).not())
            .collect::<Vec<HwmonChannelInfo>>();
        let chip = chip_name::derive(path).await;
        self.drop_ignored_channels(chip.as_ref(), &mut channels);
        channels
    }

    /// Drops the CPU channels the user's lm-sensors configuration ignores. Relabelling and hiding
    /// `coretemp` sensors are both common things to do in that file, so the CPU repository
    /// honours it exactly as the hwmon one does.
    fn drop_ignored_channels(&self, chip: Option<&ChipName>, channels: &mut Vec<HwmonChannelInfo>) {
        channels.retain(|channel| {
            let Some(source) = self
                .overrides
                .sensors_conf_ignore_source(chip, &channel.name)
            else {
                return true;
            };
            let chip_name = chip.map(ToString::to_string).unwrap_or_default();
            info!(
                "Hiding CPU channel {} of {chip_name}: ignored by {}",
                channel.name,
                source.display()
            );
            false
        });
    }

    /// Reads cpuinfo and takes what it says about this machine's processors.
    async fn set_topology(&mut self, cpuinfo_path: &Path) -> Result<()> {
        let cpu_info_data = cc_fs::read_txt(cpuinfo_path).await?;
        self.topology = CpuTopology::parse(&cpu_info_data)?;
        trace!("CPU topology: {:?}", self.topology);
        Ok(())
    }

    /// Takes one load sample for every processor, which all packages share for this poll.
    ///
    /// A sample per package would give every package after the first a near zero interval. The
    /// processor map is refreshed here when processors went offline or came online, so load
    /// follows them at runtime.
    async fn sample_cpu_load(&self) {
        let (percents, owners_are_current) = {
            let mut collector = self.cpu_percent_collector.borrow_mut();
            let percents = collector.cpu_percent_per_cpu().unwrap_or_default();
            let owners_are_current = self.topology.owners_are_current(collector.online_cpu_ids());
            (percents, owners_are_current)
        };
        if owners_are_current.not() {
            self.refresh_processor_owners().await;
        }
        *self.cpu_load_percents.borrow_mut() = percents;
    }

    /// Re-reads which physical id each online processor belongs to.
    async fn refresh_processor_owners(&self) {
        let Ok(cpu_info_data) = cc_fs::read_txt(CPUINFO_PATH).await else {
            debug!("Could not read cpuinfo to refresh the online processors");
            return;
        };
        match self.topology.refresh_owners(&cpu_info_data) {
            Ok(Some(processor_counts)) => {
                info!("Processor counts have changed and been updated to: {processor_counts:?}");
            }
            Ok(None) => (), // The same processors are online, so nothing to update.
            Err(err) => error!("Could not refresh the online processors: {err}"),
        }
    }

    async fn init_cpu_temp(path: &Path, io: &DeviceIo) -> Result<Vec<HwmonChannelInfo>> {
        let include_all_devices = "";
        temps::init_temps(path, include_all_devices, io).await
    }

    /// Counts a driver's devices, and for `coretemp` checks whether any zone is offline.
    async fn driver_census(
        driver_name: &str,
        potential_cpu_paths: &[(String, PathBuf)],
    ) -> DriverCensus {
        let driver_paths = potential_cpu_paths
            .iter()
            .filter(|(device_name, _)| device_name == driver_name)
            .map(|(_, path)| path);
        if CpuDriver::of(driver_name) == CpuDriver::Amd {
            return DriverCensus {
                device_count: driver_paths.count(),
                all_zones_online: true,
                multi_die: false,
            };
        }
        let zone_ids = driver_paths
            .filter_map(|path| devices::get_platform_device_id(path))
            .collect::<Vec<ZoneID>>();
        let platform_zone_count = devices::count_platform_devices(INTEL_DEVICE_NAME);
        let all_zones_online = association::zones_all_online(&zone_ids, platform_zone_count);
        // The die check reads every online CPU, and only the offline fallback needs it.
        let multi_die = all_zones_online.not() && Self::any_cpu_on_a_later_die().await;
        DriverCensus {
            device_count: zone_ids.len(),
            all_zones_online,
            multi_die,
        }
    }

    /// The driver's own id for this device, read from where that driver puts it: a `coretemp`
    /// platform instance id, or the PCI slot an AMD node sits at.
    fn driver_zone_id(driver: CpuDriver, path: &Path) -> Option<ZoneID> {
        match driver {
            CpuDriver::Intel => devices::get_platform_device_id(path),
            CpuDriver::Amd => devices::get_amd_node_id(path),
        }
    }

    /// Whether any online CPU reports a die id above 0, i.e. its package holds several dies.
    /// A kernel without `die_id` (before 5.2) predates multi-die support, so it reads as single.
    /// Only asked for `coretemp`: on current AMD a die is a CCD, which a single desktop part
    /// already has two of.
    async fn any_cpu_on_a_later_die() -> bool {
        let pattern = "/sys/devices/system/cpu/cpu[0-9]*/topology/die_id";
        let Ok(paths) = nu_glob::glob(pattern, nu_glob::Uninterruptible) else {
            return false;
        };
        for path in paths.filter_map(Result::ok) {
            let Ok(die_id) = cc_fs::read_txt(&path).await else {
                continue;
            };
            if die_id.trim().parse::<u32>().is_ok_and(|die_id| die_id > 0) {
                return true;
            }
        }
        false
    }

    /// The channels that only make sense once a device is tied to a processor: load, frequency
    /// and package power are all keyed by cpuinfo's physical id, not by the hwmon device.
    async fn init_socket_channels(
        &self,
        physical_id: PhysicalID,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
    ) -> Vec<HwmonChannelInfo> {
        let mut channels = Vec::with_capacity(5); // one load, up to three freqs, one power
        match self.init_cpu_load(physical_id) {
            Some(load) => channels.push(load),
            None => error!("No CPU load reading for physical processor {physical_id}"),
        }
        if cpu_freqs.is_empty().not() {
            match Self::init_cpu_freq(physical_id, cpu_freqs) {
                Ok(freqs) => channels.extend(freqs),
                Err(err) => error!("Error matching cpu frequencies to processors: {err}"),
            }
        }
        match power_cap::find_power_cap_paths().await {
            Ok(power_channels) => {
                if let Some(channel) = power_channels
                    .into_iter()
                    .find(|channel| channel.number == physical_id)
                {
                    channels.push(channel);
                }
            }
            Err(err) => debug!("Error finding power cap paths: {err}"),
        }
        channels
    }

    /// The load of one package: the average of its own online processors, from the sample taken
    /// once per poll by `sample_cpu_load`.
    ///
    /// Every status carries every channel the device has, so a package with no reading this poll
    /// reports 0 rather than leaving the channel out. That happens when all its processors are
    /// offline, which is a true 0, or for the one poll after they come back online.
    fn collect_load(&self, physical_id: PhysicalID, channel_name: &str) -> ChannelStatus {
        let load = self
            .topology
            .package_load_percent(&self.cpu_load_percents.borrow(), physical_id)
            .unwrap_or(0.0);
        ChannelStatus {
            name: channel_name.to_string(),
            duty: Some(load),
            ..Default::default()
        }
    }

    /// The frequencies of every package, read fresh from cpuinfo.
    async fn collect_freq(cpuinfo_path: &Path) -> HashMap<PhysicalID, CpuFreqs> {
        let Ok(cpu_info_data) = cc_fs::read_txt(cpuinfo_path).await else {
            return HashMap::new();
        };
        topology::parse_freqs(&cpu_info_data)
    }

    fn get_status_from_freq_output(
        physical_id: PhysicalID,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
    ) -> Option<Vec<ChannelStatus>> {
        cpu_freqs.remove(&physical_id).map(|freqs| {
            vec![
                ChannelStatus {
                    name: SINGLE_CPU_FREQ_AVG_NAME.to_string(),
                    freq: Some(freqs.avg),
                    ..Default::default()
                },
                ChannelStatus {
                    name: SINGLE_CPU_FREQ_MAX_NAME.to_string(),
                    freq: Some(freqs.max),
                    ..Default::default()
                },
                ChannelStatus {
                    name: SINGLE_CPU_FREQ_MIN_NAME.to_string(),
                    freq: Some(freqs.min),
                    ..Default::default()
                },
            ]
        })
    }

    /// The load channel exists only if the package had a load reading at initialization. From
    /// then on `collect_load` reports it in every poll, as with every other channel.
    fn init_cpu_load(&self, physical_id: PhysicalID) -> Option<HwmonChannelInfo> {
        self.topology
            .package_load_percent(&self.cpu_load_percents.borrow(), physical_id)?;
        Some(HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Load,
            number: physical_id,
            name: SINGLE_CPU_LOAD_NAME.to_string(),
            label: Some(SINGLE_CPU_LOAD_NAME.to_string()),
            ..Default::default()
        })
    }

    fn init_cpu_freq(
        physical_id: PhysicalID,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
    ) -> Result<Vec<HwmonChannelInfo>> {
        if Self::get_status_from_freq_output(physical_id, cpu_freqs).is_none() {
            Err(anyhow!("Error: no frequency found!"))
        } else {
            Ok(vec![
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Freq,
                    // This gives us a unique number for each frequency channel, also for multiple physical cpus
                    number: physical_id * 64,
                    name: SINGLE_CPU_FREQ_AVG_NAME.to_string(),
                    label: Some(SINGLE_CPU_FREQ_AVG_NAME.to_string()),
                    ..Default::default()
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Freq,
                    number: physical_id * 64 + 1,
                    name: SINGLE_CPU_FREQ_MAX_NAME.to_string(),
                    label: Some(SINGLE_CPU_FREQ_MAX_NAME.to_string()),
                    ..Default::default()
                },
                HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Freq,
                    number: physical_id * 64 + 2,
                    name: SINGLE_CPU_FREQ_MIN_NAME.to_string(),
                    label: Some(SINGLE_CPU_FREQ_MIN_NAME.to_string()),
                    ..Default::default()
                },
            ])
        }
    }

    /// Every channel a CPU device has, for this poll.
    ///
    /// Load, frequency and power exist only on a device tied to a processor, so a zone-keyed
    /// device reports its temps alone. That is enforced by the channels it was built with, and
    /// again here by the physical id its association does not have.
    async fn request_status(
        &self,
        association: CpuAssociation,
        driver: &HwmonDriverInfo,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
        init: bool,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let status_channels = match association.physical_id() {
            // Only temps, which are read below, belong to a zone-keyed device.
            None => Vec::new(),
            Some(physical_id) => {
                self.collect_socket_channels(physical_id, association, driver, cpu_freqs, init)
                    .await
            }
        };
        let (read_temps, _) = temps::extract_temp_statuses(driver).await;
        let temp_names = driver
            .channels
            .iter()
            .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
            .map(|channel| channel.name.as_str());
        // The last known temps are only needed when nothing could be read.
        let preloaded_statuses = self.preloaded_statuses.borrow();
        let last_known_temps = preloaded_statuses
            .get(&association)
            .map_or(&[][..], |(_, temps)| temps.as_slice());
        let temps = Self::fill_missing_temps(temp_names, &read_temps, last_known_temps);
        (status_channels, temps)
    }

    /// The load, frequency and power channels of a device tied to a processor.
    async fn collect_socket_channels(
        &self,
        physical_id: PhysicalID,
        association: CpuAssociation,
        driver: &HwmonDriverInfo,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
        init: bool,
    ) -> Vec<ChannelStatus> {
        debug_assert_eq!(association.physical_id(), Some(physical_id));
        let mut status_channels = Vec::with_capacity(driver.channels.len());
        let mut contains_freq = false;
        for channel in &driver.channels {
            match channel.hwmon_type {
                HwmonChannelType::Load => {
                    status_channels.push(self.collect_load(physical_id, &channel.name));
                }
                HwmonChannelType::Freq => contains_freq = true,
                HwmonChannelType::PowerCap => {
                    let joule_count =
                        power_cap::extract_power_joule_counter(&driver.io, channel).await;
                    let mut watts = self.power_watts_or_zero(physical_id, joule_count);
                    self.use_cached_value_if_zero(&mut watts, init, association, &channel.name);
                    status_channels.push(ChannelStatus {
                        name: channel.name.clone(),
                        watts: Some(watts),
                        ..Default::default()
                    });
                }
                _ => (),
            }
        }
        if contains_freq {
            Self::get_filtered_freqs(physical_id, driver, cpu_freqs, &mut status_channels);
        }
        status_channels
    }

    /// Every temp channel the device has, for this poll, in channel order.
    ///
    /// Every status carries every channel, so a temp that could not be read is still reported.
    /// An offline core's temp is removed with the core, but the core is only parked in an idle
    /// state on the same die as the rest, so it takes the hottest temp the device read this
    /// poll. That follows the die down as it cools, so fans come down, and never reads below it.
    /// With nothing read at all, as when every core of an Intel package is offline and
    /// `coretemp` removes the device, each temp keeps its last known value: the package still
    /// draws idle power, so 0 would understate it.
    fn fill_missing_temps<'a>(
        temp_names: impl Iterator<Item = &'a str>,
        read_temps: &[TempStatus],
        last_known_temps: &[TempStatus],
    ) -> Vec<TempStatus> {
        let hottest_temp = read_temps
            .iter()
            .map(|temp_status| temp_status.temp)
            .max_by(f64::total_cmp);
        let mut temps = Vec::with_capacity(read_temps.len().max(last_known_temps.len()));
        for temp_name in temp_names {
            let find = |temps: &[TempStatus]| temps.iter().find(|t| t.name == temp_name).cloned();
            let temp_status = find(read_temps)
                .or_else(|| {
                    hottest_temp.map(|temp| TempStatus {
                        name: temp_name.to_owned(),
                        temp,
                    })
                })
                .or_else(|| find(last_known_temps));
            // No reading and nothing known before, which only happens at initialization.
            if let Some(temp_status) = temp_status {
                temps.push(temp_status);
            }
        }
        temps
    }

    /// The watts to report for a power channel this poll.
    ///
    /// Every status carries every channel the device has, so a failed counter read, or a
    /// processor with no counter, reports 0 rather than leaving the channel out. After
    /// initialization `use_cached_value_if_zero` swaps that 0 for the last reading. A failed read
    /// leaves the stored count alone, so the next good read's delta is not skewed by the gap.
    fn power_watts_or_zero(&self, cpu_id: PhysicalID, joule_count: Option<f64>) -> Watts {
        joule_count
            .and_then(|joule_count| self.power_watts_since_last_tick(cpu_id, joule_count))
            .unwrap_or(0.0)
    }

    /// The watts a power channel has drawn since the last tick.
    ///
    /// Returns `None` when this processor has no energy counter, so there is no previous count to
    /// take a delta from. Every device that carries a power channel is seeded with a counter at
    /// initialization, so this is a guard and not an expected path.
    fn power_watts_since_last_tick(&self, cpu_id: PhysicalID, joule_count: f64) -> Option<Watts> {
        let previous_joule_count = self.energy_counters.get(&cpu_id)?.replace(joule_count);
        Some(power_cap::calculate_power_watts(
            joule_count,
            previous_joule_count,
            self.poll_rate,
        ))
    }

    /// CPU power should rarely be 0, but it looks like the energy counter is either not
    /// consistently updated or is regularly reset, and so sometimes it is 0. For that case, we
    /// will reuse the preload-cached value.
    ///
    /// The device initialization request will return 0, but we can't use a cached value in that case.
    fn use_cached_value_if_zero(
        &self,
        watts: &mut Watts,
        init: bool,
        association: CpuAssociation,
        channel_name: &str,
    ) {
        if *watts < 0.01 && init.not() {
            debug!("CPU counter was measured at 0 watts");
            if let Some(preloaded_status) = self.preloaded_statuses.borrow().get(&association) {
                *watts = preloaded_status
                    .0
                    .iter()
                    .find_map(|channel_status| {
                        channel_status
                            .watts
                            .filter(|_| channel_status.name == channel_name)
                    })
                    .unwrap_or_default();
            }
        }
    }

    /// Retrieves the CPU freqs and filters out the ones that are not enabled/present.
    ///
    /// Every status carries every channel, so a package cpuinfo has no frequency for this poll
    /// reports 0 MHz. That only happens with every processor of the package offline, where 0 is
    /// the true value.
    fn get_filtered_freqs(
        phys_cpu_id: PhysicalID,
        driver: &HwmonDriverInfo,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
        status_channels: &mut Vec<ChannelStatus>,
    ) {
        let mut freq_status =
            Self::get_status_from_freq_output(phys_cpu_id, cpu_freqs).unwrap_or_default();
        for channel in &driver.channels {
            if channel.hwmon_type != HwmonChannelType::Freq {
                continue;
            }
            match freq_status.iter().position(|s| s.name == channel.name) {
                Some(freq_index) => status_channels.push(freq_status.swap_remove(freq_index)),
                None => status_channels.push(ChannelStatus {
                    name: channel.name.clone(),
                    freq: Some(0),
                    ..Default::default()
                }),
            }
        }
    }

    async fn get_potential_cpu_paths() -> Vec<(String, PathBuf)> {
        let mut potential_cpu_paths = Vec::new();
        for path in devices::find_all_hwmon_device_paths() {
            let device_name = devices::get_device_name(&path).await;
            if CPU_DEVICE_NAMES_ORDERED.contains(&device_name.as_str()) {
                potential_cpu_paths.push((device_name, path));
            }
        }
        potential_cpu_paths
    }

    async fn init_hwmon_cpu_devices(
        &mut self,
        potential_cpu_paths: Vec<(String, PathBuf)>,
    ) -> HashMap<CpuAssociation, HwmonDriverInfo> {
        let mut hwmon_devices = HashMap::new();
        let num_of_cpus = self.topology.package_count();
        let mut cpu_freqs = Self::collect_freq(CPUINFO_PATH.as_ref()).await;
        if cpu_freqs.is_empty() {
            // should warn for multi-cpus, but info otherwise
            let lvl = if num_of_cpus > 1 {
                log::Level::Warn
            } else {
                log::Level::Info
            };
            log!(lvl, "No CPU frequencies found in cpuinfo");
        }
        for cpu_device_name in CPU_DEVICE_NAMES_ORDERED {
            let census = Self::driver_census(cpu_device_name, &potential_cpu_paths).await;
            if census.all_zones_online.not() {
                info!(
                    "A {cpu_device_name} zone has no online CPU, so its CPU devices are matched by \
                    zone id alone. Census: {census:?}"
                );
            }
            for (device_name, path) in &potential_cpu_paths {
                if device_name != cpu_device_name {
                    continue;
                }
                let Some(association) =
                    self.associate_cpu_device(device_name, path, census, &hwmon_devices)
                else {
                    continue;
                };
                let Some(hwmon_driver_info) = self
                    .init_cpu_device(device_name, path, association, &mut cpu_freqs)
                    .await
                else {
                    continue;
                };
                hwmon_devices.insert(association, hwmon_driver_info);
            }
            // Checked once the driver's devices are all seen, so a multi-die package's later dies
            // are registered even after every processor has its socket. A lower priority driver
            // is only consulted for processors still unmatched.
            if association::unmatched_physical_ids(&self.topology, hwmon_devices.keys().copied())
                .is_empty()
            {
                break;
            }
        }
        hwmon_devices
    }

    /// Ties a CPU hwmon device to a processor, or returns `None` when it is to be skipped.
    fn associate_cpu_device(
        &self,
        device_name: &str,
        path: &Path,
        census: DriverCensus,
        registered: &HashMap<CpuAssociation, HwmonDriverInfo>,
    ) -> Option<CpuAssociation> {
        let driver = CpuDriver::of(device_name);
        let zone_id = Self::driver_zone_id(driver, path);
        let Some(association) = association::associate(&self.topology, driver, zone_id, census)
        else {
            info!(
                "Could not tie {device_name} at {} to a physical processor. Skipping device.",
                path.display()
            );
            return None;
        };
        if registered.contains_key(&association) {
            // Expected for the later dies of a single package, which all resolve to its one
            // socket, so this is not worth an info line on every start.
            debug!(
                "A CPU device is already registered for {association:?}. \
                Skipping {device_name} at {}.",
                path.display()
            );
            return None;
        }
        if association.is_socket().not() {
            // Temps are what a cooling app needs, so they are kept. Load, frequency and power
            // are keyed by physical id and belong to the processor's own device, so they are
            // left off rather than guessed or shown twice.
            info!(
                "{device_name} at {} is not a processor's own device. Its temps are shown on \
                their own, without processor load, frequency or power.",
                path.display()
            );
        }
        Some(association)
    }

    /// Builds the hwmon driver info for an associated CPU device, or returns `None` when the
    /// device cannot be numbered or named, or the user disabled it.
    async fn init_cpu_device(
        &self,
        device_name: &str,
        path: &Path,
        association: CpuAssociation,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
    ) -> Option<HwmonDriverInfo> {
        let Some(type_index) = association::device_type_index(&self.topology, association) else {
            error!(
                "No device number left for {association:?}. Skipping {device_name} at {}.",
                path.display()
            );
            return None;
        };
        // cpu_info is set first, filling in model names:
        let Some(cpu_name) = self.topology.model_name(association.physical_id()) else {
            error!("No CPU model name found. Skipping {device_name} device.");
            return None;
        };
        let device_uid = Device::create_uid_from(&cpu_name, DeviceType::CPU, type_index, None);
        let cc_device_setting = self
            .config
            .get_cc_settings_for_device(&device_uid)
            .unwrap_or(None);
        if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
            info!("Skipping disabled device: {cpu_name} with UID: {device_uid}");
            return None;
        }
        let mut channels = Vec::new();
        // Before any value read, so detection is isolated too.
        let io = DeviceIo::isolated_or_inline(
            device_name,
            device_io::reply_timeout_for(self.config.get_settings().map_or(1.0, |s| s.poll_rate)),
        );
        match Self::init_cpu_temp(path, &io).await {
            Ok(temps) => channels.extend(temps),
            Err(err) => error!("Error initializing CPU Temps: {err}"),
        }
        if let Some(physical_id) = association.physical_id() {
            channels.extend(self.init_socket_channels(physical_id, cpu_freqs).await);
        }
        let mut channels = self
            .retain_visible_channels(channels, cc_device_setting.as_ref(), path)
            .await;
        // Detection is done with this device's channel set, so the per-tick pass can address the
        // worker's table by slot from here on.
        if let Err(err) = install_read_registry(path, None, &mut channels, &io).await {
            error!("Could not install the read table for {cpu_name}: {err}");
        }
        let pci_device_names = devices::get_device_pci_names(path).await;
        let model = devices::get_device_model_name(path).await.or_else(|| {
            pci_device_names.and_then(|names| names.subdevice_name.or(names.device_name))
        });
        let u_id = devices::get_device_unique_id(path, device_name).await;
        Some(HwmonDriverInfo {
            name: device_name.to_owned(),
            path: path.to_path_buf(),
            model,
            u_id,
            channels,
            io,
            ..Default::default()
        })
    }

    /// Seeds a processor's energy counter with a real reading, which `request_status` needs
    /// before it can take a delta. A failed initial read seeds 0, so the next good read still
    /// produces a valid forward delta.
    async fn seed_energy_counter(&mut self, physical_id: PhysicalID, driver: &HwmonDriverInfo) {
        for channel in driver.channels.iter().filter(|channel| {
            channel.hwmon_type == HwmonChannelType::PowerCap && channel.number == physical_id
        }) {
            let joule_count = power_cap::extract_power_joule_counter(&driver.io, channel)
                .await
                .unwrap_or(0.0);
            self.energy_counters
                .insert(physical_id, Cell::new(joule_count));
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
}

#[async_trait(?Send)]
impl Repository for CpuRepo {
    fn device_type(&self) -> DeviceType {
        DeviceType::CPU
    }

    #[allow(clippy::too_many_lines)]
    async fn initialize_devices(&mut self) -> Result<()> {
        debug!("Starting Device Initialization");
        let start_initialization = Instant::now();
        self.poll_rate = self.config.get_settings()?.poll_rate;
        self.set_topology(CPUINFO_PATH.as_ref()).await?;
        // The first load status is read during initialization, so it needs a sample already.
        self.sample_cpu_load().await;
        let potential_cpu_paths = Self::get_potential_cpu_paths().await;

        let num_of_cpus = self.topology.package_count();
        let hwmon_devices = self.init_hwmon_cpu_devices(potential_cpu_paths).await;
        if hwmon_devices.is_empty() {
            info!("No CPU specific HWMON devices found.");
        } else {
            let missing_ids =
                association::unmatched_physical_ids(&self.topology, hwmon_devices.keys().copied());
            if missing_ids.is_empty().not() {
                // Load, frequency and power are only shown on a device tied to its processor, so
                // these processors have none. Any temps are kept under their driver zone. There
                // is nothing for the user to act on.
                info!(
                    "No CPU HWMON device is tied to physical processor(s) {missing_ids:?}, so \
                    they show no load, frequency or power. cpuinfo count: {num_of_cpus}, \
                    hwmon devices found: {}",
                    hwmon_devices.len()
                );
            }
        }

        let mut cpu_freqs = Self::collect_freq(CPUINFO_PATH.as_ref()).await;
        // A zone-keyed device has no cpuinfo entry of its own. The name has to be resolved the
        // same way it was when the device was built, or every such device would be dropped here.
        for (association, driver) in hwmon_devices {
            let Some(cpu_name) = self.topology.model_name(association.physical_id()) else {
                error!("No CPU model name for {association:?}. Skipping device.");
                continue;
            };
            let Some(type_index) = association::device_type_index(&self.topology, association)
            else {
                error!("No device number left for {association:?}. Skipping device.");
                continue;
            };
            // Only a processor's own device has a package power channel, and the counter is per
            // processor, so a zone-keyed device seeds nothing.
            if let Some(physical_id) = association.physical_id() {
                self.seed_energy_counter(physical_id, &driver).await;
            }
            let (channels, temps) = self
                .request_status(association, &driver, &mut cpu_freqs, true)
                .await;
            self.preloaded_statuses
                .borrow_mut()
                .insert(association, (channels.clone(), temps.clone()));
            let chip = chip_name::derive(&driver.path).await;
            let temp_infos = driver
                .channels
                .iter()
                .filter(|channel| channel.hwmon_type == HwmonChannelType::Temp)
                .map(|channel| {
                    (
                        channel.name.clone(),
                        TempInfo {
                            label: self.resolve_temp_label(chip.as_ref(), channel),
                            number: channel.number,
                        },
                    )
                })
                .collect();
            let mut channel_infos = HashMap::new();
            for channel in &driver.channels {
                match channel.hwmon_type {
                    HwmonChannelType::Load
                    | HwmonChannelType::Freq
                    | HwmonChannelType::PowerCap => {
                        channel_infos.insert(
                            channel.name.clone(),
                            ChannelInfo {
                                label: channel.label.clone(),
                                kind: ChannelKind::InfoOnly,
                            },
                        );
                    }
                    _ => (),
                }
            }
            let mut device = Device::new(
                cpu_name,
                DeviceType::CPU,
                type_index,
                None,
                DeviceInfo {
                    channels: channel_infos,
                    temps: temp_infos,
                    temp_max: 100,
                    driver_info: DriverInfo {
                        drv_type: DriverType::Kernel,
                        name: devices::get_device_driver_name(&driver.path).await,
                        version: sysinfo::System::kernel_version(),
                        locations: Self::get_driver_locations(&driver.path).await,
                    },
                    ..Default::default()
                },
                None,
                self.poll_rate,
            );
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            device.initialize_status_history_with(status, self.poll_rate);
            self.devices.insert(
                device.uid.clone(),
                CpuDevice {
                    association,
                    type_index,
                    device: Rc::new(RefCell::new(device)),
                    driver: Rc::new(driver),
                },
            );
        }

        let mut init_devices = HashMap::new();
        for (uid, cpu_device) in &self.devices {
            init_devices.insert(
                uid.clone(),
                (
                    cpu_device.device.borrow().clone(),
                    cpu_device.driver.clone(),
                ),
            );
        }
        if log::max_level() == log::LevelFilter::Debug {
            info!("Initialized CPU Devices: {init_devices:?}");
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
                            ("channels", {
                                let mut ch: Vec<_> = d.1 .0.info.channels.keys().cloned().collect();
                                ch.sort();
                                ch
                            }),
                            ("temps", {
                                let mut t: Vec<_> = d.1 .0.info.temps.keys().cloned().collect();
                                t.sort();
                                t
                            }),
                        ]),
                    )
                })
                .collect();
            info!(
                "Initialized CPU Devices: {}",
                serde_json::to_string(&device_map).unwrap_or_default()
            );
        }
        trace!(
            "Time taken to initialize all CPU devices: {:?}",
            start_initialization.elapsed()
        );
        debug!("CPU Repository initialized");
        Ok(())
    }

    async fn devices(&self) -> DeviceList {
        self.devices
            .values()
            .map(|cpu_device| cpu_device.device.clone())
            .collect()
    }

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        let mut cpu_freqs = Self::collect_freq(CPUINFO_PATH.as_ref()).await;
        self.sample_cpu_load().await;
        moro_local::async_scope!(|scope| {
            for cpu_device in self.devices.values() {
                let association = cpu_device.association;
                let driver = &cpu_device.driver;
                let mut cpu_freq = HashMap::new();
                if let Some(physical_id) = association.physical_id() {
                    if let Some(freq) = cpu_freqs.remove(&physical_id) {
                        cpu_freq.insert(physical_id, freq);
                    }
                }
                let self = Rc::clone(&self);
                scope.spawn(async move {
                    let (channels, temps) = self
                        .request_status(association, driver, &mut cpu_freq, false)
                        .await;
                    self.preloaded_statuses
                        .borrow_mut()
                        .insert(association, (channels, temps));
                });
            }
        })
        .await;
        trace!(
            "STATUS PRELOAD Time taken for all CPU devices: {:?}",
            start_update.elapsed()
        );
    }

    async fn update_statuses(&self) -> Result<()> {
        for cpu_device in self.devices.values() {
            let preloaded_statuses_map = self.preloaded_statuses.borrow();
            let Some((channels, temps)) = preloaded_statuses_map.get(&cpu_device.association)
            else {
                error!(
                    "There is no status preloaded for this device: {:?}",
                    cpu_device.association
                );
                continue;
            };
            let status = Status {
                temps: temps.clone(),
                channels: channels.clone(),
                ..Default::default()
            };
            let device_id = cpu_device.type_index;
            trace!("CPU device #{device_id} status was updated with: {status:?}");
            cpu_device.device.borrow_mut().set_status(status);
        }
        Ok(())
    }

    fn unreachable_devices(&self) -> Vec<UnreachableRef> {
        let mut out = Vec::new();
        for (device_uid, cpu_device) in &self.devices {
            let DeviceHealth::Unreachable {
                consecutive_timeouts,
            } = cpu_device.driver.io.health()
            else {
                continue;
            };
            out.push(UnreachableRef {
                device_uid: device_uid.clone(),
                device_name: cpu_device.driver.name.clone(),
                consecutive_timeouts,
            });
        }
        out
    }

    async fn shutdown(&self) -> Result<()> {
        info!("CPU Repository shutdown");
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
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_speed_fixed(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _speed_fixed: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_speed_profile(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _temp_source: &TempSource,
        _speed_profile: &[(f64, u8)],
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_lighting(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lighting: &LightingSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_lcd(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _lcd: &LcdSettings,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn apply_setting_pwm_mode(
        &self,
        _device_uid: &UID,
        _channel_name: &str,
        _pwm_mode: u8,
    ) -> Result<()> {
        Err(anyhow!(
            "Applying settings is not supported for CPU devices"
        ))
    }

    async fn reinitialize_devices(&self) {
        error!("Reinitializing Devices is not supported for this Repository");
    }
}

#[cfg(test)]
mod tests {
    use crate::cc_fs;
    use crate::config::Config;
    use crate::device::{ChannelStatus, TempStatus};
    use crate::overrides::OverridesController;
    use crate::repositories::cpu::association::CpuAssociation;
    use crate::repositories::cpu::cpu_repo::{CpuRepo, SINGLE_CPU_LOAD_NAME};
    use crate::repositories::cpu::fixtures;
    use crate::repositories::cpu::percent::CpuPercent;
    use crate::repositories::cpu::topology::{CpuFreqs, CpuTopology};
    use crate::repositories::hwmon::hwmon_repo::{
        HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo,
    };
    use serial_test::serial;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::rc::Rc;

    /// A repository holding what the given cpuinfo says, which is all these tests need of it.
    /// No temp file and no cpuinfo read: the parsing itself is covered in `topology`.
    fn repo_from_cpuinfo(cpu_info_data: &str) -> CpuRepo {
        let test_config = Rc::new(Config::init_default_config().unwrap());
        let mut cpu_repo =
            CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
        cpu_repo.topology = CpuTopology::parse(cpu_info_data).unwrap();
        cpu_repo
    }

    /// Goal: a package gets a load channel only if it had a load reading at initialization, and
    /// once it has one the channel is in every status, even in a poll with no reading, since the
    /// rest of the app expects each timestamp to carry the same metrics. Method: the dual Xeon
    /// repo with a sample covering only package 0, as when package 1 is offline.
    #[test]
    #[serial]
    fn test_load_is_reported_in_every_poll() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(fixtures::INTEL_DOUBLE_CPU);
            *cpu_repo.cpu_load_percents.borrow_mut() = (0..16)
                .step_by(2)
                .map(|cpu_id| CpuPercent {
                    cpu_id,
                    percent: 50.0,
                })
                .collect();

            // when:
            let package_0 = cpu_repo.collect_load(0, "CPU Load");
            let package_1 = cpu_repo.collect_load(1, "CPU Load");

            // then:
            assert_eq!(package_0.duty, Some(50.0));
            assert_eq!(package_1.duty, Some(0.0));
            assert_eq!(package_1.name, "CPU Load");
            // then: had this been the sample at initialization, only package 0 gets the channel.
            assert!(cpu_repo.init_cpu_load(0).is_some());
            assert!(cpu_repo.init_cpu_load(1).is_none());
        });
    }

    /// Goal: a device is keyed by its association, not by arithmetic on its device number. A zone
    /// device's number used to be turned back into a physical id as `type_index - 1`, which is a
    /// number no processor owns, so nothing keyed by physical id may be reached through it.
    /// Method: a driver carrying a load channel, asked for as a socket and as a zone, with a
    /// cached status planted under the socket that the zone must not reach.
    #[test]
    #[serial]
    fn test_status_is_keyed_by_association_not_device_number() {
        cc_fs::test_runtime(async {
            // given: package 0 fully loaded, and a driver with a load channel.
            let cpu_repo = repo_from_cpuinfo(fixtures::INTEL_DOUBLE_CPU);
            *cpu_repo.cpu_load_percents.borrow_mut() = (0..16)
                .map(|cpu_id| CpuPercent {
                    cpu_id,
                    percent: 50.0,
                })
                .collect();
            let driver = HwmonDriverInfo {
                channels: vec![HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Load,
                    number: 0,
                    name: SINGLE_CPU_LOAD_NAME.to_string(),
                    ..Default::default()
                }],
                ..Default::default()
            };
            let mut freqs = HashMap::new();

            // when: the processor's own device.
            let (channels, _) = cpu_repo
                .request_status(CpuAssociation::Socket(0), &driver, &mut freqs, false)
                .await;

            // then: it reports the load of the processor it is tied to.
            assert_eq!(channels.len(), 1);
            assert_eq!(channels[0].duty, Some(50.0));

            // when: a zone device numbered so that the old `type_index - 1` would land on
            // physical id 0.
            let (channels, _) = cpu_repo
                .request_status(CpuAssociation::Zone(0), &driver, &mut freqs, false)
                .await;

            // then: nothing keyed by physical id is reported for it, not even a 0.
            assert!(channels.is_empty());
        });
    }

    fn temp(name: &str, temp: f64) -> TempStatus {
        TempStatus {
            name: name.to_owned(),
            temp,
        }
    }

    /// Goal: every temp channel is in every status. An unreadable temp takes the hottest temp
    /// read on the same device, since an offline core is parked on the same die as the rest. With
    /// nothing read at all, as for an Intel package with every core offline, each keeps its last
    /// known value. Method: three channels against each combination of reads and cache.
    #[test]
    fn test_fill_missing_temps() {
        let names = ["temp1", "temp2", "temp3"];
        let last_known = [
            temp("temp1", 70.0),
            temp("temp2", 65.0),
            temp("temp3", 60.0),
        ];

        // then: all read, so nothing is filled and the cache is ignored.
        let all_read = vec![
            temp("temp1", 50.0),
            temp("temp2", 45.0),
            temp("temp3", 40.0),
        ];
        assert_eq!(
            CpuRepo::fill_missing_temps(names.into_iter(), &all_read, &last_known),
            all_read
        );
        // then: an offline core takes the hottest temp read this poll, not its last value or 0.
        let core_offline = vec![temp("temp1", 50.0), temp("temp3", 40.0)];
        assert_eq!(
            CpuRepo::fill_missing_temps(names.into_iter(), &core_offline, &last_known),
            vec![
                temp("temp1", 50.0),
                temp("temp2", 50.0),
                temp("temp3", 40.0)
            ]
        );
        // then: nothing read, so every temp keeps its last known value.
        assert_eq!(
            CpuRepo::fill_missing_temps(names.into_iter(), &[], &last_known),
            last_known.to_vec()
        );
        // then: nothing read and nothing known, as at initialization, reports nothing.
        assert!(CpuRepo::fill_missing_temps(names.into_iter(), &[], &[]).is_empty());
    }

    /// Goal: a package's frequency channels are in every status, and report 0 MHz in a poll
    /// where cpuinfo has no frequency for it because every processor is offline. Method: a
    /// driver with the three frequency channels, with and without a frequency for package 1.
    #[test]
    fn test_offline_package_reports_zero_mhz() {
        // given:
        let driver = HwmonDriverInfo {
            channels: ["CPU Freq Avg", "CPU Freq Max", "CPU Freq Min"]
                .into_iter()
                .map(|name| HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Freq,
                    name: name.to_owned(),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        };
        let mut cpu_freqs = HashMap::from([(
            1,
            CpuFreqs {
                min: 800,
                max: 3000,
                avg: 1900,
            },
        )]);

        // then: online, so the real frequencies are reported.
        let mut online = Vec::new();
        CpuRepo::get_filtered_freqs(1, &driver, &mut cpu_freqs, &mut online);
        let freqs: Vec<Option<u32>> = online.iter().map(|status| status.freq).collect();
        assert_eq!(freqs, vec![Some(1900), Some(3000), Some(800)]);
        // then: offline, so each channel is still there, at 0 MHz.
        let mut offline = Vec::new();
        CpuRepo::get_filtered_freqs(1, &driver, &mut HashMap::new(), &mut offline);
        let names: Vec<&str> = offline.iter().map(|status| status.name.as_str()).collect();
        assert_eq!(names, vec!["CPU Freq Avg", "CPU Freq Max", "CPU Freq Min"]);
        assert!(offline.iter().all(|status| status.freq == Some(0)));
    }

    /// Goal: the power channel is in every status, even when the counter read fails, and the gap
    /// must not skew the next reading. After initialization the reported value is the last one
    /// cached, not 0. Method: a seeded repo with a failed read, a good read after it, and the
    /// cache fallback applied the way `request_status` applies it, at and after initialization.
    #[test]
    #[serial]
    fn test_power_is_reported_in_every_poll() {
        cc_fs::test_runtime(async {
            // given: a seeded counter at 10 joules, a one second poll, and 42 watts cached.
            let mut cpu_repo = repo_from_cpuinfo(fixtures::INTEL_DOUBLE_CPU);
            cpu_repo.poll_rate = 1.0;
            cpu_repo.energy_counters.insert(0, Cell::new(10.0));
            let cached = ChannelStatus {
                name: "CPU Power".to_string(),
                watts: Some(42.0),
                ..Default::default()
            };
            let socket_0 = CpuAssociation::Socket(0);
            cpu_repo
                .preloaded_statuses
                .borrow_mut()
                .insert(socket_0, (vec![cached], Vec::new()));

            // then: a failed read reports 0 and leaves the stored count alone.
            assert_eq!(cpu_repo.power_watts_or_zero(0, None), 0.0);
            assert_eq!(cpu_repo.energy_counters.get(&0).unwrap().get(), 10.0);
            // then: the next good read measures from the count before the gap.
            assert_eq!(cpu_repo.power_watts_or_zero(0, Some(25.0)), 15.0);
            // then: a processor with no counter reports 0 too.
            assert_eq!(cpu_repo.power_watts_or_zero(1, Some(25.0)), 0.0);

            // then: after initialization the 0 becomes the last cached reading.
            let mut watts = 0.0;
            cpu_repo.use_cached_value_if_zero(&mut watts, false, socket_0, "CPU Power");
            assert_eq!(watts, 42.0);
            // then: during initialization there is no cache to use yet, so 0 stands.
            let mut watts = 0.0;
            cpu_repo.use_cached_value_if_zero(&mut watts, true, socket_0, "CPU Power");
            assert_eq!(watts, 0.0);
            // then: a zone device numbered the same as this socket cannot read its cache.
            let mut watts = 0.0;
            cpu_repo.use_cached_value_if_zero(
                &mut watts,
                false,
                CpuAssociation::Zone(0),
                "CPU Power",
            );
            assert_eq!(watts, 0.0);
        });
    }

    /// Goal: a power channel whose processor has no seeded energy counter must not panic, and
    /// must not compute a delta from nothing. Method: a repo with one processor seeded, asked for
    /// that processor and for one it has no counter for.
    #[test]
    #[serial]
    fn test_power_watts_needs_a_seeded_energy_counter() {
        cc_fs::test_runtime(async {
            // given: a seeded counter at 10 joules and a one second poll rate.
            let mut cpu_repo = repo_from_cpuinfo(fixtures::INTEL_DOUBLE_CPU);
            cpu_repo.poll_rate = 1.0;
            cpu_repo.energy_counters.insert(0, Cell::new(10.0));

            // then: the delta against the seeded count becomes watts.
            assert_eq!(cpu_repo.power_watts_since_last_tick(0, 25.0), Some(15.0));
            // then: the counter has advanced, so the next tick measures from there.
            assert_eq!(cpu_repo.power_watts_since_last_tick(0, 30.0), Some(5.0));
            // then: a processor with no counter yields nothing at all, rather than 0 watts.
            assert_eq!(cpu_repo.power_watts_since_last_tick(1, 30.0), None);
        });
    }
}

#[cfg(test)]
mod sensors_conf_tests {
    use crate::config::Config;
    use crate::overrides::OverridesController;
    use crate::repositories::cpu::cpu_repo::CpuRepo;
    use crate::repositories::hwmon::chip_name::{Bus, ChipName};
    use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType};
    use crate::sensors_conf::SensorsConf;
    use std::rc::Rc;

    fn k10temp() -> ChipName {
        ChipName {
            prefix: "k10temp".to_owned(),
            bus: Bus::Pci { addr: 0x00c3 },
        }
    }

    fn repo_with(conf: SensorsConf) -> CpuRepo {
        let config = Rc::new(Config::init_default_config().unwrap());
        let overrides = OverridesController::empty().with_sensors_conf(Rc::new(conf));
        CpuRepo::new(config, Rc::new(overrides)).unwrap()
    }

    fn temp_channel_named(name: &str) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            number: 1,
            name: name.to_owned(),
            ..Default::default()
        }
    }

    fn temp_channel(label: Option<&str>) -> HwmonChannelInfo {
        HwmonChannelInfo {
            hwmon_type: HwmonChannelType::Temp,
            number: 1,
            name: "temp1".to_owned(),
            label: label.map(ToOwned::to_owned),
            ..Default::default()
        }
    }

    /// Goal: a configured label must replace the whole displayed name, prefix included. We add
    /// "CPU Temp" to driver labels to say what they are, which is not our business to do to a
    /// name the user chose.
    #[test]
    fn a_configured_label_replaces_the_prefixed_name() {
        let repo = repo_with(SensorsConf::from_config_text(
            "chip \"k10temp-*\"\n label temp1 \"Package\"\n",
        ));

        assert_eq!(
            repo.resolve_temp_label(Some(&k10temp()), &temp_channel(Some("Tctl"))),
            "Package"
        );
    }

    /// Goal: without a statement, CPU temps keep the prefixed and title-cased name they have had
    /// all along. Method: an empty configuration, an unidentified chip, and a temp with no
    /// driver label at all.
    #[test]
    fn without_a_statement_the_prefixed_name_stands() {
        let repo = repo_with(SensorsConf::default());

        assert_eq!(
            repo.resolve_temp_label(Some(&k10temp()), &temp_channel(Some("Tctl"))),
            "CPU Temp Tctl"
        );
        assert_eq!(
            repo.resolve_temp_label(None, &temp_channel(Some("Tctl"))),
            "CPU Temp Tctl"
        );
        assert_eq!(
            repo.resolve_temp_label(Some(&k10temp()), &temp_channel(None)),
            "CPU Temp Temp1"
        );
    }

    /// Goal: hiding a CPU sensor from the lm-sensors configuration must work the same as hiding
    /// any other, since `ignore` on `coretemp` is a common thing to write. Method: two ignored
    /// temps and one that nothing names.
    #[test]
    fn ignored_cpu_channels_are_dropped() {
        let repo = repo_with(SensorsConf::from_config_text(
            "chip \"k10temp-*\"\n ignore temp1\n ignore temp2\n",
        ));
        let mut channels = vec![
            temp_channel_named("temp1"),
            temp_channel_named("temp2"),
            temp_channel_named("temp3"),
        ];

        repo.drop_ignored_channels(Some(&k10temp()), &mut channels);

        let names: Vec<&str> = channels.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["temp3"]);
    }
}
