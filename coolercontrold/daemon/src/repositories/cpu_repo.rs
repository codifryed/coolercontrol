// SPDX-FileCopyrightText: 2022 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::cc_fs;
use crate::config::Config;
use crate::device::{
    ChannelInfo, ChannelKind, ChannelStatus, Device, DeviceInfo, DeviceType, DriverInfo,
    DriverType, Mhz, Status, TempInfo, TempStatus, Watts, UID,
};
use crate::overrides::OverridesController;
use crate::repositories::cpu_percent::CpuPercentCollector;
use crate::repositories::hwmon::chip_name::{self, ChipName};
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::repositories::hwmon::{devices, power_cap, temps};
use crate::repositories::repository::{DeviceList, DeviceLock, Repository};
use crate::setting::{CCDeviceSettings, LcdSettings, LightingSettings, TempSource};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use heck::ToTitleCase;
use log::{debug, error, info, log, trace, warn};
use std::time::Instant;

pub const CPU_TEMP_NAME: &str = "CPU Temp";
pub const CPU_POWER_NAME: &str = "CPU Power";
const SINGLE_CPU_LOAD_NAME: &str = "CPU Load";
const SINGLE_CPU_FREQ_AVG_NAME: &str = "CPU Freq Avg";
const SINGLE_CPU_FREQ_MAX_NAME: &str = "CPU Freq Max";
const SINGLE_CPU_FREQ_MIN_NAME: &str = "CPU Freq Min";
const INTEL_DEVICE_NAME: &str = "coretemp";
// cpu_device_names have a priority, and we want to return the first match
pub const CPU_DEVICE_NAMES_ORDERED: [&str; 4] = [
    "k10temp",         // standard AMD module
    INTEL_DEVICE_NAME, // standard Intel module
    "zenpower",        // zenpower AMD module
    "cpu_thermal",     // Raspberry Pi module
];
const CPUINFO_PATH: &str = "/proc/cpuinfo";
/// Packages on a dual-socket board, which is as wide as commodity x86 goes. Only a capacity hint,
/// so a larger machine still parses, it just grows the map once.
const EXPECTED_PACKAGE_COUNT: usize = 2;

// The ID of the actual physical CPU. On most systems, there is only one:
type PhysicalID = u8;
// A driver's own package zone or node id. Numbered by the driver, not by cpuinfo, so it is only
// a physical id on the hardware where the two happen to coincide:
type ZoneID = u8;
type ProcessorCount = u16; // the logical processor count (aka how many cores per physical cpu)

/// How confidently a CPU hwmon device is tied to a physical processor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum CpuAssociation {
    /// Tied to a cpuinfo physical id. Load, frequency and power all describe this processor.
    Socket(PhysicalID),
    /// Keyed by the driver's own zone id instead: nothing says which processor the temps came
    /// from, or they are a later die of a processor whose first die already holds its `Socket`.
    /// The temps are real, but the signals keyed by physical id are left off rather than attached
    /// to a guess or shown twice.
    Zone(ZoneID),
}

impl CpuAssociation {
    /// The id the device is keyed and numbered by, whichever kind it is. Only a `Socket` id is a
    /// physical id, so this must not be used to look anything up by physical id.
    fn device_id(self) -> u8 {
        match self {
            Self::Socket(id) | Self::Zone(id) => id,
        }
    }

    fn is_socket(self) -> bool {
        matches!(self, Self::Socket(_))
    }
}

#[derive(Default, Debug, PartialEq)]
struct CpuFreqs {
    min: Mhz,
    max: Mhz,
    avg: Mhz,
}

/// A CPU Repository for CPU status
pub struct CpuRepo {
    config: Rc<Config>,
    /// Owns the lm-sensors labels for the CPU chips, which the hwmon repo leaves to us.
    overrides: Rc<OverridesController>,
    devices: HashMap<UID, (DeviceLock, Rc<HwmonDriverInfo>)>,
    cpu_infos: HashMap<PhysicalID, Cell<ProcessorCount>>,
    cpu_model_names: HashMap<PhysicalID, String>,
    /// Physical ids in the order the kernel numbers package zones, i.e. ascending APIC id.
    cpu_apic_order: Vec<PhysicalID>,
    cpu_percent_collector: RefCell<CpuPercentCollector>,
    preloaded_statuses: RefCell<HashMap<u8, (Vec<ChannelStatus>, Vec<TempStatus>)>>,
    energy_counters: HashMap<PhysicalID, Cell<f64>>,
    poll_rate: f64,
}

impl CpuRepo {
    pub fn new(config: Rc<Config>, overrides: Rc<OverridesController>) -> Result<Self> {
        Ok(Self {
            config,
            overrides,
            devices: HashMap::new(),
            cpu_infos: HashMap::new(),
            cpu_model_names: HashMap::new(),
            cpu_apic_order: Vec::new(),
            cpu_percent_collector: RefCell::new(CpuPercentCollector::new()?),
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

    async fn set_cpu_infos(&mut self, cpuinfo_path: &Path) -> Result<()> {
        let cpu_info_data = cc_fs::read_txt(cpuinfo_path).await?;
        let mut physical_id: PhysicalID = 0;
        let mut model_name = "";
        let mut processor_count: ProcessorCount = 0;
        let mut processor_present = false;
        let mut physical_id_present = false;
        let mut model_name_present = false;
        for line in cpu_info_data.lines() {
            let mut it = line.split(':');
            let (key, value) = match (it.next(), it.next()) {
                (Some(key), Some(value)) => (key.trim(), value.trim()),
                _ => continue, // will skip empty lines and non-key-value lines
            };

            if key == "processor" {
                // processor_id = value.parse()?;
                processor_present = true;
            }
            if key == "model name" {
                model_name = value;
                model_name_present = true;
            }
            if key == "physical id" {
                physical_id = value.parse()?;
                physical_id_present = true;
            }
            if processor_present && physical_id_present && model_name_present {
                // after each processor's entry
                processor_count += 1;
                self.cpu_infos
                    .entry(physical_id)
                    .or_default()
                    .set(processor_count);
                self.cpu_model_names
                    .insert(physical_id, model_name.to_string());
                processor_present = false;
                physical_id_present = false;
                model_name_present = false;
            }
        }
        if self.cpu_infos.is_empty() && self.cpu_model_names.is_empty() {
            // Some CPUs, like the Raspberry Pi, don't have a physical id, so we need to fake one,
            // they do have a model name though.
            for line in cpu_info_data.lines() {
                let mut it = line.split(':');
                let (key, value) = match (it.next(), it.next()) {
                    (Some(key), Some(value)) => (key.trim(), value.trim()),
                    _ => continue, // will skip empty lines and non-key-value lines
                };
                if key == "processor" {
                    processor_count += 1;
                }
                if key == "Model" {
                    self.cpu_model_names.insert(0, value.to_string());
                }
            }
            self.cpu_infos.entry(0).or_default().set(processor_count);
        }
        if self.cpu_infos.is_empty().not() && self.cpu_model_names.is_empty().not() {
            self.cpu_apic_order = Self::order_physical_ids_by_apic(&cpu_info_data);
            trace!("CPUInfo: {:?}", self.cpu_infos);
            trace!("CPU APIC order: {:?}", self.cpu_apic_order);
            Ok(())
        } else {
            Err(anyhow!(
                "cpuinfo either not found or missing data on this system!"
            ))
        }
    }

    /// Updates the processor count based on the cpuinfo file.
    /// This is sometimes needed when the active processor count changes.
    /// This function expects that `set_cpu_infos` has already been run at initialization.
    async fn update_processor_count(&self, cpuinfo_path: &Path) -> Result<()> {
        let original_processor_counts = self.cpu_infos.clone();
        let cpu_info_data = cc_fs::read_txt(cpuinfo_path).await?;
        let mut physical_id: PhysicalID = 0;
        let mut processor_count: ProcessorCount = 0;
        let mut processor_present = false;
        let mut physical_id_present = false;
        for line in cpu_info_data.lines() {
            let mut it = line.split(':');
            let (key, value) = match (it.next(), it.next()) {
                (Some(key), Some(value)) => (key.trim(), value.trim()),
                _ => continue, // will skip empty lines and non-key-value lines
            };
            if key == "processor" {
                // processor_id = value.parse()?;
                processor_present = true;
            }
            if key == "physical id" {
                physical_id = value.parse()?;
                physical_id_present = true;
            }
            if processor_present && physical_id_present {
                // after each processor's entry
                processor_count += 1;
                self.cpu_infos
                    .get(&physical_id)
                    .with_context(|| {
                        format!("physical id ({physical_id}) not found. This shouldn't happen.")
                    })?
                    .set(processor_count);
                processor_present = false;
                physical_id_present = false;
            }
        }
        if self.cpu_infos.is_empty() && self.cpu_model_names.is_empty() {
            // Some CPUs, like the Raspberry Pi, don't have a physical id, so we need to fake one,
            // they do have a model name though.
            for line in cpu_info_data.lines() {
                let mut it = line.split(':');
                let (key, _value) = match (it.next(), it.next()) {
                    (Some(key), Some(value)) => (key.trim(), value.trim()),
                    _ => continue, // will skip empty lines and non-key-value lines
                };
                if key == "processor" {
                    processor_count += 1;
                }
            }
            self.cpu_infos
                .get(&0)
                .with_context(|| {
                    format!("Temp physical id ({physical_id}) not found. This shouldn't happen.")
                })?
                .set(processor_count);
        }
        if self.cpu_infos.is_empty().not() && self.cpu_model_names.is_empty().not() {
            if original_processor_counts != self.cpu_infos {
                info!(
                    "Processor counts have changed and been updated to: {:?}",
                    self.cpu_infos
                );
            }
            debug!("Updated CPUInfo: {:?}", self.cpu_infos);
            Ok(())
        } else {
            Err(anyhow!(
                "cpuinfo either not found or missing data on this system!"
            ))
        }
    }

    async fn init_cpu_temp(path: &Path) -> Result<Vec<HwmonChannelInfo>> {
        let include_all_devices = "";
        temps::init_temps(path, include_all_devices).await
    }

    /// Orders physical processors the way the kernel numbers package zones: by ascending APIC id.
    ///
    /// `topology_get_logical_id()` counts the set APIC id bits below a domain's own, so a
    /// `coretemp.N` zone number is that package's rank in this order. cpuinfo prints `apicid` in
    /// the same block as `physical id`, so wherever there is more than one package to tell apart,
    /// both are present.
    fn order_physical_ids_by_apic(cpu_info_data: &str) -> Vec<PhysicalID> {
        let mut lowest_apic_ids: HashMap<PhysicalID, u32> =
            HashMap::with_capacity(EXPECTED_PACKAGE_COUNT);
        let mut physical_id: Option<PhysicalID> = None;
        for line in cpu_info_data.lines() {
            let mut it = line.split(':');
            let (key, value) = match (it.next(), it.next()) {
                (Some(key), Some(value)) => (key.trim(), value.trim()),
                _ => continue, // will skip empty lines and non-key-value lines
            };
            if key == "physical id" {
                physical_id = value.parse().ok();
            } else if key == "apicid" {
                // cpuinfo prints physical id first, so this pairs with the current processor.
                // `initial apicid` is a different key and never lands here.
                let Some(current_physical_id) = physical_id.take() else {
                    continue;
                };
                let Ok(apic_id) = value.parse::<u32>() else {
                    continue;
                };
                lowest_apic_ids
                    .entry(current_physical_id)
                    .and_modify(|lowest| *lowest = (*lowest).min(apic_id))
                    .or_insert(apic_id);
            }
        }
        let mut ordered = lowest_apic_ids
            .into_iter()
            .collect::<Vec<(PhysicalID, u32)>>();
        // The physical id breaks ties so the order cannot vary between runs.
        ordered.sort_unstable_by_key(|(physical_id, apic_id)| (*apic_id, *physical_id));
        ordered
            .into_iter()
            .map(|(physical_id, _)| physical_id)
            .collect()
    }

    /// The model name to show for a CPU device.
    ///
    /// A device keyed by its package zone may have no cpuinfo entry under that id. Multi-socket
    /// x86 requires identical processors, so any known name describes every package. The lowest
    /// id is used rather than any, so the name and the UID derived from it cannot vary between
    /// runs with `HashMap` order.
    fn cpu_model_name(&self, cpu_id: PhysicalID) -> Option<String> {
        if let Some(model_name) = self.cpu_model_names.get(&cpu_id) {
            return Some(model_name.clone());
        }
        self.cpu_model_names
            .iter()
            .min_by_key(|(physical_id, _)| **physical_id)
            .map(|(_, model_name)| model_name.clone())
    }

    /// The 1-based number a CPU device is presented under, which its UID is derived from.
    ///
    /// A socket keeps `physical id + 1` so that existing device UIDs, and the settings saved
    /// against them, do not change. A zone is numbered past every physical id: a zone id and a
    /// physical id are different quantities, so the two would otherwise collide wherever cpuinfo
    /// numbers its packages sparsely. Keeping zones above the sockets also means the physical id
    /// that `preload_statuses` recovers as `type_index - 1` can never alias a real processor.
    fn device_type_index(&self, association: CpuAssociation) -> Option<u8> {
        match association {
            CpuAssociation::Socket(physical_id) => {
                debug_assert!(self.cpu_infos.contains_key(&physical_id));
                physical_id.checked_add(1)
            }
            CpuAssociation::Zone(zone_id) => {
                let highest_physical_id = self.cpu_infos.keys().max().copied()?;
                // One past the highest socket number, then the zone's own offset.
                let type_index = highest_physical_id.checked_add(2)?.checked_add(zone_id)?;
                debug_assert!(type_index > highest_physical_id + 1);
                Some(type_index)
            }
        }
    }

    /// The physical processors that no CPU hwmon device was matched to, ascending.
    fn unmatched_physical_ids(
        &self,
        matched: &HashMap<CpuAssociation, HwmonDriverInfo>,
    ) -> Vec<PhysicalID> {
        let mut unmatched = self
            .cpu_infos
            .keys()
            .filter(|physical_id| {
                matched
                    .contains_key(&CpuAssociation::Socket(**physical_id))
                    .not()
            })
            .copied()
            .collect::<Vec<PhysicalID>>();
        unmatched.sort_unstable();
        debug_assert!(unmatched.len() <= self.cpu_infos.len());
        unmatched
    }

    /// Ties a CPU hwmon device to the processor it measures. `driver_device_count` is how many
    /// devices this driver registered, which says whether it reports per package or per die.
    fn match_physical_id(
        &self,
        device_name: &str,
        path: &Path,
        driver_device_count: usize,
    ) -> Option<CpuAssociation> {
        if device_name == INTEL_DEVICE_NAME {
            self.match_intel_association(path, driver_device_count)
        } else {
            self.match_amd_association(path, driver_device_count)
        }
    }

    /// Intel registers one `coretemp` platform device per die zone, and the kernel numbers those
    /// zones by ascending APIC id, so the device's instance id divided by the dies per package is
    /// its package's rank in `cpu_apic_order`.
    ///
    /// The temp labels cannot answer this, which is why the `Package id N` label that this used
    /// to parse is gone. That label was the wrong quantity: `coretemp_device_add()` sets
    /// `pdata->pkg_id = zoneid`, so it prints the zone number and not cpuinfo's physical id. The
    /// two coincide on most hardware, which is the only reason parsing it worked. It is also
    /// absent entirely on packages without the PTS feature (pre-Sandy Bridge), and the remaining
    /// `Core N` labels carry core ids that repeat identically across sockets. The zone ranking
    /// gives the same answer wherever the label worked, and an answer where it did not.
    fn match_intel_association(&self, path: &Path, zone_count: usize) -> Option<CpuAssociation> {
        self.intel_association(devices::get_platform_device_id(path), zone_count)
    }

    /// The zone id and count are passed in rather than read here, so the ranking can be tested
    /// without a fake sysfs tree.
    fn intel_association(
        &self,
        zone_id: Option<ZoneID>,
        zone_count: usize,
    ) -> Option<CpuAssociation> {
        // A single package needs no ranking, and its physical id is not always 0.
        if self.cpu_infos.len() == 1 {
            return self
                .cpu_infos
                .keys()
                .next()
                .copied()
                .map(CpuAssociation::Socket);
        }
        let zone_id = zone_id?;
        // Logical die ids rank system wide and a package's dies are consecutive, since die bits
        // sit below package bits in the APIC id. This division is exact, not an assumption.
        let Some(package_rank) =
            Self::first_die_package_rank(zone_id, zone_count, self.cpu_infos.len())
        else {
            return Some(CpuAssociation::Zone(zone_id));
        };
        if let Some(physical_id) = self.cpu_apic_order.get(usize::from(package_rank)) {
            if self.cpu_infos.contains_key(physical_id) {
                return Some(CpuAssociation::Socket(*physical_id));
            }
        }
        // A zone past the ranking cannot be placed. The temps are still real, so keep them.
        Some(CpuAssociation::Zone(zone_id))
    }

    /// AMD registers one `k10temp` device per node against that node's northbridge or data
    /// fabric PCI function, which sits at a slot fixed by the hardware.
    ///
    /// The node id read from that slot replaces the hwmon enumeration index this used to assume.
    /// Hwmon paths are sorted as strings, so `hwmon10` precedes `hwmon2` and that index inverts
    /// as soon as a machine has ten or more hwmon devices.
    fn match_amd_association(&self, path: &Path, node_count: usize) -> Option<CpuAssociation> {
        // NOTE: the node cpulist was the other way to do this, and is not used due to an apparent
        // bug in the amd hwmon kernel driver. Kept as a reference to the alternative:
        // let cpu_list: Vec<ProcessorID> = devices::get_processor_ids_from_node_cpulist(index).await?;
        // for (physical_id, processor_list) in &self.cpu_infos {
        //     if cpu_list.iter().eq(processor_list.iter()) {
        //         return Ok(physical_id.clone());
        //     }
        // }
        self.amd_association(devices::get_amd_node_id(path), node_count)
    }

    /// The node id and count are passed in rather than read here, so the association can be
    /// tested without a fake sysfs tree.
    fn amd_association(
        &self,
        node_id: Option<ZoneID>,
        node_count: usize,
    ) -> Option<CpuAssociation> {
        // A single node needs no id at all, and its physical id is not always 0 (AMD APU).
        if self.cpu_infos.len() == 1 {
            return self
                .cpu_infos
                .keys()
                .next()
                .copied()
                .map(CpuAssociation::Socket);
        }
        let node_id = node_id?;
        // Pre-Zen multi-chip parts and Zen 1 EPYC hold several nodes per package. This assumes
        // their nodes are numbered package by package, as on Zen 1 where the node id is
        // {socket, die}. That is not confirmed from kernel source for every family.
        let Some(package_rank) =
            Self::first_die_package_rank(node_id, node_count, self.cpu_infos.len())
        else {
            return Some(CpuAssociation::Zone(node_id));
        };
        if self.cpu_infos.contains_key(&package_rank) {
            return Some(CpuAssociation::Socket(package_rank));
        }
        // A node cpuinfo has no physical id for cannot be placed. The temps are still real.
        Some(CpuAssociation::Zone(node_id))
    }

    /// The rank of the package a driver device is the first die of.
    ///
    /// The drivers register one device per die and number a package's dies consecutively, so
    /// each package holds `device_count / package_count` devices. Only a package's first die is
    /// tied to it: its later dies would repeat the same load, frequency and power. Returns `None`
    /// for a later die, and for an uneven split, which means a die is missing and the numbering
    /// cannot be trusted.
    fn first_die_package_rank(
        device_id: ZoneID,
        device_count: usize,
        package_count: usize,
    ) -> Option<u8> {
        if package_count == 0 {
            return None;
        }
        // One device per package, or fewer where some failed to register: each id is a rank.
        if device_count <= package_count {
            return Some(device_id);
        }
        if device_count.is_multiple_of(package_count).not() {
            return None;
        }
        let dies_per_package = device_count / package_count;
        debug_assert!(dies_per_package > 1);
        let device_index = usize::from(device_id);
        if device_index.is_multiple_of(dies_per_package).not() {
            return None;
        }
        let package_rank = u8::try_from(device_index / dies_per_package).ok()?;
        debug_assert!(package_rank < device_id || device_id == 0);
        Some(package_rank)
    }

    /// The channels that only make sense once a device is tied to a processor: load, frequency
    /// and package power are all keyed by cpuinfo's physical id, not by the hwmon device.
    async fn init_socket_channels(
        &self,
        physical_id: PhysicalID,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
    ) -> Vec<HwmonChannelInfo> {
        let mut channels = Vec::with_capacity(5); // one load, up to three freqs, one power
        match self.init_cpu_load(physical_id).await {
            Ok(load) => channels.push(load),
            Err(err) => error!("Error matching cpu load percents to processors: {err}"),
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

    /// We calculate total system load.
    #[allow(clippy::cast_precision_loss)]
    async fn collect_load(
        &self,
        physical_id: PhysicalID,
        channel_name: &str,
    ) -> Option<ChannelStatus> {
        let percents = self
            .cpu_percent_collector
            .borrow_mut()
            .cpu_percent_per_cpu()
            .unwrap_or_default();
        let num_percents = percents.len();
        let num_processors = self.cpu_infos.get(&physical_id)?.get() as usize;
        if num_percents != num_processors {
            // IF this is true, either something unexpected has happened, or the number of processors
            // has changed since we last collected data. (e.g. disabled at runtime)
            if let Err(err) = self.update_processor_count(CPUINFO_PATH.as_ref()).await {
                warn!("Failed to update processor count: {err}");
            }
            // recheck after updating
            if num_percents != self.cpu_infos.get(&physical_id)?.get() as usize {
                error!("Non-matching processors: {num_processors} and percents: {num_percents}");
                return None;
            }
        }
        let load = f64::from(percents.iter().sum::<f32>()) / num_processors as f64;
        Some(ChannelStatus {
            name: channel_name.to_string(),
            duty: Some(load),
            ..Default::default()
        })
    }

    /// Collects the average frequency per Physical CPU.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    async fn collect_freq(cpuinfo_path: &Path) -> HashMap<PhysicalID, CpuFreqs> {
        // There a few ways to get this info, but the most reliable is to read the /proc/cpuinfo.
        // cpuinfo not only will return which frequency belongs to which physical CPU,
        // which is important for CoolerControl's full multi-physical-cpu support,
        // but also it's cached and therefore consistently fast across various systems.
        // See: https://github.com/giampaolo/psutil/issues/1851
        // The alternative is to read one of:
        //   /sys/devices/system/cpu/cpu[0-9]*/cpufreq/scaling_cur_freq
        //  /sys/devices/system/cpu/cpufreq/policy[0-9]*/scaling_cur_freq
        // But these have been reported to be significantly slower on some systems, and it's not
        // clear how to associate the frequency with the physical CPU on multi-cpu systems.
        let mut cpu_freqs = HashMap::new();
        let mut cpu_info_freqs: HashMap<PhysicalID, Vec<f64>> = HashMap::new();
        let Ok(cpu_info) = cc_fs::read_txt(cpuinfo_path).await else {
            return cpu_freqs;
        };
        let mut cpu_info_physical_id: PhysicalID = 0;
        let mut cpu_info_freq: f64 = 0.;
        let mut physical_id_present = false;
        let mut freq_present = false;
        for line in cpu_info.lines() {
            if line.starts_with("physical id").not() && line.starts_with("cpu MHz").not() {
                continue;
            }
            let mut it = line.split(':');
            let (key, value) = match (it.next(), it.next()) {
                (Some(key), Some(value)) => (key.trim(), value.trim()),
                _ => continue,
            };
            if key == "physical id" {
                let Ok(phy_id) = value.parse() else {
                    return cpu_freqs;
                };
                cpu_info_physical_id = phy_id;
                physical_id_present = true;
            }
            if key == "cpu MHz" {
                let Ok(freq) = value.parse() else {
                    return cpu_freqs;
                };
                cpu_info_freq = freq;
                freq_present = true;
            }
            if physical_id_present && freq_present {
                // after each processor's entry
                cpu_info_freqs
                    .entry(cpu_info_physical_id)
                    .or_default()
                    .push(cpu_info_freq);
                physical_id_present = false;
                freq_present = false;
            }
        }
        for (physical_id, freqs) in cpu_info_freqs {
            let min = freqs
                .iter()
                .min_by(|a, b| a.total_cmp(b))
                .unwrap_or(&0.)
                .round() as Mhz;
            let max = freqs
                .iter()
                .max_by(|a, b| a.total_cmp(b))
                .unwrap_or(&0.)
                .round() as Mhz;
            #[allow(clippy::cast_precision_loss)]
            let avg = (freqs.iter().sum::<f64>() / freqs.len() as f64).round() as Mhz;
            cpu_freqs.insert(physical_id, CpuFreqs { min, max, avg });
        }
        cpu_freqs
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

    async fn init_cpu_load(&self, physical_id: PhysicalID) -> Result<HwmonChannelInfo> {
        if self
            .collect_load(physical_id, SINGLE_CPU_LOAD_NAME)
            .await
            .is_none()
        {
            Err(anyhow!("Error: no load percent found!"))
        } else {
            Ok(HwmonChannelInfo {
                hwmon_type: HwmonChannelType::Load,
                number: physical_id,
                name: SINGLE_CPU_LOAD_NAME.to_string(),
                label: Some(SINGLE_CPU_LOAD_NAME.to_string()),
                ..Default::default()
            })
        }
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

    async fn request_status(
        &self,
        phys_cpu_id: PhysicalID,
        driver: &HwmonDriverInfo,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
        init: bool,
    ) -> (Vec<ChannelStatus>, Vec<TempStatus>) {
        let mut status_channels = Vec::new();
        let mut contains_freq = false;
        for channel in &driver.channels {
            match channel.hwmon_type {
                HwmonChannelType::Load => {
                    let Some(load_status) = self.collect_load(phys_cpu_id, &channel.name).await
                    else {
                        continue;
                    };
                    status_channels.push(load_status);
                }
                HwmonChannelType::Freq => contains_freq = true,
                HwmonChannelType::PowerCap => {
                    // If the joule counter read fails, omit the channel
                    // status entry for this tick rather than fabricating
                    // 0 watts, and leave the prior energy counter intact
                    // so the next successful read's delta is not wildly
                    // skewed by a one-tick gap.
                    let Some(joule_count) =
                        power_cap::extract_power_joule_counter(&driver.fds, channel.number).await
                    else {
                        continue;
                    };
                    let Some(mut watts) =
                        self.power_watts_since_last_tick(phys_cpu_id, joule_count)
                    else {
                        continue;
                    };
                    self.use_cached_value_if_zero(&mut watts, init, phys_cpu_id, &channel.name);
                    let power_status = ChannelStatus {
                        name: channel.name.clone(),
                        watts: Some(watts),
                        ..Default::default()
                    };
                    status_channels.push(power_status);
                }
                _ => (),
            }
        }
        if contains_freq {
            Self::get_filtered_freqs(phys_cpu_id, driver, cpu_freqs, &mut status_channels);
        }
        let (extracted_temps, _) = temps::extract_temp_statuses(driver).await;
        let temps = extracted_temps
            .iter()
            .map(|temp| TempStatus {
                name: temp.name.clone(),
                temp: temp.temp,
            })
            .collect();
        (status_channels, temps)
    }

    /// The watts a power channel has drawn since the last tick.
    ///
    /// Returns `None` when this processor has no energy counter. There is then no previous count
    /// to take a delta from, and a fabricated 0 watts would be indistinguishable from a real
    /// reading, so the caller omits the channel for this tick the same way it does for a failed
    /// counter read. Every device that carries a power channel is seeded with a counter at
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
        physical_id: PhysicalID,
        channel_name: &str,
    ) {
        if *watts < 0.01 && !init {
            debug!("CPU counter was measured at 0 watts");
            let device_id = physical_id + 1;
            if let Some(preloaded_status) = self.preloaded_statuses.borrow().get(&device_id) {
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
    fn get_filtered_freqs(
        phys_cpu_id: PhysicalID,
        driver: &HwmonDriverInfo,
        cpu_freqs: &mut HashMap<PhysicalID, CpuFreqs>,
        status_channels: &mut Vec<ChannelStatus>,
    ) {
        let Some(mut freq_status) = Self::get_status_from_freq_output(phys_cpu_id, cpu_freqs)
        else {
            return;
        };
        for channel in &driver.channels {
            if channel.hwmon_type == HwmonChannelType::Freq {
                let Some(freq_index) = freq_status.iter().position(|s| s.name == channel.name)
                else {
                    continue;
                };
                status_channels.push(freq_status.swap_remove(freq_index));
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
        let num_of_cpus = self.cpu_infos.len();
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
            let driver_device_count = potential_cpu_paths
                .iter()
                .filter(|(device_name, _)| device_name == cpu_device_name)
                .count();
            for (device_name, path) in &potential_cpu_paths {
                if device_name != cpu_device_name {
                    continue;
                }
                let mut channels = Vec::new();
                match Self::init_cpu_temp(path).await {
                    Ok(temps) => channels.extend(temps),
                    Err(err) => error!("Error initializing CPU Temps: {err}"),
                }
                let Some(association) =
                    self.match_physical_id(device_name, path, driver_device_count)
                else {
                    info!(
                        "Could not tie {device_name} at {} to a physical processor. \
                        Skipping device.",
                        path.display()
                    );
                    continue;
                };
                let cpu_id = association.device_id();
                if hwmon_devices.contains_key(&association) {
                    // Expected for the later dies of a single package, which all resolve to its
                    // one socket, so this is not worth an info line on every start.
                    debug!(
                        "A CPU device is already registered for {association:?}. \
                        Skipping {device_name} at {}.",
                        path.display()
                    );
                    continue;
                }
                if association.is_socket().not() {
                    // Temps are what a cooling app needs, so they are kept. Load, frequency and
                    // power are keyed by physical id and belong to the processor's own device,
                    // so they are left off rather than guessed or shown twice.
                    info!(
                        "{device_name} at {} is not a processor's own device. Its temps are \
                        shown on their own, without processor load, frequency or power.",
                        path.display()
                    );
                }
                let Some(type_index) = self.device_type_index(association) else {
                    error!(
                        "No device number left for {association:?}. Skipping {device_name} at {}.",
                        path.display()
                    );
                    continue;
                };
                // cpu_info is set first, filling in model names:
                let Some(cpu_name) = self.cpu_model_name(cpu_id) else {
                    error!("No CPU model name found. Skipping {device_name} device.");
                    continue;
                };
                let device_uid =
                    Device::create_uid_from(&cpu_name, DeviceType::CPU, type_index, None);
                let cc_device_setting = self
                    .config
                    .get_cc_settings_for_device(&device_uid)
                    .unwrap_or(None);
                if cc_device_setting.is_some() && cc_device_setting.as_ref().unwrap().disable {
                    info!("Skipping disabled device: {cpu_name} with UID: {device_uid}");
                    continue;
                }
                if association.is_socket() {
                    channels.extend(self.init_socket_channels(cpu_id, &mut cpu_freqs).await);
                }
                let channels = self
                    .retain_visible_channels(channels, cc_device_setting.as_ref(), path)
                    .await;
                let pci_device_names = devices::get_device_pci_names(path).await;
                let model = devices::get_device_model_name(path).await.or_else(|| {
                    pci_device_names.and_then(|names| names.subdevice_name.or(names.device_name))
                });
                let u_id = devices::get_device_unique_id(path, device_name).await;
                let hwmon_driver_info = HwmonDriverInfo {
                    name: device_name.clone(),
                    path: path.clone(),
                    model,
                    u_id,
                    channels,
                    ..Default::default()
                };
                hwmon_devices.insert(association, hwmon_driver_info);
            }
            // Checked once the driver's devices are all seen, so a multi-die package's later dies
            // are registered even after every processor has its socket. A lower priority driver
            // is only consulted for processors still unmatched.
            if self.unmatched_physical_ids(&hwmon_devices).is_empty() {
                break;
            }
        }
        hwmon_devices
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
        self.set_cpu_infos(CPUINFO_PATH.as_ref()).await?;
        let potential_cpu_paths = Self::get_potential_cpu_paths().await;

        let num_of_cpus = self.cpu_infos.len();
        let hwmon_devices = self.init_hwmon_cpu_devices(potential_cpu_paths).await;
        if hwmon_devices.is_empty() {
            info!("No CPU specific HWMON devices found.");
        } else {
            let missing_ids = self.unmatched_physical_ids(&hwmon_devices);
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
        // These are keyed by `CpuAssociation::device_id()`, so a zone-keyed device has no cpuinfo
        // entry of its own. The name has to be resolved the same way it was when the device was
        // built, or every such device would be dropped here.
        for (association, driver) in hwmon_devices {
            let Some(cpu_name) = self.cpu_model_name(association.device_id()) else {
                error!("No CPU model name for {association:?}. Skipping device.");
                continue;
            };
            let Some(type_index) = self.device_type_index(association) else {
                error!("No device number left for {association:?}. Skipping device.");
                continue;
            };
            // `preload_statuses` recovers this same id from `type_index`, so both paths must
            // derive it the same way or a device's seeded counters would not be found again.
            let status_id = type_index - 1;
            for channel in driver.channels.iter().filter(|channel| {
                channel.hwmon_type == HwmonChannelType::PowerCap && channel.number == status_id
            }) {
                // Fill initial joule_count with a real count (needed before
                // request_status). If the initial read fails, seed with 0 so the
                // next successful read still produces a valid forward delta.
                let joule_count =
                    power_cap::extract_power_joule_counter(&driver.fds, channel.number)
                        .await
                        .unwrap_or(0.0);
                self.energy_counters
                    .insert(status_id, Cell::new(joule_count));
            }
            let (channels, temps) = self
                .request_status(status_id, &driver, &mut cpu_freqs, true)
                .await;
            self.preloaded_statuses
                .borrow_mut()
                .insert(type_index, (channels.clone(), temps.clone()));
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
                (Rc::new(RefCell::new(device)), Rc::new(driver)),
            );
        }

        let mut init_devices = HashMap::new();
        for (uid, (device, hwmon_info)) in &self.devices {
            init_devices.insert(uid.clone(), (device.borrow().clone(), hwmon_info.clone()));
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
            .map(|(device, _)| device.clone())
            .collect()
    }

    async fn preload_statuses(self: Rc<Self>) {
        let start_update = Instant::now();
        let mut cpu_freqs = Self::collect_freq(CPUINFO_PATH.as_ref()).await;
        moro_local::async_scope!(|scope| {
            for (device_lock, driver) in self.devices.values() {
                let device_id = device_lock.borrow().type_index;
                let physical_id = device_id - 1;
                let mut cpu_freq = HashMap::new();
                if let Some(freq) = cpu_freqs.remove(&physical_id) {
                    cpu_freq.insert(physical_id, freq);
                }
                let self = Rc::clone(&self);
                scope.spawn(async move {
                    let (channels, temps) = self
                        .request_status(physical_id, driver, &mut cpu_freq, false)
                        .await;
                    self.preloaded_statuses
                        .borrow_mut()
                        .insert(device_id, (channels, temps));
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
        for (device, _) in self.devices.values() {
            let device_id = device.borrow().type_index;
            let preloaded_statuses_map = self.preloaded_statuses.borrow();
            let preloaded_statuses = preloaded_statuses_map.get(&device_id);
            if preloaded_statuses.is_none() {
                error!("There is no status preloaded for this device: {device_id}");
                continue;
            }
            let (channels, temps) = preloaded_statuses.unwrap().clone();
            let status = Status {
                temps,
                channels,
                ..Default::default()
            };
            trace!("CPU device #{device_id} status was updated with: {status:?}");
            device.borrow_mut().set_status(status);
        }
        Ok(())
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
    use crate::device::{Device, DeviceType};
    use crate::overrides::OverridesController;
    use crate::repositories::cpu_repo::{CpuAssociation, CpuFreqs, CpuRepo, PhysicalID};
    use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;
    use serial_test::serial;
    use std::cell::Cell;
    use std::collections::HashMap;
    use std::ops::Not;
    use std::rc::Rc;

    /// CPU devices matched as sockets, the only kind that answers for a physical processor.
    fn matched(physical_ids: &[PhysicalID]) -> HashMap<CpuAssociation, HwmonDriverInfo> {
        physical_ids
            .iter()
            .map(|id| (CpuAssociation::Socket(*id), HwmonDriverInfo::default()))
            .collect()
    }

    static CPUINFO_AMD_SINGLE_CPU: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/tests/cpuinfo/amd_single_cpu"
    ));
    static CPUINFO_AMD_DOUBLE_CPU: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/tests/cpuinfo/amd_double_cpu"
    ));
    static CPUINFO_INTEL_SINGLE_CPU: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/tests/cpuinfo/intel_single_cpu"
    ));
    static CPUINFO_INTEL_DOUBLE_CPU: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/tests/cpuinfo/intel_double_cpu"
    ));
    /// Two packages whose physical ids run opposite to their APIC ids. Real firmware rarely does
    /// this, but it is the only shape that tells the two orderings apart.
    static CPUINFO_INVERTED_APIC: &str = concat!(
        "processor\t: 0\n",
        "model name\t: Test CPU\n",
        "physical id\t: 0\n",
        "apicid\t\t: 32\n",
        "\n",
        "processor\t: 1\n",
        "model name\t: Test CPU\n",
        "physical id\t: 1\n",
        "apicid\t\t: 0\n",
    );

    async fn repo_from_cpuinfo(cpu_info_data: Vec<u8>) -> CpuRepo {
        let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
        cc_fs::write(&test_cpuinfo, cpu_info_data).await.unwrap();
        let test_config = Rc::new(Config::init_default_config().unwrap());
        let mut cpu_repo =
            CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
        cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();
        cpu_repo
    }
    static CPUINFO_RASPBERRY_PI_5: &[u8] = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/tests/cpuinfo/raspberry_pi_5"
    ));

    #[test]
    #[serial]
    fn test_set_cpu_infos_amd_single_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_SINGLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();

            // when:
            let result = cpu_repo.set_cpu_infos(&test_cpuinfo).await;

            // then:
            assert!(result.is_ok(), "set_cpu_infos should return Ok: {result:?}");
            assert_eq!(
                cpu_repo.cpu_infos.len(),
                1,
                "cpu_infos should have 1 physical cpu entry"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.len(),
                1,
                "cpu_model_names should have 1 entry"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.get(&0).unwrap(),
                "AMD Ryzen 7 5800X 8-Core Processor",
                "cpu_model_names should have the correct model name"
            );
        });
    }

    #[test]
    #[serial]
    fn test_collect_freq_amd_single_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_SINGLE_CPU.to_vec())
                .await
                .unwrap();

            // when:
            let result = CpuRepo::collect_freq(&test_cpuinfo).await;

            // then:
            assert_eq!(
                result.len(),
                1,
                "collect_freq should have 1 physical cpu entry"
            );
            assert_eq!(
                result.get(&0),
                Some(&CpuFreqs {
                    avg: 3006,
                    max: 4200,
                    min: 1754,
                }),
                "collect_freq should have the correct average frequency"
            );
        });
    }

    #[test]
    #[serial]
    fn test_set_cpu_infos_amd_double_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_DOUBLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();

            // when:
            let result = cpu_repo.set_cpu_infos(&test_cpuinfo).await;

            // then:
            assert!(result.is_ok(), "set_cpu_infos should return Ok: {result:?}");
            assert_eq!(
                cpu_repo.cpu_infos.len(),
                2,
                "cpu_infos should have 2 physical cpu entries"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.len(),
                2,
                "cpu_model_names should have 2 entries"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.get(&0).unwrap(),
                "AMD Ryzen 7 5800X 8-Core Processor#1",
            );
            assert_eq!(
                cpu_repo.cpu_model_names.get(&1).unwrap(),
                "AMD Ryzen 7 5800X 8-Core Processor#2",
            );
        });
    }

    #[test]
    #[serial]
    fn test_collect_freq_amd_double_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_DOUBLE_CPU.to_vec())
                .await
                .unwrap();

            // when:
            let result = CpuRepo::collect_freq(&test_cpuinfo).await;

            // then:
            assert_eq!(
                result.len(),
                2,
                "collect_freq should have 2 physical cpu entries"
            );
            assert_eq!(
                result.get(&0),
                Some(&CpuFreqs {
                    avg: 3006,
                    max: 4200,
                    min: 1754,
                }),
                "collect_freq should have the correct frequency"
            );
            assert_eq!(
                result.get(&1),
                Some(&CpuFreqs {
                    avg: 819,
                    max: 1754,
                    min: 196,
                }),
                "collect_freq should have the correct frequency"
            );
        });
    }

    #[test]
    #[serial]
    fn test_set_cpu_infos_intel_single_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_INTEL_SINGLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();

            // when:
            let result = cpu_repo.set_cpu_infos(&test_cpuinfo).await;

            // then:
            assert!(result.is_ok(), "set_cpu_infos should return Ok: {result:?}");
            assert_eq!(
                cpu_repo.cpu_infos.len(),
                1,
                "cpu_infos should have 1 physical cpu entries"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.len(),
                1,
                "cpu_model_names should have 1 entries"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.get(&0).unwrap(),
                "Intel(R) Core(TM) i5-8265U CPU @ 1.60GHz",
            );
        });
    }

    #[test]
    #[serial]
    fn test_collect_freq_intel_single_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_INTEL_SINGLE_CPU.to_vec())
                .await
                .unwrap();

            // when:
            let result = CpuRepo::collect_freq(&test_cpuinfo).await;

            // then:
            assert_eq!(
                result.len(),
                1,
                "collect_freq should have 1 physical cpu entries"
            );
            assert_eq!(
                result.get(&0),
                Some(&CpuFreqs {
                    avg: 800,
                    max: 800,
                    min: 800,
                }),
                "collect_freq should have the correct frequency"
            );
        });
    }

    #[test]
    #[serial]
    fn test_set_cpu_infos_raspberry_pi_5() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_RASPBERRY_PI_5.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();

            // when:
            let result = cpu_repo.set_cpu_infos(&test_cpuinfo).await;

            // then:
            assert!(result.is_ok(), "set_cpu_infos should return Ok: {result:?}");
            assert_eq!(
                cpu_repo.cpu_infos.len(),
                1,
                "cpu_infos should have 1 physical cpu entries"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.len(),
                1,
                "cpu_model_names should have 1 entries"
            );
            assert_eq!(
                cpu_repo.cpu_model_names.get(&0).unwrap(),
                "Raspberry Pi Compute Module 5 Rev 1.0",
            );
        });
    }

    #[test]
    #[serial]
    fn test_collect_freq_raspberry_pi_5() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_RASPBERRY_PI_5.to_vec())
                .await
                .unwrap();

            // when:
            let result = CpuRepo::collect_freq(&test_cpuinfo).await;

            // then:
            assert_eq!(
                result.len(),
                0,
                "collect_freq should have no physical cpu entries"
            );
        });
    }

    #[test]
    #[serial]
    fn test_set_cpu_infos_empty() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, vec![]).await.unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();

            // when:
            let result = cpu_repo.set_cpu_infos(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_err(),
                "set_cpu_infos should return Err when not found: {result:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_collect_freq_empty() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, vec![]).await.unwrap();

            // when:
            let result = CpuRepo::collect_freq(&test_cpuinfo).await;

            // then:
            assert_eq!(result.len(), 0);
        });
    }

    #[test]
    #[serial]
    fn test_update_processor_count_amd_single_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_SINGLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();
            let initial_count = cpu_repo.cpu_infos.get(&0).unwrap().get();

            // when:
            let result = cpu_repo.update_processor_count(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_ok(),
                "update_processor_count should return Ok: {result:?}"
            );
            assert_eq!(
                cpu_repo.cpu_infos.get(&0).unwrap().get(),
                initial_count,
                "processor count should remain the same"
            );
        });
    }

    #[test]
    #[serial]
    fn test_update_processor_count_amd_double_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_DOUBLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();
            let initial_count_0 = cpu_repo.cpu_infos.get(&0).unwrap().get();
            let initial_count_1 = cpu_repo.cpu_infos.get(&1).unwrap().get();

            // when:
            let result = cpu_repo.update_processor_count(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_ok(),
                "update_processor_count should return Ok: {result:?}"
            );
            assert_eq!(
                cpu_repo.cpu_infos.get(&0).unwrap().get(),
                initial_count_0,
                "processor count for physical id 0 should remain the same"
            );
            assert_eq!(
                cpu_repo.cpu_infos.get(&1).unwrap().get(),
                initial_count_1,
                "processor count for physical id 1 should remain the same"
            );
        });
    }

    #[test]
    #[serial]
    fn test_update_processor_count_intel_single_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_INTEL_SINGLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();
            let initial_count = cpu_repo.cpu_infos.get(&0).unwrap().get();

            // when:
            let result = cpu_repo.update_processor_count(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_ok(),
                "update_processor_count should return Ok: {result:?}"
            );
            assert_eq!(
                cpu_repo.cpu_infos.get(&0).unwrap().get(),
                initial_count,
                "processor count should remain the same"
            );
        });
    }

    #[test]
    #[serial]
    fn test_update_processor_count_raspberry_pi_5() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_RASPBERRY_PI_5.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();
            let initial_count = cpu_repo.cpu_infos.get(&0).unwrap().get();

            // when:
            let result = cpu_repo.update_processor_count(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_ok(),
                "update_processor_count should return Ok: {result:?}"
            );
            assert_eq!(
                cpu_repo.cpu_infos.get(&0).unwrap().get(),
                initial_count,
                "processor count should remain the same"
            );
        });
    }

    #[test]
    #[serial]
    fn test_update_processor_count_fails_without_init() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_SINGLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            // Note: set_cpu_infos NOT called

            // when:
            let result = cpu_repo.update_processor_count(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_err(),
                "update_processor_count should return Err when cpu_infos is empty: {result:?}"
            );
        });
    }

    #[test]
    #[serial]
    fn test_update_processor_count_empty_file() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_SINGLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();
            let initial_count = cpu_repo.cpu_infos.get(&0).unwrap().get();

            // Overwrite with empty file
            cc_fs::write(&test_cpuinfo, vec![]).await.unwrap();

            // when:
            let result = cpu_repo.update_processor_count(&test_cpuinfo).await;

            // then:
            assert!(
                result.is_ok(),
                "update_processor_count should return Ok even with empty file: {result:?}"
            );
            // The processor count should remain unchanged
            assert_eq!(
                cpu_repo.cpu_infos.get(&0).unwrap().get(),
                initial_count,
                "processor count should remain unchanged when file is empty"
            );
        });
    }

    /// Goal: the dual Xeon X5672 in the report has no `Package id` label and identical core ids
    /// on both sockets, so the zone ranking is the only thing that can tell its packages apart.
    /// Method: its real cpuinfo, checking that package 0 ranks before package 1 by APIC id
    /// (0 against 32) and that each coretemp zone resolves to the matching socket.
    #[test]
    #[serial]
    fn test_intel_double_cpu_zones_map_to_sockets() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_INTEL_DOUBLE_CPU.to_vec()).await;

            // then: two packages, ordered by their lowest APIC id.
            assert_eq!(cpu_repo.cpu_infos.len(), 2);
            assert_eq!(cpu_repo.cpu_apic_order, vec![0, 1]);

            // then: each zone resolves to its own socket, with load and frequency attached.
            assert_eq!(
                cpu_repo.intel_association(Some(0), 2),
                Some(CpuAssociation::Socket(0))
            );
            assert_eq!(
                cpu_repo.intel_association(Some(1), 2),
                Some(CpuAssociation::Socket(1))
            );
        });
    }

    /// Goal: the ranking must follow the APIC id, not the physical id, because that is what
    /// `topology_get_logical_id()` counts when the kernel numbers coretemp zones. Method: a
    /// cpuinfo whose physical ids run opposite to its APIC ids, which is the only shape where
    /// the two orderings disagree.
    #[test]
    #[serial]
    fn test_zone_ranking_follows_apic_id_not_physical_id() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_INVERTED_APIC.as_bytes().to_vec()).await;

            // then: physical id 1 holds the lower APIC id, so it owns zone 0.
            assert_eq!(cpu_repo.cpu_apic_order, vec![1, 0]);
            assert_eq!(
                cpu_repo.intel_association(Some(0), 2),
                Some(CpuAssociation::Socket(1))
            );
            assert_eq!(
                cpu_repo.intel_association(Some(1), 2),
                Some(CpuAssociation::Socket(0))
            );
        });
    }

    /// Goal: a zone we cannot tie to a package must keep its temps rather than be dropped or
    /// guessed onto a socket, and a device with no readable zone at all must be dropped. Method:
    /// a two-package cpuinfo queried with a zone past its ranking, and with no zone.
    #[test]
    #[serial]
    fn test_unresolvable_zone_falls_back_to_temps_only() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_INTEL_DOUBLE_CPU.to_vec()).await;

            // then: one zone per package, but this one ranks past them. The temps are real, so
            // they are kept under the zone with no socket claim.
            let association = cpu_repo.intel_association(Some(2), 2);
            assert_eq!(association, Some(CpuAssociation::Zone(2)));
            assert!(association.unwrap().is_socket().not());
            assert_eq!(association.unwrap().device_id(), 2);

            // then: without a zone id there is nothing to key the device on at all.
            assert_eq!(cpu_repo.intel_association(None, 2), None);
        });
    }

    /// Goal: a single package must resolve without any zone id, since its own physical id is not
    /// always 0 and older parts expose no package label. Method: a single-socket cpuinfo queried
    /// with no zone and with a nonsense zone.
    #[test]
    #[serial]
    fn test_intel_single_cpu_needs_no_zone() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_INTEL_SINGLE_CPU.to_vec()).await;

            // then:
            assert_eq!(cpu_repo.cpu_infos.len(), 1);
            assert_eq!(
                cpu_repo.intel_association(None, 1),
                Some(CpuAssociation::Socket(0))
            );
            assert_eq!(
                cpu_repo.intel_association(Some(9), 1),
                Some(CpuAssociation::Socket(0))
            );
            // then: every die of a single package belongs to it, so a per-die driver still maps
            // to the one socket.
            assert_eq!(
                cpu_repo.intel_association(Some(1), 2),
                Some(CpuAssociation::Socket(0))
            );
        });
    }

    /// Goal: a driver that registers more devices than there are packages is reporting per die,
    /// and a package's dies are numbered consecutively, so with two dies per package zone 1 is
    /// the first package's second die, not the second package. Only each package's first die may
    /// claim its socket. Method: two-package cpuinfo queried for every zone of a four zone Intel
    /// driver (2P Cascade Lake-AP) and every node of an eight node AMD driver (2P Zen 1 EPYC),
    /// then for an uneven split and at the boundary of one device per package.
    #[test]
    #[serial]
    fn test_first_die_of_each_package_claims_its_socket() {
        cc_fs::test_runtime(async {
            // given: cpuinfo has no die id, so a two-package dump is all these paths can see.
            let intel_repo = repo_from_cpuinfo(CPUINFO_INTEL_DOUBLE_CPU.to_vec()).await;
            let amd_repo = repo_from_cpuinfo(CPUINFO_AMD_DOUBLE_CPU.to_vec()).await;

            // then: two dies per package, so zones 0 and 2 are the first dies.
            let intel_expected = [
                CpuAssociation::Socket(0),
                CpuAssociation::Zone(1),
                CpuAssociation::Socket(1),
                CpuAssociation::Zone(3),
            ];
            for (zone_id, expected) in (0..4).zip(intel_expected) {
                assert_eq!(
                    intel_repo.intel_association(Some(zone_id), 4),
                    Some(expected)
                );
            }
            // then: four nodes per package, so nodes 0 and 4 are the first dies.
            for node_id in 0..8 {
                let expected = match node_id {
                    0 => CpuAssociation::Socket(0),
                    4 => CpuAssociation::Socket(1),
                    _ => CpuAssociation::Zone(node_id),
                };
                assert_eq!(amd_repo.amd_association(Some(node_id), 8), Some(expected));
            }
            // then: an uneven split means a die is missing, so nothing is tied to a socket.
            for zone_id in 0..3 {
                assert_eq!(
                    intel_repo.intel_association(Some(zone_id), 3),
                    Some(CpuAssociation::Zone(zone_id))
                );
                assert_eq!(
                    amd_repo.amd_association(Some(zone_id), 3),
                    Some(CpuAssociation::Zone(zone_id))
                );
            }
            // then: one device per package ranks directly.
            assert_eq!(
                intel_repo.intel_association(Some(1), 2),
                Some(CpuAssociation::Socket(1))
            );
            assert_eq!(
                amd_repo.amd_association(Some(1), 2),
                Some(CpuAssociation::Socket(1))
            );
        });
    }

    /// Goal: the die arithmetic on its own, including the inputs the matchers cannot reach.
    /// Method: a table of (device id, device count, package count) against the expected rank.
    #[test]
    fn test_first_die_package_rank() {
        let cases: [(u8, usize, usize, Option<u8>); 12] = [
            // One device per package, or fewer: the id is the rank.
            (0, 2, 2, Some(0)),
            (1, 2, 2, Some(1)),
            (1, 1, 2, Some(1)),
            // Two dies per package: only even ids are first dies.
            (0, 4, 2, Some(0)),
            (1, 4, 2, None),
            (2, 4, 2, Some(1)),
            (3, 4, 2, None),
            // Four nodes per package, as on 2P Zen 1 EPYC.
            (4, 8, 2, Some(1)),
            (7, 8, 2, None),
            // An uneven split cannot be trusted.
            (0, 3, 2, None),
            (2, 5, 2, None),
            // No packages at all.
            (0, 1, 0, None),
        ];
        for (device_id, device_count, package_count, expected) in cases {
            assert_eq!(
                CpuRepo::first_die_package_rank(device_id, device_count, package_count),
                expected,
                "device {device_id} of {device_count} over {package_count} packages"
            );
        }
    }

    /// Goal: on the 5.0 code path a two-package machine always had CPU devices numbered 1 and 2,
    /// including multi-die machines, where zones 0 and 1 matched physical ids 0 and 1. Saved
    /// settings hang off the UIDs built from those numbers, so each processor's socket device
    /// must still produce exactly the same UID. Method: build the UID the way 5.0.0 did, from
    /// the physical id's model name and `physical id + 1`, and compare it with the UID built from
    /// what each first die resolves to, for one and for several dies per package.
    #[test]
    #[serial]
    fn test_socket_devices_keep_their_5_0_uids() {
        cc_fs::test_runtime(async {
            for (cpu_info_data, is_intel) in [
                (CPUINFO_INTEL_DOUBLE_CPU, true),
                (CPUINFO_AMD_DOUBLE_CPU, false),
            ] {
                // given:
                let cpu_repo = repo_from_cpuinfo(cpu_info_data.to_vec()).await;
                let uid_5_0 = |physical_id: PhysicalID| {
                    let cpu_name = cpu_repo.cpu_model_names.get(&physical_id).unwrap();
                    Device::create_uid_from(cpu_name, DeviceType::CPU, physical_id + 1, None)
                };
                let uid_now = |association: CpuAssociation| {
                    let type_index = cpu_repo.device_type_index(association).unwrap();
                    let cpu_name = cpu_repo.cpu_model_name(association.device_id()).unwrap();
                    Device::create_uid_from(&cpu_name, DeviceType::CPU, type_index, None)
                };

                // then: (first die of package 1, device count), from one to four dies each.
                for (second_first_die, device_count) in [(1, 2), (2, 4), (4, 8)] {
                    let associations = if is_intel {
                        [
                            cpu_repo.intel_association(Some(0), device_count),
                            cpu_repo.intel_association(Some(second_first_die), device_count),
                        ]
                    } else {
                        [
                            cpu_repo.amd_association(Some(0), device_count),
                            cpu_repo.amd_association(Some(second_first_die), device_count),
                        ]
                    };
                    assert_eq!(uid_now(associations[0].unwrap()), uid_5_0(0));
                    assert_eq!(uid_now(associations[1].unwrap()), uid_5_0(1));
                }
            }
        });
    }

    /// Goal: a device keyed by a zone rather than a physical id still needs a name to show, and
    /// that name must not vary between runs with `HashMap` order. Method: a two-package cpuinfo
    /// asked for a known id and for an id it has no entry for.
    #[test]
    #[serial]
    fn test_model_name_falls_back_for_zone_keyed_devices() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_INTEL_DOUBLE_CPU.to_vec()).await;
            let expected = cpu_repo.cpu_model_names.get(&0).cloned().unwrap();

            // then: a known id returns its own entry.
            assert_eq!(cpu_repo.cpu_model_name(0).as_ref(), Some(&expected));
            assert_eq!(cpu_repo.cpu_model_name(1).as_ref(), Some(&expected));
            // then: an unknown id falls back to the lowest known entry, not to nothing.
            assert_eq!(cpu_repo.cpu_model_name(7).as_ref(), Some(&expected));
        });
    }

    /// Goal: a zone-keyed device must never be looked up in `cpu_model_names` directly, because
    /// `Zone` is produced only for an id cpuinfo has no entry for, so such a lookup always misses
    /// and the device would be built and then dropped. Method: take the associations both drivers
    /// return for an unresolvable zone on a two-package machine, and assert the raw map misses
    /// while the resolver still answers.
    #[test]
    #[serial]
    fn test_zone_device_id_is_never_a_model_name_key() {
        cc_fs::test_runtime(async {
            // given: two packages, so the single-package short circuit does not apply.
            let cpu_repo = repo_from_cpuinfo(CPUINFO_INTEL_DOUBLE_CPU.to_vec()).await;

            // when: a zone beyond the packages cpuinfo knows about.
            let associations = [
                cpu_repo.intel_association(Some(2), 3).unwrap(),
                cpu_repo.amd_association(Some(2), 3).unwrap(),
            ];

            // then:
            for association in associations {
                assert!(association.is_socket().not());
                let device_id = association.device_id();
                // The raw map cannot answer for a zone id, which is why the call site must not
                // use it.
                assert!(cpu_repo.cpu_model_names.contains_key(&device_id).not());
                // The resolver must, or the device gets no name and is skipped.
                assert!(cpu_repo.cpu_model_name(device_id).is_some());
            }
        });
    }

    /// Goal: a power channel whose processor has no seeded energy counter must be omitted, not
    /// panic and not report a fabricated 0 watts, since 0 is a value the counter genuinely
    /// reports. Method: a repo with one processor seeded, asked for that processor and for one it
    /// has no counter for.
    #[test]
    #[serial]
    fn test_power_watts_needs_a_seeded_energy_counter() {
        cc_fs::test_runtime(async {
            // given: a seeded counter at 10 joules and a one second poll rate.
            let mut cpu_repo = repo_from_cpuinfo(CPUINFO_INTEL_DOUBLE_CPU.to_vec()).await;
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

    /// Goal: a two-socket AMD system must key its k10temp devices by node id, not by the hwmon
    /// enumeration index the old code assumed. Method: a two-package cpuinfo against the node ids
    /// a dual EPYC reports from PCI slots 0x18 and 0x19.
    #[test]
    #[serial]
    fn test_amd_double_cpu_nodes_map_to_sockets() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_AMD_DOUBLE_CPU.to_vec()).await;

            // then:
            assert_eq!(cpu_repo.cpu_infos.len(), 2);
            assert_eq!(
                cpu_repo.amd_association(Some(0), 2),
                Some(CpuAssociation::Socket(0))
            );
            assert_eq!(
                cpu_repo.amd_association(Some(1), 2),
                Some(CpuAssociation::Socket(1))
            );
            // then: a node cpuinfo has no physical id for keeps its temps under the node rather
            // than guessing a socket for them.
            assert_eq!(
                cpu_repo.amd_association(Some(2), 2),
                Some(CpuAssociation::Zone(2))
            );
            // then: no readable node id leaves nothing to key the device on.
            assert_eq!(cpu_repo.amd_association(None, 2), None);
        });
    }

    /// Goal: the single-socket AMD path must be untouched by the node id change, since that is
    /// every AMD desktop and the short circuit runs before any id is consulted. Method: a
    /// single-package cpuinfo with no node id and with a node id that does not exist.
    #[test]
    #[serial]
    fn test_amd_single_cpu_ignores_the_node_id() {
        cc_fs::test_runtime(async {
            // given:
            let cpu_repo = repo_from_cpuinfo(CPUINFO_AMD_SINGLE_CPU.to_vec()).await;

            // then:
            assert_eq!(cpu_repo.cpu_infos.len(), 1);
            assert_eq!(
                cpu_repo.amd_association(None, 1),
                Some(CpuAssociation::Socket(0))
            );
            assert_eq!(
                cpu_repo.amd_association(Some(3), 1),
                Some(CpuAssociation::Socket(0))
            );
        });
    }

    /// Goal: a partial match must be reported precisely rather than failing the repository, so
    /// the caller can name the processors whose temps could not be placed. Method: a two-socket
    /// cpuinfo against each possible set of matched devices, covering both the fully matched and
    /// the fully unmatched ends.
    #[test]
    #[serial]
    fn test_unmatched_physical_ids_double_cpu() {
        cc_fs::test_runtime(async {
            // given:
            let test_cpuinfo = tempfile::NamedTempFile::new().unwrap().path().to_path_buf();
            cc_fs::write(&test_cpuinfo, CPUINFO_AMD_DOUBLE_CPU.to_vec())
                .await
                .unwrap();
            let test_config = Rc::new(Config::init_default_config().unwrap());
            let mut cpu_repo =
                CpuRepo::new(test_config, Rc::new(OverridesController::empty())).unwrap();
            cpu_repo.set_cpu_infos(&test_cpuinfo).await.unwrap();

            // then: nothing matched, so both sockets are reported, ascending.
            assert_eq!(cpu_repo.unmatched_physical_ids(&matched(&[])), vec![0, 1]);
            // then: a partial match reports only the socket left over.
            assert_eq!(cpu_repo.unmatched_physical_ids(&matched(&[0])), vec![1]);
            assert_eq!(cpu_repo.unmatched_physical_ids(&matched(&[1])), vec![0]);
            // then: a full match reports nothing.
            assert!(cpu_repo
                .unmatched_physical_ids(&matched(&[0, 1]))
                .is_empty());
            // then: a device we have no cpuinfo entry for must not appear as unmatched.
            assert!(cpu_repo
                .unmatched_physical_ids(&matched(&[0, 1, 7]))
                .is_empty());
            // then: a zone-keyed device places no processor, so it must not mark one matched
            // even where its id happens to equal a physical id.
            let zone_only = HashMap::from([(CpuAssociation::Zone(0), HwmonDriverInfo::default())]);
            assert_eq!(cpu_repo.unmatched_physical_ids(&zone_only), vec![0, 1]);
            // then: zones beside a socket still leave the other processor unmatched. The device
            // search stops on an empty result, so this is what lets it register every die.
            let socket_and_zones = HashMap::from([
                (CpuAssociation::Socket(0), HwmonDriverInfo::default()),
                (CpuAssociation::Zone(1), HwmonDriverInfo::default()),
                (CpuAssociation::Zone(2), HwmonDriverInfo::default()),
            ]);
            assert_eq!(cpu_repo.unmatched_physical_ids(&socket_and_zones), vec![1]);
        });
    }

    /// Goal: a socket and a zone that share a number are two different devices and must each get
    /// their own device number, since that number is what the device UID is built from. Method: a
    /// two-package cpuinfo, asked to number both kinds across the whole id range.
    #[test]
    #[serial]
    fn test_zone_and_socket_device_numbers_cannot_collide() {
        cc_fs::test_runtime(async {
            // given: physical ids 0 and 1.
            let cpu_repo = repo_from_cpuinfo(CPUINFO_AMD_DOUBLE_CPU.to_vec()).await;

            // then: a socket keeps its historical number, so saved settings still resolve.
            assert_eq!(
                cpu_repo.device_type_index(CpuAssociation::Socket(0)),
                Some(1)
            );
            assert_eq!(
                cpu_repo.device_type_index(CpuAssociation::Socket(1)),
                Some(2)
            );
            // then: a zone sharing a socket's id gets a different number anyway.
            assert_ne!(
                cpu_repo.device_type_index(CpuAssociation::Zone(0)),
                cpu_repo.device_type_index(CpuAssociation::Socket(0))
            );
            // then: every zone number sits above every socket number, so the physical id that
            // `preload_statuses` recovers as `type_index - 1` can never alias a processor.
            // Every zone the numbering has room for, given the highest physical id here is 1.
            for zone_id in 0..=(u8::MAX - 3) {
                let type_index = cpu_repo
                    .device_type_index(CpuAssociation::Zone(zone_id))
                    .unwrap();
                assert!(type_index > 2);
                assert!(cpu_repo.cpu_infos.contains_key(&(type_index - 1)).not());
            }
            // then: a zone that would number past the range is refused, not wrapped.
            assert_eq!(
                cpu_repo.device_type_index(CpuAssociation::Zone(u8::MAX)),
                None
            );
        });
    }
}

#[cfg(test)]
mod sensors_conf_tests {
    use crate::config::Config;
    use crate::overrides::OverridesController;
    use crate::repositories::cpu_repo::CpuRepo;
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
