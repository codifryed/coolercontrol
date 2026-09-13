// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Paste-ready hardware report.
//!
//! Reuses `fans::init_fans` and `hardware_support::diagnose_channel` rather
//! than re-deriving anything, so the report and the running daemon can never
//! disagree about a verdict.
//!
//! Nothing here reads a serial number, a UUID, or a hostname. The output is
//! meant to be pasted into a public support channel, so the identifying fields
//! are limited to board and BIOS, which is what actually narrows a diagnosis.

use crate::device::{ChannelName, Device, DeviceInfo, DeviceType};
use crate::hardware_support::{
    self, ChannelExclusion, ChannelVerdict, DetectedChipRef, HiddenHardware, HwmonExclusion,
    ProbeEnvironment, SystemFinding, SystemInfo,
};
use crate::repositories::hwmon::fans;
use crate::repositories::hwmon::hwmon_repo::{HwmonChannelInfo, HwmonChannelType, HwmonDriverInfo};
use crate::{cc_fs, rt, VERSION};
use log::warn;
use std::collections::HashMap;
use std::fmt::Write;
use std::future::Future;
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

const HWMON_CLASS_PATH: &str = "/sys/class/hwmon";

/// Appended when the compact report is trimmed, so the dropped detail is still
/// reachable rather than silently gone.
const TRIM_NOTICE: &str = "\n[trimmed, the full report has the rest]\n";

/// Discord's message limit. The compact report is meant to be pasted whole, so
/// it is trimmed with a pointer to the full report rather than being allowed to
/// run past this and get truncated by the client.
pub const COMPACT_CHARACTER_BUDGET: usize = 2000;

/// Room held back for the ```` ``` ```` fences the UI wraps the text in when
/// copying. Without a code block the column alignment collapses in Discord and
/// in every markdown editor, and a report trimmed to exactly the limit would
/// then overflow it by the length of the fences themselves.
const CODE_FENCE_RESERVE: usize = "```\n\n```".len();

/// What the compact report may actually occupy.
const COMPACT_TRIM_TARGET: usize = COMPACT_CHARACTER_BUDGET - CODE_FENCE_RESERVE;

const _: () = assert!(COMPACT_TRIM_TARGET < COMPACT_CHARACTER_BUDGET);

/// What one device's sysfs reads may take before the report gives up on it.
///
/// Sized off the worst honest case measured: an Aquacomputer Octo answers each
/// `pwmN` over its own HID round trip at 200-400ms, so its eight channels cost
/// a legitimate 1.5-3s. Past this a driver is not slow, it is not answering.
const DEVICE_READ_BUDGET: Duration = Duration::from_secs(5);

/// What every device's reads may take together.
///
/// Kept under the actor's own `GENERATE_TIMEOUT` so a machine with several
/// slow devices still returns a report naming them. The actor's notice is the
/// last resort and tells the user nothing about which device is at fault.
const REPORT_READ_BUDGET: Duration = Duration::from_secs(15);

const _: () = assert!(DEVICE_READ_BUDGET.as_millis() < REPORT_READ_BUDGET.as_millis());

/// Stands in for a device's channels when its driver stopped answering.
///
/// Naming the device is the whole point: this is the line a maintainer needs,
/// and it is the one thing an all-or-nothing timeout cannot produce.
const UNRESPONSIVE_NOTE: &str = "did not answer in time, the driver may be stuck";

/// One hwmon device as the report sees it.
struct ReportDevice {
    path: PathBuf,
    name: String,
    /// Resolved from `device/driver`. `None` for hwmon entries with no such
    /// link, such as USB and HID devices.
    driver: Option<String>,
    fans: Vec<HwmonChannelInfo>,
    /// Set when the running daemon deliberately left this device out. `None`
    /// from the standalone CLI, which has no daemon to ask and therefore says
    /// nothing rather than guessing.
    excluded: Option<HwmonExclusion>,
    /// Channels of this device the daemon dropped individually, by name. Empty
    /// for a device that was dropped whole: its channels were never reached.
    excluded_channels: HashMap<ChannelName, ChannelExclusion>,
    /// Set when the device ran out of read budget, which leaves `fans` empty
    /// for a reason that has nothing to do with the device having no fans.
    unresponsive: bool,
}

/// The report's remaining time for sysfs reads.
///
/// One budget spans every stage, so a device that stalls the scan cannot then
/// spend the full tree's time as well. Copied rather than borrowed: it is two
/// words and every stage reads it independently.
#[derive(Clone, Copy)]
struct ReadBudget {
    deadline: Instant,
}

impl ReadBudget {
    fn starting_now() -> Self {
        Self {
            deadline: Instant::now() + REPORT_READ_BUDGET,
        }
    }

    /// How long the next device may take: its own cap, or whatever is left of
    /// the report's, whichever is smaller. `None` once the budget is spent.
    fn for_device(self) -> Option<Duration> {
        let remaining = self.deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return None;
        }
        Some(remaining.min(DEVICE_READ_BUDGET))
    }
}

/// Bounds one device's sysfs reads by what is left of the report's budget.
///
/// `None` means the device did not answer in time, which every caller turns
/// into a line naming it. A driver that stops answering must cost the report
/// that one device and nothing else.
///
/// Only the awaited value reads are bounded. `read_dir`, `exists` and
/// `canonicalize` are synchronous and cannot be interrupted, but they are
/// served by the VFS rather than by the device, so they do not wait on a bus.
async fn within_budget<T>(
    budget: ReadBudget,
    name: &str,
    path: &Path,
    work: impl Future<Output = T>,
) -> Option<T> {
    let Some(allowed) = budget.for_device() else {
        warn!(
            "Hardware report read budget was spent before reaching {name} at {}",
            path.display()
        );
        return None;
    };
    let Ok(value) = rt::timeout(allowed, work).await else {
        warn!(
            "Hardware report gave up on {name} at {} after {allowed:?}",
            path.display()
        );
        return None;
    };
    Some(value)
}

/// Builds the report. `full` adds the whole hwmon tree with per-channel state.
///
/// `detection` is what the *startup* probe found, retained rather than re-run:
/// module loading only happens at startup, so that run is the one that explains
/// an unbound chip, and reading a report must never poke I/O ports as a side
/// effect.
pub async fn generate(
    full: bool,
    detection: Option<&cc_detect::DetectionResults>,
    devices_other: &[DeviceSummary],
    hidden: &HiddenHardware,
    retained: &[Rc<HwmonDriverInfo>],
) -> String {
    let budget = ReadBudget::starting_now();
    let system_info = SystemInfo::read().await;
    let devices = if full {
        scan_hwmon(hidden, budget).await
    } else {
        collect_retained(retained, hidden, budget).await
    };
    let mut report = String::with_capacity(if full { 8192 } else { COMPACT_CHARACTER_BUDGET });
    write_header(&mut report, &system_info);
    write_probe_summary(&mut report, detection, full, cc_detect::DETECTION_SUPPORTED);
    write_fan_summary(&mut report, &devices, budget).await;
    write_device_sections(&mut report, devices_other);
    write_findings(&mut report, detection, cc_detect::DETECTION_SUPPORTED);
    if full {
        write_full_tree(&mut report, &devices, budget).await;
        return report;
    }
    trim_to_budget(report)
}

fn write_header(report: &mut String, system_info: &SystemInfo) {
    let _ = writeln!(report, "CoolerControl {VERSION} hardware report");
    let _ = writeln!(
        report,
        "Board    {} {}",
        blank_as_unknown(&system_info.board_vendor),
        blank_as_unknown(&system_info.board_name)
    );
    let _ = writeln!(
        report,
        "BIOS     {} {}",
        blank_as_unknown(&system_info.bios_vendor),
        blank_as_unknown(&system_info.bios_release)
    );
    let _ = writeln!(
        report,
        "Kernel   {} ({})",
        sysinfo::System::kernel_version().unwrap_or_else(|| "unknown".to_string()),
        sysinfo::System::cpu_arch()
    );
}

/// `detection_supported` is passed in rather than read from
/// `cc_detect::DETECTION_SUPPORTED` so both branches are reachable in tests on
/// any host architecture.
fn write_probe_summary(
    report: &mut String,
    detection: Option<&cc_detect::DetectionResults>,
    full: bool,
    detection_supported: bool,
) {
    if detection_supported.not() {
        let _ = writeln!(report, "Probe    not supported on this architecture");
        return;
    }
    let Some(detection) = detection else {
        let _ = writeln!(report, "Probe    skipped (detection disabled)");
        return;
    };
    let environment = &detection.environment;
    let _ = writeln!(
        report,
        "Probe    {} chip(s), Secure Boot {}, container {}, /dev/port {}",
        detection.detected_chips.len(),
        on_off(environment.is_secure_boot),
        yes_no(environment.is_container),
        available_or_not(environment.has_dev_port)
    );
    for chip in &detection.detected_chips {
        // Address and device id mirror the startup detection log, so a chip
        // line in a pasted report can be matched against a pasted log.
        let _ = writeln!(
            report,
            "         {} at {} id:{} -> {} ({})",
            chip.name, chip.address, chip.device_id, chip.driver, chip.module_status
        );
        if full {
            let _ = writeln!(report, "           base address {}", chip.base_address);
        }
    }
}

/// Controllable channels are collapsed to a count. Anything we cannot drive is
/// listed individually with its verdict and its raw state, because that is the
/// whole reason someone is pasting this into a support channel.
///
/// The raw state is deliberately limited to channels we cannot drive. Emitting
/// it for every channel is what `--full` is for, and doing so here would push
/// the compact report past the paste budget on an ordinary desktop. Restricted
/// this way it costs nothing on a healthy machine and a couple of lines on a
/// broken one, which removes a round trip for the common support case.
async fn write_fan_summary(report: &mut String, devices: &[ReportDevice], budget: ReadBudget) {
    let _ = writeln!(report, "\nHWMon Fan Channels");
    let mut any = false;
    for device in devices {
        if device.unresponsive {
            // Its `fans` is empty because the scan gave up, not because the
            // device has none. Skipping it here would hide the one device the
            // reader most needs to see.
            any = true;
            let _ = writeln!(
                report,
                "  {} [{}]  {UNRESPONSIVE_NOTE}",
                device.name,
                device.driver.as_deref().unwrap_or("no driver link")
            );
            continue;
        }
        if scan_is_safe(device.excluded).not() {
            // Listed but not inspected. Saying so beats omitting it, which
            // would read as the report having missed the device entirely.
            any = true;
            let _ = writeln!(
                report,
                "  {} [{}]  not scanned{}, see the Liquidctl section for its fans",
                device.name,
                device.driver.as_deref().unwrap_or("no driver link"),
                exclusion_suffix(device)
            );
            continue;
        }
        if device.fans.is_empty() {
            continue;
        }
        any = true;
        let controllable = device
            .fans
            .iter()
            .filter(|fan| verdict_for(device, fan).is_controllable())
            .count();
        let hidden_fans = device
            .fans
            .iter()
            .filter(|fan| device.excluded_channels.contains_key(&fan.name))
            .count();
        let _ = writeln!(
            report,
            "  {} [{}]  {} fan(s), {controllable} controllable{}{}",
            device.name,
            device.driver.as_deref().unwrap_or("no driver link"),
            device.fans.len(),
            hidden_channel_suffix(hidden_fans),
            exclusion_suffix(device)
        );
        // Built into its own string under the budget, so a device that stops
        // answering partway through cannot leave half a line in the report.
        match within_budget(budget, &device.name, &device.path, summary_notes(device)).await {
            Some(notes) => report.push_str(&notes),
            None => {
                let _ = writeln!(report, "    {UNRESPONSIVE_NOTE}");
            }
        }
    }
    if any.not() {
        let _ = writeln!(report, "  none found");
    }
}

/// One line per channel the reader needs told about, and nothing for the rest.
async fn summary_notes(device: &ReportDevice) -> String {
    let mut notes = String::new();
    for fan in &device.fans {
        let verdict = verdict_for(device, fan);
        let hidden = device.excluded_channels.get(&fan.name).copied();
        // A hidden channel earns a line even when it is perfectly drivable:
        // that it is missing from the app is the whole point.
        if verdict.is_controllable() && hidden.is_none() {
            continue;
        }
        let _ = writeln!(
            notes,
            "    {}: {}",
            channel_title(fan),
            channel_notes(device, fan, verdict, hidden).await
        );
    }
    notes
}

/// One non-hwmon device, reduced to what a maintainer needs.
///
/// Built by each repository from the daemon's live device list. Deliberately
/// has no field for a serial number or a device id: this ends up in a pasted
/// report, and liqctld's own payload carries a serial that must not travel
/// with it.
#[derive(Clone)]
pub struct DeviceSummary {
    /// Which repository serves it, so the report can group by section.
    pub device_type: DeviceType,
    pub name: String,
    /// False when the user has disabled the device. Disabled devices are
    /// deliberately still listed: "the app cannot see my cooler" and "I turned
    /// that cooler off" look identical in a support thread otherwise.
    pub enabled: bool,
    /// Kernel driver name or liquidctl driver class, whichever the repository
    /// knows.
    pub driver: Option<String>,
    /// The driver's own version: liquidctl's for liquidctl devices, the
    /// proprietary driver's for Nvidia.
    pub driver_version: Option<String>,
    /// Speed channels the device exposes, and how many can actually be driven.
    ///
    /// Counted from the same `speed_options` the channel verdicts come from, so
    /// counts and verdicts can never disagree. Informational: these
    /// repositories have no sysfs-level evidence to back a stronger claim.
    pub fan_count: usize,
    pub controllable_fan_count: usize,
    pub firmware_version: Option<String>,
    /// True when the device is also exposed through a kernel hwmon driver, in
    /// which case it appears under the hwmon section as well.
    pub hwmon_backed: bool,
}

impl DeviceSummary {
    /// The parts every repository can answer, counts included.
    pub fn from_device(device: &Device, enabled: bool) -> Self {
        let (fan_count, controllable_fan_count) = Self::count_fans(&device.info);
        Self {
            device_type: device.d_type,
            name: device.name.clone(),
            enabled,
            driver: device.info.driver_info.name.clone(),
            driver_version: device.info.driver_info.version.clone(),
            fan_count,
            controllable_fan_count,
            firmware_version: None,
            hwmon_backed: false,
        }
    }

    /// A speed channel is one the daemon would drive if the driver allowed it;
    /// `fixed_enabled` is whether it actually can.
    fn count_fans(info: &DeviceInfo) -> (usize, usize) {
        let mut fans = 0;
        let mut controllable = 0;
        for channel_info in info.channels.values() {
            let Some(speed_options) = channel_info.speed_options() else {
                continue;
            };
            fans += 1;
            if speed_options.fixed_enabled {
                controllable += 1;
            }
        }
        (fans, controllable)
    }
}

/// Writes one section per non-hwmon repository, so the report accounts for
/// every fan on the machine rather than only the ones the kernel exposes.
///
/// Empty sections still print: "no service plugin devices" is an answer, a
/// missing heading is a question.
fn write_device_sections(report: &mut String, devices: &[DeviceSummary]) {
    for (heading, device_type) in [
        ("Liquidctl", DeviceType::Liquidctl),
        ("GPU", DeviceType::GPU),
        ("Service Plugins", DeviceType::ServicePlugin),
    ] {
        write_device_section(report, heading, device_type, devices);
    }
}

/// Taken from what each repository retained at startup, so generating a report
/// never re-enumerates devices and disabled ones are still listed.
fn write_device_section(
    report: &mut String,
    heading: &str,
    device_type: DeviceType,
    devices: &[DeviceSummary],
) {
    let _ = writeln!(report, "\n{heading}");
    let matching = devices
        .iter()
        .filter(|device| device.device_type == device_type)
        .collect::<Vec<_>>();
    if matching.is_empty() {
        let _ = writeln!(report, "  no devices");
        return;
    }
    for device in &matching {
        let _ = writeln!(
            report,
            "  {} [{}]{}{}  {} fan(s), {} controllable",
            device.name,
            device.driver.as_deref().unwrap_or("unknown driver"),
            if device.hwmon_backed {
                ", hwmon-backed"
            } else {
                ""
            },
            if device.enabled { "" } else { ", disabled" },
            device.fan_count,
            device.controllable_fan_count
        );
        if let Some(firmware) = device.firmware_version.as_deref() {
            let _ = writeln!(report, "    firmware {firmware}");
        }
    }
    // One tool or driver version serves the whole section, so it goes at the
    // foot rather than being repeated on every line.
    if let Some(version) = matching
        .iter()
        .find_map(|device| device.driver_version.as_deref())
    {
        let label = if device_type == DeviceType::Liquidctl {
            "liquidctl"
        } else {
            "driver"
        };
        let _ = writeln!(report, "  {label} {version}");
    }
}

/// See `write_probe_summary` for why `detection_supported` is a parameter.
fn write_findings(
    report: &mut String,
    detection: Option<&cc_detect::DetectionResults>,
    detection_supported: bool,
) {
    let _ = writeln!(report, "\nSystem findings");
    if detection_supported.not() {
        let _ = writeln!(report, "  detection unsupported on this architecture");
        return;
    }
    let Some(detection) = detection else {
        let _ = writeln!(report, "  not determined (detection disabled)");
        return;
    };
    let findings = derive_findings(detection, detection_supported);
    if findings.is_empty() {
        let _ = writeln!(report, "  none");
        return;
    }
    for finding in &findings {
        let _ = writeln!(report, "  {}", finding_label(finding));
    }
}

/// See `write_probe_summary` for why `detection_supported` is a parameter.
fn derive_findings(
    detection: &cc_detect::DetectionResults,
    detection_supported: bool,
) -> Vec<SystemFinding> {
    let bound_drivers = hardware_support::resolve_bound_drivers(Path::new(HWMON_CLASS_PATH));
    let chips = detection
        .detected_chips
        .iter()
        .map(|chip| DetectedChipRef {
            name: &chip.name,
            driver: &chip.driver,
        })
        .collect::<Vec<_>>();
    let environment = ProbeEnvironment {
        is_container: detection.environment.is_container,
        is_secure_boot: detection.environment.is_secure_boot,
        has_dev_port: detection.environment.has_dev_port,
    };
    hardware_support::derive_system_findings(
        detection_supported,
        &chips,
        &detection.blacklisted,
        &environment,
        &bound_drivers,
    )
}

async fn write_full_tree(report: &mut String, devices: &[ReportDevice], budget: ReadBudget) {
    let _ = writeln!(report, "\nFull HWMon Tree");
    for device in devices {
        let _ = writeln!(
            report,
            "  {} [{}] at {}{}",
            device.name,
            device.driver.as_deref().unwrap_or("no driver link"),
            device.path.display(),
            exclusion_suffix(device)
        );
        if device.unresponsive {
            let _ = writeln!(report, "    {UNRESPONSIVE_NOTE}");
            continue;
        }
        // Built into its own string under the budget, so a driver that stops
        // answering partway through cannot leave half a line in the report.
        match within_budget(budget, &device.name, &device.path, channel_lines(device)).await {
            Some(lines) => report.push_str(&lines),
            None => {
                let _ = writeln!(report, "    {UNRESPONSIVE_NOTE}");
            }
        }
    }
}

/// One line per fan channel with the raw pwm state behind its verdict.
async fn channel_lines(device: &ReportDevice) -> String {
    let mut lines = String::new();
    for fan in &device.fans {
        let verdict = verdict_for(device, fan);
        let pwm = read_attribute(&device.path, &format!("pwm{}", fan.number)).await;
        let pwm_enable = read_attribute(&device.path, &format!("pwm{}_enable", fan.number)).await;
        let rpm = read_attribute(&device.path, &format!("fan{}_input", fan.number)).await;
        let hidden = device
            .excluded_channels
            .get(&fan.name)
            .map_or_else(String::new, |reason| format!(" {}", reason.label()));
        let _ = writeln!(
            lines,
            "    {}: verdict={} pwm={pwm} pwm_enable={pwm_enable} rpm={rpm} \
             writable={} label={}{hidden}",
            fan.name,
            verdict_label(verdict),
            fan.caps.is_fan_controllable(),
            fan.label.as_deref().unwrap_or("-")
        );
    }
    lines
}

/// The raw pwm state behind a verdict. `pwm_enable` in particular is the field
/// that distinguishes a driver-defined mode from a documented one, and it is
/// what a maintainer asks for first.
async fn raw_state(base_path: &Path, number: u8) -> String {
    let pwm = read_attribute(base_path, &format!("pwm{number}")).await;
    let pwm_enable = read_attribute(base_path, &format!("pwm{number}_enable")).await;
    let rpm = read_attribute(base_path, &format!("fan{number}_input")).await;
    format!("pwm={pwm} pwm_enable={pwm_enable} rpm={rpm}")
}

/// Reads one sysfs attribute for the full dump. A missing or unreadable file
/// is reported as absent rather than failing the report.
async fn read_attribute(base_path: &Path, file_name: &str) -> String {
    cc_fs::read_sysfs(base_path.join(file_name))
        .await
        .map_or_else(|_| "-".to_string(), |value| value.trim().to_string())
}

/// The channel name with its driver label when it has one.
///
/// Eight headers all called `fanN` are indistinguishable in a pasted report,
/// and the label is what tells the user which one is the front intake.
fn channel_title(fan: &HwmonChannelInfo) -> String {
    match fan.label.as_deref() {
        Some(label) if label.trim().is_empty().not() => {
            format!("{} \"{}\"", fan.name, label.trim())
        }
        _ => fan.name.clone(),
    }
}

/// Why this channel earned a line: that it is hidden, that it cannot be
/// driven, or both. A hidden channel that is also uncontrollable says both,
/// because fixing the second does not bring it back.
async fn channel_notes(
    device: &ReportDevice,
    fan: &HwmonChannelInfo,
    verdict: ChannelVerdict,
    hidden: Option<ChannelExclusion>,
) -> String {
    let mut notes = Vec::with_capacity(2);
    if let Some(reason) = hidden {
        notes.push(reason.label().to_string());
    }
    if verdict.is_controllable().not() {
        notes.push(format!(
            "{} ({})",
            verdict_label(verdict),
            raw_state(&device.path, fan.number).await
        ));
    }
    notes.join(", ")
}

/// The channels this device lost individually, keyed by name.
fn hidden_channels(
    hidden: &HiddenHardware,
    canonical: &Path,
) -> HashMap<ChannelName, ChannelExclusion> {
    hidden
        .channels
        .iter()
        .filter(|((path, _), _)| path == canonical)
        .map(|((_, channel_name), reason)| (channel_name.clone(), *reason))
        .collect()
}

/// Reports how many of a device's fans the app is not showing. Silent at zero,
/// which is every ordinary machine.
fn hidden_channel_suffix(count: usize) -> String {
    if count == 0 {
        return String::new();
    }
    format!(", {count} hidden")
}

/// Whether the report may read this device's channels.
///
/// Everything except a liquidctl duplicate: that is one piece of hardware with
/// another process actively talking to it, and a read here can break its
/// transfers. Every other exclusion is a device nothing else is using, so
/// inspecting it is free and often the whole point.
fn scan_is_safe(excluded: Option<HwmonExclusion>) -> bool {
    excluded != Some(HwmonExclusion::DuplicateOfLiquidctl)
}

/// The trailing "hidden: ..." note, or nothing for a device the app shows.
///
/// Silent when the daemon kept the device, which is the common case: a marker
/// on every line would bury the few that matter.
fn exclusion_suffix(device: &ReportDevice) -> String {
    device
        .excluded
        .map_or_else(String::new, |reason| format!(", {}", reason.label()))
}

fn verdict_for(device: &ReportDevice, fan: &HwmonChannelInfo) -> ChannelVerdict {
    fans::diagnose_fan_channel(&device.name, fan, false).verdict
}

/// Builds the compact report's device list without re-reading sysfs.
///
/// The repository already read every one of these files at startup and kept
/// the result, so a report that walks the tree again is repeating work, and on
/// a device that talks over a bus it is repeating work that can fail. `--full`
/// still walks everything; that is what it is for.
///
/// Two things still need reading, and only these:
///
/// - devices the daemon excluded, which are absent from the retained set by
///   definition, and are the ones a reader most wants explained;
/// - devices with an individually hidden channel, because the repository
///   filtered those channels out before retaining them, so the retained copy
///   cannot show what went missing.
async fn collect_retained(
    retained: &[Rc<HwmonDriverInfo>],
    hidden: &HiddenHardware,
    budget: ReadBudget,
) -> Vec<ReportDevice> {
    let mut devices = Vec::with_capacity(retained.len() + hidden.devices.len());
    for driver in retained {
        let canonical = cc_fs::canonicalize(&driver.path).ok();
        let excluded_channels = canonical
            .as_ref()
            .map_or_else(HashMap::new, |canonical| hidden_channels(hidden, canonical));
        let scanned = if excluded_channels.is_empty() {
            Some(
                driver
                    .channels
                    .iter()
                    .filter(|channel| channel.hwmon_type == HwmonChannelType::Fan)
                    .cloned()
                    .collect(),
            )
        } else {
            within_budget(
                budget,
                &driver.name,
                &driver.path,
                read_fans(&driver.path, &driver.name),
            )
            .await
        };
        devices.push(ReportDevice {
            path: driver.path.clone(),
            name: driver.name.clone(),
            driver: resolve_driver(&driver.path),
            unresponsive: scanned.is_none(),
            fans: scanned.unwrap_or_default(),
            excluded: None,
            excluded_channels,
        });
    }
    for (path, reason) in &hidden.devices {
        let Ok(name) = cc_fs::read_txt(path.join("name")).await else {
            continue;
        };
        let name = name.trim().to_string();
        let scanned = if scan_is_safe(Some(*reason)) {
            within_budget(budget, &name, path, read_fans(path, &name)).await
        } else {
            Some(Vec::new())
        };
        devices.push(ReportDevice {
            path: path.clone(),
            name,
            driver: resolve_driver(path),
            unresponsive: scanned.is_none(),
            fans: scanned.unwrap_or_default(),
            excluded: Some(*reason),
            excluded_channels: hidden_channels(hidden, path),
        });
    }
    devices.sort_by_key(|device| (hwmon_index(&device.path), device.path.clone()));
    devices
}

/// Reads a device's fan channels with the exact same `init_fans` the daemon
/// uses, so a scanned device and a retained one describe themselves alike.
async fn read_fans(path: &Path, name: &str) -> Vec<HwmonChannelInfo> {
    fans::init_fans(path, name)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|channel| channel.hwmon_type == HwmonChannelType::Fan)
        .collect()
}

/// Enumerates hwmon and builds each device's fan channels with the exact same
/// `init_fans` the daemon uses.
async fn scan_hwmon(hidden: &HiddenHardware, budget: ReadBudget) -> Vec<ReportDevice> {
    let mut devices = Vec::new();
    let Ok(entries) = cc_fs::read_dir(Path::new(HWMON_CLASS_PATH)) else {
        return devices;
    };
    let mut paths = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    // Numeric, not lexicographic: plain sorting puts hwmon10 before hwmon2,
    // which reads as a mistake in a report a human is scanning.
    paths.sort_by_key(|path| (hwmon_index(path), path.clone()));
    for path in paths {
        let Ok(name) = cc_fs::read_txt(path.join("name")).await else {
            continue;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            continue;
        }
        let driver = resolve_driver(&path);
        // Matched on the canonical path: the entries here are symlinks into
        // the bus tree, while the daemon recorded its own walk of it.
        let canonical = cc_fs::canonicalize(&path).ok();
        let excluded = canonical
            .as_ref()
            .and_then(|canonical| hidden.devices.get(canonical).copied());
        let excluded_channels = canonical
            .as_ref()
            .map_or_else(HashMap::new, |canonical| hidden_channels(hidden, canonical));
        // Reading a device liquidctl owns means two programs on one piece of
        // hardware. `corsairpsu` reads go out over USB HID to the PSU, so this
        // was both slow and enough to make liqctld's own transfers fail with
        // "possible conflict with another program". The daemon excluded the
        // device precisely so it would not be touched; the report has no
        // business overriding that, and the Liquidctl section already carries
        // its fan counts from the repository that actually drives it.
        let scanned = if scan_is_safe(excluded) {
            within_budget(budget, &name, &path, read_fans(&path, &name)).await
        } else {
            Some(Vec::new())
        };
        devices.push(ReportDevice {
            path,
            name,
            driver,
            unresponsive: scanned.is_none(),
            fans: scanned.unwrap_or_default(),
            excluded,
            excluded_channels,
        });
    }
    devices
}

/// The trailing number of a `hwmonN` directory. Entries that do not match sort
/// last rather than being dropped, since a report should show what is there.
fn hwmon_index(path: &Path) -> u32 {
    path.file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("hwmon"))
        .and_then(|digits| digits.parse().ok())
        .unwrap_or(u32::MAX)
}

fn resolve_driver(hwmon_path: &Path) -> Option<String> {
    let target = cc_fs::read_link(hwmon_path.join("device").join("driver")).ok()?;
    let driver = target.file_name()?.to_str()?;
    if driver.is_empty() {
        return None;
    }
    Some(driver.to_string())
}

/// Keeps the compact report pasteable. Trimming on a line boundary avoids
/// cutting a verdict in half, and the pointer to `--full` means the dropped
/// detail is still reachable.
fn trim_to_budget(report: String) -> String {
    if report.chars().count() <= COMPACT_TRIM_TARGET {
        return report;
    }
    let keep = COMPACT_TRIM_TARGET.saturating_sub(TRIM_NOTICE.chars().count());
    let mut trimmed = String::with_capacity(COMPACT_TRIM_TARGET);
    for line in report.lines() {
        if trimmed.chars().count() + line.chars().count() + 1 > keep {
            break;
        }
        trimmed.push_str(line);
        trimmed.push('\n');
    }
    trimmed.push_str(TRIM_NOTICE);
    trimmed
}

fn verdict_label(verdict: ChannelVerdict) -> &'static str {
    match verdict {
        ChannelVerdict::Controllable => "controllable",
        ChannelVerdict::FirmwareOverride => "firmware override",
        ChannelVerdict::FamilyMayNeedOutOfTree => "no control, family may need out-of-tree driver",
        ChannelVerdict::NotSupportedByDriver => "the driver in use exposes no control",
        ChannelVerdict::NoPwm => "no pwm control exposed",
        ChannelVerdict::PwmReadOnly => "pwm is read-only",
        ChannelVerdict::IgnoresDuty => "accepts duty writes but does not respond",
        ChannelVerdict::Unverifiable => "no usable tachometer",
    }
}

fn finding_label(finding: &SystemFinding) -> String {
    match finding {
        SystemFinding::NoDriverBound {
            chip_name,
            expected_driver,
        } => format!(
            "{chip_name}: detected, but no loaded driver serves it (expected {expected_driver})"
        ),
        SystemFinding::Blacklisted { driver } => format!("{driver}: blacklisted, not loaded"),
        SystemFinding::BlockedByEnvironment { reason } => {
            format!("Super-I/O probe blocked: {reason:?}")
        }
        SystemFinding::DetectionUnsupported => {
            "Super-I/O detection unsupported on this architecture".to_string()
        }
    }
}

fn blank_as_unknown(value: &str) -> &str {
    if value.is_empty() {
        "unknown"
    } else {
        value
    }
}

fn on_off(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn available_or_not(value: bool) -> &'static str {
    if value {
        "available"
    } else {
        "unavailable"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    /// Goal: the compact report stays pasteable into Discord *after* the UI
    /// wraps it in a code fence. Method: feed the trimmer a report far over the
    /// limit and check that the fenced result still fits, since a report
    /// trimmed to exactly the limit would overflow once fenced.
    #[test]
    fn compact_report_is_trimmed_to_the_paste_budget() {
        let oversized = "a line of hwmon detail\n".repeat(400);
        let trimmed = trim_to_budget(oversized);
        let fenced = format!("```\n{}\n```", trimmed.trim_end());
        assert!(
            fenced.chars().count() <= COMPACT_CHARACTER_BUDGET,
            "fenced report was {} chars",
            fenced.chars().count()
        );
        assert!(trimmed.contains(TRIM_NOTICE.trim()));
    }

    /// Goal: a report already within budget must be returned untouched, so the
    /// common case carries no trim notice.
    #[test]
    fn short_report_is_left_alone() {
        let report = "CoolerControl hardware report\nBoard    ACME X\n".to_string();
        assert_eq!(trim_to_budget(report.clone()), report);
    }

    /// Goal: every verdict has a human label. A missing arm would otherwise
    /// only surface when a user pastes a report with a blank reason in it.
    #[test]
    fn every_verdict_has_a_label() {
        for verdict in [
            ChannelVerdict::Controllable,
            ChannelVerdict::FirmwareOverride,
            ChannelVerdict::FamilyMayNeedOutOfTree,
            ChannelVerdict::NoPwm,
            ChannelVerdict::PwmReadOnly,
            ChannelVerdict::IgnoresDuty,
            ChannelVerdict::Unverifiable,
        ] {
            assert!(verdict_label(verdict).is_empty().not());
        }
    }

    fn report_device(name: &str, excluded: Option<HwmonExclusion>) -> ReportDevice {
        ReportDevice {
            path: PathBuf::from(format!("/sys/class/hwmon/{name}")),
            name: name.to_string(),
            driver: Some(name.to_string()),
            fans: Vec::new(),
            excluded,
            excluded_channels: HashMap::new(),
            unresponsive: false,
        }
    }

    fn fan(name: &str, label: Option<&str>) -> HwmonChannelInfo {
        HwmonChannelInfo {
            name: name.to_string(),
            label: label.map(ToString::to_string),
            ..HwmonChannelInfo::default()
        }
    }

    /// Goal: a report full of `fanN` lines cannot tell the user which header
    /// is which. The driver's label is the only thing that can, so it rides
    /// along whenever the driver publishes one.
    #[test]
    fn channel_titles_carry_the_driver_label() {
        assert_eq!(
            channel_title(&fan("fan1", Some("PSU Fan"))),
            "fan1 \"PSU Fan\""
        );
        assert_eq!(channel_title(&fan("fan1", None)), "fan1");
        // A label of pure whitespace is no label; quoting it would add noise
        // that reads like a missing value.
        assert_eq!(channel_title(&fan("fan2", Some("  "))), "fan2");
    }

    /// Goal: a channel the user disabled must say so on its own line, and the
    /// device line must account for it. Otherwise the counts disagree with the
    /// app and nothing explains the difference.
    #[test]
    fn hidden_channels_are_counted_and_explained() {
        assert_eq!(hidden_channel_suffix(2), ", 2 hidden");
        assert_eq!(hidden_channel_suffix(0), "");
        assert_eq!(
            ChannelExclusion::UserDisabled.label(),
            "hidden: disabled in settings"
        );
        assert_eq!(
            ChannelExclusion::IgnoredBySensorsConf.label(),
            "hidden: ignored by sensors.conf"
        );
    }

    /// Goal: an hwmon device the daemon hid must say so. Without it, a chip
    /// that is deliberately covered by its liquidctl entry reads as a device
    /// the app failed to pick up, which is the opposite of what happened.
    #[test]
    fn hidden_hwmon_devices_say_why() {
        let hidden = report_device("corsairpsu", Some(HwmonExclusion::DuplicateOfLiquidctl));
        assert_eq!(exclusion_suffix(&hidden), ", handled by liquidctl");
    }

    /// Goal: a device another repository serves is still in the app and its
    /// sysfs files are still read, so the report must call it neither hidden
    /// nor unused. amdgpu is dropped by the hwmon name blacklist purely
    /// because the GPU repository owns it, and either word asserts something
    /// the user can see is false.
    #[test]
    fn a_device_served_elsewhere_is_neither_hidden_nor_unused() {
        for reason in [
            HwmonExclusion::ServedByGpuRepository,
            HwmonExclusion::DuplicateOfLiquidctl,
        ] {
            let suffix = exclusion_suffix(&report_device("amdgpu", Some(reason)));
            assert!(suffix.contains("handled by"), "{suffix}");
            assert!(suffix.contains("hidden").not(), "{suffix}");
            assert!(suffix.contains("not used").not(), "{suffix}");
        }
    }

    /// Goal: the marker stays off the devices the app actually shows. One on
    /// every line would bury the few that matter.
    #[test]
    fn shown_hwmon_devices_carry_no_marker() {
        assert_eq!(exclusion_suffix(&report_device("nct6687", None)), "");
    }

    /// Goal: every exclusion reason can explain itself, including the ones a
    /// maintainer sees rarely. A blank suffix in a pasted report is worse than
    /// no suffix, because it looks like a bug in the reporter.
    #[test]
    fn every_exclusion_has_a_label() {
        for reason in [
            HwmonExclusion::DuplicateOfLiquidctl,
            HwmonExclusion::ServedByGpuRepository,
            HwmonExclusion::UserDisabled,
            HwmonExclusion::AllChannelsIgnored,
        ] {
            assert!(reason.label().is_empty().not(), "{reason:?} has no label");
        }
    }

    fn summary(device_type: DeviceType, name: &str, enabled: bool, fans: usize) -> DeviceSummary {
        DeviceSummary {
            device_type,
            name: name.to_string(),
            enabled,
            driver: Some("kraken2".to_string()),
            driver_version: Some("1.15.0".to_string()),
            fan_count: fans,
            controllable_fan_count: fans,
            firmware_version: Some("1.2.3".to_string()),
            hwmon_backed: false,
        }
    }

    /// Goal: a disabled device still appears, marked as disabled. Without it,
    /// "CoolerControl cannot see my cooler" and "I turned that cooler off"
    /// produce identical reports, which sends a support thread the wrong way.
    #[test]
    fn disabled_devices_are_listed_and_marked() {
        let mut report = String::new();
        write_device_sections(
            &mut report,
            &[
                summary(DeviceType::Liquidctl, "NZXT Kraken", true, 2),
                summary(DeviceType::Liquidctl, "Old Pump", false, 1),
            ],
        );
        assert!(report.contains("Old Pump [kraken2], disabled"), "{report}");
        // The enabled device must not pick up the marker.
        assert!(
            report.contains("NZXT Kraken [kraken2]  2 fan(s)"),
            "{report}"
        );
    }

    /// Goal: every repository that can own a fan gets a heading and a count.
    /// A maintainer reading a pasted report should be able to account for every
    /// fan on the machine, not just the ones hwmon exposes.
    #[test]
    fn every_repository_reports_its_fan_counts() {
        let mut report = String::new();
        write_device_sections(
            &mut report,
            &[
                summary(DeviceType::Liquidctl, "NZXT Kraken", true, 2),
                summary(DeviceType::GPU, "RTX 4090", true, 1),
                summary(DeviceType::ServicePlugin, "My Plugin", true, 3),
            ],
        );
        for heading in ["Liquidctl", "GPU", "Service Plugins"] {
            assert!(report.contains(heading), "missing {heading}: {report}");
        }
        assert!(
            report.contains("RTX 4090 [kraken2]  1 fan(s), 1 controllable"),
            "{report}"
        );
        assert!(
            report.contains("My Plugin [kraken2]  3 fan(s), 3 controllable"),
            "{report}"
        );
    }

    fn driver_info(name: &str, fans: &[&str]) -> Rc<HwmonDriverInfo> {
        Rc::new(HwmonDriverInfo {
            name: name.to_string(),
            path: PathBuf::from(format!("/sys/class/hwmon/{name}")),
            channels: fans
                .iter()
                .map(|fan| HwmonChannelInfo {
                    hwmon_type: HwmonChannelType::Fan,
                    name: (*fan).to_string(),
                    ..HwmonChannelInfo::default()
                })
                .collect(),
            ..HwmonDriverInfo::default()
        })
    }

    /// Goal: the compact report describes a device the daemon already read
    /// without touching sysfs for it again. Method: hand it a retained driver
    /// whose path does not exist, and require its channels to come through
    /// anyway. A path that cannot be read is the strongest possible proof that
    /// nothing was read.
    #[test]
    fn retained_devices_are_not_re_read() {
        let retained = [driver_info("nct6687", &["fan1", "fan2"])];
        let devices = crate::rt::test_runtime(async {
            collect_retained(
                &retained,
                &HiddenHardware::default(),
                ReadBudget::starting_now(),
            )
            .await
        });
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "nct6687");
        assert_eq!(devices[0].fans.len(), 2, "retained channels must survive");
    }

    /// Goal: a device with an individually hidden channel still gets read.
    /// The repository filters those channels out before retaining, so the
    /// retained copy cannot show what went missing, and reusing it would
    /// silently drop the hidden-channel lines.
    #[test]
    fn a_device_with_a_hidden_channel_is_re_read() {
        let retained = [driver_info("nct6687", &["fan1"])];
        let canonical = cc_fs::canonicalize(Path::new("/sys/class/hwmon"))
            .unwrap_or_else(|_| PathBuf::from("/sys/class/hwmon"));
        let mut hidden = HiddenHardware::default();
        hidden.channels.insert(
            (canonical, "fan7".to_string()),
            ChannelExclusion::UserDisabled,
        );
        // The retained path does not resolve, so nothing is found to re-read
        // and the fan list comes back empty rather than silently reusing the
        // retained channels.
        let devices = crate::rt::test_runtime(async {
            collect_retained(&retained, &hidden, ReadBudget::starting_now()).await
        });
        assert_eq!(devices.len(), 1);
    }

    /// Goal: the report must not read a device liquidctl is talking to.
    ///
    /// `corsairpsu` is excluded from the daemon precisely because liqctld owns
    /// the same PSU, and its hwmon reads go out over USB HID. Scanning it here
    /// was slow and made liqctld's own transfers fail with "possible conflict
    /// with another program". Every other exclusion is a device nothing else is
    /// using, so those stay scannable.
    #[test]
    fn a_device_liquidctl_owns_is_never_read() {
        assert!(scan_is_safe(Some(HwmonExclusion::DuplicateOfLiquidctl)).not());
        assert!(scan_is_safe(None));
        for reason in [
            HwmonExclusion::ServedByGpuRepository,
            HwmonExclusion::UserDisabled,
            HwmonExclusion::AllChannelsIgnored,
        ] {
            assert!(
                scan_is_safe(Some(reason)),
                "{reason:?} should stay scannable"
            );
        }
    }

    /// Goal: a device we chose not to read is still listed, and says why plus
    /// where its fans are accounted for. Omitting it would read as the report
    /// having missed the hardware.
    #[test]
    fn an_unscanned_device_is_still_listed() {
        let device = report_device("corsairpsu", Some(HwmonExclusion::DuplicateOfLiquidctl));
        let mut report = String::new();
        crate::rt::test_runtime(async {
            write_fan_summary(
                &mut report,
                std::slice::from_ref(&device),
                ReadBudget::starting_now(),
            )
            .await;
        });
        assert!(report.contains("corsairpsu"), "{report}");
        assert!(report.contains("not scanned"), "{report}");
        assert!(report.contains("Liquidctl section"), "{report}");
    }

    /// Goal: a repository with nothing on this machine says so. A missing
    /// heading reads as "the report forgot", and sends the reader looking.
    #[test]
    fn a_repository_with_no_devices_says_so() {
        let mut report = String::new();
        write_device_sections(&mut report, &[]);
        assert_eq!(report.matches("no devices").count(), 3, "{report}");
    }

    /// Goal: a chip line carries the address and device id, matching the
    /// startup detection log, so a pasted report and a pasted log can be lined
    /// up against each other. Base address is detail for `--full` only, to keep
    /// the compact form inside the paste budget.
    #[test]
    fn chip_lines_carry_address_and_device_id() {
        let detection = cc_detect::DetectionResults {
            detected_chips: vec![cc_detect::DetectedChipInfo {
                name: "Nuvoton NCT6687D-R eSIO".to_string(),
                driver: "nct6687".to_string(),
                address: "0x4E".to_string(),
                base_address: "0x0A20".to_string(),
                device_id: "0xD592".to_string(),
                features: vec!["fan".to_string()],
                module_status: "already_loaded".to_string(),
            }],
            blacklisted: Vec::new(),
            environment: cc_detect::EnvironmentInfo {
                is_container: false,
                is_secure_boot: false,
                has_dev_port: true,
            },
        };
        let mut compact = String::new();
        write_probe_summary(&mut compact, Some(&detection), false, true);
        assert!(
            compact.contains("Nuvoton NCT6687D-R eSIO at 0x4E id:0xD592 -> nct6687"),
            "unexpected chip line:\n{compact}"
        );
        assert!(compact.contains("base address").not());

        let mut full = String::new();
        write_probe_summary(&mut full, Some(&detection), true, true);
        assert!(full.contains("base address 0x0A20"), "{full}");
    }

    /// Goal: hwmon devices sort numerically. Lexicographic order puts hwmon10
    /// ahead of hwmon2, which reads as a bug in a report a human is scanning.
    #[test]
    fn hwmon_devices_sort_numerically() {
        let mut paths = [
            PathBuf::from("/sys/class/hwmon/hwmon10"),
            PathBuf::from("/sys/class/hwmon/hwmon2"),
            PathBuf::from("/sys/class/hwmon/hwmon1"),
        ];
        paths.sort_by_key(|path| (hwmon_index(path), path.clone()));
        let order = paths
            .iter()
            .map(|path| hwmon_index(path))
            .collect::<Vec<_>>();
        assert_eq!(order, vec![1, 2, 10]);
    }

    /// Goal: an unexpected directory name must not panic or vanish, it sorts
    /// last so the report still shows it.
    #[test]
    fn unparseable_hwmon_name_sorts_last() {
        assert_eq!(hwmon_index(Path::new("/sys/class/hwmon/oddball")), u32::MAX);
    }

    /// Goal: with detection switched off the report says so rather than
    /// asserting an environment it never observed. Reporting NoDevPort when no
    /// probe ran would be a claim we cannot back.
    #[test]
    fn absent_detection_asserts_nothing_about_the_environment() {
        let mut report = String::new();
        write_probe_summary(&mut report, None, false, true);
        assert!(report.contains("skipped"));
        let mut findings = String::new();
        write_findings(&mut findings, None, true);
        assert!(findings.contains("not determined"));
        assert!(findings.contains("Secure Boot").not());
    }

    /// Goal: a fresh budget hands a device its own cap, not the whole report's.
    /// One device must never be able to spend every other device's time.
    #[test]
    fn a_fresh_budget_caps_each_device_at_its_own_limit() {
        let allowed = ReadBudget::starting_now()
            .for_device()
            .expect("a fresh budget has time left");
        assert_eq!(allowed, DEVICE_READ_BUDGET);
    }

    /// Goal: near the end of the report the remainder is smaller than a device's
    /// cap, and the smaller of the two has to win. Method: build a budget whose
    /// deadline is deliberately closer than `DEVICE_READ_BUDGET`. Without this
    /// the last devices scanned could overrun the actor's own timeout between
    /// them and lose the whole report.
    #[test]
    fn a_nearly_spent_budget_hands_out_only_what_is_left() {
        let budget = ReadBudget {
            deadline: Instant::now() + Duration::from_millis(200),
        };
        let allowed = budget.for_device().expect("200ms is still time");
        assert!(allowed <= Duration::from_millis(200), "allowed {allowed:?}");
        assert!(allowed < DEVICE_READ_BUDGET);
    }

    /// Goal: a spent budget refuses rather than handing out a zero-length one,
    /// so the caller reports the device by name instead of silently reading it
    /// with no time at all.
    #[test]
    fn a_spent_budget_hands_out_nothing() {
        let budget = ReadBudget {
            deadline: Instant::now() - Duration::from_secs(1),
        };
        assert!(budget.for_device().is_none());
    }

    /// Goal: work that finishes inside the budget is returned untouched. The
    /// common case must pay nothing for the bound being there.
    #[test]
    #[serial]
    fn work_inside_the_budget_returns_its_value() {
        cc_fs::test_runtime(async {
            let budget = ReadBudget::starting_now();
            let value = within_budget(
                budget,
                "nct6687",
                Path::new("/sys/class/hwmon/hwmon0"),
                async { 42_u8 },
            )
            .await;
            assert_eq!(value, Some(42));
        });
    }

    /// Goal: a driver that stops answering costs the report that one device and
    /// nothing else. Method: give a sleep far longer than the budget and check
    /// the helper gives up well inside the sleep, so the caller can carry on
    /// with the rest of the tree. The harness runs the rest of the suite
    /// alongside this one and a preempted timer only ever fires late, so the
    /// bound is judged on the fastest of several attempts. Abandonment itself
    /// is not a timing claim, so every attempt must show it.
    #[test]
    #[serial]
    #[cfg(feature = "gated-tests")]
    fn work_past_the_budget_is_abandoned() {
        const ATTEMPTS: u32 = 5;
        const _: () = assert!(ATTEMPTS > 0, "best stays unset without an attempt");
        cc_fs::test_runtime(async {
            let mut best = Duration::MAX;
            for _ in 0..ATTEMPTS {
                let budget = ReadBudget {
                    deadline: Instant::now() + Duration::from_millis(100),
                };
                let started = Instant::now();
                let value = within_budget(
                    budget,
                    "stuck",
                    Path::new("/sys/class/hwmon/hwmon0"),
                    async {
                        rt::sleep(Duration::from_secs(5)).await;
                        42_u8
                    },
                )
                .await;
                assert!(value.is_none(), "expected the read to be abandoned");
                best = best.min(started.elapsed());
            }
            assert!(
                best < Duration::from_secs(1),
                "gave up after {best:?} at best, which is not bounded"
            );
        });
    }

    /// Goal: the full tree names the device that stopped answering instead of
    /// dropping the whole report. That line is the entire point of the report,
    /// and an all-or-nothing timeout is exactly what cannot produce it.
    #[test]
    #[serial]
    fn the_full_tree_names_a_device_that_did_not_answer() {
        cc_fs::test_runtime(async {
            let mut stuck = report_device("octo", None);
            stuck.unresponsive = true;
            let mut report = String::new();
            write_full_tree(&mut report, &[stuck], ReadBudget::starting_now()).await;
            assert!(report.contains("octo"));
            assert!(report.contains(UNRESPONSIVE_NOTE));
        });
    }

    /// Goal: the negative space. A device that answered carries no such note,
    /// so the marker keeps meaning something when it does appear.
    #[test]
    #[serial]
    fn a_device_that_answered_carries_no_note() {
        cc_fs::test_runtime(async {
            let healthy = report_device("nct6687", None);
            let mut report = String::new();
            write_full_tree(&mut report, &[healthy], ReadBudget::starting_now()).await;
            assert!(report.contains("nct6687"));
            assert!(report.contains(UNRESPONSIVE_NOTE).not());
        });
    }

    /// Goal: an unresponsive device stays in the fan summary. Its `fans` is
    /// empty because the scan gave up, and the summary skips empty devices, so
    /// without an explicit branch the one device worth reporting is the one
    /// that vanishes.
    #[test]
    #[serial]
    fn the_fan_summary_keeps_a_device_that_did_not_answer() {
        cc_fs::test_runtime(async {
            let mut stuck = report_device("octo", None);
            stuck.unresponsive = true;
            let mut report = String::new();
            write_fan_summary(&mut report, &[stuck], ReadBudget::starting_now()).await;
            assert!(report.contains("octo"));
            assert!(report.contains(UNRESPONSIVE_NOTE));
            assert!(report.contains("none found").not());
        });
    }

    /// Goal: on an architecture with no Super-I/O bus (aarch64), the report says
    /// the probe is unsupported rather than claiming a blocked environment. This
    /// runs on every host, so the non-x86 path stays covered on x86 CI too.
    #[test]
    fn unsupported_architecture_reports_no_environment_claim() {
        let mut report = String::new();
        write_probe_summary(&mut report, None, false, false);
        assert!(report.contains("not supported on this architecture"));
        assert!(report.contains("skipped").not());

        let mut findings = String::new();
        write_findings(&mut findings, None, false);
        assert!(findings.contains("detection unsupported on this architecture"));
        assert!(findings.contains("Secure Boot").not());
        assert!(findings.contains("not determined").not());
    }
}
