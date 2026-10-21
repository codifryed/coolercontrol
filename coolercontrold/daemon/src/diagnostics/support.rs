// SPDX-FileCopyrightText: 2026 Guy Boldon, Eren Simsek and contributors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Hardware support diagnostics.
//!
//! The daemon asserts observations, the docs assert remedies. Nothing in this
//! module names a kernel module, a module parameter, or a fix, so the guidance
//! can never go stale against the docs site.
//!
//! Two scopes, deliberately separate. A `ChannelVerdict` explains one fan
//! channel that hwmon already exposes. A `SystemFinding` explains a chip that
//! produced no hwmon channel at all, which is why it cannot be a channel
//! verdict: there is nothing to attach it to.

use crate::cc_fs;
use crate::device::{ChannelName, DeviceInfo, DeviceUID};
use crate::rt;
use log::{info, warn};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Not;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::repositories::hwmon::hwmon_repo::HwmonDriverInfo;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where bound drivers are resolved from. Every hwmon device appears here as a
/// symlink regardless of which bus it lives on.
const HWMON_CLASS_PATH: &str = "/sys/class/hwmon";

/// Super-I/O families whose in-tree driver frequently lacks fan control that
/// an out-of-tree variant provides.
///
/// Matched against the hwmon `name` attribute, which is the *chip*, not the
/// module: the `it87` module presents as `it8728`, `it8686` and friends, so a
/// literal `it87` match would miss most of the family.
const OUT_OF_TREE_CANDIDATE_PREFIXES: [&str; 2] = ["nct6", "it8"];

/// Why a fan channel can or cannot be driven, from local evidence only.
///
/// `FirmwareOverride`, `IgnoresDuty` and `Unverifiable` can only come from a
/// duty-response probe, which is not part of this work. They stay in the
/// taxonomy and in the wire shape so the probe can land later without moving
/// the API; until then nothing produces them, which is the honest answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChannelVerdict {
    /// `pwmN` is present and writable and nothing has reclaimed it.
    Controllable,
    /// Manual mode was set and later read back as something else.
    FirmwareOverride,
    /// No writable control, on a family where an out-of-tree driver may help.
    FamilyMayNeedOutOfTree,
    /// The driver in use exposes no control for this channel. Reported by
    /// repositories that know only whether a channel is drivable, with no
    /// sysfs-level facts to back a more specific verdict.
    NotSupportedByDriver,
    /// The driver exposes no `pwmN` for this channel.
    NoPwm,
    /// `pwmN` exists but the driver cleared its write bit. An Apple driver's own control file
    /// in that state gets this verdict too, without evidence.
    PwmReadOnly,
    /// Probe proved writes are accepted and the fan never responded.
    IgnoresDuty,
    /// No usable tachometer, so no probe can settle the question.
    Unverifiable,
}

impl ChannelVerdict {
    /// Whether this verdict means `CoolerControl` can currently drive the fan.
    pub fn is_controllable(self) -> bool {
        matches!(self, Self::Controllable)
    }

    /// Whether this is a fact about the hardware that cannot change while the
    /// machine is running, as opposed to a condition that can clear.
    ///
    /// `FirmwareOverride` is deliberately *not* permanent: it is only
    /// observable after control was attempted, and it stops being true if the
    /// firmware stops reclaiming the channel. Grouping it with the permanent
    /// capabilities would misrepresent both.
    pub fn is_permanent_capability(self) -> bool {
        matches!(
            self,
            Self::Controllable
                | Self::FamilyMayNeedOutOfTree
                | Self::NotSupportedByDriver
                | Self::NoPwm
                | Self::PwmReadOnly
                | Self::IgnoresDuty
                | Self::Unverifiable
        )
    }
}

/// The raw facts a `ChannelVerdict` was computed from.
///
/// Carried alongside the verdict so every surface can show what was measured
/// rather than asking the user to trust a label.
///
/// The bools are four genuinely independent sysfs observations, not a state
/// machine that wants an enum: any combination of them can occur on real
/// hardware, and collapsing them would discard the evidence this type exists
/// to carry.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelEvidence {
    pub has_pwm: bool,
    pub pwm_writable: bool,
    pub has_rpm: bool,
    /// A missing `pwmN_enable` is not a defect. Drivers implement different
    /// feature sets; its absence usually means only that fan control cannot be
    /// handed back to the BIOS. It is evidence *against* `FirmwareOverride`,
    /// because there is no auto mode to revert to.
    pub has_pwm_enable: bool,
}

/// One channel's diagnosis: the verdict and the evidence behind it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelDiagnosis {
    pub verdict: ChannelVerdict,
    /// `None` when the reporting repository has no sysfs-level facts. Absent
    /// beats all-false: four zeroed booleans read as measurements that were
    /// never taken.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ChannelEvidence>,
}

/// A diagnosis bound to the channel it describes, for the health snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ChannelVerdictRef {
    pub device_uid: DeviceUID,
    pub channel_name: ChannelName,
    pub verdict: ChannelVerdict,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<ChannelEvidence>,
}

/// Stable ordering so a client rendering the snapshot does not see rows
/// shuffle between polls of an unordered map.
fn sort_key(reference: &ChannelVerdictRef) -> (&str, &str) {
    (&reference.device_uid, &reference.channel_name)
}

/// Computes the passive verdict for one fan channel.
///
/// `firmware_override_observed` comes from the `pwm_enable` read-back and is
/// ignored unless the channel actually has a `pwm_enable` file.
pub fn diagnose_channel(
    evidence: ChannelEvidence,
    hwmon_name: &str,
    firmware_override_observed: bool,
) -> ChannelDiagnosis {
    debug_assert!(
        evidence.pwm_writable.not() || evidence.has_pwm,
        "a channel cannot have a writable pwm without having a pwm"
    );
    debug_assert!(
        evidence.has_pwm_enable.not() || evidence.has_pwm,
        "a channel cannot have pwm_enable without having a pwm"
    );
    let verdict = if evidence.has_pwm.not() {
        family_fallback(hwmon_name, ChannelVerdict::NoPwm)
    } else if evidence.pwm_writable.not() {
        family_fallback(hwmon_name, ChannelVerdict::PwmReadOnly)
    } else if firmware_override_observed && evidence.has_pwm_enable {
        ChannelVerdict::FirmwareOverride
    } else {
        ChannelVerdict::Controllable
    };
    ChannelDiagnosis {
        verdict,
        evidence: Some(evidence),
    }
}

/// The diagnosis a repository can make when all it knows is whether the
/// channel is drivable.
///
/// Deliberately carries no evidence: liquidctl, GPU and plugin repositories
/// have no pwm file to inspect, and reporting hwmon's fields as false would
/// assert measurements nobody took.
pub fn diagnose_driver_channel(controllable: bool) -> ChannelDiagnosis {
    ChannelDiagnosis {
        verdict: if controllable {
            ChannelVerdict::Controllable
        } else {
            ChannelVerdict::NotSupportedByDriver
        },
        evidence: None,
    }
}

/// The diagnosis for a driver that exposes a fan's control file read-only, where that file is
/// not a `pwmN`.
///
/// Carries no evidence, for the reason `diagnose_driver_channel` gives.
pub fn diagnose_read_only_driver_channel() -> ChannelDiagnosis {
    ChannelDiagnosis {
        verdict: ChannelVerdict::PwmReadOnly,
        evidence: None,
    }
}

/// Family guidance is evidence-gated, not identity-gated: it only ever
/// replaces a verdict that already established there is no writable control.
/// A board with eleven working fans and one locked header must not send the
/// user module-hunting.
fn family_fallback(hwmon_name: &str, otherwise: ChannelVerdict) -> ChannelVerdict {
    debug_assert!(
        otherwise.is_controllable().not(),
        "family guidance must only replace a no-control verdict"
    );
    if is_out_of_tree_candidate_family(hwmon_name) {
        ChannelVerdict::FamilyMayNeedOutOfTree
    } else {
        otherwise
    }
}

/// Whether an hwmon `name` belongs to a family with a known out-of-tree driver.
pub fn is_out_of_tree_candidate_family(hwmon_name: &str) -> bool {
    let name_lower = hwmon_name.to_ascii_lowercase();
    OUT_OF_TREE_CANDIDATE_PREFIXES
        .iter()
        .any(|prefix| name_lower.starts_with(prefix))
}

/// Why the hwmon repository left a device out.
///
/// The report enumerates hwmon itself, so it sees devices the repository
/// dropped. Without this, a device deliberately left out and one that failed to
/// come up look identical in a pasted report.
///
/// Two different things live here, and the wording keeps them apart. Some of
/// these devices are gone from the app entirely; others are very much present,
/// just read through a repository that offers more than hwmon does. For that
/// second kind the sysfs files are read either way, so neither "hidden" nor
/// "not used" is true: all that differs is that the hwmon entry is not a
/// separate device in the app.
///
/// Not on the wire: this shapes a line of the text report, nothing more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HwmonExclusion {
    /// A liquidctl device already covers this chip and duplicates are hidden.
    /// The liquidctl entry offers more features, so it wins. Still in the app.
    DuplicateOfLiquidctl,
    /// Excluded by name because the GPU repository serves it. Still in the
    /// app, and the same sysfs files are still read, just through that
    /// repository. Named for the GPU one because that is the only thing the
    /// hwmon name blacklist holds; a compile-time assertion beside the list
    /// keeps this honest if it ever gains an entry.
    ServedByGpuRepository,
    /// The user disabled the device in settings. Absent from the app.
    UserDisabled,
    /// The lm-sensors configuration ignores every channel it has. Absent from
    /// the app.
    AllChannelsIgnored,
    /// Reading its identity got no answer at startup, so it is absent from the
    /// app until the next start.
    Unanswered,
}

impl HwmonExclusion {
    /// Phrased for a report a user pastes into a support channel, so it says
    /// what happened rather than naming the internal rule.
    ///
    /// The first two take no prefix at all. Both the device and its sysfs
    /// files are still in use; only the standalone hwmon entry is absent, so
    /// "hidden" and "not used" would each assert something false. "hidden"
    /// belongs only where the device really is gone from the app.
    pub fn label(self) -> &'static str {
        match self {
            Self::DuplicateOfLiquidctl => "handled by liquidctl",
            Self::ServedByGpuRepository => "handled by the GPU repository",
            Self::UserDisabled => "hidden: disabled in settings",
            Self::AllChannelsIgnored => "hidden: every channel ignored by sensors.conf",
            Self::Unanswered => "hidden: did not answer at startup",
        }
    }
}

/// Why one channel is absent from the app while sysfs still exposes it.
///
/// Separate from `HwmonExclusion` because the device is present either way:
/// the user sees most of a chip and wonders where the rest of it went, which
/// reads as a detection failure rather than as their own configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelExclusion {
    /// The user disabled this channel in settings.
    UserDisabled,
    /// The lm-sensors configuration ignores it.
    IgnoredBySensorsConf,
}

impl ChannelExclusion {
    pub fn label(self) -> &'static str {
        match self {
            Self::UserDisabled => "hidden: disabled in settings",
            Self::IgnoredBySensorsConf => "hidden: ignored by sensors.conf",
        }
    }
}

/// Everything the daemon left out, for the report to explain.
///
/// Absent from the standalone CLI, which has no daemon to ask and therefore
/// says nothing rather than implying nothing is hidden.
#[derive(Debug, Clone, Default)]
pub struct HiddenHardware {
    pub devices: HashMap<PathBuf, HwmonExclusion>,
    pub channels: HashMap<(PathBuf, ChannelName), ChannelExclusion>,
}

/// Why the Super-I/O probe could not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentBlocker {
    SecureBoot,
    Container,
    NoDevPort,
}

/// A fact about the machine rather than about one channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SystemFinding {
    /// A chip was detected but no loaded driver serves its family.
    NoDriverBound {
        chip_name: String,
        expected_driver: String,
    },
    /// modprobe refused because the driver is blacklisted.
    Blacklisted { driver: String },
    /// The probe could not run at all.
    BlockedByEnvironment { reason: EnvironmentBlocker },
    /// Not an `x86_64` target, so no probe was ever attempted. Distinct from
    /// `BlockedByEnvironment` because the stub reports `has_dev_port: false`
    /// on platforms where `/dev/port` is simply irrelevant, and reporting that
    /// as a blocker would be a claim we cannot back with an observation.
    DetectionUnsupported,
}

/// Normalizes a driver or chip name to its family key.
///
/// In-tree and out-of-tree drivers for the same chip have different module
/// names (`nct6683` in-tree versus `nct6687` out-of-tree), and the chip
/// database records the *expected* in-tree driver. Comparing exact names would
/// report a fully working board as having no driver bound.
fn driver_family(name: &str) -> String {
    let name_lower = name.to_ascii_lowercase();
    for prefix in OUT_OF_TREE_CANDIDATE_PREFIXES {
        if name_lower.starts_with(prefix) {
            return prefix.to_string();
        }
    }
    name_lower
}

/// Whether any loaded driver serves the same family as `expected_driver`.
fn is_chip_served(expected_driver: &str, bound_drivers: &HashSet<String>) -> bool {
    let expected_family = driver_family(expected_driver);
    bound_drivers
        .iter()
        .any(|bound| driver_family(bound) == expected_family)
}

/// A chip detected by the Super-I/O probe, reduced to what the findings need.
pub struct DetectedChipRef<'a> {
    pub name: &'a str,
    pub driver: &'a str,
}

/// The environment the probe ran in.
pub struct ProbeEnvironment {
    pub is_container: bool,
    pub is_secure_boot: bool,
    pub has_dev_port: bool,
}

/// Derives the machine-scope findings.
///
/// `bound_drivers` must come from resolving each hwmon device's `device/driver`
/// symlink, not from the hwmon `name` attribute: `name` is the chip, and for
/// several families it does not match any module name.
pub fn derive_system_findings(
    detection_supported: bool,
    chips: &[DetectedChipRef<'_>],
    blacklisted: &[String],
    environment: &ProbeEnvironment,
    bound_drivers: &HashSet<String>,
) -> Vec<SystemFinding> {
    if detection_supported.not() {
        return vec![SystemFinding::DetectionUnsupported];
    }
    let mut findings = Vec::with_capacity(chips.len() + blacklisted.len() + 1);
    if let Some(reason) = environment_blocker(environment) {
        findings.push(SystemFinding::BlockedByEnvironment { reason });
    }
    for driver in blacklisted {
        findings.push(SystemFinding::Blacklisted {
            driver: driver.clone(),
        });
    }
    for chip in chips {
        if is_chip_served(chip.driver, bound_drivers) {
            continue;
        }
        findings.push(SystemFinding::NoDriverBound {
            chip_name: chip.name.to_string(),
            expected_driver: chip.driver.to_string(),
        });
    }
    findings
}

/// Secure Boot is checked first because it is the cause users are least likely
/// to suspect and it silently defeats module loading.
fn environment_blocker(environment: &ProbeEnvironment) -> Option<EnvironmentBlocker> {
    if environment.is_secure_boot {
        return Some(EnvironmentBlocker::SecureBoot);
    }
    if environment.is_container {
        return Some(EnvironmentBlocker::Container);
    }
    if environment.has_dev_port.not() {
        return Some(EnvironmentBlocker::NoDevPort);
    }
    None
}

/// Resolves the driver actually bound to each hwmon device.
///
/// Reads `<hwmon>/device/driver`, whose symlink target basename is the module
/// serving the device. This is the authoritative answer and the reason we do
/// not use the hwmon `name` attribute: `name` is the chip, and for several
/// families it matches no module name at all.
///
/// Devices with no such link (USB and HID hwmon entries among them) simply
/// contribute nothing, which is correct: they are not Super-I/O chips and
/// cannot satisfy a detected-chip lookup.
pub fn resolve_bound_drivers(hwmon_class_path: &Path) -> HashSet<String> {
    let mut bound = HashSet::new();
    let Ok(entries) = cc_fs::read_dir(hwmon_class_path) else {
        warn!(
            "Could not enumerate {} for bound driver resolution",
            hwmon_class_path.display()
        );
        return bound;
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let driver_link = entry.path().join("device").join("driver");
        let Ok(target) = cc_fs::read_link(&driver_link) else {
            continue;
        };
        let Some(driver_name) = target.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if driver_name.is_empty() {
            continue;
        }
        bound.insert(driver_name.to_ascii_lowercase());
    }
    bound
}

/// Machine identity, used to pick the right docs anchor and for the report.
///
/// Already read at startup for the log banner and previously discarded.
/// Deliberately excludes serials and UUIDs: this is destined for a report the
/// user pastes into a support channel.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SystemInfo {
    pub board_vendor: String,
    pub board_name: String,
    pub board_version: String,
    pub bios_vendor: String,
    pub bios_release: String,
}

impl SystemInfo {
    /// Read on the blocking pool, so no sysfs read runs on the runtime thread.
    pub async fn read() -> Self {
        rt::spawn_blocking(Self::read_blocking)
            .await
            .unwrap_or_default()
    }

    fn read_blocking() -> Self {
        Self {
            board_vendor: crate::logger::read_dmi_system_info("board_vendor"),
            board_name: crate::logger::read_dmi_system_info("board_name"),
            board_version: crate::logger::read_dmi_system_info("board_version"),
            bios_vendor: crate::logger::read_dmi_system_info("bios_vendor"),
            bios_release: crate::logger::read_dmi_system_info("bios_release"),
        }
    }
}

/// Retains what startup detection learned so it can be served later.
///
/// Detection runs before the device repositories are initialized, and module
/// loading only happens at that point, so re-probing on demand would answer a
/// different question than the one the user is asking.
pub struct HardwareSupportController {
    pub system_info: SystemInfo,
    /// What the last detection run found, seeded at startup and replaced by an
    /// explicit `POST /detect`. That request is the only other point where
    /// module loading can happen, so leaving this frozen would keep reporting a
    /// chip as unbound after the user just loaded its driver.
    detection: RefCell<Option<cc_detect::DetectionResults>>,
    /// Whether this architecture has a Super-I/O bus to probe at all. Retained
    /// rather than read from `cc_detect::DETECTION_SUPPORTED` at each use so a
    /// test can exercise both branches on any host.
    detection_supported: bool,
    /// Derived from `detection` and the bound drivers, so it is recomputed
    /// whenever `detection` is replaced.
    system_findings: RefCell<Vec<SystemFinding>>,
    /// Keyed by `(device_uid, channel_name)`. Seeded when a repository
    /// initializes its channels and updated when a runtime observation, such
    /// as a firmware reclaim, changes a verdict.
    channel_diagnoses: RefCell<HashMap<(DeviceUID, ChannelName), ChannelDiagnosis>>,
    /// Every non-hwmon device found at startup, including ones the user has
    /// disabled. Retained here so the report never has to re-enumerate devices
    /// just to be generated.
    ///
    /// Keyed by device rather than appended: a repository that publishes twice
    /// (a re-initialization after sleep, say) must not double every row in the
    /// report.
    device_summaries: RefCell<HashMap<DeviceUID, crate::diagnostics::report::DeviceSummary>>,
    /// The hwmon channel information the repository built at startup, kept so
    /// the report can describe a device it already knows without walking sysfs
    /// a second time.
    hwmon_drivers: RefCell<Vec<Rc<HwmonDriverInfo>>>,
    /// hwmon devices and channels the daemon deliberately dropped, keyed by
    /// canonical path. Recorded where each decision is actually made rather
    /// than re-derived, so the report cannot disagree with what the daemon did.
    hidden_hardware: RefCell<HiddenHardware>,
}

impl HardwareSupportController {
    pub async fn init(
        detection: Option<cc_detect::DetectionResults>,
        detection_supported: bool,
    ) -> Self {
        let system_info = SystemInfo::read().await;
        let system_findings = Self::compute_findings(detection.as_ref(), detection_supported).await;
        Self {
            system_info,
            detection: RefCell::new(detection),
            detection_supported,
            system_findings: RefCell::new(system_findings),
            channel_diagnoses: RefCell::new(HashMap::new()),
            device_summaries: RefCell::new(HashMap::new()),
            hwmon_drivers: RefCell::new(Vec::new()),
            hidden_hardware: RefCell::new(HiddenHardware::default()),
        }
    }

    /// What the last detection run found. `None` means no probe was made.
    pub fn detection(&self) -> Option<cc_detect::DetectionResults> {
        self.detection.borrow().clone()
    }

    /// The findings the health snapshot and the report both answer from.
    pub fn system_findings(&self) -> Vec<SystemFinding> {
        self.system_findings.borrow().clone()
    }

    /// Replaces the retained results after an explicit re-scan, and recomputes
    /// the findings from them so the health snapshot cannot keep asserting a
    /// driver is unbound after the same request loaded it.
    pub async fn refresh_detection(&self, results: cc_detect::DetectionResults) {
        let findings = Self::compute_findings(Some(&results), self.detection_supported).await;
        *self.detection.borrow_mut() = Some(results);
        *self.system_findings.borrow_mut() = findings;
    }

    /// Records that an hwmon device was left out, and why.
    ///
    /// Canonicalized on the way in because the report enumerates
    /// `/sys/class/hwmon`, whose entries are symlinks into the bus tree, while
    /// the repository walks its own paths. Both resolve to the same real path.
    pub fn record_excluded_hwmon(&self, path: &Path, reason: HwmonExclusion) {
        let Ok(canonical) = cc_fs::canonicalize(path) else {
            return;
        };
        self.hidden_hardware
            .borrow_mut()
            .devices
            .insert(canonical, reason);
    }

    /// Records that one channel of an otherwise-present device was left out.
    pub fn record_excluded_channel(
        &self,
        path: &Path,
        channel_name: &str,
        reason: ChannelExclusion,
    ) {
        let Ok(canonical) = cc_fs::canonicalize(path) else {
            return;
        };
        self.hidden_hardware
            .borrow_mut()
            .channels
            .insert((canonical, channel_name.to_string()), reason);
    }

    /// What was left out, for the report.
    pub fn hidden_hardware(&self) -> HiddenHardware {
        self.hidden_hardware.borrow().clone()
    }

    /// Records the drivability of a channel whose repository has no
    /// sysfs-level facts. One line at each repository's call site.
    pub fn record_driver_channel(&self, device_uid: &str, channel_name: &str, controllable: bool) {
        self.record_channel_diagnosis(
            device_uid,
            channel_name,
            diagnose_driver_channel(controllable),
        );
    }

    /// Records drivability for every speed channel on a device.
    ///
    /// For repositories whose only fact is `SpeedOptions::fixed_enabled`.
    /// hwmon does not use this: it has real sysfs evidence and publishes a
    /// specific verdict per channel instead.
    pub fn record_device_channels(&self, device_uid: &str, info: &DeviceInfo) {
        for (channel_name, channel_info) in &info.channels {
            let Some(speed_options) = channel_info.speed_options() else {
                continue;
            };
            self.record_driver_channel(device_uid, channel_name, speed_options.fixed_enabled);
        }
    }

    /// Records a non-hwmon device found at startup, enabled or not.
    pub fn record_device_summary(
        &self,
        device_uid: &str,
        summary: crate::diagnostics::report::DeviceSummary,
    ) {
        self.device_summaries
            .borrow_mut()
            .insert(device_uid.to_string(), summary);
    }

    /// The retained devices, for the report.
    ///
    /// Sorted by name so a report generated twice reads the same way; an
    /// unordered map would shuffle the rows between runs.
    pub fn device_summaries(&self) -> Vec<crate::diagnostics::report::DeviceSummary> {
        let mut summaries = self
            .device_summaries
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        summaries.sort_by(|a, b| a.name.cmp(&b.name));
        summaries
    }

    /// Retains an hwmon device's channel information for the report.
    ///
    /// The repository has already read every one of these files; re-reading
    /// them to print a report is work the daemon does not need to repeat, and
    /// on a device that talks over a bus it is work that can go wrong.
    pub fn record_hwmon_driver(&self, driver: Rc<HwmonDriverInfo>) {
        self.hwmon_drivers.borrow_mut().push(driver);
    }

    /// The retained hwmon devices, for the report.
    pub fn hwmon_drivers(&self) -> Vec<Rc<HwmonDriverInfo>> {
        self.hwmon_drivers.borrow().clone()
    }

    /// Records or replaces one channel's diagnosis.
    pub fn record_channel_diagnosis(
        &self,
        device_uid: &str,
        channel_name: &str,
        diagnosis: ChannelDiagnosis,
    ) {
        self.channel_diagnoses.borrow_mut().insert(
            (device_uid.to_string(), channel_name.to_string()),
            diagnosis,
        );
    }

    /// Splits the recorded diagnoses into the permanent capabilities and the
    /// conditions that can clear.
    ///
    /// Kept separate because a verdict is a fact about hardware while a
    /// firmware reclaim is a current state, and presenting them together would
    /// misrepresent both. Channels we can drive are omitted from both lists:
    /// there is nothing to report about hardware that works.
    pub fn partitioned_verdicts(&self) -> (Vec<ChannelVerdictRef>, Vec<ChannelVerdictRef>) {
        let diagnoses = self.channel_diagnoses.borrow();
        let mut permanent = Vec::with_capacity(diagnoses.len());
        let mut current = Vec::new();
        for ((device_uid, channel_name), diagnosis) in diagnoses.iter() {
            if diagnosis.verdict.is_controllable() {
                continue;
            }
            let reference = ChannelVerdictRef {
                device_uid: device_uid.clone(),
                channel_name: channel_name.clone(),
                verdict: diagnosis.verdict,
                evidence: diagnosis.evidence.clone(),
            };
            if diagnosis.verdict.is_permanent_capability() {
                permanent.push(reference);
            } else {
                current.push(reference);
            }
        }
        permanent.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
        current.sort_by(|a, b| sort_key(a).cmp(&sort_key(b)));
        (permanent, current)
    }

    /// `None` detection means the probe was switched off by config or
    /// environment, which is not an observation and therefore yields no
    /// findings.
    async fn compute_findings(
        detection: Option<&cc_detect::DetectionResults>,
        detection_supported: bool,
    ) -> Vec<SystemFinding> {
        if detection_supported.not() {
            return vec![SystemFinding::DetectionUnsupported];
        }
        let Some(results) = detection else {
            return Vec::new();
        };
        // A walk of the hwmon class, on the blocking pool like the probe before it.
        let resolved =
            rt::spawn_blocking(|| resolve_bound_drivers(Path::new(HWMON_CLASS_PATH))).await;
        let Ok(bound_drivers) = resolved else {
            // An empty set would report every detected chip as unbound, so report nothing.
            warn!("Could not resolve the bound hwmon drivers; no hardware findings this time");
            return Vec::new();
        };
        let chips = results
            .detected_chips
            .iter()
            .map(|chip| DetectedChipRef {
                name: &chip.name,
                driver: &chip.driver,
            })
            .collect::<Vec<_>>();
        let environment = ProbeEnvironment {
            is_container: results.environment.is_container,
            is_secure_boot: results.environment.is_secure_boot,
            has_dev_port: results.environment.has_dev_port,
        };
        derive_system_findings(
            true,
            &chips,
            &results.blacklisted,
            &environment,
            &bound_drivers,
        )
    }

    /// Secure Boot silently defeating module loading is invisible in the app
    /// today, so the findings are worth a startup line even before any surface
    /// consumes them.
    pub fn log_findings(&self) {
        if self.has_actionable_finding() {
            self.log_machine_context();
        }
        for finding in self.system_findings.borrow().iter() {
            match finding {
                SystemFinding::NoDriverBound {
                    chip_name,
                    expected_driver,
                } => warn!(
                    "Hardware support: {chip_name} was detected but no loaded driver serves it \
                     (expected something like {expected_driver})"
                ),
                SystemFinding::Blacklisted { driver } => {
                    warn!("Hardware support: driver {driver} is blacklisted and was not loaded");
                }
                SystemFinding::BlockedByEnvironment { reason } => warn!(
                    "Hardware support: Super-I/O detection could not run ({reason:?}); \
                     chips needing a Super-I/O probe will not be found"
                ),
                SystemFinding::DetectionUnsupported => {
                    info!("Hardware support: Super-I/O detection is x86_64 only");
                }
            }
        }
    }

    /// `DetectionUnsupported` is a statement about the build, not a problem
    /// with the machine, so it does not warrant dumping board context.
    fn has_actionable_finding(&self) -> bool {
        self.system_findings
            .borrow()
            .iter()
            .any(|finding| matches!(finding, SystemFinding::DetectionUnsupported).not())
    }

    /// The board and chip count are what a maintainer needs first when reading
    /// a log alongside a support question.
    fn log_machine_context(&self) {
        let SystemInfo {
            board_vendor,
            board_name,
            bios_vendor,
            bios_release,
            ..
        } = &self.system_info;
        info!(
            "Hardware support: board {board_vendor} {board_name}, BIOS {bios_vendor} \
             {bios_release}"
        );
        if let Some(detection) = self.detection.borrow().as_ref() {
            info!(
                "Hardware support: {} Super-I/O chip(s) detected",
                detection.detected_chips.len()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Goal: an explicit re-scan must move the findings the health snapshot is
    /// built from, otherwise `/devices/health` keeps naming a chip as unbound
    /// after the request that loaded its driver. Method: start from no
    /// detection at all, hand in results naming a driver nothing can be bound
    /// to, and read the findings back.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn refreshed_detection_recomputes_the_findings() {
        crate::cc_fs::test_runtime(async {
            let controller = HardwareSupportController::init(None, true).await;
            assert!(controller.system_findings().is_empty());

            controller
                .refresh_detection(cc_detect::DetectionResults {
                    detected_chips: vec![cc_detect::DetectedChipInfo {
                        name: "CC-TEST-CHIP".to_string(),
                        driver: "cc_test_no_such_driver".to_string(),
                        address: "0x4e".to_string(),
                        base_address: "0x4e".to_string(),
                        device_id: "0xd592".to_string(),
                        features: Vec::new(),
                        module_status: "detection_only".to_string(),
                    }],
                    blacklisted: Vec::new(),
                    environment: cc_detect::EnvironmentInfo {
                        is_container: false,
                        is_secure_boot: false,
                        has_dev_port: true,
                    },
                })
                .await;

            let findings = controller.system_findings();
            assert_eq!(findings.len(), 1, "expected exactly one finding");
            assert!(matches!(
                &findings[0],
                SystemFinding::NoDriverBound { chip_name, .. } if chip_name == "CC-TEST-CHIP"
            ));
            assert!(controller.detection().is_some());
        });
    }

    /// Goal: on an architecture with no Super-I/O bus (aarch64), findings say
    /// detection is unsupported rather than asserting an environment nothing
    /// observed, and a re-scan cannot talk the controller out of it. Runs on
    /// every host, so the non-x86 branch stays covered on x86 CI too.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn unsupported_architecture_yields_only_the_unsupported_finding() {
        crate::cc_fs::test_runtime(async {
            let controller = HardwareSupportController::init(None, false).await;
            assert_eq!(
                controller.system_findings(),
                vec![SystemFinding::DetectionUnsupported]
            );

            controller
                .refresh_detection(cc_detect::DetectionResults {
                    detected_chips: vec![cc_detect::DetectedChipInfo {
                        name: "CC-TEST-CHIP".to_string(),
                        driver: "cc_test_no_such_driver".to_string(),
                        address: "0x4e".to_string(),
                        base_address: "0x4e".to_string(),
                        device_id: "0xd592".to_string(),
                        features: Vec::new(),
                        module_status: "detection_only".to_string(),
                    }],
                    blacklisted: Vec::new(),
                    environment: cc_detect::EnvironmentInfo {
                        is_container: false,
                        is_secure_boot: false,
                        has_dev_port: true,
                    },
                })
                .await;

            assert_eq!(
                controller.system_findings(),
                vec![SystemFinding::DetectionUnsupported],
                "an unsupported arch must not derive findings from a re-scan"
            );
        });
    }

    /// Goal: the path every non-hwmon repository publishes through must turn
    /// `fixed_enabled` into a verdict, skip channels that are not speed
    /// channels, and leave drivable ones out of the health lists. Method: one
    /// device carrying all three cases, read back through the same partition
    /// the health snapshot is built from.
    #[test]
    #[cfg(feature = "gated-tests")]
    fn recorded_driver_channels_become_verdicts() {
        crate::cc_fs::test_runtime(async {
            let controller = HardwareSupportController::init(None, true).await;
            let mut info = DeviceInfo::default();
            info.channels.insert(
                "fan1".to_string(),
                crate::device::ChannelInfo {
                    label: None,
                    kind: crate::device::ChannelKind::Speed(crate::device::SpeedOptions {
                        fixed_enabled: true,
                        ..Default::default()
                    }),
                },
            );
            info.channels.insert(
                "fan2".to_string(),
                crate::device::ChannelInfo {
                    label: None,
                    kind: crate::device::ChannelKind::Speed(crate::device::SpeedOptions {
                        fixed_enabled: false,
                        ..Default::default()
                    }),
                },
            );
            info.channels.insert(
                "led1".to_string(),
                crate::device::ChannelInfo {
                    label: None,
                    kind: crate::device::ChannelKind::Lighting(Vec::new()),
                },
            );

            controller.record_device_channels("gpu-uid", &info);

            let (permanent, current) = controller.partitioned_verdicts();
            assert!(current.is_empty(), "unexpected current-state verdicts");
            // Only the undrivable speed channel: the drivable one has nothing
            // to report and the lighting channel has no speed options at all.
            assert_eq!(permanent.len(), 1);
            assert_eq!(permanent[0].channel_name, "fan2");
            assert_eq!(permanent[0].verdict, ChannelVerdict::NotSupportedByDriver);
            // No sysfs was read, so no evidence may be claimed.
            assert!(permanent[0].evidence.is_none());
        });
    }

    /// The negative space of the case above: a repository that can drive the
    /// channel must not put it in front of the user at all.
    #[test]
    fn drivable_driver_channel_is_controllable() {
        let diagnosis = diagnose_driver_channel(true);
        assert_eq!(diagnosis.verdict, ChannelVerdict::Controllable);
        assert!(diagnosis.verdict.is_controllable());
        assert!(diagnosis.evidence.is_none());
    }

    fn evidence(has_pwm: bool, pwm_writable: bool, has_pwm_enable: bool) -> ChannelEvidence {
        ChannelEvidence {
            has_pwm,
            pwm_writable,
            has_rpm: true,
            has_pwm_enable,
        }
    }

    /// A writable pwm with nothing reclaiming it is the ordinary healthy case.
    #[test]
    fn writable_pwm_is_controllable() {
        let diagnosis = diagnose_channel(evidence(true, true, true), "nct6687", false);
        assert_eq!(diagnosis.verdict, ChannelVerdict::Controllable);
    }

    /// A channel with no pwm_enable is still fully controllable. Its absence
    /// only means control cannot be handed back to the BIOS.
    #[test]
    fn missing_pwm_enable_is_not_a_defect() {
        let diagnosis = diagnose_channel(evidence(true, true, false), "some_driver", false);
        assert_eq!(diagnosis.verdict, ChannelVerdict::Controllable);
    }

    /// The override read-back must be ignored when there is no pwm_enable file
    /// to have read back, otherwise absence would be treated as evidence for.
    #[test]
    fn override_requires_pwm_enable_to_exist() {
        let with_file = diagnose_channel(evidence(true, true, true), "some_driver", true);
        assert_eq!(with_file.verdict, ChannelVerdict::FirmwareOverride);
        let without_file = diagnose_channel(evidence(true, true, false), "some_driver", true);
        assert_eq!(without_file.verdict, ChannelVerdict::Controllable);
    }

    /// No writable control on a plain driver reports the honest mechanical
    /// reason rather than sending the user looking for a module.
    #[test]
    fn no_control_on_unknown_family_reports_mechanism() {
        let no_pwm = diagnose_channel(evidence(false, false, false), "some_driver", false);
        assert_eq!(no_pwm.verdict, ChannelVerdict::NoPwm);
        let read_only = diagnose_channel(evidence(true, false, false), "some_driver", false);
        assert_eq!(read_only.verdict, ChannelVerdict::PwmReadOnly);
    }

    /// Family guidance replaces a no-control verdict, and only that. A working
    /// channel on the same chip must stay Controllable.
    #[test]
    fn family_guidance_is_evidence_gated() {
        let locked = diagnose_channel(evidence(true, false, false), "nct6798", false);
        assert_eq!(locked.verdict, ChannelVerdict::FamilyMayNeedOutOfTree);
        let working = diagnose_channel(evidence(true, true, true), "nct6798", false);
        assert_eq!(working.verdict, ChannelVerdict::Controllable);
    }

    /// The it87 module presents as it8xxx chip names, so the family predicate
    /// has to match the chip prefix rather than the module name.
    #[test]
    fn it87_family_matches_chip_names() {
        assert!(is_out_of_tree_candidate_family("it8728"));
        assert!(is_out_of_tree_candidate_family("it8686"));
        assert!(is_out_of_tree_candidate_family("nct6683"));
        assert!(is_out_of_tree_candidate_family("NCT6687"));
        assert!(is_out_of_tree_candidate_family("k10temp").not());
        assert!(is_out_of_tree_candidate_family("amdgpu").not());
    }

    /// Regression guard for the worst false positive available to this
    /// feature. The chip database maps NCT6687D to the in-tree nct6683, but a
    /// working board may be served by the out-of-tree nct6687. Reporting
    /// NoDriverBound there would tell a user with working fans to go load
    /// kernel modules.
    #[test]
    fn out_of_tree_driver_counts_as_bound() {
        let chips = [DetectedChipRef {
            name: "Nuvoton NCT6687D eSIO",
            driver: "nct6683",
        }];
        let bound = HashSet::from(["nct6687".to_string()]);
        let findings = derive_system_findings(true, &chips, &[], &clear_environment(), &bound);
        assert!(
            findings.is_empty(),
            "expected no findings, got {findings:?}"
        );
    }

    /// A chip with genuinely nothing loaded for its family is the case the
    /// finding exists for.
    #[test]
    fn unserved_chip_reports_no_driver_bound() {
        let chips = [DetectedChipRef {
            name: "ITE IT8728F Super IO Sensors",
            driver: "it87",
        }];
        let bound = HashSet::from(["k10temp".to_string()]);
        let findings = derive_system_findings(true, &chips, &[], &clear_environment(), &bound);
        assert_eq!(
            findings,
            vec![SystemFinding::NoDriverBound {
                chip_name: "ITE IT8728F Super IO Sensors".to_string(),
                expected_driver: "it87".to_string(),
            }]
        );
    }

    /// On non-x86_64 the stub reports has_dev_port: false, which is not an
    /// observation. Reporting it as a blocker would fire on every ARM board.
    #[test]
    fn unsupported_detection_never_reports_blockers() {
        let environment = ProbeEnvironment {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: false,
        };
        let findings = derive_system_findings(false, &[], &[], &environment, &HashSet::new());
        assert_eq!(findings, vec![SystemFinding::DetectionUnsupported]);
    }

    /// Secure Boot outranks the other blockers because it is the one users are
    /// least likely to suspect.
    #[test]
    fn secure_boot_outranks_other_blockers() {
        let environment = ProbeEnvironment {
            is_container: true,
            is_secure_boot: true,
            has_dev_port: false,
        };
        let findings = derive_system_findings(true, &[], &[], &environment, &HashSet::new());
        assert_eq!(
            findings,
            vec![SystemFinding::BlockedByEnvironment {
                reason: EnvironmentBlocker::SecureBoot
            }]
        );
    }

    /// A blacklisted driver is strongly actionable and must survive alongside
    /// the chip scan.
    #[test]
    fn blacklisted_drivers_are_reported() {
        let blacklisted = vec!["it87".to_string()];
        let findings = derive_system_findings(
            true,
            &[],
            &blacklisted,
            &clear_environment(),
            &HashSet::new(),
        );
        assert_eq!(
            findings,
            vec![SystemFinding::Blacklisted {
                driver: "it87".to_string()
            }]
        );
    }

    /// Only Controllable counts as drivable. FirmwareOverride in particular
    /// must not, since the whole point is that the write does not stick.
    #[test]
    fn only_controllable_is_drivable() {
        assert!(ChannelVerdict::Controllable.is_controllable());
        assert!(ChannelVerdict::FirmwareOverride.is_controllable().not());
        assert!(ChannelVerdict::NoPwm.is_controllable().not());
    }

    fn clear_environment() -> ProbeEnvironment {
        ProbeEnvironment {
            is_container: false,
            is_secure_boot: false,
            has_dev_port: true,
        }
    }
}
